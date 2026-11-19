//! SPDX-License-Identifier: Apache-2.0
//! The modem73-style desktop layout, picked with `[ui] layout = "modem73"`,
//! `wcr-gui --ui-layout modem73` or `WCR_UI_LAYOUT=modem73`. Classic stays
//! the default and lives untouched in `gui.rs`.
//!
//! Top to bottom: header, tabs, instrument strip (STATION, SIGNAL,
//! ACTIVITY; hidden on setup), work area, key-hint footer. Every action is
//! the classic one; only where it sits and how it looks changes.

use super::*;
use crate::activity_bars::{self, Cell, Sample, Tint};

static LAYOUT_OVERRIDE: OnceLock<String> = OnceLock::new();

/// The `--ui-layout` flag: wins over `WCR_UI_LAYOUT` and `[ui] layout`.
pub fn set_layout_override(name: &str) {
    let _ = LAYOUT_OVERRIDE.set(name.trim().to_string());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Layout {
    Classic,
    Modem73,
}

impl Layout {
    fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "classic" => Some(Self::Classic),
            "modem73" => Some(Self::Modem73),
            _ => None,
        }
    }

    /// Flag, then environment, then wcr.toml; anything unknown is modem73
    /// (the default since v1.0.18-rc8).
    fn resolve(flag: Option<&str>, env: Option<&str>, config: &str) -> Self {
        [flag, env, Some(config)]
            .into_iter()
            .flatten()
            .find_map(Self::parse)
            .unwrap_or(Self::Modem73)
    }

    pub(super) fn chosen() -> Self {
        let config = Config::load(&Config::active_path())
            .map(|c| c.ui.layout)
            .unwrap_or_default();
        Self::resolve(
            LAYOUT_OVERRIDE.get().map(String::as_str),
            std::env::var("WCR_UI_LAYOUT").ok().as_deref(),
            &config,
        )
    }
}

// Palette (modem73's colour pairs).
const M_BG: Color32 = Color32::from_rgb(0x10, 0x00, 0x08);
const M_FIELD: Color32 = Color32::from_rgb(0x0a, 0x00, 0x05);
const M_FG: Color32 = Color32::from_rgb(0xe8, 0xe0, 0xe4);
const M_DIM: Color32 = Color32::from_rgb(0x6e, 0x66, 0x6a);
const M_MUTE: Color32 = Color32::from_rgb(0x8a, 0x82, 0x86);
const M_LINE: Color32 = Color32::from_rgb(0x3a, 0x32, 0x36);
/// The header's `///`: the website's --accent, --orange and --purple.
const LOGO_SLASHES: [Color32; 3] = [
    Color32::from_rgb(0x7d, 0x9b, 0xff),
    Color32::from_rgb(0xff, 0x7a, 0x3d),
    Color32::from_rgb(0xc4, 0xb5, 0xfd),
];
const M_GREEN: Color32 = Color32::from_rgb(0x18, 0xfc, 0x7c);
const M_RED: Color32 = Color32::from_rgb(0xff, 0x4d, 0x4d);
const M_GOLD: Color32 = Color32::from_rgb(0xe6, 0xb8, 0x00);
const M_CYAN: Color32 = Color32::from_rgb(0x00, 0xd0, 0xd0);
const M_MAGENTA: Color32 = Color32::from_rgb(0xb8, 0x4c, 0xc4);
const M_PLUS: Color32 = Color32::from_rgb(0xff, 0x4d, 0xff);
/// Other stations' callsigns in the chat log.
const M_PEER: Color32 = Color32::from_rgb(0xc8, 0xb4, 0xf0);

const STRIP_H: f32 = 214.0;
const DELIVERY_W: f32 = 300.0;
/// Fewest cells a SIGNAL meter shows, on a narrow window.
const METER_W: usize = 18;
const PAD: f32 = 12.0;

fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

fn text(t: impl Into<String>, color: Color32) -> RichText {
    RichText::new(t).color(color).font(mono(13.0))
}

/// Section label: dim, small, letter-spaced (`STATION`, `DELIVERY`).
fn label(t: &str) -> RichText {
    RichText::new(t)
        .color(M_DIM)
        .font(mono(11.0))
        .extra_letter_spacing(1.5)
}

fn flat_btn(t: &str, color: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(t.to_string()).color(color).font(mono(12.0)))
        .fill(Color32::TRANSPARENT)
        .stroke(hairline(M_LINE))
        .rounding(0.0)
}

/// A hairline across the whole width of `ui`.
fn rule(ui: &mut egui::Ui) {
    let r = ui.max_rect();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(r.width(), 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [
            egui::pos2(r.left(), rect.center().y),
            egui::pos2(r.right(), rect.center().y),
        ],
        hairline(M_LINE),
    );
}

pub(super) fn apply_visuals(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = M_BG;
    v.window_fill = M_BG;
    v.extreme_bg_color = M_FIELD;
    v.faint_bg_color = M_BG;
    v.override_text_color = Some(M_FG);
    v.hyperlink_color = M_CYAN;
    v.selection.bg_fill = Color32::from_rgb(0x2a, 0x10, 0x30);
    v.selection.stroke = hairline(M_CYAN);
    v.window_stroke = hairline(M_LINE);
    v.window_rounding = egui::Rounding::ZERO;
    v.menu_rounding = egui::Rounding::ZERO;
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    for (w, stroke) in [
        (&mut v.widgets.noninteractive, M_LINE),
        (&mut v.widgets.inactive, M_LINE),
        (&mut v.widgets.hovered, M_MUTE),
        (&mut v.widgets.active, M_CYAN),
        (&mut v.widgets.open, M_LINE),
    ] {
        w.bg_fill = M_FIELD;
        w.weak_bg_fill = M_FIELD;
        w.bg_stroke = hairline(stroke);
        w.rounding = egui::Rounding::ZERO;
    }
    v.widgets.noninteractive.fg_stroke = hairline(M_FG);
    v.widgets.inactive.fg_stroke = hairline(M_FG);
    v.widgets.hovered.fg_stroke = hairline(M_FG);
    v.widgets.active.fg_stroke = hairline(M_CYAN);
    let mut style = (*ctx.style()).clone();
    style.visuals = v;
    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
    macos_force_dark_appearance();
}

/// `[` fill `]`: how many of `width` cells a 0..1 level fills.
fn meter_fill(frac: f32, width: usize) -> usize {
    ((frac.clamp(0.0, 1.0) * width as f32).round() as usize).min(width)
}

/// Characters kept free right of a SIGNAL meter for its reading: the
/// widest ("-15.0 dB", "-28 good") plus a gap before it. Readings are
/// right-aligned, so the column's right padding matches its left.
const METER_TAIL_CHARS: f32 = 10.0;

/// How many cells a meter gets to fill the rest of its row: the width
/// left after the key, less the reading on the right, in monospace
/// cells. Never fewer than `METER_W`.
fn meter_cells(ui: &egui::Ui) -> usize {
    let cell = ui.fonts(|f| f.glyph_width(&mono(13.0), '-')).max(1.0);
    meter_cells_for(ui.available_width(), cell)
}

fn meter_cells_for(avail: f32, cell: f32) -> usize {
    // Two cells for the `[` and `]`.
    let n = (avail / cell - METER_TAIL_CHARS - 2.0).floor();
    if n.is_finite() && n > METER_W as f32 {
        n as usize
    } else {
        METER_W
    }
}

/// A meter's reading, flush with the right edge of the SIGNAL column.
/// Widgets added in `add` run right to left, so add the rightmost first.
fn reading<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
        .inner
}

/// An ASCII meter `[====------]` of `width` cells, the fill in `color`.
fn ascii_meter(frac: f32, width: usize, color: Color32) -> egui::text::LayoutJob {
    let n = meter_fill(frac, width);
    let mut job = egui::text::LayoutJob::default();
    let fmt = |c: Color32| egui::TextFormat::simple(mono(13.0), c);
    job.append("[", 0.0, fmt(M_MUTE));
    job.append(&"=".repeat(n), 0.0, fmt(color));
    job.append(&"-".repeat(width - n), 0.0, fmt(M_MUTE));
    job.append("]", 0.0, fmt(M_MUTE));
    job
}

/// A key in a fixed-width, left-aligned column.
fn key_cell(ui: &mut egui::Ui, k: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(64.0, 18.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(64.0);
            ui.label(text(k, M_DIM).size(12.0));
        },
    );
}

/// A STATION value, its text lined up with the text inside the MODE,
/// FREQ and PRESET pickers.
fn station_text(ui: &mut egui::Ui, v: RichText) {
    ui.add_space(ui.spacing().button_padding.x + 1.0);
    ui.add(egui::Label::new(v).truncate());
}

/// One STATION key and value: the key in the fixed column, the value lined
/// up with the pickers' text.
fn station_kv(ui: &mut egui::Ui, k: &str, v: RichText) {
    ui.horizontal(|ui| {
        key_cell(ui, k);
        station_text(ui, v);
    });
}

/// One key and value, the key in a fixed column.
fn kv(ui: &mut egui::Ui, k: &str, v: RichText) -> egui::Response {
    ui.horizontal(|ui| {
        key_cell(ui, k);
        ui.add(egui::Label::new(v).truncate());
    })
    .response
}

/// Delivery marks in modem73 colours: green delivered, red refused or
/// still out, gold heard by radio.
fn mark_color(mark: &str) -> Color32 {
    if mark.contains("[ok]") || mark.contains("[all]") {
        M_GREEN
    } else if mark.contains("[no]") || mark.contains("[tx]") {
        M_RED
    } else if mark.contains("[rf]") {
        M_GOLD
    } else if mark.contains("[net]") {
        M_CYAN
    } else {
        M_MUTE
    }
}

fn word_tone(word: &str) -> Color32 {
    match word {
        "good" => M_GREEN,
        "high" | "noisy" => M_GOLD,
        "clipping" => M_RED,
        _ => M_MUTE,
    }
}

fn tint_color(t: Tint) -> Color32 {
    match t {
        Tint::Tone => M_RED,
        Tint::Dcd => M_MAGENTA,
        Tint::Above => M_CYAN,
        Tint::Quiet => M_GREEN,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OnAirFilter {
    #[default]
    All,
    Messages,
    Receipts,
    Beacons,
    Mail,
}

impl OnAirFilter {
    const ALL: [Self; 5] = [
        Self::All,
        Self::Messages,
        Self::Receipts,
        Self::Beacons,
        Self::Mail,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Messages => "Messages",
            Self::Receipts => "Receipts",
            Self::Beacons => "Beacons",
            Self::Mail => "Mail",
        }
    }

    fn keeps(self, kind: &str) -> bool {
        match self {
            Self::All => true,
            Self::Messages => matches!(kind, "dm" | "bulletin" | "group"),
            Self::Receipts => kind == "receipt",
            Self::Beacons => kind == "beacon",
            Self::Mail => kind == "mail",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Chat,
    Email,
    OnAir,
    Setup,
}

/// State only this layout keeps.
#[derive(Default)]
pub(super) struct State {
    activity: activity_bars::History,
    next_sample: Option<Instant>,
    last_tone: Option<Instant>,
    on_air: OnAirFilter,
    /// Height of the STATS block under ACTIVITY last frame, so the BARS
    /// graph can take the rest of the strip.
    stats_h: f32,
    /// Widest STATS row that cannot wrap, last frame: room for the MAIL GW key.
    stats_row_w: f32,
    /// Q was pressed: the "Quit?" box is showing.
    quit_ask: bool,
}

impl State {
    /// One ACTIVITY sample per 100 ms: the level that drives the IN meter,
    /// red ("tone") while this station keys and for 450 ms after.
    fn tick(&mut self, now: Instant, s: Option<&StatusSnapshot>) {
        if s.is_some_and(|s| s.ptt_on) {
            self.last_tone = Some(now);
        }
        let step = Duration::from_millis(activity_bars::SAMPLE_MS);
        let next = self.next_sample.get_or_insert(now);
        if now.saturating_duration_since(*next) > step * activity_bars::HISTORY as u32 {
            *next = now;
        }
        while *next <= now {
            let tone = self.last_tone.is_some_and(|t| {
                now.saturating_duration_since(t)
                    <= Duration::from_millis(activity_bars::TONE_HOLD_MS)
            });
            let level_db = match s {
                Some(s) if crate::presets::audio_meter_live(&s.audio_label) => {
                    // While keyed, the burst shows the level going out.
                    if tone {
                        s.audio_in_db.max(s.audio_out_db)
                    } else {
                        s.audio_in_db
                    }
                }
                _ => activity_bars::EMPTY_DB,
            };
            self.activity.push(Sample {
                level_db,
                // No reliable DCD bit from the station yet: never magenta.
                dcd: false,
                tone,
            });
            *next += step;
        }
    }
}

impl GuiApp {
    pub(super) fn update_modem73(
        &mut self,
        ctx: &egui::Context,
        suggestions: &[crate::slash::Suggestion],
        enter: bool,
    ) {
        ctx.request_repaint_after(Duration::from_millis(activity_bars::SAMPLE_MS));
        self.m73.tick(Instant::now(), self.status.as_ref());
        self.m73_keys(ctx);
        self.m73_header(ctx);
        self.m73_footer(ctx);
        self.m73_quit_ask(ctx);
        self.software.window(ctx);
        if !self.configured() || self.show_setup {
            self.paint_setup(ctx, M_BG);
            self.sync_tray();
            return;
        }
        self.m73_strip(ctx);
        self.paint_mail_send(ctx);
        self.paint_mail_get(ctx);
        self.paint_mail_new(ctx);
        self.paint_confirm_radio(ctx);
        match self.center {
            CenterView::Email => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(M_BG).inner_margin(PAD))
                    .show(ctx, |ui| self.paint_email(ui));
            }
            CenterView::Traffic => self.m73_on_air(ctx),
            CenterView::Chat => self.m73_chat(ctx, suggestions, enter),
        }
        self.sync_tray();
    }

    fn m73_tab(&self) -> Tab {
        if !self.configured() || self.show_setup {
            return Tab::Setup;
        }
        match self.center {
            CenterView::Chat => Tab::Chat,
            CenterView::Email => Tab::Email,
            CenterView::Traffic => Tab::OnAir,
        }
    }

    /// Why Email cannot open now (classic dims the link the same way).
    fn m73_email_block(&self) -> Option<String> {
        if self.callsign.trim().starts_with('~') {
            return Some("Email needs a licensed callsign.".into());
        }
        let (mode, hub_ok, running) = self
            .status
            .as_ref()
            .map(|s| (s.mode, s.hub_ok, true))
            .unwrap_or((Mode::InternetRadio, false, false));
        crate::mail::email_block_reason(
            &self.callsign,
            mode,
            &self.mail_gateway_choice(),
            hub_ok,
            running,
        )
        .map(|r| r.to_string())
    }

    /// The same moves as classic's nav links.
    fn m73_go(&mut self, tab: Tab) {
        match tab {
            Tab::Chat => {
                self.center = CenterView::Chat;
                self.leave_setup();
            }
            Tab::Email => {
                if self.m73_email_block().is_none() {
                    self.center = CenterView::Email;
                    self.leave_setup();
                }
            }
            Tab::OnAir => {
                self.center = CenterView::Traffic;
                self.last_traffic_poll = Instant::now() - Duration::from_secs(5);
                self.leave_setup();
            }
            Tab::Setup => {
                if self.configured() {
                    self.error.clear();
                    if self.show_setup {
                        self.leave_setup();
                    } else {
                        self.show_setup = true;
                        self.hw = None;
                        self.hw_rx = None;
                    }
                }
            }
        }
    }

    fn m73_start_stop(&mut self) {
        if self.status.is_some() {
            self.stop_station();
        } else {
            self.start_station();
        }
    }

    fn m73_weechat(&mut self) {
        if let Err(e) = crate::weechat_app::run(false, &Config::active_path()) {
            self.error = e.to_string();
        }
    }

    /// Footer keys. F1 and F2 always work; the letters and Tab only while
    /// no text box has the keyboard, so typing a message never fires them.
    fn m73_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        let none = egui::Modifiers::NONE;
        let (mut f1, mut f2, mut tab, mut s, mut m, mut w, mut q) =
            (false, false, false, false, false, false, false);
        ctx.input_mut(|i| {
            f1 = i.consume_key(none, egui::Key::F1);
            f2 = i.consume_key(none, egui::Key::F2);
            if !typing {
                tab = i.consume_key(none, egui::Key::Tab);
                s = i.consume_key(none, egui::Key::S);
                m = i.consume_key(none, egui::Key::M);
                w = i.consume_key(none, egui::Key::W);
                q = i.consume_key(none, egui::Key::Q);
            }
        });
        if f1 {
            self.help_open = !self.help_open;
        }
        if f2 {
            self.show_key = !self.show_key;
        }
        if tab && self.configured() {
            let blocked = self.m73_email_block().is_some();
            let next = match self.m73_tab() {
                Tab::Chat if blocked => Tab::OnAir,
                Tab::Chat => Tab::Email,
                Tab::Email => Tab::OnAir,
                Tab::OnAir => Tab::Setup,
                Tab::Setup => Tab::Chat,
            };
            if next == Tab::Chat && self.show_setup {
                self.center = CenterView::Chat;
                self.leave_setup();
            } else {
                self.m73_go(next);
            }
        }
        if s {
            self.m73_start_stop();
        }
        if m {
            let _ = open::that(crate::config::PUBLIC_MAP);
        }
        if w {
            self.m73_weechat();
        }
        if q {
            self.m73.quit_ask = true;
        }
    }

    /// The "Quit?" box, centred on the window. Enter or Y quits, Esc or N
    /// keeps the station running.
    fn m73_quit_ask(&mut self, ctx: &egui::Context) {
        if !self.m73.quit_ask {
            return;
        }
        let (mut yes, mut no) = (false, false);
        ctx.input_mut(|i| {
            let none = egui::Modifiers::NONE;
            yes = i.consume_key(none, egui::Key::Enter) || i.consume_key(none, egui::Key::Y);
            no = i.consume_key(none, egui::Key::Escape) || i.consume_key(none, egui::Key::N);
        });
        egui::Window::new(text("QUIT", M_DIM).size(12.0))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::none()
                    .fill(M_BG)
                    .stroke(hairline(M_LINE))
                    .inner_margin(PAD),
            )
            .show(ctx, |ui| {
                ui.label(text("Are you sure you want to quit?", M_FG));
                let station = if self.status.is_some() {
                    "This also stops the station."
                } else {
                    "The station is not running."
                };
                ui.label(text(station, M_DIM).size(12.0));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(flat_btn("Quit", M_RED)).clicked() {
                        yes = true;
                    }
                    if ui.add(flat_btn("Cancel", M_FG)).clicked() {
                        no = true;
                    }
                    ui.label(text("Enter quits · Esc cancels", M_DIM).size(11.0));
                });
            });
        if yes {
            self.m73.quit_ask = false;
            self.quit_app(ctx);
        } else if no {
            self.m73.quit_ask = false;
        }
    }

    fn m73_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("m73_header")
            .frame(egui::Frame::none().fill(M_BG))
            .show_separator_line(true)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(PAD, 6.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| self.m73_header_row(ui));
                    });
                rule(ui);
                self.m73_tabs_row(ui);
            });
    }

    fn m73_header_row(&mut self, ui: &mut egui::Ui) {
        let sep = |ui: &mut egui::Ui| {
            ui.label(text("│", M_LINE));
        };
        // Three slashes in weechat-radio's own colours (accent, orange,
        // purple, as on the website), not modem73's purple logo.
        let mut logo = egui::text::LayoutJob::default();
        for c in LOGO_SLASHES {
            let mut f = egui::TextFormat::simple(mono(14.0), c);
            f.extra_letter_spacing = 3.0;
            logo.append("/", 0.0, f);
        }
        ui.label(logo);
        ui.add_space(4.0);
        ui.label(
            RichText::new("WEECHAT RADIO")
                .color(M_FG)
                .font(mono(13.0))
                .strong()
                .extra_letter_spacing(1.0),
        );
        let version = ui
            .add(egui::Label::new(text(version_label(), M_DIM)).sense(egui::Sense::click()))
            .on_hover_text("Software: updates and restarts");
        if version.clicked() {
            self.software.open = true;
        }
        sep(ui);
        let tx = self.status.as_ref().is_some_and(|s| s.ptt_on);
        ui.label(
            RichText::new(if tx { "TX" } else { "RX" })
                .color(if tx { M_RED } else { M_GREEN })
                .font(mono(13.0))
                .strong()
                .extra_letter_spacing(1.0),
        );
        sep(ui);
        if let Some(s) = &self.status {
            ui.label(text(&s.callsign, M_FG).strong());
            ui.label(text(s.mode.as_str(), M_PLUS));
            let khz = self.freq_pending.unwrap_or(s.freq_khz);
            if khz > 0 {
                ui.label(text(
                    format!("{} {}", crate::band::fmt_mhz(khz), s.band),
                    M_MUTE,
                ));
            }
        } else {
            ui.label(text(self.callsign.trim(), M_FG).strong());
            ui.label(text("station stopped", M_GOLD));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(text(chrono::Local::now().format("LCL %H:%M").to_string(), M_DIM).size(11.0));
            ui.label(text(chrono::Utc::now().format("%H:%M:%S").to_string(), M_GOLD).strong())
                .on_hover_text("UTC");
            ui.add_space(8.0);
            self.software.top_link(ui);
        });
    }

    fn m73_tabs_row(&mut self, ui: &mut egui::Ui) {
        let current = self.m73_tab();
        let unread = self.direct_unread_total();
        let email_block = self.m73_email_block();
        let chat = if unread > 0 {
            format!("LIVE CHAT {unread}")
        } else {
            "LIVE CHAT".into()
        };
        let email = if self.mail_unread > 0 {
            format!("EMAIL {}", self.mail_unread)
        } else {
            "EMAIL".into()
        };
        let tabs = [
            (Tab::Chat, chat),
            (Tab::Email, email),
            (Tab::OnAir, "ON AIR".to_string()),
            (Tab::Setup, "SETUP".to_string()),
        ];
        let full = ui.max_rect();
        let cell_w = full.width() / tabs.len() as f32;
        let (row, _) = ui.allocate_exact_size(egui::vec2(full.width(), 26.0), egui::Sense::hover());
        let mut go = None;
        for (i, (tab, name)) in tabs.into_iter().enumerate() {
            let rect = egui::Rect::from_min_size(
                egui::pos2(full.left() + i as f32 * cell_w, row.top()),
                egui::vec2(cell_w, row.height()),
            );
            let on = tab == current;
            let blocked = tab == Tab::Email && email_block.is_some();
            let hidden = tab == Tab::Setup && !self.configured() && !on;
            if hidden {
                continue;
            }
            let resp = ui.interact(rect, ui.id().with(("m73_tab", i)), egui::Sense::click());
            let color = if on {
                M_FG
            } else if blocked {
                M_LINE
            } else if resp.hovered() {
                M_MUTE
            } else {
                M_DIM
            };
            let shown = if on { format!("> {name}") } else { name };
            let galley = ui.painter().layout_job({
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    &shown,
                    0.0,
                    egui::TextFormat {
                        font_id: mono(12.0),
                        color,
                        extra_letter_spacing: 2.0,
                        ..Default::default()
                    },
                );
                job
            });
            let pos = egui::pos2(rect.left() + PAD, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(pos, galley.clone(), color);
            if on {
                // Bold: the same text again, one pixel over.
                ui.painter()
                    .galley(pos + egui::vec2(0.6, 0.0), galley, color);
            }
            if let (true, Some(reason)) = (blocked, &email_block) {
                hover_tip(&resp, reason.clone());
            } else if resp.clicked() {
                go = Some(tab);
            }
        }
        if let Some(tab) = go {
            self.m73_go(tab);
        }
    }

    fn m73_footer(&mut self, ctx: &egui::Context) {
        let running = self.status.is_some();
        let configured = self.configured();
        egui::TopBottomPanel::bottom("m73_footer")
            .frame(
                egui::Frame::none()
                    .fill(M_BG)
                    .inner_margin(egui::Margin::symmetric(PAD, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let hint = |ui: &mut egui::Ui, key: &str, what: &str, tip: &str| -> bool {
                        let r = ui
                            .horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                ui.label(text(key, M_FG).size(12.0).strong());
                                ui.label(text(what, M_DIM).size(12.0));
                            })
                            .response;
                        ui.add_space(10.0);
                        ui.interact(
                            r.rect,
                            ui.id().with(("m73_hint", key)),
                            egui::Sense::click(),
                        )
                        .on_hover_text(tip)
                        .clicked()
                    };
                    if configured
                        && hint(
                            ui,
                            "Tab",
                            "switch",
                            "Next tab (when no text box is selected)",
                        )
                    {
                        self.m73_go(match self.m73_tab() {
                            Tab::Chat => Tab::Email,
                            Tab::Email => Tab::OnAir,
                            Tab::OnAir => Tab::Setup,
                            Tab::Setup => Tab::Chat,
                        });
                    }
                    if hint(ui, "F1", "help", "Keys and commands") {
                        self.help_open = !self.help_open;
                    }
                    if hint(ui, "F2", "key", "What the marks after each line mean") {
                        self.show_key = !self.show_key;
                    }
                    let (word, tip) = if running {
                        ("stop", "Stop the station")
                    } else {
                        ("start", "Start the station")
                    };
                    if hint(ui, "S", word, tip) {
                        self.m73_start_stop();
                    }
                    if hint(ui, "M", "map", "Open the public map in the browser") {
                        let _ = open::that(crate::config::PUBLIC_MAP);
                    }
                    if hint(ui, "W", "weechat", "Open WeeChat on this station") {
                        self.m73_weechat();
                    }
                    if hint(ui, "Q", "quit", "Stop the station and quit") {
                        self.m73.quit_ask = true;
                    }
                });
            });
    }

    fn m73_strip(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("m73_strip")
            .exact_height(STRIP_H)
            .frame(egui::Frame::none().fill(M_BG))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                if self.help_open {
                    let inner = full.shrink2(egui::vec2(PAD + 2.0, 10.0));
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| {
                        ui.label(label("HELP · F1 closes"));
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical()
                            .id_salt("m73_help")
                            .auto_shrink([false, false])
                            .show(ui, cheat_sheet);
                    });
                    return;
                }
                let w = full.width() / 3.0;
                for i in 1..3 {
                    let x = full.left() + w * i as f32;
                    ui.painter().line_segment(
                        [egui::pos2(x, full.top()), egui::pos2(x, full.bottom())],
                        hairline(M_LINE),
                    );
                }
                let col = |i: usize| {
                    egui::Rect::from_min_max(
                        egui::pos2(full.left() + w * i as f32, full.top()),
                        egui::pos2(full.left() + w * (i + 1) as f32, full.bottom()),
                    )
                    .shrink2(egui::vec2(PAD + 2.0, 8.0))
                };
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(col(0)), |ui| {
                    ui.set_clip_rect(col(0));
                    self.m73_station(ui);
                });
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(col(1)), |ui| {
                    ui.set_clip_rect(col(1));
                    self.m73_signal(ui);
                });
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(col(2)), |ui| {
                    ui.set_clip_rect(col(2));
                    self.m73_activity(ui);
                });
            });
    }

    /// STATION: who and where, with classic's MODE, FREQ and PRESET pickers.
    fn m73_station(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(label("STATION"));
        ui.add_space(6.0);
        let Some(s) = self.status.clone() else {
            ui.label(text("node offline", M_GOLD));
            ui.label(text("S start · Q quit", M_DIM).size(12.0));
            return;
        };
        let mut mode_cmd = None;
        let mut preset_cmd = None;
        let mut freq_cmd = None;
        let mut tower_cmd = None;
        let display_khz = self.freq_pending.unwrap_or(s.freq_khz);
        // One column: keys in the same left-aligned column as SIGNAL, the
        // pickers as wide as the panel so their text is not cut off.
        ui.horizontal(|ui| {
            key_cell(ui, "CALL");
            station_text(ui, text(&s.callsign, M_FG).strong());
            ui.add_space(16.0);
            ui.label(text("GRID", M_DIM).size(12.0));
            ui.label(text(&s.grid, M_FG));
        });
        if let Some(next) = mode_pick(ui, s.mode, key_cell) {
            if next == Mode::Radio && s.mode != Mode::Radio {
                self.confirm_radio = true;
            } else if next != s.mode {
                mode_cmd = Some(format!("/mode {}", next.as_str()));
            }
        }
        if let Some(c) = freq_pick(
            ui,
            display_khz,
            &s.freq_source,
            self.freq_pick_last,
            key_cell,
        ) {
            self.freq_pick_last = Some(c);
            if c.khz != display_khz {
                self.freq_pending = Some(c.khz);
                freq_cmd = Some(format!("/qsy {}", crate::band::fmt_mhz(c.khz)));
            }
        }
        if !s.tnc.is_empty() {
            station_kv(ui, "PRESET", text(&s.preset, M_FG));
        } else if let Some(next) = preset_pick(ui, &s.preset, key_cell) {
            if next != s.preset {
                preset_cmd = Some(format!("/preset {next}"));
            }
        }
        if let Some(note) = s.preset_pending_note() {
            ui.label(RichText::new(note).color(M_GOLD).small());
        }
        if s.mode == Mode::InternetRadio {
            if let Some(on) = tower_toggle(ui, s.tower, key_cell) {
                tower_cmd = Some(format!("/tower {}", if on { "on" } else { "off" }));
            }
        }
        let (ptt, c) = if s.ptt_on {
            ("TX".to_string(), M_RED)
        } else if s.deferred {
            ("WAIT".to_string(), M_GOLD)
        } else {
            (s.channel.to_uppercase(), M_FG)
        };
        station_kv(ui, "PTT", text(ptt, c).strong());
        if !s.builtin_tnc() && !s.modem73.is_empty() {
            station_kv(ui, "MODEM", text(format!("modem73 v{}", s.modem73), M_MUTE));
        }
        if !s.tnc.is_empty() {
            station_kv(
                ui,
                "RADIO",
                text(&s.tnc, if s.tnc_ok { M_FG } else { M_GOLD }),
            );
        }
        if !s.gps.is_empty() {
            let c = match crate::gps::light(&s.gps) {
                crate::gps::Light::Green => M_GREEN,
                crate::gps::Light::Amber => M_GOLD,
                crate::gps::Light::Red => M_RED,
            };
            station_kv(
                ui,
                "GPS",
                text(format!("● {}", s.gps.trim_start_matches("GPS: ")), c),
            );
        }
        m73_heard(ui, &s.heard);
        for cmd in [mode_cmd, tower_cmd, preset_cmd, freq_cmd]
            .into_iter()
            .flatten()
        {
            self.send_cmd(&cmd);
        }
    }

    /// SIGNAL: IN and OUT as ASCII meters, RX GAIN and TX LVL, SNR, OCC,
    /// and RETRY / AIR / HOLD.
    fn m73_signal(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(label("SIGNAL"));
        ui.add_space(6.0);
        let Some(s) = self.status.clone() else {
            return;
        };
        let live = crate::presets::audio_meter_live(&s.audio_label);
        let frac = |db: f32| {
            if live && !crate::presets::audio_level_idle(db) {
                crate::presets::audio_level_frac(db)
            } else {
                0.0
            }
        };
        let db_text = |db: f32| {
            if live && !crate::presets::audio_level_idle(db) {
                format!("{db:.0}")
            } else {
                "—".into()
            }
        };
        let r = ui
            .horizontal(|ui| {
                key_cell(ui, "IN");
                let cells = meter_cells(ui);
                ui.label(ascii_meter(frac(s.audio_in_db), cells, M_GREEN));
                reading(ui, |ui| {
                    // Short word (good / low / hot) so the reading fits the
                    // tail; the hover tip has the detail.
                    if live && !s.audio_in_word.is_empty() {
                        let word = crate::status::label_for(&s.audio_in_word, s.audio_in_db);
                        ui.label(text(word, word_tone(&s.audio_in_word)));
                    }
                    ui.label(text(db_text(s.audio_in_db), M_MUTE));
                });
            })
            .response;
        let mut tip = format!("Receive audio from the radio: {:.0} dBFS", s.audio_in_db);
        if let Some(p) = s.audio_in_peak_db {
            tip.push_str(&format!(", peak {p:.0}"));
        }
        if let Some(c) = s.audio_in_clip_pct {
            tip.push_str(&format!(", {c:.1}% clipped"));
        }
        if !s.audio_in_hint.is_empty() {
            tip.push_str("\n\n");
            tip.push_str(&s.audio_in_hint);
        }
        hover_tip(
            &ui.interact(r.rect, ui.id().with("m73_in"), egui::Sense::hover()),
            tip,
        );
        let r = ui
            .horizontal(|ui| {
                key_cell(ui, "OUT");
                let cells = meter_cells(ui);
                ui.label(ascii_meter(
                    frac(s.audio_out_db),
                    cells,
                    if s.ptt_on { M_RED } else { M_MUTE },
                ));
                reading(ui, |ui| {
                    ui.label(text(db_text(s.audio_out_db), M_MUTE));
                });
            })
            .response;
        hover_tip(
            &ui.interact(r.rect, ui.id().with("m73_out"), egui::Sense::hover()),
            "Transmit audio to the radio (sound card playback). Fills while PTT is keyed.",
        );
        let mut level_cmd = None;
        if s.shows_level_sliders() {
            if let Some(g) = &s.rx_gain {
                let tip = if g.kind == "bluetooth" {
                    "RX gain: wcr's gain on the radio's Bluetooth audio. Click or drag the bar (or scroll on it) to change, double-click for 0 dB."
                } else {
                    "RX gain: the sound card's capture gain. Set it so IN reads good. Click or drag the bar (or scroll on it) to change, double-click for 0 dB."
                };
                if let Some(db) = level_text(
                    ui,
                    "RX GAIN",
                    &mut self.rx_held,
                    g.db,
                    g.min_db..=g.max_db,
                    0.5,
                    tip,
                ) {
                    level_cmd = Some(format!("/level rx {db:.1}"));
                }
            } else {
                let r = kv(ui, "RX GAIN", text("—", M_MUTE));
                if !s.rx_gain_note.is_empty() {
                    hover_tip(&r, s.rx_gain_note.clone());
                }
            }
            if let Some(db) = level_text(
                ui,
                "TX LVL",
                &mut self.tx_held,
                s.tx_level_db,
                crate::levels::TX_LEVEL_MIN_DB..=crate::levels::TX_LEVEL_MAX_DB,
                1.0,
                "TX level: only turns down, the modem already sends at full scale. Set the deviation with the radio's own mic gain, then lower TX level until the far end's SNR stops getting better. Click or drag the bar (or scroll on it) to change, double-click for 0 dB.",
            ) {
                level_cmd = Some(format!("/level tx {db:.0}"));
            }
        }
        let rung = if s.tx_rung.is_empty() {
            "—"
        } else {
            &s.tx_rung
        };
        let r = ui
            .horizontal(|ui| {
                key_cell(ui, "SNR");
                ui.label(text(format!("{:.0} dB", s.snr), M_GOLD));
                ui.add_space(12.0);
                ui.label(text("TX", M_DIM).size(12.0));
                ui.label(text(rung, M_FG));
            })
            .response;
        hover_tip(
            &ui.interact(r.rect, ui.id().with("m73_snr"), egui::Sense::hover()),
            "SNR of the last frame decoded. TX is the robustness rung frames go out on.",
        );
        let r = ui
            .horizontal(|ui| {
                key_cell(ui, "OCC");
                let cells = meter_cells(ui);
                ui.label(ascii_meter(s.occupancy_pct as f32 / 100.0, cells, M_GOLD));
                reading(ui, |ui| {
                    ui.label(text(format!("{}%", s.occupancy_pct), M_MUTE));
                });
            })
            .response;
        hover_tip(
            &ui.interact(r.rect, ui.id().with("m73_occ"), egui::Sense::hover()),
            "Channel occupancy 0–100 from the modem CSMA sense. High values mean the frequency is busy — beacons slow down and relays wait.",
        );
        ui.add_space(4.0);
        let now = crate::status::unix_now_f64();
        let hold = s.hold_lane(now);
        let hold_color = if !hold.live {
            M_MUTE
        } else if hold.frac < 0.08 || s.hold_kind == "retry" {
            M_GOLD
        } else {
            M_FG
        };
        let r = ui
            .horizontal(|ui| {
                ui.label(text("RETRY", M_DIM).size(12.0));
                ui.label(text(s.retries.to_string(), M_FG).strong());
                ui.add_space(10.0);
                ui.label(text("AIR", M_DIM).size(12.0));
                ui.label(text(s.queue_air.to_string(), M_FG).strong());
                ui.add_space(10.0);
                ui.label(text("HOLD", M_DIM).size(12.0));
                ui.label(text(&hold.label, hold_color).strong());
            })
            .response;
        hover_tip(
            &ui.interact(r.rect, ui.id().with("m73_mini"), egui::Sense::hover()),
            "RETRY: last ARQ retry count. AIR: radio frames waiting to leave this station now. HOLD: next store-and-forward relay or your own unacked retry.",
        );
        let snrs: Vec<f32> = self
            .traffic
            .iter()
            .filter(|r| r["dir"].as_str() != Some("sent"))
            .filter_map(|r| r["snr"].as_f64().map(|v| v as f32))
            .collect();
        m73_snr_history(ui, &snrs);
        if let Some(cmd) = level_cmd {
            self.send_cmd(&cmd);
        }
    }

    /// ACTIVITY: modem73's BARS graph, then STATS and the link state.
    fn m73_activity(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(label("ACTIVITY"));
        ui.add_space(6.0);
        let threshold = self
            .status
            .as_ref()
            .and_then(|s| s.csma_threshold_db)
            .unwrap_or(activity_bars::DEFAULT_THRESHOLD_DB);
        // MAIL GW key (#362): bottom-right corner, beside STATS, so the
        // graph keeps the full width (Tobias on rc1). Only when STATS'
        // widest row (last frame) leaves room for it; otherwise the MAIL GW
        // dots' hover tip still explains the states.
        let show_key = self
            .status
            .as_ref()
            .is_some_and(|s| s.mode == Mode::RadioPlus && !s.mail_gateway.is_empty());
        let full_w = ui.available_width();
        let gap = 10.0;
        let row_w = self.m73.stats_row_w;
        let fits = |wide: bool| {
            let w = mail_gw_key_width(ui, wide);
            (row_w + gap + w <= full_w).then_some((wide, w))
        };
        let key = if show_key {
            (full_w >= MAIL_GW_KEY_WIDE)
                .then(|| fits(true))
                .flatten()
                .or_else(|| fits(false))
        } else {
            None
        };
        // The graph takes whatever height STATS (or the key) leaves, so the
        // strip has no empty band at the bottom.
        let mut stats_h = if self.m73.stats_h > 0.0 {
            self.m73.stats_h
        } else {
            70.0
        };
        if key.is_some() {
            stats_h = stats_h.max(MAIL_GW_KEY_H);
        }
        let bars_h = ui.available_height() - stats_h - 4.0;
        paint_bars(ui, &self.m73.activity, threshold, bars_h);
        ui.add_space(4.0);
        let stats_top = ui.cursor().top();
        match key {
            Some((wide, key_w)) => {
                let area = ui.available_rect_before_wrap();
                let left = egui::Rect::from_min_max(
                    area.min,
                    egui::pos2(area.right() - key_w - gap, area.bottom()),
                );
                let mut row_w = 0.0;
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left), |ui| {
                    row_w = self.m73_stats(ui);
                });
                self.m73.stats_row_w = row_w;
                let bottom = ui.max_rect().bottom();
                let key_rect = egui::Rect::from_min_max(
                    egui::pos2(area.right() - key_w, bottom - MAIL_GW_KEY_H),
                    egui::pos2(area.right(), bottom),
                );
                paint_mail_gw_key(ui, key_rect, wide);
            }
            None => self.m73.stats_row_w = self.m73_stats(ui),
        }
        self.m73.stats_h = ui.cursor().top() - stats_top;
    }

    /// The STATS block under the graph. Returns the width of its widest
    /// row that cannot wrap, so the MAIL GW key knows if it fits beside.
    fn m73_stats(&mut self, ui: &mut egui::Ui) -> f32 {
        ui.label(text(">>> STATS", M_CYAN).size(12.0));
        let (rx, tx, letters) = self.m73_counts();
        let queue = self.status.as_ref().map(|s| s.queue_out).unwrap_or(0);
        let r = ui
            .horizontal(|ui| {
                ui.label(text("RX", M_FG).size(12.0));
                ui.label(text(rx.to_string(), M_GREEN).strong());
                ui.add_space(8.0);
                ui.label(text("TX", M_FG).size(12.0));
                ui.label(text(tx.to_string(), M_RED).strong());
                ui.add_space(8.0);
                ui.label(text("Queue", M_FG).size(12.0));
                ui.label(text(queue.to_string(), if queue > 0 { M_RED } else { M_FG }).strong());
                ui.add_space(8.0);
                ui.label(text("Letters", M_FG).size(12.0));
                ui.label(text(letters.to_string(), M_FG).strong());
            })
            .response;
        hover_tip(
            &ui.interact(r.rect, ui.id().with("m73_stats"), egui::Sense::hover()),
            "RX / TX: lines received and sent in this channel. Queue: your messages not yet delivered. Letters: lines in this channel.",
        );
        let mut row_w = r.rect.width();
        let Some(s) = self.status.clone() else {
            return row_w;
        };
        let now = crate::status::unix_now_f64();
        let beacon = s.beacon_lane(now);
        let (hub, _) = hub_row(s.mode, s.hub_ok);
        let hub_color = match hub {
            "UP" => M_GREEN,
            "DOWN" => M_GOLD,
            _ => M_MUTE,
        };
        let hub_row = ui.horizontal(|ui| {
            ui.label(text("HUB", M_DIM).size(12.0));
            ui.label(text(hub, hub_color).size(12.0));
            ui.add_space(8.0);
            let b = ui
                .horizontal(|ui| {
                    ui.label(text("BEACON", M_DIM).size(12.0));
                    ui.label(
                        text(&beacon.label, if beacon.live { M_GREEN } else { M_MUTE }).size(12.0),
                    );
                })
                .response;
            hover_tip(
                &ui.interact(b.rect, ui.id().with("m73_beacon"), egui::Sense::hover()),
                "Next automatic identity beacon. Off on VOX so the tone does not walk on inbound chat.",
            );
            if s.mode == Mode::RadioPlus && !s.mail_gateway.is_empty() {
                ui.add_space(8.0);
                let (lit, c) = m73_mail_gw_look(&s.mail_gateway);
                let dots: Vec<&str> = (0..MAIL_GW_STEPS)
                    .map(|i| if i < lit { "●" } else { "·" })
                    .collect();
                let g = ui
                    .horizontal(|ui| {
                        ui.label(text("MAIL GW", M_DIM).size(12.0));
                        ui.label(text(format!("[{}]", dots.join(" ")), c).size(12.0));
                    })
                    .response;
                hover_tip(
                    &ui.interact(g.rect, ui.id().with("m73_mailgw"), egui::Sense::hover()),
                    s.mail_gateway.clone(),
                );
            }
        });
        row_w = f32::max(row_w, hub_row.response.rect.width());
        let (_, crypto) = channel_security(&self.active_channel, Some(s.mode), &s.preset, s.hub_ok);
        ui.label(text(crypto, M_MUTE).size(11.0));
        if s.mode.uses_internet() && !s.hub_ok && !s.hub_banner.is_empty() {
            ui.label(text(&s.hub_banner, M_GOLD).size(11.0));
        }
        if s.clock_warn {
            ui.label(text("Clock skew — check system time or GPS", M_GOLD).size(11.0));
        }
        if !self.error.is_empty() {
            ui.label(text(&self.error, M_GOLD).size(11.0));
        }
        row_w
    }

    /// RX, TX and all lines in the open channel.
    fn m73_counts(&self) -> (usize, usize, usize) {
        let (mut rx, mut tx) = (0, 0);
        for line in self.m73_channel_lines() {
            if line.nick.eq_ignore_ascii_case(&self.callsign) {
                tx += 1;
            } else {
                rx += 1;
            }
        }
        (rx, tx, rx + tx)
    }

    fn m73_channel_lines(&self) -> impl Iterator<Item = &ChatLine> {
        self.chat.iter().filter(move |l| {
            !l.sys && (l.channel.is_empty() || l.channel.eq_ignore_ascii_case(&self.active_channel))
        })
    }

    fn m73_chat(
        &mut self,
        ctx: &egui::Context,
        suggestions: &[crate::slash::Suggestion],
        enter: bool,
    ) {
        egui::SidePanel::right("m73_delivery")
            .exact_width(DELIVERY_W)
            .resizable(false)
            .frame(egui::Frame::none().fill(M_BG))
            .show(ctx, |ui| self.m73_delivery(ui));
        let palette_h = if suggestions.is_empty() {
            0.0
        } else {
            (suggestions.len().min(8) as f32) * 28.0 + 28.0
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(M_BG))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let in_direct = crate::chat_line::is_direct(&self.active_channel);
                if in_direct {
                    self.direct_unread.remove(&self.active_channel);
                }
                self.m73_channel_tabs(ui);
                rule(ui);
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(PAD, 4.0))
                    .show(ui, |ui| self.m73_channel_tools(ui, in_direct));
                rule(ui);
                let panel_top = ui.max_rect().top();
                let mut input_rect = None;
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(4.0);
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(PAD, 6.0))
                        .show(ui, |ui| {
                            // As tall as the text in the box (#365 rc4).
                            let row_h = super::chat_input_height(
                                ui,
                                &self.draft,
                                ui.available_width() - 100.0,
                                &mono(14.0),
                            )
                            .max(24.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), row_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(text("$", M_DIM).size(14.0));
                                    let width = ui.available_width() - 80.0;
                                    let resp = super::chat_input(
                                        ui,
                                        &mut self.draft,
                                        width,
                                        RichText::new(if in_direct {
                                            "direct message, or /commands"
                                        } else {
                                            "message in this channel, or /join /invite /part"
                                        })
                                        .color(M_DIM),
                                        mono(14.0),
                                        false,
                                    );
                                    if enter {
                                        resp.request_focus();
                                    }
                                    input_rect = Some(resp.rect + INPUT_MARGIN);
                                    if ui
                                        .add(
                                            flat_btn("SEND", M_FG).min_size(egui::vec2(64.0, 24.0)),
                                        )
                                        .clicked()
                                    {
                                        self.send_chat();
                                        resp.request_focus();
                                    }
                                },
                            );
                        });
                    rule(ui);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(PAD, 6.0))
                            .show(ui, |ui| self.m73_chat_log(ui));
                    });
                });
                self.paint_slash_palette(ui, input_rect, panel_top, suggestions, palette_h);
            });
    }

    /// `> #bulletin  #local  M0ABC 2` and the letter count, clear and key.
    fn m73_channel_tabs(&mut self, ui: &mut egui::Ui) {
        let mut switch_to = None;
        let mut open_direct = None;
        let mut clear = false;
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(PAD, 4.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let tab = |ui: &mut egui::Ui, name: &str, on: bool, color: Color32| {
                        let shown = if on { format!("> {name}") } else { name.to_string() };
                        let mut t = text(shown, if on { M_FG } else { color }).size(12.0);
                        if on {
                            t = t.strong();
                        }
                        ui.add(egui::Label::new(t).sense(egui::Sense::click()))
                            .clicked()
                    };
                    for ch in self.joined.clone() {
                        if tab(ui, &ch, ch.eq_ignore_ascii_case(&self.active_channel), M_DIM) {
                            switch_to = Some(ch);
                        }
                        ui.add_space(8.0);
                    }
                    for peer in self.direct.clone() {
                        let n = self.direct_unread.get(&peer).copied().unwrap_or(0);
                        let name = if n > 0 { format!("{peer} {n}") } else { peer.clone() };
                        let on = peer.eq_ignore_ascii_case(&self.active_channel);
                        if tab(ui, &name, on, if n > 0 { M_GOLD } else { M_DIM }) {
                            open_direct = Some(peer);
                        }
                        ui.add_space(8.0);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let key = ui
                            .add(flat_btn(if self.show_key { "hide key" } else { "key" }, M_FG))
                            .on_hover_text("What the short marks after each line mean (F2).");
                        if key.clicked() {
                            self.show_key = !self.show_key;
                        }
                        clear = ui
                            .add(flat_btn("clear", M_FG))
                            .on_hover_text(
                                "Remove this channel's chat from this computer only. Other stations keep their copy. Station must be running to forget stored history.",
                            )
                            .clicked();
                        let (_, _, letters) = self.m73_counts();
                        ui.label(text(format!("{letters} letters"), M_DIM).size(12.0));
                    });
                });
            });
        if clear {
            self.clear_active_chat();
        }
        if let Some(ch) = switch_to {
            self.switch_channel(ch);
        }
        if let Some(peer) = open_direct {
            self.open_direct(&peer);
        }
    }

    /// New room, direct chat, invite, leave/close/files, priority, and the
    /// PUBLIC / identity / hub / RF line: every classic channel control.
    fn m73_channel_tools(&mut self, ui: &mut egui::Ui, in_direct: bool) {
        let field = |ui: &mut egui::Ui, buf: &mut String, hint: &str| {
            ui.add(
                egui::TextEdit::singleline(buf)
                    .desired_width(96.0)
                    .hint_text(RichText::new(hint).color(M_DIM))
                    .font(mono(12.0)),
            )
        };
        let mut open_direct = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(text("room", M_DIM).size(12.0));
            field(ui, &mut self.chan_new, "8 letters");
            if ui.add(flat_btn("create", M_FG)).clicked() {
                self.create_channel();
            }
            ui.add_space(10.0);
            ui.label(text("direct", M_DIM).size(12.0));
            let typed = field(ui, &mut self.direct_new, "M0ABC");
            let enter_typed = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .add(flat_btn("open", M_FG))
                .on_hover_text("Start a direct chat with this callsign.")
                .clicked()
                || enter_typed
            {
                let peer = self.direct_new.trim().to_string();
                if !peer.is_empty() {
                    open_direct = Some(peer);
                }
            }
            if in_direct {
                ui.add_space(10.0);
                if ui
                    .add(flat_btn("close chat", M_RED))
                    .on_hover_text(
                        "Take this direct chat off the list. A new message from them opens it again.",
                    )
                    .clicked()
                {
                    self.close_direct();
                }
                let files = ui.add(flat_btn("files", M_FG)).on_hover_text(format!(
                    "To send a file, drag it onto this window (up to {}, vhf-fm, vhf-fast or afsk-1200 only). Click to open the folder where received files are saved.",
                    crate::proto::file::human_size(crate::proto::file::FILE_MAX_BYTES)
                ));
                if files.clicked() {
                    self.open_received_files();
                }
            } else if !crate::slash::is_bulletin(&self.active_channel) {
                ui.add_space(10.0);
                ui.label(text("invite", M_DIM).size(12.0));
                field(ui, &mut self.invite_draft, "M0ABC");
                if ui.add(flat_btn("send invite", M_FG)).clicked() {
                    self.invite_to_current();
                }
                if ui
                    .add(flat_btn("leave room", M_RED))
                    .on_hover_text(
                        "Remove this room on this computer and tell the others you left. The room stays for anyone still in it.",
                    )
                    .clicked()
                {
                    self.leave_current();
                }
            }
        });
        if let Some(peer) = open_direct {
            self.direct_new.clear();
            self.open_direct(&peer);
        }
        let in_direct = crate::chat_line::is_direct(&self.active_channel);
        let (access, crypto) = channel_security(
            &self.active_channel,
            self.status.as_ref().map(|s| s.mode),
            self.status
                .as_ref()
                .map(|s| s.preset.as_str())
                .unwrap_or(""),
            self.status.as_ref().map(|s| s.hub_ok).unwrap_or(false),
        );
        let note = if in_direct {
            "Direct chat: only this station is addressed. Lines here clear when the app restarts; the station keeps its copy."
        } else if crate::slash::is_bulletin(&self.active_channel) {
            "Anyone on the network can read #bulletin."
        } else {
            "Closed channels are invite lists."
        };
        let mut prio_cmd = None;
        ui.horizontal(|ui| {
            // The other station's callsign stands out in its own colour
            // ("DIRECT · M7TJF only · …", #365).
            let r = match access
                .strip_prefix("DIRECT · ")
                .and_then(|r| r.strip_suffix(" only"))
            {
                Some(call) => {
                    let row = ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(text("DIRECT · ", M_MUTE).size(11.0));
                        ui.label(text(call, M_CYAN).size(11.0).strong());
                        ui.label(text(format!(" only · {crypto}"), M_MUTE).size(11.0));
                    });
                    row.response
                }
                None => ui.label(text(format!("{access} · {crypto}"), M_MUTE).size(11.0)),
            };
            hover_tip(
                &r,
                format!("{note} Amateur radio cannot hide message text on RF; hub traffic is TLS."),
            );
            if (self.active_channel.starts_with('#') || self.active_channel.starts_with('&'))
                && !crate::slash::is_bulletin(&self.active_channel)
            {
                let current = self
                    .status
                    .as_ref()
                    .map(|s| s.prio_for_channel(&self.active_channel).to_string())
                    .unwrap_or_else(|| "routine".into());
                if let Some(next) = prio_pick(ui, &current) {
                    prio_cmd = Some(format!("/prio {next}"));
                }
            }
        });
        if let Some(cmd) = prio_cmd {
            self.send_cmd(&cmd);
        }
        let people = self
            .members
            .get(&self.active_channel)
            .cloned()
            .unwrap_or_default();
        if !people.is_empty() {
            ui.label(text(format!("members {}", people.join(" ")), M_PEER).size(11.0));
        }
    }

    fn m73_chat_log(&mut self, ui: &mut egui::Ui) {
        let active = self.active_channel.clone();
        let show_snr = self.status.as_ref().map(|s| s.show_snr).unwrap_or(true);
        let mut open_folder_of: Option<String> = None;
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            // A drag selects text (#365 rc4); the wheel and the bar scroll.
            .drag_to_scroll(false)
            .show(ui, |ui| {
                ui.style_mut().interaction.selectable_labels = true;
                for line in &self.chat {
                    if !line.channel.is_empty() && !line.channel.eq_ignore_ascii_case(&active) {
                        continue;
                    }
                    if line.sys {
                        ui.label(text(&line.text, M_MUTE));
                        continue;
                    }
                    let mine = line.nick.eq_ignore_ascii_case(&self.callsign);
                    let in_flight = mine && line.ticks.contains("[tx]");
                    // The marks column, wider on a long message so its
                    // progress sits on the same line as [tx] / [rf] (#365 rc4).
                    let marks_w = 120.0 + super::group_footer_width(ui, line, &mono(13.0));
                    let left_w =
                        (ui.available_width() - marks_w - 2.0 * ui.spacing().item_spacing.x - 4.0)
                            .max(80.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui(egui::vec2(left_w, 4000.0), |ui| {
                            ui.set_min_width(left_w);
                            ui.horizontal_wrapped(|ui| {
                                if !line.when.is_empty() {
                                    ui.label(text(&line.when, M_DIM))
                                        .on_hover_text(format!("received {}", line.when));
                                }
                                let nick = text(
                                    format!("{:<10}", format!("[{}]", line.nick)),
                                    if mine { M_FG } else { M_CYAN },
                                );
                                ui.label(if mine { nick.strong() } else { nick });
                                let body = if in_flight { M_MUTE } else { M_FG };
                                match crate::chat_line::saved_file(&line.text) {
                                    Some((lead, path)) => {
                                        ui.label(text(lead.trim_end(), body));
                                        let link = ui
                                            .link(text(path, M_CYAN).underline())
                                            .on_hover_text("open the folder it is in");
                                        if link.clicked() {
                                            open_folder_of = Some(path.to_string());
                                        }
                                    }
                                    None => {
                                        if let Some((m, tone)) = super::group_mark(line) {
                                            let c = if tone == crate::chat_line::FooterTone::Missing
                                            {
                                                M_RED
                                            } else {
                                                M_GOLD
                                            };
                                            ui.label(text(m, c));
                                        }
                                        ui.label(text(super::shown_text(ui, &line.text), body));
                                    }
                                }
                                if !line.progress.is_empty() {
                                    ui.label(text(&line.progress, M_GREEN)).on_hover_text(
                                        if mine {
                                            "pieces the far station has receipted"
                                        } else {
                                            "pieces received so far"
                                        },
                                    );
                                }
                            });
                        });
                        ui.allocate_ui_with_layout(
                            egui::vec2(marks_w, 18.0),
                            egui::Layout::right_to_left(egui::Align::Min),
                            |ui| {
                                self.m73_marks(ui, line, mine, show_snr);
                                super::group_footer_label(
                                    ui,
                                    line,
                                    [M_GOLD, M_GREEN, M_RED],
                                    mono(13.0),
                                );
                            },
                        );
                    });
                }
            });
        if let Some(path) = open_folder_of {
            if let Some(dir) = std::path::Path::new(&path).parent() {
                let _ = open::that(dir);
            }
        }
    }

    /// The marks at the right of a line, laid out right to left.
    fn m73_marks(&self, ui: &mut egui::Ui, line: &ChatLine, mine: bool, show_snr: bool) {
        if mine {
            if line.tries > 0 {
                ui.label(text(crate::store::tries_bracket(line.tries), M_MUTE))
                    .on_hover_text(format!(
                        "sent {} time{} over radio",
                        line.tries,
                        if line.tries == 1 { "" } else { "s" }
                    ));
            }
            if !line.ticks.is_empty() {
                ui.label(text(&line.ticks, mark_color(&line.ticks)))
                    .on_hover_text(tick_hint(&line.ticks));
            }
        } else {
            if let Some(db) = line.snr.filter(|_| show_snr) {
                ui.label(text(format!("{db:.0} dB"), M_MUTE))
                    .on_hover_text(SNR_HINT);
            }
            if !line.via.is_empty() {
                let via = crate::store::via_bracket(&line.via);
                ui.label(text(via, mark_color(via)))
                    .on_hover_text(crate::store::via_hint(&line.via));
            }
        }
    }

    /// DELIVERY: the marks of the newest lines, the key, and stations heard.
    fn m73_delivery(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 2.0;
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(PAD, 7.0))
            .show(ui, |ui| ui.label(label("DELIVERY")));
        rule(ui);
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(PAD, 8.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("m73_delivery")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let show_snr = self.status.as_ref().map(|s| s.show_snr).unwrap_or(true);
                        let rows: Vec<&ChatLine> = self.m73_channel_lines().collect();
                        if rows.is_empty() {
                            ui.label(text("nothing sent or heard here yet", M_DIM).size(12.0));
                        }
                        for line in rows.into_iter().rev().take(24) {
                            let mine = line.nick.eq_ignore_ascii_case(&self.callsign);
                            ui.horizontal(|ui| {
                                ui.label(
                                    text(
                                        if mine { "TX" } else { "RX" },
                                        if mine { M_RED } else { M_GREEN },
                                    )
                                    .size(12.0)
                                    .strong(),
                                );
                                ui.add_space(4.0);
                                ui.label(text(&line.nick, M_FG).size(12.0));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| self.m73_marks(ui, line, mine, show_snr),
                                );
                            });
                        }
                        ui.add_space(10.0);
                        let key: &[(&str, &str)] = if self.show_key {
                            MARK_KEY
                        } else {
                            &MARK_KEY_SHORT
                        };
                        for (mark, meaning) in key {
                            ui.label(text(format!("{mark} {meaning}"), M_DIM).size(11.0));
                        }
                        if let Some(s) = &self.status {
                            if !s.heard.is_empty() {
                                ui.add_space(10.0);
                                ui.label(label("HEARD"));
                                for h in s.heard.iter().take(16) {
                                    let tag = match h.band_tag() {
                                        "inet" => "net",
                                        tag => tag,
                                    };
                                    ui.label(
                                        text(format!("{} [{tag}]", h.callsign), M_PEER).size(12.0),
                                    );
                                }
                            }
                        }
                    });
            });
    }

    /// ON AIR: every frame heard or sent, newest first, with a kind filter.
    fn m73_on_air(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(M_BG))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let filter = self.m73.on_air;
                let shown: Vec<&serde_json::Value> = self
                    .traffic
                    .iter()
                    .rev()
                    .filter(|r| filter.keeps(r["kind"].as_str().unwrap_or("")))
                    .collect();
                let mut pick = None;
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(PAD, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for f in OnAirFilter::ALL {
                                let on = f == filter;
                                let t = if on {
                                    text(format!("> {}", f.label()), M_FG).strong()
                                } else {
                                    text(f.label(), M_DIM)
                                };
                                if ui
                                    .add(egui::Label::new(t.size(12.0)).sense(egui::Sense::click()))
                                    .clicked()
                                {
                                    pick = Some(f);
                                }
                                ui.add_space(10.0);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    text(format!("{} frames · newest first", shown.len()), M_DIM)
                                        .size(12.0),
                                );
                            });
                        });
                    });
                rule(ui);
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(PAD, 6.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("m73_on_air")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if shown.is_empty() {
                                    ui.label(text("Nothing heard or sent since the station started.", M_DIM));
                                }
                                for r in &shown {
                                    on_air_row(ui, r);
                                }
                                ui.add_space(10.0);
                                ui.label(
                                    text(
                                        "Every frame this station heard or sent, newest first. Read only: to answer a station, type its callsign in the direct box on live chat.",
                                        M_DIM,
                                    )
                                    .size(11.0),
                                );
                            });
                    });
                if let Some(f) = pick {
                    self.m73.on_air = f;
                }
            });
    }
}

/// The short key under DELIVERY; F2 shows every mark.
const MARK_KEY_SHORT: [(&str, &str); 5] = [
    ("[ok]", "delivered, they sent a receipt"),
    ("[rf]", "reached you by radio"),
    ("[tx]", "sent, no receipt yet"),
    ("[no]", "refused, or no receipt after the last try"),
    ("[x2]", "how many times it was keyed"),
];

fn on_air_row(ui: &mut egui::Ui, r: &serde_json::Value) {
    let s = |k: &str| r[k].as_str().unwrap_or("").to_string();
    let when = chrono::DateTime::from_timestamp(r["ts"].as_i64().unwrap_or(0), 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();
    let kind = s("kind");
    let kind_color = match kind.as_str() {
        "dm" | "bulletin" | "group" => M_PEER,
        "receipt" => M_GREEN,
        "mail" => M_GOLD,
        _ => M_MUTE,
    };
    let sent = s("dir") == "sent";
    let to = s("to");
    let who = if to.is_empty() {
        s("from")
    } else {
        format!("{} -> {to}", s("from"))
    };
    let left_w = (ui.available_width() - 130.0 - 2.0 * ui.spacing().item_spacing.x - 4.0).max(80.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui(egui::vec2(left_w, 4000.0), |ui| {
            ui.set_min_width(left_w);
            ui.horizontal_wrapped(|ui| {
                ui.label(text(when, M_DIM));
                ui.allocate_ui_with_layout(
                    egui::vec2(76.0, 18.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(76.0);
                        ui.label(text(&kind, kind_color));
                    },
                );
                ui.label(text(who, if sent { M_FG } else { M_MUTE }));
                let body = s("text");
                if !body.is_empty() {
                    ui.label(text(body, M_FG));
                }
            });
        });
        ui.allocate_ui_with_layout(
            egui::vec2(130.0, 18.0),
            egui::Layout::right_to_left(egui::Align::Min),
            |ui| {
                if let Some(snr) = r["snr"].as_f64() {
                    ui.label(text(format!("{snr:.0} dB"), M_MUTE));
                }
                let via = s("via");
                let (mark, color) = match (sent, via.as_str()) {
                    (true, _) if r["relayed"].as_bool() == Some(true) => {
                        ("relayed".to_string(), M_MUTE)
                    }
                    (true, _) => ("sent".to_string(), M_RED),
                    (false, "inet") => ("[net]".to_string(), M_CYAN),
                    (false, v) => (format!("[{v}]"), mark_color(&format!("[{v}]"))),
                };
                ui.label(text(mark, color));
            },
        );
    });
}

/// RX GAIN / TX LVL as a text value that works like classic's knob: drag
/// up or right to turn it up, scroll to step it, double-click for 0 dB.
/// Returns the value to send once the operator lets go.
fn level_text(
    ui: &mut egui::Ui,
    k: &str,
    held: &mut Option<(f32, Instant)>,
    now_db: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    tip: &str,
) -> Option<f32> {
    let (lo, hi) = (*range.start(), *range.end());
    let snap = |v: f32| (((v as f64 / step).round() * step) as f32).clamp(lo, hi);
    let mut v = match *held {
        Some((v, at)) if at.elapsed() < Duration::from_secs(4) => v,
        _ => now_db,
    }
    .clamp(lo, hi);
    let mut send = None;
    ui.horizontal(|ui| {
        key_cell(ui, k);
        // The bar: click or drag anywhere on it to set the level there.
        let cells = meter_cells(ui);
        let bar = ui
            .add(
                egui::Label::new(ascii_meter((v - lo) / (hi - lo), cells, M_CYAN))
                    .sense(egui::Sense::click_and_drag()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if bar.is_pointer_button_down_on() {
            if let Some(p) = bar.interact_pointer_pos() {
                // Skip the `[` and `]` cells so the ends map to lo and hi.
                let cell = bar.rect.width() / (cells + 2) as f32;
                let left = bar.rect.left() + cell;
                let frac = ((p.x - left) / (cell * cells as f32)).clamp(0.0, 1.0);
                v = snap(lo + frac * (hi - lo));
                *held = Some((v, Instant::now()));
            }
        }
        if bar.drag_stopped() || bar.clicked() {
            send = Some(v);
        }
        if bar.double_clicked() {
            v = 0.0f32.clamp(lo, hi);
            *held = Some((v, Instant::now()));
            send = Some(v);
        }
        if bar.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                v = snap(v + scroll.signum() * step as f32);
                *held = Some((v, Instant::now()));
                send = Some(v);
            }
        }
        hover_tip(&bar, tip);
        let resp = reading(ui, |ui| {
            ui.add(
                egui::Label::new(text(format!("{v:+.1} dB"), M_FG))
                    .sense(egui::Sense::click_and_drag()),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
        });
        if resp.dragged() {
            let d = resp.drag_delta();
            v = (v + (d.x - d.y) * (hi - lo) / 160.0).clamp(lo, hi);
            *held = Some((v, Instant::now()));
        }
        if resp.drag_stopped() {
            v = snap(v);
            *held = Some((v, Instant::now()));
            send = Some(v);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                v = snap(v + scroll.signum() * step as f32);
                *held = Some((v, Instant::now()));
                send = Some(v);
            }
        }
        if resp.double_clicked() {
            v = 0.0f32.clamp(lo, hi);
            *held = Some((v, Instant::now()));
            send = Some(v);
        }
        hover_tip(&resp, tip);
    });
    send
}

/// Rows of the BARS graph share `height`, never shorter than 14 px.
fn bar_row_height(height: f32) -> f32 {
    let h = height / activity_bars::ROWS as f32;
    if h.is_finite() && h > 14.0 {
        h
    } else {
        14.0
    }
}

/// One HEARD row: call, band, SNR and how long ago, in fixed columns.
const HEARD_ROW_H: f32 = 15.0;

/// STATION's lower part (Tobias on rc1: use the empty space): the stations
/// heard most recently, as many as fit. Same list as the email HEARD.
fn m73_heard(ui: &mut egui::Ui, heard: &[crate::status::HeardBrief]) {
    let room = ui.available_height() - 6.0 - HEARD_ROW_H;
    if room < HEARD_ROW_H {
        return;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(label("HEARD"));
        if !heard.is_empty() {
            ui.label(text(heard.len().to_string(), M_DIM).size(11.0));
        }
    });
    if heard.is_empty() {
        ui.label(text("nobody yet", M_DIM).size(12.0));
        return;
    }
    let now = crate::status::unix_now_f64() as i64;
    let fits = (room / HEARD_ROW_H).floor().max(1.0) as usize;
    for h in heard.iter().take(fits) {
        let tag = match h.band_tag() {
            "inet" => "net",
            tag => tag,
        };
        let snr = h.snr.map(|v| format!("{v:.0} dB")).unwrap_or_default();
        let ago = ago_short(now - h.last_heard as i64);
        ui.horizontal(|ui| {
            ui.set_height(HEARD_ROW_H - 1.0);
            key_cell(ui, &h.callsign);
            ui.label(text(format!("{tag:<5}"), M_MUTE).size(12.0));
            reading(ui, |ui| {
                ui.label(text(format!("{ago:>4}"), M_DIM).size(12.0));
                if !snr.is_empty() {
                    ui.label(text(snr, M_GOLD).size(12.0));
                }
            });
        });
    }
}

/// "12s", "4m", "2h", "3d".
fn ago_short(secs: i64) -> String {
    let s = secs.max(0);
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

/// SNR scale of the history graph: weak decodes near the bottom, a strong
/// local signal at the top.
const SNR_LO: f32 = -10.0;
const SNR_HI: f32 = 30.0;

/// SIGNAL's lower part (Tobias on rc1: use the empty space): the SNR of
/// each frame heard, oldest left, as bars that fill the rest of the column.
/// The key sits in the same column as IN, OUT and SNR.
fn m73_snr_history(ui: &mut egui::Ui, snrs: &[f32]) {
    let h = ui.available_height() - 6.0;
    if h < 14.0 {
        return;
    }
    ui.add_space(6.0);
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let key_w = 64.0;
    painter.text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        "SNR LOG",
        mono(12.0),
        M_DIM,
    );
    let area = egui::Rect::from_min_max(egui::pos2(rect.left() + key_w, rect.top()), rect.max);
    painter.line_segment([area.left_bottom(), area.right_bottom()], hairline(M_LINE));
    let bar_w = 4.0;
    let step = bar_w + 2.0;
    let n = ((area.width() / step).floor() as usize).max(1);
    let shown = &snrs[snrs.len().saturating_sub(n)..];
    if shown.is_empty() {
        painter.text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            "no frames heard yet",
            mono(11.0),
            M_DIM,
        );
    }
    // Right-aligned so the newest frame is always at the right edge.
    let x0 = area.right() - shown.len() as f32 * step + 2.0;
    for (i, &v) in shown.iter().enumerate() {
        let frac = ((v - SNR_LO) / (SNR_HI - SNR_LO)).clamp(0.06, 1.0);
        let top = area.bottom() - 1.0 - frac * (area.height() - 2.0);
        let x = x0 + i as f32 * step;
        let bar = egui::Rect::from_min_max(
            egui::pos2(x, top),
            egui::pos2(x + bar_w, area.bottom() - 1.0),
        );
        let c = if v < 3.0 {
            M_RED
        } else if v < 10.0 {
            M_GOLD
        } else {
            M_GREEN
        };
        painter.rect_filled(bar, 0.0, c.gamma_multiply(0.85));
    }
    let mut tip = String::from(
        "SNR of each frame this station decoded, oldest left, newest right. Green 10 dB and up, gold 3 to 10, red below 3 (close to the decode limit).",
    );
    if let (Some(lo), Some(hi), Some(last)) = (
        shown.iter().copied().reduce(f32::min),
        shown.iter().copied().reduce(f32::max),
        shown.last(),
    ) {
        tip.push_str(&format!(
            "\n\nLast {last:.0} dB, range {lo:.0} to {hi:.0} dB over {} frames.",
            shown.len()
        ));
    }
    hover_tip(&resp, tip);
}

/// MAIL GW dots in the modem73 palette: lit segments and their colour.
fn m73_mail_gw_look(line: &str) -> (usize, Color32) {
    let (lit, _) = mail_gw_stage(line);
    let c = if lit == 0 {
        M_MUTE
    } else if lit < MAIL_GW_STEPS {
        M_PLUS
    } else if line.starts_with(crate::mail::pick::NO_GATEWAY_HEARD) || line.starts_with("Radio+") {
        M_GOLD
    } else {
        M_GREEN
    };
    (lit, c)
}

/// The MAIL GW key (#362): lit dots, colour, name, short meaning. Same
/// states as [`mail_gw_stage`]; no new ones.
const MAIL_GW_KEY: [(usize, Color32, &str, &str); 4] = [
    (0, M_MUTE, "Not asked yet", "Auto has not asked"),
    (1, M_PLUS, "Asking", "asking on air"),
    (MAIL_GW_STEPS, M_GREEN, "Gateway ready", "mail will relay"),
    (MAIL_GW_STEPS, M_GOLD, "No gateway", "none heard or set"),
];
const MAIL_GW_KEY_ROWS: usize = MAIL_GW_KEY.len();
/// Column width from which the key also shows the short meanings.
const MAIL_GW_KEY_WIDE: f32 = 520.0;
const KEY_ROW_H: f32 = 13.0;
const KEY_PAD: f32 = 4.0;
const KEY_FONT: f32 = 10.0;
const MAIL_GW_KEY_H: f32 = MAIL_GW_KEY_ROWS as f32 * KEY_ROW_H + 2.0 * KEY_PAD;
const MAIL_GW_KEY_TIP: &str = "MAIL GW key\n\
grey: Not asked yet: Auto has not yet asked which gateways hear you\n\
magenta: Asking: Auto is asking on air which gateways hear you\n\
green: Gateway ready: a gateway is in use (Auto picked it or it is typed in setup); mail will relay\n\
gold: No gateway: none heard, or none set in setup";

fn mail_gw_key_row(lit: usize, name: &str, meaning: &str, wide: bool) -> (String, String) {
    let dots: String = (0..MAIL_GW_STEPS)
        .map(|i| if i < lit { '●' } else { '·' })
        .collect();
    let words = if wide {
        format!("{name:<13}  {meaning}")
    } else {
        name.to_string()
    };
    (dots, words)
}

/// Width of the key box: the widest row plus padding.
fn mail_gw_key_width(ui: &egui::Ui, wide: bool) -> f32 {
    let font = mono(KEY_FONT);
    ui.fonts(|f| {
        MAIL_GW_KEY
            .iter()
            .map(|&(lit, _, name, meaning)| {
                let (dots, words) = mail_gw_key_row(lit, name, meaning, wide);
                let d = f.layout_no_wrap(dots, font.clone(), M_FG).size().x;
                let w = f.layout_no_wrap(words, font.clone(), M_FG).size().x;
                d + 6.0 + w
            })
            .fold(0.0, f32::max)
    }) + 2.0 * KEY_PAD
}

/// Paints the key in a dashed box; the full meanings are in the hover tip.
fn paint_mail_gw_key(ui: &mut egui::Ui, rect: egui::Rect, wide: bool) {
    let painter = ui.painter_at(rect.expand(1.0));
    let r = rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(
        &corners,
        hairline(M_LINE),
        3.0,
        2.0,
    ));
    let font = mono(KEY_FONT);
    let dots_w = ui.fonts(|f| f.layout_no_wrap("●●●".into(), font.clone(), M_FG).size().x);
    for (i, &(lit, color, name, meaning)) in MAIL_GW_KEY.iter().enumerate() {
        let (dots, words) = mail_gw_key_row(lit, name, meaning, wide);
        let y = rect.top() + KEY_PAD + KEY_ROW_H * (i as f32 + 0.5);
        let x = rect.left() + KEY_PAD;
        painter.text(
            egui::pos2(x, y),
            egui::Align2::LEFT_CENTER,
            dots,
            font.clone(),
            color,
        );
        painter.text(
            egui::pos2(x + dots_w + 6.0, y),
            egui::Align2::LEFT_CENTER,
            words,
            font.clone(),
            M_MUTE,
        );
    }
    let resp = ui.interact(rect, ui.id().with("m73_mailgw_key"), egui::Sense::hover());
    hover_tip(&resp, MAIL_GW_KEY_TIP);
}

/// The BARS graph: narrow pickets for full cells, `. : | #` for part-full
/// ones, a dim gold line on the threshold row. One column per character
/// of the pane's width.
fn paint_bars(ui: &mut egui::Ui, history: &activity_bars::History, threshold_db: f32, height: f32) {
    let font = mono(12.0);
    let ch_w = ui.fonts(|f| f.glyph_width(&font, '0')).max(4.0);
    let row_h = bar_row_height(height);
    let width = ui.available_width();
    let cols = ((width / ch_w).floor() as usize).max(1);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, row_h * activity_bars::ROWS as f32),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let grid = history.grid(cols, threshold_db);
    let gold_dim = M_GOLD.gamma_multiply(0.6);
    for (row, cells) in grid.iter().enumerate() {
        let y0 = rect.top() + row as f32 * row_h;
        for (col, cell) in cells.iter().enumerate() {
            let x0 = rect.left() + col as f32 * ch_w;
            let cell_rect = egui::Rect::from_min_size(egui::pos2(x0, y0), egui::vec2(ch_w, row_h));
            match *cell {
                Cell::Full(t) => {
                    // U+25AE drawn as a narrow block so the gaps stay.
                    let bar = egui::Rect::from_center_size(
                        cell_rect.center(),
                        egui::vec2(ch_w * 0.58, row_h - 3.0),
                    );
                    painter.rect_filled(bar, 0.0, tint_color(t));
                }
                Cell::Partial(c, t) => {
                    if c != ' ' {
                        painter.text(
                            cell_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            c,
                            font.clone(),
                            tint_color(t).gamma_multiply(0.85),
                        );
                    }
                }
                Cell::Threshold => {
                    let y = cell_rect.center().y;
                    painter.line_segment(
                        [
                            egui::pos2(cell_rect.left(), y),
                            egui::pos2(cell_rect.right(), y),
                        ],
                        hairline(gold_dim),
                    );
                }
                Cell::Blank => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_gw_key_matches_the_live_dots() {
        // One real status line per state; the key row must look the same.
        let lines = [
            "Auto: not asked yet",
            "Auto: asking which gateways hear you…",
            "Auto: G4ABC, link 12 dB",
            crate::mail::pick::NO_GATEWAY_HEARD,
        ];
        for (line, &(lit, color, _, _)) in lines.iter().zip(MAIL_GW_KEY.iter()) {
            assert_eq!(m73_mail_gw_look(line), (lit, color), "{line}");
        }
        assert_eq!(m73_mail_gw_look("G4ABC (typed in setup)").1, M_GREEN);
        assert_eq!(m73_mail_gw_look(crate::mail::NO_GATEWAY_REASON).1, M_GOLD);
    }

    #[test]
    fn modem73_unless_something_asks_for_classic() {
        assert_eq!(Layout::resolve(None, None, ""), Layout::Modem73);
        assert_eq!(Layout::resolve(None, None, "nonsense"), Layout::Modem73);
        assert_eq!(Layout::resolve(None, None, "modem73"), Layout::Modem73);
        assert_eq!(Layout::resolve(None, None, "classic"), Layout::Classic);
        assert_eq!(Layout::resolve(None, None, " Classic "), Layout::Classic);
    }

    #[test]
    fn flag_beats_environment_beats_config() {
        assert_eq!(
            Layout::resolve(Some("classic"), Some("modem73"), "modem73"),
            Layout::Classic
        );
        assert_eq!(
            Layout::resolve(None, Some("modem73"), "classic"),
            Layout::Modem73
        );
        assert_eq!(
            Layout::resolve(Some("bogus"), Some("modem73"), "classic"),
            Layout::Modem73
        );
    }

    #[test]
    fn config_default_is_modem73() {
        assert_eq!(Config::default().ui.layout, "modem73");
        let cfg: Config = toml::from_str("[ui]\nlayout = \"classic\"\n").unwrap();
        assert_eq!(cfg.ui.layout, "classic");
        let old: Config = toml::from_str("[ui]\ntheme = \"tron\"\n").unwrap();
        assert_eq!(old.ui.layout, "modem73");
    }

    #[test]
    fn on_air_starts_on_all() {
        assert_eq!(State::default().on_air, OnAirFilter::All);
    }

    #[test]
    fn bar_rows_share_the_height() {
        assert_eq!(bar_row_height(120.0), 20.0);
        assert_eq!(bar_row_height(30.0), 14.0);
        assert_eq!(bar_row_height(f32::NAN), 14.0);
    }

    #[test]
    fn ascii_meter_fills_by_level() {
        assert_eq!(meter_fill(0.0, 18), 0);
        assert_eq!(meter_fill(0.5, 18), 9);
        assert_eq!(meter_fill(1.0, 18), 18);
        assert_eq!(meter_fill(2.0, 18), 18);
    }

    #[test]
    fn meters_stretch_to_the_row() {
        // 8 px cells: 400 px is 50 cells, less 10 for the reading and 2
        // for the brackets.
        assert_eq!(meter_cells_for(400.0, 8.0), 38);
        assert_eq!(meter_cells_for(800.0, 8.0), 88);
        // Narrow column: never below the old fixed width.
        assert_eq!(meter_cells_for(100.0, 8.0), METER_W);
        assert_eq!(meter_cells_for(f32::INFINITY, 8.0), METER_W);
    }

    #[test]
    fn on_air_filters_by_kind() {
        assert!(OnAirFilter::All.keeps("ping"));
        assert!(OnAirFilter::Messages.keeps("dm"));
        assert!(OnAirFilter::Messages.keeps("bulletin"));
        assert!(!OnAirFilter::Messages.keeps("receipt"));
        assert!(OnAirFilter::Receipts.keeps("receipt"));
        assert!(OnAirFilter::Beacons.keeps("beacon"));
        assert!(OnAirFilter::Mail.keeps("mail"));
        assert!(!OnAirFilter::Mail.keeps("dm"));
    }

    #[test]
    fn marks_take_modem73_colours() {
        assert_eq!(mark_color("[ok]"), M_GREEN);
        assert_eq!(mark_color("[rl] [ok]"), M_GREEN);
        assert_eq!(mark_color("[no]"), M_RED);
        assert_eq!(mark_color("[tx]"), M_RED);
        assert_eq!(mark_color("[rf]"), M_GOLD);
        assert_eq!(mark_color("[x2]"), M_MUTE);
    }

    #[test]
    fn a_stopped_station_samples_blank_every_100_ms() {
        let mut st = State::default();
        let t0 = Instant::now();
        st.tick(t0, None);
        st.tick(t0 + Duration::from_millis(250), None);
        let s = st.activity.samples();
        assert_eq!(s.len(), activity_bars::HISTORY);
        let last = s.back().unwrap();
        assert_eq!(last.level_db, activity_bars::EMPTY_DB);
        assert!(!last.tone && !last.dcd);
    }
}
