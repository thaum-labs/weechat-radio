//! SPDX-License-Identifier: Apache-2.0
//! Desktop GUI: setup, station start/stop, live chat, status.

use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::modes::Mode;
use crate::presets::Preset;
use crate::proto::Callsign;
use crate::status::StatusSnapshot;
use eframe::egui::{self, Color32, FontId, RichText, Stroke};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(7, 7, 10);
const FG: Color32 = Color32::from_rgb(216, 208, 232);
const ACCENT: Color32 = Color32::from_rgb(125, 155, 255);
const ORANGE: Color32 = Color32::from_rgb(255, 122, 61);
const DIM: Color32 = Color32::from_rgb(122, 115, 136);
const GREEN: Color32 = Color32::from_rgb(57, 255, 20);

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 680.0])
            .with_min_inner_size([820.0, 520.0])
            .with_title("WeeChat Radio"),
        ..Default::default()
    };
    eframe::run_native(
        "WeeChat Radio",
        options,
        Box::new(|cc| {
            apply_visuals(&cc.egui_ctx);
            Ok(Box::new(GuiApp::new()))
        }),
    )
    .map_err(|e| Error::Msg(e.to_string()))
}

fn apply_visuals(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = Color32::from_rgb(5, 5, 8);
    v.faint_bg_color = Color32::from_rgb(16, 16, 22);
    v.override_text_color = Some(FG);
    v.widgets.inactive.bg_fill = Color32::from_rgb(16, 16, 22);
    v.widgets.hovered.bg_fill = Color32::from_rgb(28, 32, 48);
    v.widgets.active.bg_fill = Color32::from_rgb(36, 42, 64);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.selection.bg_fill = Color32::from_rgb(40, 52, 96);
    v.widgets.inactive.rounding = egui::Rounding::same(2.0);
    style.visuals = v;
    ctx.set_style(style);
}

struct ChatLine {
    nick: String,
    text: String,
    sys: bool,
}

enum IrcEvent {
    Line(ChatLine),
    Status(String),
}

struct GuiApp {
    callsign: String,
    grid: String,
    path: usize,
    com_port: String,
    error: String,
    draft: String,
    chat: Vec<ChatLine>,
    status: Option<StatusSnapshot>,
    node: Option<Child>,
    last_poll: Instant,
    irc_tx: Option<Sender<String>>,
    irc_rx: Option<Receiver<IrcEvent>>,
    auto_started: bool,
}

impl GuiApp {
    fn new() -> Self {
        let (callsign, grid, path, com_port) = load_form();
        Self {
            callsign,
            grid,
            path,
            com_port,
            error: String::new(),
            draft: String::new(),
            chat: Vec::new(),
            status: None,
            node: None,
            last_poll: Instant::now() - Duration::from_secs(2),
            irc_tx: None,
            irc_rx: None,
            auto_started: false,
        }
    }

    fn configured(&self) -> bool {
        Config::default_path().is_file() && Callsign::parse(&self.callsign).is_ok()
    }

    fn save_setup(&mut self) {
        self.error.clear();
        if let Err(e) = Callsign::parse(&self.callsign) {
            self.error = e.to_string();
            return;
        }
        if !self.grid.is_empty() {
            if let Err(e) = crate::grid::normalize(&self.grid) {
                self.error = e.to_string();
                return;
            }
        }
        let mut cfg = if Config::default_path().is_file() {
            Config::load(&Config::default_path()).unwrap_or_default()
        } else {
            Config::default()
        };
        cfg.callsign = self.callsign.trim().to_ascii_uppercase();
        cfg.grid = self.grid.trim().to_ascii_uppercase();
        match self.path {
            0 => {
                cfg.mode = Mode::Internet;
                cfg.modem.manage = false;
                cfg.modem.ptt = "none".into();
            }
            1 => {
                cfg.mode = Mode::InternetRadio;
                cfg.modem.ptt = "digirig".into();
                cfg.modem.preset = Preset::VhfFm.as_str().into();
                cfg.modem.com_port = self.com_port.clone();
                cfg.modem.com_line = "rts".into();
            }
            2 => {
                cfg.mode = Mode::InternetRadio;
                cfg.modem.ptt = "vox".into();
                cfg.modem.preset = Preset::VoxSafe.as_str().into();
            }
            3 => {
                cfg.mode = Mode::InternetRadio;
                cfg.modem.ptt = "rigctl".into();
                cfg.modem.preset = Preset::HfGood.as_str().into();
            }
            _ => {}
        }
        if let Err(e) = config::ensure_dirs() {
            self.error = e.to_string();
            return;
        }
        if let Err(e) = cfg.save(&Config::default_path()) {
            self.error = e.to_string();
            return;
        }
        let _ = crate::weechat_app::configure();
        self.chat.push(ChatLine {
            nick: String::new(),
            text: format!("saved {}", Config::default_path().display()),
            sys: true,
        });
    }

    fn start_station(&mut self) {
        self.error.clear();
        if fetch_status().is_some() {
            self.connect_chat();
            return;
        }
        match spawn_node() {
            Ok(child) => {
                self.node = Some(child);
                self.chat.push(ChatLine {
                    nick: String::new(),
                    text: "station starting…".into(),
                    sys: true,
                });
            }
            Err(e) => self.error = e.to_string(),
        }
    }

    fn stop_station(&mut self) {
        if let Some(mut c) = self.node.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.irc_tx = None;
        self.irc_rx = None;
        self.status = None;
        self.chat.push(ChatLine {
            nick: String::new(),
            text: "station stopped".into(),
            sys: true,
        });
    }

    fn connect_chat(&mut self) {
        if self.irc_tx.is_some() {
            return;
        }
        let nick = self.callsign.trim().to_ascii_uppercase();
        if nick.is_empty() {
            return;
        }
        let (out_tx, out_rx) = mpsc::channel::<String>();
        let (ev_tx, ev_rx) = mpsc::channel::<IrcEvent>();
        std::thread::spawn(move || irc_session(nick, out_rx, ev_tx));
        self.irc_tx = Some(out_tx);
        self.irc_rx = Some(ev_rx);
    }

    fn send_chat(&mut self) {
        let text = self.draft.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.draft.clear();
        if let Some(tx) = &self.irc_tx {
            let _ = tx.send(text.clone());
            self.chat.push(ChatLine {
                nick: self.callsign.to_ascii_uppercase(),
                text,
                sys: false,
            });
        } else {
            self.error = "station is not connected — press Start".into();
        }
    }

    fn drain_irc(&mut self) {
        let mut incoming = Vec::new();
        if let Some(rx) = &self.irc_rx {
            while let Ok(ev) = rx.try_recv() {
                incoming.push(ev);
            }
        }
        for ev in incoming {
            match ev {
                IrcEvent::Line(line) => self.chat.push(line),
                IrcEvent::Status(s) => self.chat.push(ChatLine {
                    nick: String::new(),
                    text: s,
                    sys: true,
                }),
            }
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));
        if self.last_poll.elapsed() > Duration::from_millis(800) {
            self.status = fetch_status();
            self.last_poll = Instant::now();
            if self.status.is_some() {
                self.connect_chat();
            }
        }
        if self.configured() && !self.auto_started {
            self.auto_started = true;
            self.start_station();
        }
        self.drain_irc();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("WEECHAT")
                        .color(ACCENT)
                        .font(FontId::monospace(18.0)),
                );
                ui.label(
                    RichText::new("RADIO")
                        .color(ORANGE)
                        .font(FontId::monospace(18.0)),
                );
                ui.add_space(16.0);
                let running = self.status.is_some();
                if running {
                    if ui
                        .add(egui::Button::new(RichText::new("Stop").color(ORANGE)))
                        .clicked()
                    {
                        self.stop_station();
                    }
                } else if ui
                    .add(egui::Button::new(
                        RichText::new("Start station").color(ACCENT),
                    ))
                    .clicked()
                {
                    self.start_station();
                }
                if ui.button("Open WeeChat").clicked() {
                    if let Err(e) = crate::weechat_app::run(false) {
                        self.error = e.to_string();
                    }
                }
                if ui.button("Live map").clicked() {
                    let _ = open::that("https://weechatradio.com/");
                }
            });
            ui.add_space(6.0);
            ui.separator();
        });

        if !self.configured() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new("  SETUP")
                        .color(ACCENT)
                        .font(FontId::monospace(16.0)),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new("Callsign").color(DIM));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.callsign)
                            .desired_width(160.0)
                            .hint_text("M7TJF or ~NICK"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new("Grid    ").color(DIM));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.grid)
                            .desired_width(160.0)
                            .hint_text("IO81UF"),
                    );
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new("How you get on the air").color(DIM));
                });
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.radio_value(&mut self.path, 0, "Internet only");
                    ui.radio_value(&mut self.path, 1, "Handheld + Digirig");
                    ui.radio_value(&mut self.path, 2, "Audio cable (VOX)");
                    ui.radio_value(&mut self.path, 3, "HF rig (CAT)");
                });
                if self.path == 1 {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(RichText::new("Serial").color(DIM));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.com_port)
                                .desired_width(160.0)
                                .hint_text("COM5"),
                        );
                    });
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    if ui
                        .add(egui::Button::new(
                            RichText::new("Save and start").color(ACCENT),
                        ))
                        .clicked()
                    {
                        self.save_setup();
                        if self.error.is_empty() {
                            self.auto_started = true;
                            self.start_station();
                        }
                    }
                });
                if !self.error.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(&self.error).color(ORANGE));
                }
            });
            return;
        }

        egui::SidePanel::left("status")
            .exact_width(260.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("STATION").color(ACCENT).monospace());
                ui.add_space(8.0);
                if let Some(s) = &self.status {
                    kv(ui, "CALL", &s.callsign, ACCENT);
                    kv(ui, "GRID", &s.grid, FG);
                    kv(ui, "MODE", s.mode.as_str(), mode_color(s.mode));
                    let ptt_label = if s.ptt_on {
                        "TX".to_string()
                    } else {
                        s.channel.to_uppercase()
                    };
                    kv(ui, "PTT", &ptt_label, if s.ptt_on { ORANGE } else { DIM });
                    kv(ui, "PRESET", &s.preset, FG);
                    kv(ui, "AUDIO", &s.audio_label, GREEN);
                    kv(ui, "SNR", &format!("{:.0}", s.snr), FG);
                    kv(ui, "QUEUE", &format!("{}", s.queue_out), FG);
                    kv(
                        ui,
                        "HUB",
                        if s.hub_ok { "UP" } else { "DOWN" },
                        if s.hub_ok { GREEN } else { ORANGE },
                    );
                } else {
                    ui.label(RichText::new("node offline").color(ORANGE).monospace());
                }
                if !self.error.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new(&self.error).color(ORANGE).small());
                }
                ui.add_space(16.0);
                ui.label(
                    RichText::new("Chat is #bulletin on this computer.")
                        .color(DIM)
                        .small(),
                );
                ui.label(
                    RichText::new(
                        "Closing this window does not stop the station unless you press Stop.",
                    )
                    .color(DIM)
                    .small(),
                );
            });

        egui::TopBottomPanel::bottom("input")
            .exact_height(52.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.draft)
                            .desired_width(ui.available_width() - 90.0)
                            .hint_text("message to #bulletin"),
                    );
                    if ui
                        .add(egui::Button::new(RichText::new("Send").color(ORANGE)))
                        .clicked()
                        || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        self.send_chat();
                        resp.request_focus();
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("#bulletin").color(ACCENT).monospace());
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in &self.chat {
                        if line.sys {
                            ui.label(RichText::new(&line.text).color(DIM).monospace());
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!("<{}>", line.nick))
                                        .color(ACCENT)
                                        .monospace(),
                                );
                                ui.label(RichText::new(&line.text).color(FG).monospace());
                            });
                        }
                    }
                });
        });
    }
}

fn kv(ui: &mut egui::Ui, k: &str, v: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{k:7}")).color(DIM).monospace());
        ui.label(RichText::new(v).color(color).monospace());
    });
}

fn mode_color(m: Mode) -> Color32 {
    match m {
        Mode::Internet => Color32::from_rgb(0, 229, 255),
        Mode::InternetRadio => GREEN,
        Mode::Radio => Color32::from_rgb(255, 191, 0),
        Mode::RadioPlus => Color32::from_rgb(255, 77, 255),
    }
}

fn load_form() -> (String, String, usize, String) {
    if let Ok(cfg) = Config::load(&Config::default_path()) {
        let path = match (cfg.mode, cfg.modem.ptt.as_str()) {
            (Mode::Internet, _) => 0,
            (_, "digirig") => 1,
            (_, "vox") => 2,
            (_, "rigctl") => 3,
            _ => 0,
        };
        return (cfg.callsign, cfg.grid, path, cfg.modem.com_port);
    }
    (String::new(), String::new(), 0, String::new())
}

fn spawn_node() -> Result<Child> {
    let mut cmd = node_command();
    cmd.arg("node")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| Error::Msg(e.to_string()))
}

fn node_command() -> Command {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("wcr"));
    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("wcr");
    if stem.eq_ignore_ascii_case("wcr-gui") {
        Command::new(exe.with_file_name(if cfg!(windows) { "wcr.exe" } else { "wcr" }))
    } else {
        Command::new(exe)
    }
}

fn fetch_status() -> Option<StatusSnapshot> {
    let mut stream =
        TcpStream::connect_timeout(&"127.0.0.1:8074".parse().ok()?, Duration::from_millis(250))
            .ok()?;
    stream
        .write_all(b"GET /status HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut body = String::new();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    reader.read_to_string(&mut body).ok()?;
    serde_json::from_str(body.trim()).ok()
}

fn irc_session(nick: String, outgoing: Receiver<String>, events: Sender<IrcEvent>) {
    let addr = "127.0.0.1:6667";
    let mut last_try = Instant::now() - Duration::from_secs(5);
    loop {
        if last_try.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(400));
        }
        last_try = Instant::now();
        let Ok(mut stream) = TcpStream::connect_timeout(
            &addr
                .parse()
                .unwrap_or_else(|_| "127.0.0.1:6667".parse().unwrap()),
            Duration::from_secs(2),
        ) else {
            let _ = events.send(IrcEvent::Status("waiting for local node…".into()));
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
        let _ = stream.set_nodelay(true);
        let hello = format!("NICK {nick}\r\nUSER {nick} 0 * :WeeChat Radio\r\nJOIN #bulletin\r\n");
        if stream.write_all(hello.as_bytes()).is_err() {
            continue;
        }
        let _ = events.send(IrcEvent::Status(format!("connected as {nick}")));
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            while let Ok(msg) = outgoing.try_recv() {
                let line = format!("PRIVMSG #bulletin :{msg}\r\n");
                if stream.write_all(line.as_bytes()).is_err() {
                    break;
                }
            }
            let mut buf = String::new();
            match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let line = buf.trim_end().to_string();
                    if line.starts_with("PING ") {
                        let pong = format!("PONG {}\r\n", line.trim_start_matches("PING ").trim());
                        let _ = stream.write_all(pong.as_bytes());
                        continue;
                    }
                    if let Some(chat) = parse_privmsg(&line) {
                        if !chat.nick.eq_ignore_ascii_case(&nick) {
                            let _ = events.send(IrcEvent::Line(chat));
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
        let _ = events.send(IrcEvent::Status("disconnected — retrying".into()));
    }
}

fn parse_privmsg(line: &str) -> Option<ChatLine> {
    // :nick!user@host PRIVMSG #bulletin :text
    let rest = line.strip_prefix(':')?;
    let (prefix, cmd) = rest.split_once(' ')?;
    if !cmd.starts_with("PRIVMSG ") {
        return None;
    }
    let nick = prefix.split('!').next().unwrap_or(prefix).to_string();
    let text = cmd.split_once(" :")?.1.to_string();
    Some(ChatLine {
        nick,
        text,
        sys: false,
    })
}
