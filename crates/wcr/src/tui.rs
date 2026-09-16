//! SPDX-License-Identifier: Apache-2.0
//! WeeChat-like terminal client talking to the local IRC server.

use crate::config::Config;
use crate::error::Result;
use crate::presets::{self, Preset};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;
use std::collections::VecDeque;
use std::io::Write;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

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
}

#[derive(Clone, Copy)]
struct Theme {
    bg: Color,
    fg: Color,
    accent: Color,
    warn: Color,
}

impl Theme {
    fn hacker() -> Self {
        Self {
            bg: Color::Rgb(5, 8, 7),
            fg: Color::Rgb(57, 255, 20),
            accent: Color::Rgb(0, 229, 255),
            warn: Color::Rgb(255, 191, 0),
        }
    }
    fn terminal() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::Reset,
            accent: Color::Cyan,
            warn: Color::Yellow,
        }
    }
}

impl App {
    fn new(nick: String, unicode: bool, hacker: bool) -> Self {
        Self {
            buffers: vec![Buffer {
                name: "#bulletin".into(),
                lines: VecDeque::new(),
                nicks: vec![nick.clone()],
            }],
            current: 0,
            input: String::new(),
            status: "connecting…".into(),
            theme: if hacker {
                Theme::hacker()
            } else {
                Theme::terminal()
            },
            show_nicks: true,
            show_activity: true,
            mode_picker: false,
            confirm_radio: false,
            nick,
            unicode,
            preset: Preset::VhfFm,
            heard: Vec::new(),
            banner: String::new(),
            running: true,
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

    enable_raw_mode()?;
    let mut terminal = ratatui::init();
    let mut app = App::new(nick.clone(), cfg.ui.unicode, cfg.ui.theme != "terminal");
    app.preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm);
    let mut out = write;

    let res = loop {
        while let Ok(line) = rx_in.try_recv() {
            handle_irc(&mut app, &line);
        }
        terminal.draw(|f| draw(f, &app))?;
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
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

fn draw(f: &mut Frame, app: &App) {
    let t = app.theme;
    f.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.fg)),
        f.area(),
    );
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if app.banner.is_empty() { 0 } else { 1 }),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(f.area());
    if !app.banner.is_empty() {
        f.render_widget(
            Paragraph::new(app.banner.as_str()).style(Style::default().bg(t.warn).fg(Color::Black)),
            chunks[0],
        );
    }
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(16),
            Constraint::Min(20),
            Constraint::Length(if app.show_nicks { 14 } else { 0 }),
        ])
        .split(chunks[1]);

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
        List::new(items).block(Block::default().borders(Borders::ALL).title("buf")),
        body[0],
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
    f.render_widget(
        Paragraph::new(chat).block(
            Block::default()
                .borders(Borders::ALL)
                .title(&*app.cur().name),
        ),
        body[1],
    );
    if app.show_nicks {
        let nicks: Vec<ListItem> = app
            .cur()
            .nicks
            .iter()
            .map(|n| ListItem::new(n.clone()))
            .collect();
        f.render_widget(
            List::new(nicks).block(Block::default().borders(Borders::ALL).title("nicks")),
            body[2],
        );
    }

    let status = format!(
        " {} | {} | heard {} | {} ",
        app.status.chars().take(40).collect::<String>(),
        app.preset.as_str(),
        app.heard.len(),
        if app.mode_picker {
            "F2: 1 inet  2 inet+radio  3 radio  4 radio+"
        } else if app.confirm_radio {
            "Drop internet and go Radio-only? y/N"
        } else {
            "F2 mode"
        }
    );
    f.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(10, 20, 12)).fg(t.accent)),
        chunks[2],
    );

    let over = app.input.len() > 300;
    let frag = app.input.len() > 170;
    let color = if over {
        Color::Red
    } else if frag {
        t.warn
    } else {
        t.fg
    };
    let input = format!("> {}   {}", app.input, app.airtime());
    f.render_widget(
        Paragraph::new(input)
            .style(Style::default().fg(color))
            .block(Block::default().borders(Borders::ALL)),
        chunks[3],
    );
    let _ = Rect::default();
}

pub fn notify(title: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show();
}
