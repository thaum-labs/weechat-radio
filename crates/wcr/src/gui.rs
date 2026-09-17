//! SPDX-License-Identifier: Apache-2.0
//! Desktop GUI: setup, station start/stop, live chat, status.

use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::modes::Mode;
use crate::presets::Preset;
use crate::proto::Callsign;
use crate::status::StatusSnapshot;
use eframe::egui::{self, Color32, FontId, IconData, RichText, Stroke, ViewportCommand};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

// Match weechatradio.com :root in web/styles.css
const BG: Color32 = Color32::from_rgb(7, 7, 10);
const TOPBAR: Color32 = Color32::from_rgb(5, 5, 8);
const SHELL: Color32 = Color32::from_rgb(21, 21, 30);
const FG: Color32 = Color32::from_rgb(216, 208, 232);
const ACCENT: Color32 = Color32::from_rgb(125, 155, 255);
const ORANGE: Color32 = Color32::from_rgb(255, 122, 61);
const PURPLE: Color32 = Color32::from_rgb(196, 181, 253);
const DIM: Color32 = Color32::from_rgb(122, 115, 136);
const LINE: Color32 = Color32::from_rgb(33, 40, 64);
const GREEN: Color32 = Color32::from_rgb(57, 255, 20);
const CODE: Color32 = Color32::from_rgb(18, 18, 24);

static TRAY_QUIT_ID: OnceLock<MenuId> = OnceLock::new();
static TRAY_SHOW_ID: OnceLock<MenuId> = OnceLock::new();
static TRAY_STOP_ID: OnceLock<MenuId> = OnceLock::new();
static TRAY_ACTION: AtomicU8 = AtomicU8::new(0);
static TRAY_WATCH: AtomicBool = AtomicBool::new(false);
static GUI_CTX: OnceLock<egui::Context> = OnceLock::new();

pub fn run() -> Result<()> {
    let icon = app_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 680.0])
            .with_min_inner_size([820.0, 520.0])
            .with_title("WeeChat Radio")
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "WeeChat Radio",
        options,
        Box::new(|cc| {
            apply_fonts(&cc.egui_ctx);
            apply_visuals(&cc.egui_ctx);
            Ok(Box::new(GuiApp::new()))
        }),
    )
    .map_err(|e| Error::Msg(e.to_string()))
}

fn hairline(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}

fn apply_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(mono) = fonts.families.get(&egui::FontFamily::Monospace).cloned() {
        fonts.families.insert(egui::FontFamily::Proportional, mono);
    }
    ctx.set_fonts(fonts);
}

fn paint_scan(ctx: &egui::Context) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("scan"),
    ));
    let rect = ctx.screen_rect();
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), y + 3.0),
                egui::pos2(rect.right(), y + 4.0),
            ),
            0.0,
            Color32::from_black_alpha(46),
        );
        y += 4.0;
    }
}

fn module_title(ui: &mut egui::Ui, left: &str, right: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(left)
                .color(ACCENT)
                .font(FontId::monospace(12.0)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(right)
                    .color(ORANGE)
                    .font(FontId::monospace(12.0)),
            );
        });
    });
}

fn prompt_mark(ui: &mut egui::Ui, wave: Color32) {
    paint_brand_mark(ui, wave, 26.0);
    ui.add_space(8.0);
    ui.label(
        RichText::new("weechat")
            .color(FG)
            .strong()
            .font(FontId::monospace(18.0)),
    );
    ui.label(
        RichText::new("radio")
            .color(ACCENT)
            .font(FontId::monospace(18.0)),
    );
}

fn paint_brand_mark(ui: &mut egui::Ui, wave: Color32, height: f32) {
    let size = egui::vec2(height * 64.0 / 60.0, height);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let s = rect.width() / 64.0;
    let to = |x: f32, y: f32| rect.min + egui::vec2(x * s, y * s);
    let painter = ui.painter();
    let w5 = 5.0 * s;
    let w4 = 4.0 * s;
    let p1 = to(8.0, 20.0);
    let p2 = to(20.0, 30.0);
    let p3 = to(8.0, 40.0);
    painter.line_segment([p1, p2], Stroke::new(w5, FG));
    painter.line_segment([p2, p3], Stroke::new(w5, FG));
    let cap = w5 * 0.5;
    painter.circle_filled(p1, cap, FG);
    painter.circle_filled(p2, cap, FG);
    painter.circle_filled(p3, cap, FG);
    painter.rect_filled(
        egui::Rect::from_min_size(to(25.0, 36.0), egui::vec2(15.0 * s, 5.0 * s)),
        1.0 * s,
        wave,
    );
    let c = to(40.0, 36.0);
    paint_quarter_arc(painter, c, 7.0 * s, Stroke::new(w4, wave));
    paint_quarter_arc(
        painter,
        c,
        13.0 * s,
        Stroke::new(w4, with_opacity(wave, 0.7)),
    );
    paint_quarter_arc(
        painter,
        c,
        19.0 * s,
        Stroke::new(w4, with_opacity(wave, 0.4)),
    );
}

fn with_opacity(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0) as u8)
}

fn paint_quarter_arc(painter: &egui::Painter, c: egui::Pos2, r: f32, stroke: Stroke) {
    let n = 18;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let ang = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::FRAC_PI_2;
        pts.push(egui::pos2(c.x + r * ang.cos(), c.y + r * ang.sin()));
    }
    if let (Some(&a), Some(&b)) = (pts.first(), pts.last()) {
        painter.add(egui::Shape::line(pts, stroke));
        let cap = stroke.width * 0.5;
        painter.circle_filled(a, cap, stroke.color);
        painter.circle_filled(b, cap, stroke.color);
    }
}
fn apply_visuals(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = TOPBAR;
    v.faint_bg_color = SHELL;
    v.window_stroke = hairline(LINE);
    v.widgets.noninteractive.bg_stroke = hairline(LINE);
    v.override_text_color = Some(FG);
    v.hyperlink_color = ORANGE;
    v.selection.bg_fill = Color32::from_rgb(40, 52, 96);
    v.selection.stroke = hairline(ACCENT);
    v.widgets.inactive.bg_fill = Color32::BLACK;
    v.widgets.inactive.weak_bg_fill = CODE;
    v.widgets.inactive.bg_stroke = hairline(LINE);
    v.widgets.inactive.fg_stroke = hairline(ACCENT);
    v.widgets.inactive.rounding = egui::Rounding::ZERO;
    v.widgets.hovered.bg_fill = Color32::from_rgb(12, 12, 18);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(12, 12, 18);
    v.widgets.hovered.bg_stroke = hairline(ORANGE);
    v.widgets.hovered.fg_stroke = hairline(ORANGE);
    v.widgets.hovered.rounding = egui::Rounding::ZERO;
    v.widgets.active.bg_fill = Color32::from_rgb(18, 22, 36);
    v.widgets.active.bg_stroke = hairline(ORANGE);
    v.widgets.active.fg_stroke = hairline(ORANGE);
    v.widgets.active.rounding = egui::Rounding::ZERO;
    v.widgets.open.rounding = egui::Rounding::ZERO;
    v.widgets.open.bg_stroke = hairline(ACCENT);
    style.visuals = v;
    ctx.set_style(style);
}

fn chrome(fill: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(hairline(LINE))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .rounding(0.0)
}

fn nav_link(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let font = FontId::monospace(13.0);
    let galley = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), FG));
    let size = galley.size() + egui::vec2(10.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let color = if resp.hovered() || resp.is_pointer_button_down_on() {
        ORANGE
    } else {
        PURPLE
    };
    ui.painter().text(
        rect.left_center() + egui::vec2(5.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

struct ChatLine {
    nick: String,
    text: String,
    sys: bool,
    channel: String,
}

enum IrcEvent {
    Line(ChatLine),
    Status(String),
    Joined(String),
    Invited { from: String, channel: String },
}

struct GuiApp {
    callsign: String,
    grid: String,
    grid_note: String,
    grid_overwrite: bool,
    grid_rx: Option<Receiver<Option<crate::grid::DetectedGrid>>>,
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
    palette_i: usize,
    confirm_radio: bool,
    tray: Option<TrayIcon>,
    tray_show: MenuId,
    tray_stop: MenuId,
    tray_quit: MenuId,
    tray_icon_key: i8,
    allow_close: bool,
    user_stopped: bool,
    active_channel: String,
    joined: Vec<String>,
    chan_new: String,
    invite_draft: String,
    members: HashMap<String, Vec<String>>,
}

impl GuiApp {
    fn new() -> Self {
        let (callsign, grid, path, com_port) = load_form();
        let need_grid = grid.is_empty();
        let mut app = Self {
            callsign,
            grid,
            grid_note: if need_grid {
                "detecting from IP…".into()
            } else {
                String::new()
            },
            grid_overwrite: false,
            grid_rx: if need_grid {
                Some(spawn_grid_detect())
            } else {
                None
            },
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
            palette_i: 0,
            confirm_radio: false,
            tray: None,
            tray_show: MenuId::new(""),
            tray_stop: MenuId::new(""),
            tray_quit: MenuId::new(""),
            tray_icon_key: 0,
            allow_close: false,
            user_stopped: false,
            active_channel: "#bulletin".into(),
            joined: vec!["#bulletin".into()],
            chan_new: String::new(),
            invite_draft: String::new(),
            members: HashMap::new(),
        };
        app.install_tray();
        app
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
        cfg.ui.theme = "tron".into();
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
            channel: String::new(),
        });
    }

    fn start_station(&mut self) {
        self.error.clear();
        self.user_stopped = false;
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
                    channel: String::new(),
                });
            }
            Err(e) => self.error = e.to_string(),
        }
    }

    fn stop_station(&mut self) {
        self.user_stopped = true;
        if let Some(mut c) = self.node.take() {
            kill_pid_tree(c.id());
            let _ = c.kill();
        }
        kill_sidecars();
        self.irc_tx = None;
        self.irc_rx = None;
        self.status = None;
        self.chat.push(ChatLine {
            nick: String::new(),
            text: "station stopped".into(),
            sys: true,
            channel: String::new(),
        });
        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some("WeeChat Radio — station stopped"));
        }
    }

    fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        if let Some(tray) = &self.tray {
            let tip = if self.status.is_some() {
                "WeeChat Radio — station running"
            } else {
                "WeeChat Radio"
            };
            let _ = tray.set_tooltip(Some(tip));
        }
    }

    fn quit_app(&mut self, ctx: &egui::Context) {
        self.allow_close = true;
        self.user_stopped = true;
        drop(self.tray.take());
        if let Some(mut c) = self.node.take() {
            let _ = c.kill();
        }
        ctx.send_viewport_cmd(ViewportCommand::Close);
        force_quit();
    }

    fn install_tray(&mut self) {
        let show = MenuItem::new("Show window", true, None);
        let stop = MenuItem::new("Stop station", true, None);
        let quit = MenuItem::new("Quit", true, None);
        self.tray_show = show.id().clone();
        self.tray_stop = stop.id().clone();
        self.tray_quit = quit.id().clone();
        let _ = TRAY_SHOW_ID.set(self.tray_show.clone());
        let _ = TRAY_STOP_ID.set(self.tray_stop.clone());
        let _ = TRAY_QUIT_ID.set(self.tray_quit.clone());
        start_tray_watch();
        let menu = Menu::new();
        if menu
            .append_items(&[&show, &stop, &PredefinedMenuItem::separator(), &quit])
            .is_err()
        {
            return;
        }
        let (rgba, w, h) = raster_mark(32, [125, 155, 255], true);
        let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h) else {
            return;
        };
        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("WeeChat Radio")
            .with_icon(icon)
            .with_title("WeeChat Radio")
            .build()
        {
            Ok(tray) => self.tray = Some(tray),
            Err(_) => {}
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let _ = GUI_CTX.set(ctx.clone());
        match TRAY_ACTION.swap(0, Ordering::SeqCst) {
            1 => self.show_window(ctx),
            2 => self.stop_station(),
            _ => {}
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.allow_close || self.tray.is_none() {
                self.allow_close = true;
                force_quit();
            } else {
                self.hide_to_tray(ctx);
            }
        }
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
        self.send_line(&text);
    }

    fn send_cmd(&mut self, cmd: &str) {
        self.send_line(cmd);
    }

    fn send_line(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let wires = crate::slash::to_wire(text, &self.active_channel);
        if wires.is_empty() {
            self.error = "need a channel name or callsign".into();
            return;
        }
        if self.irc_tx.is_none() {
            self.error = "station is not connected — press Start".into();
            return;
        }
        if text.starts_with('/') {
            self.chat.push(ChatLine {
                nick: String::new(),
                text: format!("▸ {text}"),
                sys: true,
                channel: self.active_channel.clone(),
            });
        } else {
            self.chat.push(ChatLine {
                nick: self.callsign.to_ascii_uppercase(),
                text: text.to_string(),
                sys: false,
                channel: self.active_channel.clone(),
            });
        }
        for line in &wires {
            if let Some(ch) = line.strip_prefix("JOIN ") {
                self.ensure_joined(ch);
                self.active_channel = ch.to_string();
            }
        }
        if let Some(tx) = &self.irc_tx {
            for line in wires {
                let _ = tx.send(line);
            }
        }
    }

    fn ensure_joined(&mut self, ch: &str) {
        let ch = crate::slash::normalize_channel(ch);
        if !self.joined.iter().any(|c| c.eq_ignore_ascii_case(&ch)) {
            self.joined.push(ch);
        }
    }

    fn create_channel(&mut self) {
        self.error.clear();
        let ch = crate::slash::normalize_channel(&self.chan_new);
        if ch.len() < 3 {
            self.error = "channel name is too short".into();
            return;
        }
        self.chan_new.clear();
        self.ensure_joined(&ch);
        self.active_channel = ch.clone();
        let me = self.callsign.to_ascii_uppercase();
        if !crate::slash::is_bulletin(&ch) {
            self.members
                .entry(ch.clone())
                .or_insert_with(|| vec![me.clone()]);
            self.send_line(&format!("/join {ch}"));
            self.send_line(&format!(
                "/group create {} {me}",
                ch.trim_start_matches('#')
            ));
        } else {
            self.send_line(&format!("/join {ch}"));
        }
    }

    fn invite_to_current(&mut self) {
        self.error.clear();
        if crate::slash::is_bulletin(&self.active_channel) {
            self.error = "create a closed channel first — #bulletin is public".into();
            return;
        }
        let nick = self.invite_draft.trim().to_ascii_uppercase();
        if nick.is_empty() || Callsign::parse(&nick).is_err() {
            self.error = "invite needs a callsign (or ~NICK)".into();
            return;
        }
        self.invite_draft.clear();
        self.members
            .entry(self.active_channel.clone())
            .or_default()
            .push(nick.clone());
        self.send_line(&format!("/invite {nick}"));
    }

    fn switch_channel(&mut self, ch: String) {
        let ch = crate::slash::normalize_channel(&ch);
        self.ensure_joined(&ch);
        if !ch.eq_ignore_ascii_case(&self.active_channel) {
            self.active_channel = ch.clone();
            self.send_line(&format!("/join {ch}"));
        }
    }

    fn apply_suggestion(&mut self, s: &crate::slash::Suggestion, send: bool) {
        self.draft = s.insert.clone();
        if send && s.send_now {
            self.send_chat();
        }
        self.palette_i = 0;
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
                    channel: self.active_channel.clone(),
                }),
                IrcEvent::Joined(ch) => {
                    self.ensure_joined(&ch);
                }
                IrcEvent::Invited { from, channel } => {
                    self.ensure_joined(&channel);
                    self.chat.push(ChatLine {
                        nick: String::new(),
                        text: format!("{from} invited you to {channel}"),
                        sys: true,
                        channel: channel.clone(),
                    });
                    self.active_channel = crate::slash::normalize_channel(&channel);
                    self.send_line(&format!("/join {channel}"));
                }
            }
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));
        self.poll_grid_detect();
        self.poll_tray(ctx);
        if self.last_poll.elapsed() > Duration::from_millis(800) {
            self.last_poll = Instant::now();
            if self.user_stopped {
                if fetch_status().is_some() {
                    kill_sidecars();
                }
                self.status = None;
            } else {
                self.status = fetch_status();
                if self.status.is_some() {
                    self.connect_chat();
                }
            }
        }
        if self.configured() && !self.auto_started && !self.user_stopped {
            self.auto_started = true;
            self.start_station();
        }
        self.drain_irc();

        let mut suggestions = crate::slash::suggestions(&self.draft);
        if self.palette_i >= suggestions.len() {
            self.palette_i = 0;
        }
        let mut tab = false;
        let mut up = false;
        let mut down = false;
        let mut enter = false;
        let mut esc = false;
        let palette_open = !suggestions.is_empty();
        if self.configured() {
            ctx.input_mut(|i| {
                enter = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                if palette_open {
                    down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                    up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                    tab = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                    esc = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                }
            });
        }
        if palette_open {
            if down {
                self.palette_i = (self.palette_i + 1) % suggestions.len();
            }
            if up {
                self.palette_i = (self.palette_i + suggestions.len() - 1) % suggestions.len();
            }
            if tab {
                if let Some(s) = suggestions.get(self.palette_i) {
                    self.draft = s.insert.clone();
                }
            }
            if esc {
                self.draft.clear();
            }
            if enter {
                if let Some(s) = suggestions.get(self.palette_i).cloned() {
                    let typed = self.draft.trim();
                    let target = s.insert.trim();
                    let completing = typed != target
                        && (target.starts_with(typed)
                            || (!typed.contains(' ') && s.label.starts_with(typed)));
                    if completing {
                        self.apply_suggestion(&s, true);
                    } else {
                        self.send_chat();
                    }
                }
            }
        } else if enter {
            self.send_chat();
        }
        suggestions = crate::slash::suggestions(&self.draft);
        if self.palette_i >= suggestions.len() {
            self.palette_i = 0;
        }

        egui::TopBottomPanel::top("top")
            .frame(chrome(TOPBAR))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let wave = self
                        .status
                        .as_ref()
                        .map(|s| mode_color(s.mode))
                        .unwrap_or(ACCENT);
                    prompt_mark(ui, wave);
                    ui.add_space(14.0);
                    let running = self.status.is_some();
                    if running {
                        if nav_link(ui, "stop").clicked() {
                            self.stop_station();
                        }
                    } else if nav_link(ui, "start").clicked() {
                        self.start_station();
                    }
                    if nav_link(ui, "weechat").clicked() {
                        if let Err(e) = crate::weechat_app::run(false) {
                            self.error = e.to_string();
                        }
                    }
                    if nav_link(ui, "map").clicked() {
                        let _ = open::that("https://weechatradio.com/");
                    }
                    if nav_link(ui, "quit").clicked() {
                        self.quit_app(ctx);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(s) = &self.status {
                            ui.label(
                                RichText::new(if s.hub_ok { "UP" } else { "DOWN" })
                                    .color(if s.hub_ok { GREEN } else { ORANGE })
                                    .font(FontId::monospace(12.0)),
                            );
                            ui.label(
                                RichText::new("hub")
                                    .color(DIM)
                                    .font(FontId::monospace(12.0)),
                            );
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(format!("{}", s.queue_out))
                                    .color(ACCENT)
                                    .font(FontId::monospace(12.0)),
                            );
                            ui.label(
                                RichText::new("queue")
                                    .color(DIM)
                                    .font(FontId::monospace(12.0)),
                            );
                        }
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(chrono::Local::now().format("LCL %H:%M").to_string())
                                .color(DIM)
                                .font(FontId::monospace(11.0)),
                        );
                        ui.label(
                            RichText::new(chrono::Utc::now().format("UTC %H:%M:%S").to_string())
                                .color(DIM)
                                .font(FontId::monospace(11.0)),
                        );
                    });
                });
            });

        if !self.configured() {
            egui::CentralPanel::default()
                .frame(chrome(SHELL))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    module_title(ui, "SETUP", "FIRST RUN");
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
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Detect").color(PURPLE).monospace(),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(hairline(LINE)),
                            )
                            .clicked()
                        {
                            self.grid_note = "detecting from IP…".into();
                            self.grid_overwrite = true;
                            self.grid_rx = Some(spawn_grid_detect());
                        }
                    });
                    if !self.grid_note.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.label(
                                RichText::new(&self.grid_note)
                                    .color(DIM)
                                    .font(FontId::monospace(11.0)),
                            );
                        });
                    }
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
                            .add(
                                egui::Button::new(
                                    RichText::new("Save and start").color(ACCENT).monospace(),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(hairline(ACCENT)),
                            )
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
            .frame(chrome(TOPBAR))
            .show(ctx, |ui| {
                module_title(
                    ui,
                    "STATION",
                    if self.status.is_some() {
                        "LIVE"
                    } else {
                        "OFFLINE"
                    },
                );
                ui.add_space(8.0);
                let mut mode_cmd = None;
                let mut preset_cmd = None;
                let mut want_radio_confirm = false;
                if let Some(s) = &self.status {
                    kv(ui, "CALL", &s.callsign, ACCENT);
                    kv(ui, "GRID", &s.grid, PURPLE);
                    if let Some(next) = mode_pick(ui, s.mode) {
                        if next == Mode::Radio && s.mode != Mode::Radio {
                            want_radio_confirm = true;
                        } else if next != s.mode {
                            mode_cmd = Some(format!("/mode {}", next.as_str()));
                        }
                    }
                    let ptt_label = if s.ptt_on {
                        "TX".to_string()
                    } else if s.deferred {
                        "WAIT".to_string()
                    } else {
                        s.channel.to_uppercase()
                    };
                    kv(ui, "PTT", &ptt_label, if s.ptt_on { ORANGE } else { DIM });
                    if let Some(next) = preset_pick(ui, &s.preset) {
                        if next != s.preset {
                            preset_cmd = Some(format!("/preset {next}"));
                        }
                    }
                    kv(ui, "AUDIO", &s.audio_label, GREEN);
                    kv(ui, "SNR", &format!("{:.0}", s.snr), PURPLE);
                    kv(
                        ui,
                        "TX",
                        if s.tx_rung.is_empty() {
                            "—"
                        } else {
                            &s.tx_rung
                        },
                        PURPLE,
                    );
                    kv(ui, "RETRY", &format!("{}", s.retries), PURPLE);
                    kv(ui, "OCC", &format!("{}%", s.occupancy_pct), PURPLE);
                    kv(
                        ui,
                        "QUEUE",
                        &format!("{} air {}", s.queue_out, s.queue_air),
                        PURPLE,
                    );
                    kv(
                        ui,
                        "HUB",
                        if s.hub_ok { "UP" } else { "DOWN" },
                        if s.hub_ok { GREEN } else { ORANGE },
                    );
                    if !s.hub_ok && !s.hub_banner.is_empty() {
                        ui.label(
                            RichText::new(&s.hub_banner)
                                .color(ORANGE)
                                .small()
                                .monospace(),
                        );
                    }
                } else {
                    ui.label(RichText::new("node offline").color(ORANGE).monospace());
                }
                if want_radio_confirm {
                    self.confirm_radio = true;
                }
                if let Some(cmd) = mode_cmd {
                    self.send_cmd(&cmd);
                }
                if let Some(cmd) = preset_cmd {
                    self.send_cmd(&cmd);
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
                        "Close hides to the tray. Quit from the tray to stop the station and the beacon.",
                    )
                    .color(DIM)
                    .small(),
                );
            });

        if self.confirm_radio {
            egui::Window::new("Switch to radio?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(chrome(TOPBAR))
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("Radio mode drops the internet. Messages stay on RF only.")
                            .color(FG)
                            .monospace(),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Confirm").color(ORANGE).monospace(),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(hairline(ORANGE)),
                            )
                            .clicked()
                        {
                            self.send_cmd("/mode radio confirm");
                            self.confirm_radio = false;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Cancel").color(PURPLE).monospace(),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(hairline(LINE)),
                            )
                            .clicked()
                        {
                            self.confirm_radio = false;
                        }
                    });
                });
        }

        let palette_h = if suggestions.is_empty() {
            0.0
        } else {
            (suggestions.len().min(8) as f32) * 28.0 + 28.0
        };
        egui::TopBottomPanel::bottom("input")
            .exact_height(78.0 + palette_h)
            .frame(chrome(TOPBAR))
            .show(ctx, |ui| {
                module_title(ui, "TRAFFIC LOG", "INPUT");
                if !suggestions.is_empty() {
                    ui.label(
                        RichText::new("tab complete  ·  ↑↓ pick  ·  enter run")
                            .color(DIM)
                            .small()
                            .monospace(),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(palette_h - 8.0)
                        .show(ui, |ui| {
                            for (i, s) in suggestions.iter().enumerate() {
                                let selected = i == self.palette_i;
                                let row = format!("{:<22}  {}", s.label, s.hint);
                                let color = if selected { ORANGE } else { PURPLE };
                                let resp = ui.selectable_label(
                                    selected,
                                    RichText::new(row).color(color).monospace(),
                                );
                                if resp.clicked() {
                                    self.apply_suggestion(s, true);
                                }
                            }
                        });
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("$")
                            .color(ACCENT)
                            .font(FontId::monospace(14.0)),
                    );
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.draft)
                            .desired_width(ui.available_width() - 90.0)
                            .hint_text("message in this channel, or /join /invite")
                            .font(FontId::monospace(14.0)),
                    );
                    if enter {
                        resp.request_focus();
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("send").color(ACCENT).monospace())
                                .fill(Color32::TRANSPARENT)
                                .stroke(hairline(ACCENT))
                                .min_size(egui::vec2(72.0, 28.0)),
                        )
                        .clicked()
                    {
                        self.send_chat();
                        resp.request_focus();
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(chrome(SHELL))
            .show(ctx, |ui| {
                module_title(ui, "LIVE CHAT", &self.active_channel);
                ui.add_space(6.0);
                let mut switch_to = None;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("channel").color(DIM).monospace());
                    let current = self.active_channel.clone();
                    egui::ComboBox::from_id_salt("chan_pick")
                        .selected_text(RichText::new(&current).color(ACCENT).monospace())
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for ch in &self.joined {
                                if ui
                                    .selectable_label(
                                        ch.eq_ignore_ascii_case(&current),
                                        RichText::new(ch).color(ACCENT).monospace(),
                                    )
                                    .clicked()
                                {
                                    switch_to = Some(ch.clone());
                                }
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chan_new)
                            .desired_width(110.0)
                            .hint_text("new name")
                            .font(FontId::monospace(13.0)),
                    );
                    if ui
                        .add(
                            egui::Button::new(RichText::new("create").color(ORANGE).monospace())
                                .fill(Color32::TRANSPARENT)
                                .stroke(hairline(ORANGE)),
                        )
                        .clicked()
                    {
                        self.create_channel();
                    }
                });
                if let Some(ch) = switch_to {
                    self.switch_channel(ch);
                }
                let (access, crypto) = channel_security(
                    &self.active_channel,
                    self.status.as_ref().map(|s| s.mode),
                    self.status.as_ref().map(|s| s.preset.as_str()).unwrap_or(""),
                    self.status.as_ref().map(|s| s.hub_ok).unwrap_or(false),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&access)
                            .color(if crate::slash::is_bulletin(&self.active_channel) {
                                PURPLE
                            } else {
                                GREEN
                            })
                            .font(FontId::monospace(12.0)),
                    );
                    ui.label(RichText::new("·").color(DIM).monospace());
                    ui.label(
                        RichText::new(&crypto)
                            .color(ORANGE)
                            .font(FontId::monospace(12.0)),
                    );
                });
                ui.label(
                    RichText::new(
                        "Amateur radio cannot hide message text on RF. Closed channels are invite lists; hub traffic is TLS.",
                    )
                    .color(DIM)
                    .font(FontId::monospace(11.0)),
                );
                if !crate::slash::is_bulletin(&self.active_channel) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("invite").color(DIM).monospace());
                        ui.add(
                            egui::TextEdit::singleline(&mut self.invite_draft)
                                .desired_width(120.0)
                                .hint_text("M0ABC")
                                .font(FontId::monospace(13.0)),
                        );
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("send invite").color(ACCENT).monospace(),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(hairline(ACCENT)),
                            )
                            .clicked()
                        {
                            self.invite_to_current();
                        }
                    });
                    let people = self
                        .members
                        .get(&self.active_channel)
                        .cloned()
                        .unwrap_or_default();
                    if !people.is_empty() {
                        ui.label(
                            RichText::new(format!("members {}", people.join(" ")))
                                .color(PURPLE)
                                .font(FontId::monospace(12.0)),
                        );
                    }
                } else {
                    ui.label(
                        RichText::new("anyone on the network can read #bulletin")
                            .color(DIM)
                            .font(FontId::monospace(11.0)),
                    );
                }
                ui.add_space(6.0);
                let active = self.active_channel.clone();
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for line in &self.chat {
                            if !line.channel.is_empty()
                                && !line.channel.eq_ignore_ascii_case(&active)
                            {
                                continue;
                            }
                            if line.sys {
                                ui.label(RichText::new(&line.text).color(PURPLE).monospace());
                            } else {
                                let mine = line.nick.eq_ignore_ascii_case(&self.callsign);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(format!("[{}]", line.nick))
                                            .color(if mine { ORANGE } else { ACCENT })
                                            .monospace(),
                                    );
                                    ui.label(RichText::new(&line.text).color(FG).monospace());
                                });
                            }
                        }
                    });
            });
        self.sync_tray();
        paint_scan(ctx);
    }
}

impl GuiApp {
    fn sync_tray(&mut self) {
        if self.tray.is_none() {
            return;
        }
        let mode = self.status.as_ref().map(|s| s.mode);
        let key = tray_key(mode);
        if self.tray_icon_key == key {
            return;
        }
        self.tray_icon_key = key;
        let (rgba, w, h) = raster_mark(32, wave_rgb(mode), true);
        let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h) else {
            return;
        };
        let tip = match mode {
            Some(m) => format!("WeeChat Radio — {}", m.as_str()),
            None => "WeeChat Radio".into(),
        };
        if let Some(tray) = &self.tray {
            let _ = tray.set_icon(Some(icon));
            let _ = tray.set_tooltip(Some(tip.as_str()));
        }
    }
}

fn kv(ui: &mut egui::Ui, k: &str, v: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{k:7}")).color(DIM).monospace());
        ui.label(RichText::new(v).color(color).monospace());
    });
}

fn mode_pick(ui: &mut egui::Ui, current: Mode) -> Option<Mode> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{:7}", "MODE"))
                .color(DIM)
                .monospace(),
        );
        egui::ComboBox::from_id_salt("mode_pick")
            .selected_text(
                RichText::new(current.as_str())
                    .color(mode_color(current))
                    .monospace(),
            )
            .width(168.0)
            .show_ui(ui, |ui| {
                for m in Mode::all() {
                    if ui
                        .selectable_label(
                            m == current,
                            RichText::new(m.as_str()).color(mode_color(m)).monospace(),
                        )
                        .on_hover_text(m.display_name())
                        .clicked()
                    {
                        chosen = Some(m);
                    }
                }
            });
    });
    chosen
}

fn preset_pick(ui: &mut egui::Ui, current: &str) -> Option<String> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{:7}", "PRESET"))
                .color(DIM)
                .monospace(),
        );
        egui::ComboBox::from_id_salt("preset_pick")
            .selected_text(RichText::new(current).color(PURPLE).monospace())
            .width(168.0)
            .show_ui(ui, |ui| {
                for p in Preset::all() {
                    let value = p.as_str();
                    if ui
                        .selectable_label(
                            value == current,
                            RichText::new(value).color(PURPLE).monospace(),
                        )
                        .on_hover_text(p.description())
                        .clicked()
                    {
                        chosen = Some(value.to_string());
                    }
                }
            });
    });
    chosen
}

fn mode_color(m: Mode) -> Color32 {
    match m {
        Mode::Internet => Color32::from_rgb(0, 229, 255),
        Mode::InternetRadio => GREEN,
        Mode::Radio => Color32::from_rgb(255, 191, 0),
        Mode::RadioPlus => Color32::from_rgb(255, 77, 255),
    }
}

fn tray_key(mode: Option<Mode>) -> i8 {
    match mode {
        None => 0,
        Some(Mode::Internet) => 1,
        Some(Mode::InternetRadio) => 2,
        Some(Mode::Radio) => 3,
        Some(Mode::RadioPlus) => 4,
    }
}

fn wave_rgb(mode: Option<Mode>) -> [u8; 3] {
    let c = mode.map(mode_color).unwrap_or(ACCENT);
    [c.r(), c.g(), c.b()]
}

fn app_icon() -> IconData {
    let (rgba, width, height) = raster_mark(256, [125, 155, 255], true);
    IconData {
        rgba,
        width,
        height,
    }
}

fn dist_seg(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let vx = bx - ax;
    let vy = by - ay;
    let l2 = vx * vx + vy * vy;
    if l2 < 1e-12 {
        return (px - ax).hypot(py - ay);
    }
    let t = ((px - ax) * vx + (py - ay) * vy) / l2;
    let t = t.clamp(0.0, 1.0);
    (px - (ax + t * vx)).hypot(py - (ay + t * vy))
}

fn dist_arc(px: f32, py: f32, cx: f32, cy: f32, radius: f32) -> f32 {
    let dx = px - cx;
    let dy = py - cy;
    let ang = dy.atan2(dx);
    if (-std::f32::consts::FRAC_PI_2..=0.0).contains(&ang) {
        return (dx.hypot(dy) - radius).abs();
    }
    let e0 = (px - cx).hypot(py - (cy - radius));
    let e1 = (px - (cx + radius)).hypot(py - cy);
    e0.min(e1)
}

fn rbox(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, rad: f32) -> f32 {
    let dx = (px - cx).abs() - (hw - rad);
    let dy = (py - cy).abs() - (hh - rad);
    let ox = dx.max(0.0);
    let dy0 = dy.max(0.0);
    ox.hypot(dy0) + dx.max(dy).min(0.0) - rad
}

fn cover(signed: f32, aa: f32) -> f32 {
    (0.5 - signed / aa).clamp(0.0, 1.0)
}

fn over(dst: &mut [u8; 4], r: u8, g: u8, b: u8, a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let inv = 1.0 - a;
    dst[0] = (r as f32 * a + dst[0] as f32 * inv).round() as u8;
    dst[1] = (g as f32 * a + dst[1] as f32 * inv).round() as u8;
    dst[2] = (b as f32 * a + dst[2] as f32 * inv).round() as u8;
    dst[3] = (255.0 * (a + (dst[3] as f32 / 255.0) * inv)).round() as u8;
}

fn raster_mark(size: u32, wave: [u8; 3], framed: bool) -> (Vec<u8>, u32, u32) {
    const CHEVRON: [u8; 3] = [216, 208, 232];
    const TILE: [u8; 3] = [7, 7, 10];
    const BORDER: [u8; 3] = [125, 155, 255];
    let n = size as usize;
    let mut rgba = vec![0u8; n * n * 4];
    let dim = size as f32;
    let pad = if framed { dim * 0.18 } else { dim * 0.08 };
    let inner = dim - 2.0 * pad;
    let scale = (inner / 64.0).min(inner / 60.0);
    let ox = (dim - 64.0 * scale) * 0.5;
    let oy = (dim - 60.0 * scale) * 0.5;
    let aa = 0.65_f32;
    let radius = dim * (26.0 / 120.0);
    let border_w = (dim / 120.0).max(1.0);
    for y in 0..n {
        for x in 0..n {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let mut pix = [0u8; 4];
            if framed {
                let sdf = rbox(
                    px,
                    py,
                    dim * 0.5,
                    dim * 0.5,
                    dim * 0.5 - 0.5,
                    dim * 0.5 - 0.5,
                    radius,
                );
                over(&mut pix, TILE[0], TILE[1], TILE[2], cover(sdf, aa));
                over(
                    &mut pix,
                    BORDER[0],
                    BORDER[1],
                    BORDER[2],
                    cover(sdf.abs() - border_w * 0.5, aa) * 0.22,
                );
            }
            let mx = (px - ox) / scale;
            let my = (py - oy) / scale;
            let aa_v = aa / scale;
            let d_chev = dist_seg(mx, my, 8.0, 20.0, 20.0, 30.0)
                .min(dist_seg(mx, my, 20.0, 30.0, 8.0, 40.0));
            over(
                &mut pix,
                CHEVRON[0],
                CHEVRON[1],
                CHEVRON[2],
                cover(d_chev - 2.5, aa_v),
            );
            let d_bar = rbox(mx, my, 32.5, 38.5, 7.5, 2.5, 1.0);
            over(&mut pix, wave[0], wave[1], wave[2], cover(d_bar, aa_v));
            for (arc_r, op) in [(7.0, 1.0), (13.0, 0.7), (19.0, 0.4)] {
                let d = dist_arc(mx, my, 40.0, 36.0, arc_r);
                over(
                    &mut pix,
                    wave[0],
                    wave[1],
                    wave[2],
                    cover(d - 2.0, aa_v) * op,
                );
            }
            let i = (y * n + x) * 4;
            rgba[i..i + 4].copy_from_slice(&pix);
        }
    }
    (rgba, size, size)
}

#[cfg(windows)]
fn system32(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join(name)
}

#[cfg(windows)]
fn run_hidden(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return;
    };
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
}

fn force_quit() -> ! {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        for name in ["wcr.exe", "modem73.exe"] {
            let _ = Command::new(system32("taskkill.exe"))
                .args(["/F", "/T", "/IM", name])
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
    #[cfg(not(windows))]
    {
        for name in ["wcr", "modem73"] {
            let _ = Command::new("pkill").args(["-x", name]).spawn();
        }
    }
    std::process::exit(0);
}

fn request_show() {
    TRAY_ACTION.store(1, Ordering::SeqCst);
    if let Some(ctx) = GUI_CTX.get() {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

fn start_tray_watch() {
    if TRAY_WATCH.swap(true, Ordering::SeqCst) {
        return;
    }
    MenuEvent::set_event_handler(Some(|ev: MenuEvent| {
        if TRAY_QUIT_ID.get().is_some_and(|id| &ev.id == id) {
            force_quit();
        } else if TRAY_SHOW_ID.get().is_some_and(|id| &ev.id == id) {
            request_show();
        } else if TRAY_STOP_ID.get().is_some_and(|id| &ev.id == id) {
            TRAY_ACTION.store(2, Ordering::SeqCst);
            if let Some(ctx) = GUI_CTX.get() {
                ctx.request_repaint();
            }
        }
    }));
    TrayIconEvent::set_event_handler(Some(|event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            request_show();
        }
    }));
}

fn kill_pid_tree(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new(system32("taskkill.exe"));
        cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        run_hidden(&mut cmd);
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        let _ = Command::new("pkill")
            .args(["-P", &pid.to_string()])
            .status();
    }
}

fn kill_sidecars() {
    #[cfg(windows)]
    {
        for name in ["wcr.exe", "modem73.exe"] {
            let mut cmd = Command::new(system32("taskkill.exe"));
            cmd.args(["/F", "/T", "/IM", name]);
            run_hidden(&mut cmd);
        }
        let our = std::process::id();
        let mut ps = Command::new(system32("WindowsPowerShell\\v1.0\\powershell.exe"));
        ps.args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!(
                "Get-Process -Name wcr,modem73 -ErrorAction SilentlyContinue | Where-Object {{ $_.Id -ne {our} }} | Stop-Process -Force"
            ),
        ]);
        run_hidden(&mut ps);
    }
    #[cfg(not(windows))]
    {
        for name in ["wcr", "modem73"] {
            let _ = Command::new("pkill").args(["-x", name]).status();
        }
    }
}

fn spawn_grid_detect() -> Receiver<Option<crate::grid::DetectedGrid>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(crate::grid::detect_from_ip());
    });
    rx
}

impl GuiApp {
    fn poll_grid_detect(&mut self) {
        let result = match &self.grid_rx {
            Some(rx) => match rx.try_recv() {
                Ok(v) => Some(Ok(v)),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(())),
            },
            None => return,
        };
        let Some(result) = result else {
            return;
        };
        self.grid_rx = None;
        match result {
            Ok(Some(hit)) => {
                let note = if hit.label.is_empty() {
                    format!("from IP · {}", hit.grid)
                } else {
                    format!("from IP · {}", hit.label)
                };
                if self.grid.is_empty() || self.grid_overwrite {
                    self.grid = hit.grid;
                    self.grid_note = note;
                } else {
                    self.grid_note = format!("detected {note} (kept your value)");
                }
            }
            Ok(None) | Err(()) => {
                if self.grid.is_empty() {
                    self.grid_note = "could not detect — enter your Maidenhead square".into();
                }
            }
        }
        self.grid_overwrite = false;
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
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
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
                let line = if irc_command(&msg) {
                    format!("{}\r\n", msg.trim_end())
                } else {
                    format!("PRIVMSG #bulletin :{msg}\r\n")
                };
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
                    if let Some((from, ch)) = parse_invite(&line) {
                        let _ = events.send(IrcEvent::Invited { from, channel: ch });
                    } else if let Some(ch) = parse_join(&line) {
                        let _ = events.send(IrcEvent::Joined(ch));
                    } else if let Some(chat) = parse_privmsg(&line) {
                        if !chat.nick.eq_ignore_ascii_case(&nick) {
                            let _ = events.send(IrcEvent::Line(chat));
                        }
                    } else if let Some(notice) = parse_notice(&line) {
                        let _ = events.send(IrcEvent::Status(notice));
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

fn irc_payload(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix('@') {
        rest.split_once(' ').map(|(_, c)| c).unwrap_or(rest)
    } else {
        line
    }
}

fn irc_command(msg: &str) -> bool {
    let head = msg
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        head.as_str(),
        "PRIVMSG" | "JOIN" | "PART" | "INVITE" | "RADIO" | "PONG" | "NICK" | "USER" | "QUIT"
    )
}

fn parse_notice(line: &str) -> Option<String> {
    let rest = irc_payload(line).strip_prefix(':')?;
    let (_, cmd) = rest.split_once(' ')?;
    if !cmd.starts_with("NOTICE ") {
        return None;
    }
    Some(cmd.split_once(" :")?.1.to_string())
}

fn parse_privmsg(line: &str) -> Option<ChatLine> {
    let rest = irc_payload(line).strip_prefix(':')?;
    let (prefix, cmd) = rest.split_once(' ')?;
    if !cmd.starts_with("PRIVMSG ") {
        return None;
    }
    let nick = prefix.split('!').next().unwrap_or(prefix).to_string();
    let (target, text) = cmd.strip_prefix("PRIVMSG ")?.split_once(" :")?;
    Some(ChatLine {
        nick,
        text: text.to_string(),
        sys: false,
        channel: target.to_string(),
    })
}

fn parse_join(line: &str) -> Option<String> {
    let rest = irc_payload(line).strip_prefix(':')?;
    let (_, cmd) = rest.split_once(' ')?;
    let ch = cmd.strip_prefix("JOIN ")?;
    Some(ch.split_whitespace().next()?.to_string())
}

fn parse_invite(line: &str) -> Option<(String, String)> {
    let rest = irc_payload(line).strip_prefix(':')?;
    let (prefix, cmd) = rest.split_once(' ')?;
    let rest = cmd.strip_prefix("INVITE ")?;
    let mut bits = rest.split_whitespace();
    bits.next()?;
    let ch = bits.next()?.to_string();
    let from = prefix.split('!').next().unwrap_or(prefix).to_string();
    Some((from, ch))
}

fn channel_security(
    channel: &str,
    mode: Option<Mode>,
    preset: &str,
    hub_ok: bool,
) -> (String, String) {
    let access = if crate::slash::is_bulletin(channel) {
        "PUBLIC".to_string()
    } else {
        "CLOSED · invite only".to_string()
    };
    let signed = mode.map(|m| m.uses_internet()).unwrap_or(false);
    let unsigned_hf = matches!(preset, "hf-poor" | "hf-weak" | "hf-deep");
    let mut bits = Vec::new();
    bits.push(if signed {
        "identity SIGNED"
    } else {
        "identity UNSIGNED"
    });
    bits.push(if hub_ok { "hub TLS" } else { "hub OFF" });
    bits.push(if unsigned_hf {
        "RF plaintext (unsigned HF)"
    } else {
        "RF plaintext"
    });
    (access, bits.join(" · "))
}
