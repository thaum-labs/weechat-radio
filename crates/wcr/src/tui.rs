//! SPDX-License-Identifier: Apache-2.0
//! WeeChat-like terminal client talking to the local IRC server.

use crate::config::Config;
use crate::error::Result;
use crate::presets::{self, Preset};
use crate::status::StatusSnapshot;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;
use std::collections::VecDeque;
use std::io::Write;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

const ASCII_BORDERS: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

const BOOT_LINES: &[&str] = &[
    "WEECHAT RADIO",
    "BOOT SEQUENCE",
    "> load theme ………… OK",
    "> irc bind ………… OK",
    "> status link …… OK",
    "READY.",
];

#[derive(Clone)]
struct ChatLine {
    from: String,
    text: String,
    ticks: String,
    emergency: bool,
}

struct Buffer {
    name: String,
    lines: VecDeque<ChatLine>,
    nicks: Vec<String>,
}

struct App {
    buffers: Vec<Buffer>,
    current: usize,
    input: String,
    status: String,
    theme: Theme,
    theme_name: String,
    show_nicks: bool,
    #[allow(dead_code)]
    show_activity: bool,
    mode_picker: bool,
    confirm_radio: bool,
    nick: String,
    unicode: bool,
    preset: Preset,
    heard: Vec<String>,
    banner: String,
    running: bool,
    snapshot: Option<StatusSnapshot>,
    boot_until: Instant,
}

#[derive(Clone, Copy)]
struct Theme {
    bg: Color,
    fg: Color,
    accent: Color,
    warn: Color,
    border: Color,
    title: Color,
    dim: Color,
}

impl Theme {
    fn tron() -> Self {
        Self {
            bg: Color::Rgb(7, 7, 10),
            fg: Color::Rgb(216, 208, 232),
            accent: Color::Rgb(125, 155, 255),
            warn: Color::Rgb(255, 122, 61),
            border: Color::Rgb(40, 36, 48),
            title: Color::Rgb(125, 155, 255),
            dim: Color::Rgb(122, 115, 136),
        }
    }
    fn hacker() -> Self {
        Self {
            bg: Color::Rgb(5, 8, 7),
            fg: Color::Rgb(57, 255, 20),
            accent: Color::Rgb(0, 229, 255),
            warn: Color::Rgb(255, 191, 0),
            border: Color::Rgb(26, 46, 28),
            title: Color::Rgb(0, 229, 255),
            dim: Color::Rgb(40, 80, 45),
        }
    }
    fn terminal() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::Reset,
            accent: Color::Cyan,
            warn: Color::Yellow,
            border: Color::DarkGray,
            title: Color::Cyan,
            dim: Color::DarkGray,
        }
    }
    fn from_name(name: &str) -> Self {
        match name {
            "terminal" => Self::terminal(),
            "hacker" => Self::hacker(),
            _ => Self::tron(),
        }
    }
}

impl App {
    fn new(nick: String, cfg: &Config) -> Self {
        let theme_name = cfg.ui.theme.clone();
        let boot_ms = if cfg.ui.boot && theme_name != "terminal" && theme_name != "hacker" {
            900
        } else {
            0
        };
        Self {
            buffers: vec![Buffer {
                name: "#bulletin".into(),
                lines: VecDeque::new(),
                nicks: vec![nick.clone()],
            }],
            current: 0,
            input: String::new(),
            status: "connecting…".into(),
            theme: Theme::from_name(&theme_name),
            theme_name,
            show_nicks: true,
            show_activity: cfg.ui.activity_panel,
            mode_picker: false,
            confirm_radio: false,
            nick,
            unicode: cfg.ui.unicode,
            preset: Preset::VhfFm,
            heard: Vec::new(),
            banner: String::new(),
            running: true,
            snapshot: None,
            boot_until: Instant::now() + Duration::from_millis(boot_ms),
        }
    }

    fn cur(&self) -> &Buffer {
        &self.buffers[self.current]
    }

    fn cur_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.current]
    }

    fn airtime(&self) -> String {
        let n = self.input.len();
        let secs = presets::airtime_secs(self.preset, n, 0);
        format!("{n} B ~{secs:.1} s @ {}", self.preset.as_str())
    }

    fn booting(&self) -> bool {
        Instant::now() < self.boot_until
    }
}

pub async fn run(cfg: &Config) -> Result<()> {
    let nick = if cfg.callsign.is_empty() {
        "guest".into()
    } else {
        cfg.callsign.clone()
    };
    let stream = match TcpStream::connect(&cfg.irc.bind).await {
        Ok(s) => s,
        Err(_) => {
            // Start a node in-process, then retry.
            let cfg_n = cfg.clone();
            tokio::spawn(async move {
                let _ = crate::node::run_node(cfg_n, false).await;
            });
            tokio::time::sleep(Duration::from_millis(400)).await;
            TcpStream::connect(&cfg.irc.bind).await?
        }
    };
    stream.set_nodelay(true)?;
    let (read, mut write) = stream.into_split();
    write
        .write_all(format!("CAP LS\r\nNICK {nick}\r\nUSER {nick} 0 * :wcr tui\r\nCAP REQ :message-tags echo-message server-time msgid\r\nCAP END\r\nJOIN #bulletin\r\n").as_bytes())
        .await?;

    let (tx_in, mut rx_in) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx_in.send(line).is_err() {
                break;
            }
        }
    });

    let status_url = format!("http://{}/status", cfg.status.bind);
    let (tx_st, mut rx_st) = mpsc::unbounded_channel::<StatusSnapshot>();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(800))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        loop {
            if let Ok(res) = client.get(&status_url).send().await {
                if let Ok(snap) = res.json::<StatusSnapshot>().await {
                    if tx_st.send(snap).is_err() {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    enable_raw_mode()?;
    let mut terminal = ratatui::init();
    let mut app = App::new(nick.clone(), cfg);
    app.preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm);
    let mut out = write;

    let res = loop {
        while let Ok(line) = rx_in.try_recv() {
            handle_irc(&mut app, &line);
        }
        while let Ok(snap) = rx_st.try_recv() {
            if !snap.preset.is_empty() {
                if let Some(p) = Preset::parse(&snap.preset) {
                    app.preset = p;
                }
            }
            if !snap.hub_banner.is_empty() {
                app.banner = snap.hub_banner.clone();
            }
            app.snapshot = Some(snap);
        }
        terminal.draw(|f| draw(f, &app))?;
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                if app.booting() {
                    app.boot_until = Instant::now();
                    continue;
                }
                if app.mode_picker {
                    match k.code {
                        KeyCode::Esc => app.mode_picker = false,
                        KeyCode::Char('1') => send_mode(&mut out, &mut app, "internet").await?,
                        KeyCode::Char('2') => {
                            send_mode(&mut out, &mut app, "internet-radio").await?
                        }
                        KeyCode::Char('3') => {
                            app.confirm_radio = true;
                            app.mode_picker = false;
                        }
                        KeyCode::Char('4') => send_mode(&mut out, &mut app, "radio-plus").await?,
                        _ => {}
                    }
                    continue;
                }
                if app.confirm_radio {
                    match k.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            send_mode(&mut out, &mut app, "radio confirm").await?;
                            app.confirm_radio = false;
                        }
                        _ => app.confirm_radio = false,
                    }
                    continue;
                }
                match k.code {
                    KeyCode::F(2) => app.mode_picker = true,
                    KeyCode::Esc | KeyCode::Char('c')
                        if k.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        app.running = false;
                    }
                    KeyCode::Enter => {
                        let line = std::mem::take(&mut app.input);
                        if !line.is_empty() {
                            if let Some(cmd) = line.strip_prefix('/') {
                                out.write_all(format!("{cmd}\r\n").as_bytes()).await?;
                            } else {
                                let target = app.cur().name.clone();
                                let nick = app.nick.clone();
                                let ticks = if app.unicode { "·" } else { "-" };
                                out.write_all(format!("PRIVMSG {target} :{line}\r\n").as_bytes())
                                    .await?;
                                app.cur_mut().lines.push_back(ChatLine {
                                    from: nick,
                                    text: line,
                                    ticks: ticks.into(),
                                    emergency: false,
                                });
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Tab => {
                        if let Some(prefix) = app.input.split_whitespace().last() {
                            if let Some(n) = app.cur().nicks.iter().find(|n| n.starts_with(prefix))
                            {
                                let p = prefix.to_string();
                                app.input = app.input.replacen(&p, n, 1);
                            }
                        }
                    }
                    KeyCode::Char(d)
                        if k.modifiers.contains(KeyModifiers::ALT) && d.is_ascii_digit() =>
                    {
                        if let Some(i) = d.to_digit(10) {
                            let i = i as usize;
                            if i >= 1 && i <= app.buffers.len() {
                                app.current = i - 1;
                            }
                        }
                    }
                    KeyCode::Char(c) => app.input.push(c),
                    _ => {}
                }
            }
        }
        if !app.running {
            break Ok(());
        }
    };

    disable_raw_mode()?;
    ratatui::restore();
    let _ = std::io::stdout().flush();
    res
}

async fn send_mode(
    out: &mut tokio::net::tcp::OwnedWriteHalf,
    app: &mut App,
    mode: &str,
) -> Result<()> {
    out.write_all(format!("RADIO mode {mode}\r\n").as_bytes())
        .await?;
    app.mode_picker = false;
    Ok(())
}

fn handle_irc(app: &mut App, line: &str) {
    if line.starts_with("PING") {
        return;
    }
    app.status = line.chars().take(80).collect();
    if line.contains("PRIVMSG") {
        if let Some((head, text)) = line.split_once(" :") {
            let from = head
                .rsplit_once(' ')
                .and_then(|_| head.split(':').nth(1))
                .unwrap_or("?")
                .split('!')
                .next()
                .unwrap_or("?")
                .to_string();
            let emergency = text.starts_with("!!");
            if !app.heard.contains(&from) && from != app.nick {
                app.heard.push(from.clone());
            }
            app.cur_mut().lines.push_back(ChatLine {
                from,
                text: text.to_string(),
                ticks: String::new(),
                emergency,
            });
            if app.cur().lines.len() > 500 {
                app.cur_mut().lines.pop_front();
            }
        }
    }
    if line.contains("radio/delivery=") {
        if let Some(rest) = line.split("radio/delivery=").nth(1) {
            let state = rest.split(';').next().unwrap_or("");
            let uni = app.unicode;
            if let Some(last) = app.cur_mut().lines.back_mut() {
                last.ticks = match state {
                    "sent" => if uni { "✓" } else { "v" }.into(),
                    "relayed" => if uni { "✓✓" } else { "vv" }.into(),
                    "delivered" => if uni { "✓✓" } else { "VV" }.into(),
                    "all" => if uni { "✓✓✓" } else { "VVV" }.into(),
                    _ => last.ticks.clone(),
                };
            }
        }
    }
    if line.contains("Internet down") {
        app.banner = "Internet down, radio only".into();
    }
}

fn panel<'a>(title: &'a str, meta: &'a str, app: &App) -> Block<'a> {
    let t = app.theme;
    let mut b = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.border))
        .title(Span::styled(
            title,
            Style::default().fg(t.title).add_modifier(Modifier::BOLD),
        ))
        .title(Line::from(Span::styled(meta, Style::default().fg(t.dim))).right_aligned())
        .style(Style::default().bg(t.bg).fg(t.fg));
    if !app.unicode {
        b = b.border_set(ASCII_BORDERS);
    }
    b
}

fn kv_lines(app: &App, rows: &[(&str, String)]) -> Vec<Line<'static>> {
    let t = app.theme;
    rows.iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k:<8} "), Style::default().fg(t.dim)),
                Span::styled(v.clone(), Style::default().fg(t.fg)),
            ])
        })
        .collect()
}

fn draw(f: &mut Frame, app: &App) {
    let t = app.theme;
    f.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.fg)),
        f.area(),
    );
    if app.booting() {
        draw_boot(f, app);
        return;
    }

    let banner_h = if app.banner.is_empty() { 0 } else { 1 };
    let chunks = Layout::vertical([
        Constraint::Length(banner_h),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .split(f.area());

    if !app.banner.is_empty() {
        f.render_widget(
            Paragraph::new(app.banner.as_str()).style(Style::default().bg(t.warn).fg(Color::Black)),
            chunks[0],
        );
    }

    let snap = app.snapshot.as_ref();
    let mode = snap
        .map(|s| s.mode.as_str().to_ascii_uppercase())
        .unwrap_or_else(|| "MODE".into());
    let hub = match snap {
        Some(s) if s.hub_ok => "HUB UP",
        Some(_) => "HUB DOWN",
        None => "HUB …",
    };
    let clock = chrono::Utc::now().format("%H:%M:%S").to_string();
    let ident = snap
        .map(|s| {
            format!(
                "{} {}",
                if s.callsign.is_empty() {
                    app.nick.as_str()
                } else {
                    s.callsign.as_str()
                },
                s.grid
            )
        })
        .unwrap_or_else(|| app.nick.clone());
    let top = format!(" {clock} UTC  │  {ident}  │  {mode}  │  {hub} ");
    f.render_widget(
        Paragraph::new(top).style(Style::default().fg(t.accent).bg(t.bg)),
        chunks[1],
    );

    let right_w = if app.show_nicks { 22 } else { 0 };
    let body = Layout::horizontal([
        Constraint::Length(24),
        Constraint::Min(20),
        Constraint::Length(right_w),
    ])
    .split(chunks[2]);

    let left = Layout::vertical([
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Min(4),
    ])
    .split(body[0]);

    let items: Vec<ListItem> = app
        .buffers
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let style = if i == app.current {
                Style::default().add_modifier(Modifier::BOLD).fg(t.accent)
            } else {
                Style::default().fg(t.fg)
            };
            ListItem::new(b.name.clone()).style(style)
        })
        .collect();
    f.render_widget(
        List::new(items).block(panel("BUFFERS", &format!("{}", app.buffers.len()), app)),
        left[0],
    );

    let ptt = snap
        .map(|s| {
            if s.ptt_on {
                "TX".into()
            } else {
                s.channel.clone()
            }
        })
        .unwrap_or_else(|| "idle".into());
    let station_rows = [
        ("preset", app.preset.as_str().into()),
        ("ptt", ptt),
        (
            "snr",
            snap.map(|s| format!("{:.0}", s.snr))
                .unwrap_or_else(|| "—".into()),
        ),
        (
            "audio",
            snap.map(|s| s.audio_label.clone())
                .unwrap_or_else(|| "—".into()),
        ),
        (
            "queue",
            snap.map(|s| format!("{}", s.queue_out))
                .unwrap_or_else(|| "—".into()),
        ),
    ];
    f.render_widget(
        Paragraph::new(kv_lines(app, &station_rows)).block(panel("STATION", &app.nick, app)),
        left[1],
    );

    let heard: Vec<ListItem> = if app.heard.is_empty() {
        vec![ListItem::new("—")]
    } else {
        app.heard.iter().map(|n| ListItem::new(n.clone())).collect()
    };
    f.render_widget(
        List::new(heard).block(panel("HEARD", &format!("{}", app.heard.len()), app)),
        left[2],
    );

    let chat: Vec<Line> = app
        .cur()
        .lines
        .iter()
        .map(|l| {
            let style = if l.emergency {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.fg)
            };
            Line::from(vec![
                Span::styled(format!("{:<8} ", l.from), Style::default().fg(t.accent)),
                Span::styled(l.text.clone(), style),
                Span::raw(" "),
                Span::styled(l.ticks.clone(), Style::default().fg(t.accent)),
            ])
        })
        .collect();
    let hint = if app.mode_picker {
        "F2 1/2/3/4"
    } else if app.confirm_radio {
        "RADIO-ONLY? y/N"
    } else {
        "F2 MODE"
    };
    f.render_widget(
        Paragraph::new(chat).block(panel("TERMINAL", hint, app)),
        body[1],
    );

    if app.show_nicks {
        let right = Layout::vertical([Constraint::Min(4), Constraint::Length(9)]).split(body[2]);
        let nicks: Vec<ListItem> = app
            .cur()
            .nicks
            .iter()
            .map(|n| ListItem::new(n.clone()))
            .collect();
        f.render_widget(
            List::new(nicks).block(panel("NICKS", &format!("{}", app.cur().nicks.len()), app)),
            right[0],
        );
        let net_rows = [
            (
                "hub",
                snap.map(|s| if s.hub_ok { "up".into() } else { "down".into() })
                    .unwrap_or_else(|| "…".into()),
            ),
            (
                "lan",
                snap.map(|s| format!("{}", s.lan_peers))
                    .unwrap_or_else(|| "—".into()),
            ),
            (
                "freq",
                snap.map(|s| {
                    if s.frequency.is_empty() {
                        s.preset.clone()
                    } else {
                        s.frequency.clone()
                    }
                })
                .unwrap_or_else(|| app.preset.as_str().into()),
            ),
            ("irc", app.status.chars().take(18).collect::<String>()),
        ];
        f.render_widget(
            Paragraph::new(kv_lines(app, &net_rows)).block(panel("NETWORK", &app.theme_name, app)),
            right[1],
        );
    }

    let over = app.input.len() > 300;
    let frag = app.input.len() > 170;
    let color = if over {
        Color::Red
    } else if frag {
        t.warn
    } else {
        t.fg
    };
    let prompt = if app.unicode { "▸ " } else { "> " };
    let input = format!("{prompt}{}   {}", app.input, app.airtime());
    f.render_widget(
        Paragraph::new(input)
            .style(Style::default().fg(color))
            .block(panel("INPUT", "ENTER SEND", app)),
        chunks[3],
    );
}

fn draw_boot(f: &mut Frame, app: &App) {
    let t = app.theme;
    let elapsed = app
        .boot_until
        .saturating_duration_since(Instant::now())
        .as_millis();
    let shown = BOOT_LINES
        .len()
        .saturating_sub((elapsed / 140) as usize)
        .max(2);
    let text: Vec<Line> = BOOT_LINES
        .iter()
        .take(shown.min(BOOT_LINES.len()))
        .map(|l| Line::from(Span::styled(*l, Style::default().fg(t.fg))))
        .collect();
    let area = centered(f.area(), 40, 10);
    f.render_widget(Paragraph::new(text).block(panel("BOOT", "TRON", app)), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w.min(area.width),
        height: h.min(area.height),
    }
}

pub fn notify(title: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show();
}
