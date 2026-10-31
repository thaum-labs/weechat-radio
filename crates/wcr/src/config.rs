//! SPDX-License-Identifier: Apache-2.0
//! Node configuration (`wcr.toml`).

use crate::error::{Error, Result};
use crate::modes::Mode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PUBLIC_HUB: &str = "wss://hub.weechatradio.com/ws";
pub const PUBLIC_TELEMETRY: &str = "https://hub.weechatradio.com/api/v1/report";
/// Live map (site homepage is separate at weechatradio.com).
pub const PUBLIC_MAP: &str = "https://weechatradio.com/map.html";

static ACTIVE_PATH: parking_lot::Mutex<Option<PathBuf>> = parking_lot::Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub callsign: String,
    pub grid: String,
    pub mode: Mode,
    pub irc: IrcConfig,
    pub modem: ModemConfig,
    pub hub: HubConfig,
    pub telemetry: TelemetryConfig,
    pub gateway: GatewayConfig,
    pub store: StoreConfig,
    pub group: GroupConfig,
    pub lan: LanConfig,
    pub ui: UiConfig,
    pub status: StatusConfig,
    pub relay: RelayConfig,
    pub rig: RigConfig,
    pub rf: RfConfig,
    pub tnc: TncConfig,
    pub mail: MailConfig,
    pub gps: GpsConfig,
    pub web: WebConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            callsign: String::new(),
            grid: String::new(),
            mode: Mode::InternetRadio,
            irc: IrcConfig::default(),
            modem: ModemConfig::default(),
            hub: HubConfig::default(),
            telemetry: TelemetryConfig::default(),
            gateway: GatewayConfig::default(),
            store: StoreConfig::default(),
            group: GroupConfig::default(),
            lan: LanConfig::default(),
            ui: UiConfig::default(),
            status: StatusConfig::default(),
            relay: RelayConfig::default(),
            rig: RigConfig::default(),
            rf: RfConfig::default(),
            tnc: TncConfig::default(),
            mail: MailConfig::default(),
            gps: GpsConfig::default(),
            web: WebConfig::default(),
        }
    }
}

/// `wcr web`: the station's web page for other devices on the LAN (a
/// Raspberry Pi gateway, for example). HTTPS only, behind a password.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    pub bind: String,
    /// Argon2 hash of the password. Empty: `wcr web` refuses to start.
    /// Set with `wcr web password`.
    pub password_hash: String,
    /// Answer browsers outside the LAN too. Off unless set on purpose.
    pub allow_public: bool,
    /// PEM certificate and key. Empty: a self-signed pair in the data folder.
    pub cert: String,
    pub key: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8443".into(),
            password_hash: String::new(),
            allow_public: false,
            cert: String::new(),
            key: String::new(),
        }
    }
}

/// USB GPS receiver for a moving station. An empty port means no GPS: the
/// grid typed in setup is used.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GpsConfig {
    /// Serial port of the receiver, e.g. `COM5` or `/dev/tty.usbmodem1101`.
    pub port: String,
    /// Most USB receivers send at 9600; older ones at 4800.
    pub baud: u32,
}

impl Default for GpsConfig {
    fn default() -> Self {
        Self {
            port: String::new(),
            baud: 9600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IrcConfig {
    pub bind: String,
}

impl Default for IrcConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:6667".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModemConfig {
    /// modem73 (sound-card modem) | bluetooth (radio with built-in KISS TNC) | serial (KISS TNC on a port)
    pub backend: String,
    pub host: String,
    pub kiss_port: u16,
    pub control_port: u16,
    pub manage: bool,
    pub binary: String,
    /// vox | digirig | cm108 | rigctl | hamlib | bluetooth | tnc | none
    /// (`bluetooth`: modem73 audio and PTT over the radio's Bluetooth audio
    /// channel, see `crate::btaudio`).
    pub ptt: String,
    /// Supported radio picked in setup, by its setup name; empty for the
    /// "Other" setups.
    pub radio: String,
    pub preset: String,
    pub com_port: String,
    pub com_line: String,
    pub vox_lead_ms: u32,
    pub vox_tail_ms: u32,
    pub cm108_gpio: u8,
    pub rigctl: String,
    /// `ptt = "hamlib"`: modem73 talks CAT to the radio itself (no rigctld).
    /// Hamlib model number (`rigctl -l`), e.g. 3087 for the Xiegu X6100.
    pub hamlib_model: u32,
    /// CAT serial port (`COM5`, `/dev/ttyUSB0`).
    pub hamlib_device: String,
    /// CAT baud rate; 0 = the radio's Hamlib default.
    pub hamlib_baud: u32,
    /// Optional capture device name for modem73 (empty = system default).
    pub audio_input: String,
    /// Optional playback device name for modem73 (empty = system default).
    pub audio_output: String,
}

impl Default for ModemConfig {
    fn default() -> Self {
        Self {
            backend: "modem73".into(),
            host: "127.0.0.1".into(),
            kiss_port: 8001,
            control_port: 8073,
            manage: true,
            binary: "modem73".into(),
            ptt: "none".into(),
            radio: String::new(),
            preset: "vhf-fm".into(),
            com_port: String::new(),
            com_line: "rts".into(),
            vox_lead_ms: 900,
            vox_tail_ms: 300,
            cm108_gpio: 3,
            rigctl: "127.0.0.1:4532".into(),
            hamlib_model: 0,
            hamlib_device: String::new(),
            hamlib_baud: 0,
            audio_input: String::new(),
            audio_output: String::new(),
        }
    }
}

impl ModemConfig {
    /// The radio has its own TNC (VR-N76, UV-PRO, GA-5WB, Mobilinkd…): no modem73.
    pub fn uses_tnc(&self) -> bool {
        matches!(
            self.backend.trim().to_ascii_lowercase().as_str(),
            "bluetooth" | "bt" | "serial" | "tnc"
        )
    }

    pub fn is_bluetooth(&self) -> bool {
        matches!(
            self.backend.trim().to_ascii_lowercase().as_str(),
            "bluetooth" | "bt"
        )
    }

    /// VOX keys the radio (or speakers) for every RF burst, including beacons.
    pub fn is_vox(&self) -> bool {
        self.ptt.trim().eq_ignore_ascii_case("vox")
    }

    /// modem73's audio and PTT go over the radio's Bluetooth audio channel
    /// (VR-N76, VR-N7600 and the other Benshi radios), no cable.
    pub fn is_bt_audio(&self) -> bool {
        !self.uses_tnc() && self.ptt.trim().eq_ignore_ascii_case("bluetooth")
    }
}

/// Radio-side KISS TNC (Bluetooth SPP or a serial port).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TncConfig {
    /// Bluetooth device name to look for (VR-N76, UV-PRO, GA-5WB…). Case-insensitive.
    pub bt_name: String,
    /// Bluetooth address `38:D2:00:01:03:49`. Empty = find by name among paired devices.
    pub bt_addr: String,
    /// Serial path for `backend = "serial"`: `COM7`, `/dev/rfcomm0`, `/dev/cu.VR-N76`.
    pub serial: String,
    /// KISS TXDELAY. The VR-N76 clips the first symbols below ~600 ms.
    pub txdelay_ms: u32,
    /// KISS persistence 0–255.
    pub persist: u8,
    /// KISS slot time.
    pub slot_ms: u32,
    /// Wrap frames in an AX.25 UI header so other packet stations see your callsign.
    pub ax25: bool,
    /// AX.25 destination for those UI frames.
    pub ax25_dest: String,
    /// Pause between frames so the radio's one-frame-per-PTT TNC keeps up.
    pub frame_gap_ms: u32,
}

impl Default for TncConfig {
    fn default() -> Self {
        Self {
            bt_name: "VR-N76".into(),
            bt_addr: String::new(),
            serial: String::new(),
            txdelay_ms: 600,
            persist: 63,
            slot_ms: 100,
            ax25: true,
            ax25_dest: "WCR".into(),
            frame_gap_ms: 400,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HubConfig {
    pub url: String,
    pub peers: Vec<String>,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            url: PUBLIC_HUB.into(),
            peers: Vec::new(),
        }
    }
}

impl HubConfig {
    /// Empty URL means do not dial a hub (LAN-only / e2e).
    pub fn enabled(&self) -> bool {
        url_configured(&self.url)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub url: String,
    pub interval_secs: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            url: PUBLIC_TELEMETRY.into(),
            interval_secs: 30,
        }
    }
}

impl TelemetryConfig {
    pub fn enabled(&self) -> bool {
        url_configured(&self.url)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GatewayConfig {
    pub rf_egress: bool,
    /// allow | deny
    pub third_party: String,
    /// Internet-radio only: public map draws a tower and the dial frequency.
    /// On by default: a gateway shows as a tower (Tobias, #220).
    pub tower: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            rf_egress: true,
            third_party: "deny".into(),
            tower: true,
        }
    }
}

impl GatewayConfig {
    pub fn third_party_allow(&self) -> bool {
        self.third_party.eq_ignore_ascii_case("allow")
    }

    /// Tower pin only while this node is an internet-radio gateway. A
    /// gateway run by `wcr web` always shows as one (see [`always_tower`]).
    pub fn show_tower(&self, mode: Mode) -> bool {
        (self.tower || always_tower()) && mode.is_gateway()
    }
}

/// Set by `wcr web` on the station it runs.
pub const ALWAYS_TOWER_ENV: &str = "WCR_ALWAYS_TOWER";

/// A gateway run by `wcr web` always shows as a tower on the map (Tobias,
/// #220). Forced at runtime only, so `[gateway] tower` in wcr.toml is never
/// rewritten (#225) and still counts when the same station runs without
/// `wcr web`.
pub fn always_tower() -> bool {
    std::env::var_os(ALWAYS_TOWER_ENV).is_some_and(|v| v == "1")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    pub path: PathBuf,
    /// Override identity key path (for tests and multi-station e2e). Empty = default.
    #[serde(default)]
    pub identity_key: PathBuf,
    pub max_age_hours: u64,
    pub max_msgs: u64,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            identity_key: PathBuf::new(),
            max_age_hours: 72,
            max_msgs: 10_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupConfig {
    pub history_max_msgs: usize,
    pub history_max_age_hours: u64,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            history_max_msgs: 50,
            history_max_age_hours: 24,
        }
    }
}

pub const DEFAULT_LAN_SERVICE: &str = "_wcr._tcp.local.";
pub const E2E_LAN_SERVICE: &str = "_wcr-e2e._tcp.local.";
pub const E2E_LAN_PORT: u16 = 7375;
pub const E2E_HUB_PORT: u16 = 7376;
pub const E2E_IRC_BIND: &str = "127.0.0.1:16667";
pub const E2E_STATUS_BIND: &str = "127.0.0.1:18074";
pub const E2E_RADIO_KISS_A: u16 = 18001;
pub const E2E_RADIO_CTRL_A: u16 = 18073;
pub const E2E_RADIO_KISS_B: u16 = 18002;
pub const E2E_RADIO_CTRL_B: u16 = 18074;
pub const E2E_RADIO_IRC_A: &str = "127.0.0.1:16668";
pub const E2E_RADIO_IRC_B: &str = "127.0.0.1:16669";
pub const E2E_RADIO_STATUS_A: &str = "127.0.0.1:18075";
pub const E2E_RADIO_STATUS_B: &str = "127.0.0.1:18076";
pub const E2E_RADIO_HUB_PORT: u16 = 18077;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LanConfig {
    pub discovery: bool,
    pub port: u16,
    /// mDNS service type. Override for isolated meshes (e.g. paired e2e).
    pub service: String,
    /// Optional `host:port` advertised in UDP hellos so peers can find a private hub.
    pub hub_advertise: String,
}

impl Default for LanConfig {
    fn default() -> Self {
        Self {
            discovery: true,
            port: 7373,
            service: DEFAULT_LAN_SERVICE.into(),
            hub_advertise: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub unicode: bool,
    pub activity_panel: bool,
    pub bell: bool,
    /// Show a short boot sequence in the TUI (tron theme).
    #[serde(default = "default_true")]
    pub boot: bool,
    /// Show the SNR next to each line received by radio (`/snr on|off`).
    #[serde(default = "default_true")]
    pub show_snr: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "tron".into(),
            unicode: true,
            activity_panel: true,
            bell: false,
            boot: true,
            show_snr: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusConfig {
    pub bind: String,
}

impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8074".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayConfig {
    pub default_ttl: u8,
    pub max_message_bytes: usize,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            default_ttl: 3,
            max_message_bytes: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RfConfig {
    /// Retransmit our own unacked RF messages this many times.
    pub max_retries: u32,
    /// Data shards when erasure-coding a frame that does not fit the PHY MTU.
    pub frag_k: u8,
    /// Parity shards (any k of k+m reconstruct).
    pub frag_m: u8,
    /// Delay before a second copy of an emergency (`!!`) frame.
    pub emergency_dup_ms: u32,
    /// Application-layer busy gate / p-persistence (modem73 CSMA is separate).
    pub csma: bool,
    pub slot_ms: u32,
    pub quiet_ms: u32,
    pub max_defer_ms: u32,
    pub emergency_max_defer_ms: u32,
    /// Extra pause after a TX so ACKs can be heard before we key again.
    pub turnaround_ms: u32,
    /// Occupancy at or above this drops beacons/HAVE and defers relays.
    pub congested_pct: u8,
    /// Random delay before a group ACK so responders do not collide.
    pub ack_dither_ms: u32,
    /// Beacons go every 60 s for the first three minutes, then every 5 min,
    /// each ± this many seconds.
    pub beacon_jitter_s: u32,
    /// Randomise ARQ retry hold (±25%).
    pub retry_jitter: bool,
    /// Our own lines sent this many seconds after start wait until another
    /// station has been heard: a neighbour started at the same time may not
    /// be listening yet. `0` sends them at once.
    pub startup_hold_s: u32,
    /// Unacked outbound older than this is marked `[no]` instead of being
    /// keyed again after a restart. `0` disables the limit.
    pub outbound_max_age_secs: u32,
    /// Dial frequency in kHz (0 = unknown). Manual; rigctl overrides when CAT is up.
    #[serde(default)]
    pub frequency_khz: u32,
    /// Answer a `/report` request (`SNR?`) with the SNR we heard it at.
    /// At most once per callsign every five minutes.
    pub auto_report: bool,
}

impl Default for RfConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            frag_k: 2,
            frag_m: 1,
            emergency_dup_ms: 400,
            csma: true,
            slot_ms: 100,
            quiet_ms: 300,
            max_defer_ms: 15_000,
            emergency_max_defer_ms: 3_000,
            turnaround_ms: 250,
            congested_pct: 60,
            ack_dither_ms: 800,
            beacon_jitter_s: 15,
            retry_jitter: true,
            startup_hold_s: crate::relay::STARTUP_HOLD_S,
            outbound_max_age_secs: 3600,
            frequency_khz: 0,
            auto_report: true,
        }
    }
}

/// Email gateway. The domain is fixed; there is no domain field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MailConfig {
    /// Radio-plus sends RF to this callsign. Empty until set in setup.
    pub gateway: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RigConfig {
    pub enabled: bool,
    pub host: String,
}

impl Default for RigConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1:4532".into(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Keys of `full` that `old` already has or whose value is not the default.
fn sparse(full: &toml::Value, base: &toml::Value, old: &toml::Value) -> toml::Value {
    let (Some(full), Some(base)) = (full.as_table(), base.as_table()) else {
        return full.clone();
    };
    let empty = toml::Table::new();
    let old = old.as_table().unwrap_or(&empty);
    let mut out = toml::Table::new();
    for (key, value) in full {
        let default = base.get(key);
        match (value, old.get(key)) {
            (toml::Value::Table(_), kept) => {
                let none = toml::Value::Table(toml::Table::new());
                let inner = sparse(value, default.unwrap_or(&none), kept.unwrap_or(&none));
                let has_keys = inner.as_table().is_some_and(|t| !t.is_empty());
                if has_keys || kept.is_some() {
                    out.insert(key.clone(), inner);
                }
            }
            (_, Some(_)) => {
                out.insert(key.clone(), value.clone());
            }
            (_, None) if default != Some(value) => {
                out.insert(key.clone(), value.clone());
            }
            _ => {}
        }
    }
    toml::Value::Table(out)
}

/// Write the keys of `wanted` into `doc`, touching only the ones whose value
/// differs from `old` (the file as last read). Comments and order are kept.
fn edit_in_place(
    doc: &mut toml_edit::Table,
    wanted: &toml::Value,
    old: &toml::Value,
) -> Result<()> {
    let Some(wanted) = wanted.as_table() else {
        return Ok(());
    };
    for (key, value) in wanted {
        let before = old.get(key);
        if before == Some(value) {
            continue;
        }
        if let toml::Value::Table(_) = value {
            let none = toml::Value::Table(toml::Table::new());
            if !doc.get(key).is_some_and(|item| item.is_table()) {
                doc.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
            }
            let table = doc[key.as_str()].as_table_mut().expect("just made a table");
            edit_in_place(table, value, before.unwrap_or(&none))?;
            continue;
        }
        let mut one = toml::Table::new();
        one.insert(key.clone(), value.clone());
        let text = toml::to_string(&one).map_err(|e| Error::config(e.to_string()))?;
        let parsed = text
            .parse::<toml_edit::Document>()
            .map_err(|e| Error::config(e.to_string()))?;
        let Some(mut item) = parsed.get(key).cloned() else {
            continue;
        };
        // Replace only the value, so the key's comment lines and the line's
        // trailing comment stay.
        match doc.get_mut(key) {
            Some(slot) => {
                if let (Some(new), Some(prev)) = (item.as_value_mut(), slot.as_value()) {
                    *new.decor_mut() = prev.decor().clone();
                }
                *slot = item;
            }
            None => {
                doc.insert(key, item);
            }
        }
    }
    Ok(())
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut cfg: Config = toml::from_str(&text)?;
        cfg.normalize();
        if !cfg.callsign.is_empty() {
            crate::proto::Callsign::parse(&cfg.callsign)?;
        }
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if !self.callsign.is_empty() {
            crate::proto::Callsign::parse(&self.callsign)?;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let full = toml::Value::try_from(self).map_err(|e| Error::config(e.to_string()))?;
        // An existing file keeps its own keys and only gains the ones that
        // now differ from the default, so a short `-c` file stays short. It is
        // edited in place, so comments and line order stay as written.
        let existing = std::fs::read_to_string(path).ok().and_then(|text| {
            let old = toml::from_str::<toml::Value>(&text).ok()?;
            let doc = text.parse::<toml_edit::Document>().ok()?;
            Some((old, doc))
        });
        let text = match existing {
            Some((old, mut doc)) => {
                let mut base: Config = toml::from_str("")?;
                base.normalize();
                let base =
                    toml::Value::try_from(&base).map_err(|e| Error::config(e.to_string()))?;
                let wanted = sparse(&full, &base, &old);
                edit_in_place(doc.as_table_mut(), &wanted, &old)?;
                doc.to_string()
            }
            None => toml::to_string_pretty(&full).map_err(|e| Error::config(e.to_string()))?,
        };
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn normalize(&mut self) {
        self.callsign = self.callsign.trim().to_ascii_uppercase();
        self.grid = self.grid.trim().to_ascii_uppercase();
        if self.store.path.as_os_str().is_empty() {
            self.store.path = default_data_dir().join("wcr.db");
        }
        if url_configured(&self.hub.url) {
            self.hub.url = crate::net::hub_client::websocket_url(&self.hub.url);
        } else {
            self.hub.url.clear();
        }
        if self.lan.service.trim().is_empty() {
            self.lan.service = DEFAULT_LAN_SERVICE.into();
        }
        self.mail.gateway = self.mail.gateway.trim().to_ascii_uppercase();
        if self.modem.uses_tnc() {
            // The radio's TNC is fixed 1200 bd AFSK; modem73 is not involved.
            self.modem.manage = false;
            self.modem.preset = crate::presets::Preset::Afsk1200.as_str().into();
            if self.modem.ptt == "none" || self.modem.ptt.is_empty() {
                self.modem.ptt = "tnc".into();
            }
        }
        if self.modem.is_bt_audio() {
            // wcr runs modem73 against its own Bluetooth bridge. 8PSK and
            // faster fail over the radio's FM + SBC audio path (#202).
            self.modem.manage = true;
            if !crate::presets::Preset::parse(&self.modem.preset)
                .is_some_and(crate::btaudio::preset_allowed)
            {
                self.modem.preset = crate::presets::Preset::VhfFm.as_str().into();
            }
        }
    }

    pub fn default_path() -> PathBuf {
        default_config_dir().join("wcr.toml")
    }

    /// Remember the file this process loaded (`wcr -c FILE node`), so
    /// settings changed by commands are saved back to it.
    pub fn set_active_path(path: &Path) {
        *ACTIVE_PATH.lock() = Some(path.to_path_buf());
    }

    /// The file commands save to: the one loaded with `-c`, else the default.
    pub fn active_path() -> PathBuf {
        ACTIVE_PATH
            .lock()
            .clone()
            .unwrap_or_else(Self::default_path)
    }

    pub fn key_path() -> PathBuf {
        default_data_dir().join("identity.key")
    }

    pub fn identity_key_path(&self) -> PathBuf {
        if self.store.identity_key.as_os_str().is_empty() {
            Self::key_path()
        } else {
            self.store.identity_key.clone()
        }
    }

    /// True when this station will open a hub WebSocket.
    pub fn dials_hub(&self) -> bool {
        self.mode.uses_internet() && self.hub.enabled()
    }

    /// Seconds our own lines wait after start for another station to be
    /// heard. The hold waits for a neighbour's first beacon, and VOX sends no
    /// beacons, so on VOX it would only keep the speaker silent: no hold.
    pub fn startup_hold_s(&self) -> u32 {
        if self.modem.is_vox() {
            0
        } else {
            self.rf.startup_hold_s
        }
    }

    /// True when this station will POST map telemetry.
    ///
    /// Independent of chat mode: radio-only conversations stay off the hub,
    /// but a configured telemetry URL still updates the live map.
    pub fn reports_telemetry(&self) -> bool {
        self.telemetry.enabled()
    }
}

/// Non-empty after trim. Empty hub/telemetry URLs mean "do not dial".
pub fn url_configured(url: &str) -> bool {
    !url.trim().is_empty()
}

/// Config and data directories under a `WCR_HOME` root.
pub fn dirs_under_home(home: &Path) -> (PathBuf, PathBuf) {
    (home.join("config"), home.join("data"))
}

fn home_from_env() -> Option<PathBuf> {
    std::env::var_os("WCR_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn default_config_dir() -> PathBuf {
    if let Some(home) = home_from_env() {
        return dirs_under_home(&home).0;
    }
    directories::ProjectDirs::from("com", "thaum-labs", "wcr")
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join(".wcr"))
}

pub fn default_data_dir() -> PathBuf {
    if let Some(home) = home_from_env() {
        return dirs_under_home(&home).1;
    }
    directories::ProjectDirs::from("com", "thaum-labs", "wcr")
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join(".wcr"))
}

pub fn ensure_dirs() -> Result<()> {
    std::fs::create_dir_all(default_config_dir())?;
    std::fs::create_dir_all(default_data_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_rejects_invalid_callsign() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wcr.toml");
        std::fs::write(&path, "callsign = \"!!BAD!!\"\n").unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(err.to_string().contains("callsign") || err.to_string().contains("protocol"));
    }

    #[test]
    fn public_map_is_not_the_site_homepage() {
        assert!(PUBLIC_MAP.ends_with("/map.html"));
        assert_ne!(PUBLIC_MAP, "https://weechatradio.com");
        assert_ne!(PUBLIC_MAP, "https://weechatradio.com/");
    }

    #[test]
    fn vox_has_no_startup_hold() {
        // VOX sends no beacons, so nothing would lift the hold early and the
        // speaker stayed silent for two minutes after every start.
        let mut cfg = Config::default();
        assert_eq!(cfg.startup_hold_s(), crate::relay::STARTUP_HOLD_S);
        cfg.modem.ptt = "vox".into();
        assert_eq!(cfg.startup_hold_s(), 0);
    }

    #[test]
    fn save_keeps_a_short_file_short() {
        // QA v0.1.110: one changed setting rewrote a -c file with ~110 lines.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wcr.toml");
        std::fs::write(&path, "callsign = \"Q0RFA\"\n[modem]\nkiss_port = 8101\n").unwrap();
        let mut cfg = Config::load(&path).unwrap();
        cfg.rf.auto_report = false;
        cfg.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.lines().count() < 10, "{text}");
        let back = Config::load(&path).unwrap();
        assert_eq!(back.callsign, "Q0RFA");
        assert_eq!(back.modem.kiss_port, 8101);
        assert!(!back.rf.auto_report);
        assert_eq!(
            toml::to_string(&back).unwrap(),
            toml::to_string(&cfg).unwrap()
        );
        // A new file is still written in full.
        let fresh = dir.path().join("new.toml");
        cfg.save(&fresh).unwrap();
        assert!(std::fs::read_to_string(&fresh).unwrap().lines().count() > 50);
    }

    #[test]
    fn save_keeps_comments_and_order() {
        // QA v0.1.111: a hand comment vanished and keys were sorted A-Z.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wcr.toml");
        let text = "# my station\ncallsign = \"Q0RFA\"\n\n[ui]\n# quiet\nshow_snr = false\n\n[modem]\nkiss_port = 8101 # B side\n";
        std::fs::write(&path, text).unwrap();
        let mut cfg = Config::load(&path).unwrap();
        cfg.ui.show_snr = true;
        cfg.rf.auto_report = false;
        cfg.save(&path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        let expected = text.replace("show_snr = false", "show_snr = true");
        assert!(saved.starts_with(&expected), "{saved}");
        assert!(
            saved[expected.len()..].contains("auto_report = false"),
            "{saved}"
        );
        let back = Config::load(&path).unwrap();
        assert!(back.ui.show_snr);
        assert!(!back.rf.auto_report);
        assert_eq!(back.modem.kiss_port, 8101);
        // Saving again with nothing changed leaves the file as it is.
        back.save(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    }

    #[test]
    fn roundtrip_toml() {
        let cfg = Config::default();
        let s = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back.mode, Mode::InternetRadio);
        assert_eq!(back.hub.url, PUBLIC_HUB);
        assert!(!back.modem.is_vox());
        let mut vox = Config::default();
        vox.modem.ptt = "vox".into();
        assert!(vox.modem.is_vox());
        assert_eq!(back.modem.backend, "modem73");
        assert_eq!(back.modem.audio_input, "");
        assert_eq!(back.modem.audio_output, "");
        assert_eq!(back.tnc.bt_name, "VR-N76");
        assert!(back.gateway.tower);
    }

    #[test]
    fn gateway_tower_defaults_on_when_missing() {
        let cfg: Config =
            toml::from_str("callsign = \"G4ABC\"\n[gateway]\nrf_egress = true\n").unwrap();
        assert!(cfg.gateway.tower);
        assert!(cfg.gateway.show_tower(Mode::InternetRadio));
        let mut off = cfg.clone();
        off.gateway.tower = false;
        assert!(!off.gateway.show_tower(Mode::InternetRadio));
        let on = cfg;
        assert!(on.gateway.show_tower(Mode::InternetRadio));
        assert!(!on.gateway.show_tower(Mode::Radio));
        assert!(!on.gateway.show_tower(Mode::Internet));
        assert!(!on.gateway.show_tower(Mode::RadioPlus));
    }

    #[test]
    fn tnc_backend_forces_afsk_and_no_modem73() {
        let mut cfg = Config::default();
        cfg.modem.backend = "bluetooth".into();
        cfg.modem.manage = true;
        cfg.modem.preset = "hf-good".into();
        cfg.normalize();
        assert!(cfg.modem.uses_tnc());
        assert!(cfg.modem.is_bluetooth());
        assert!(!cfg.modem.manage);
        assert_eq!(cfg.modem.preset, "afsk-1200");
        assert_eq!(cfg.modem.ptt, "tnc");
    }

    #[test]
    fn dirs_under_home_split_config_and_data() {
        let home = PathBuf::from("/tmp/wcr-e2e-home");
        let (cfg, data) = dirs_under_home(&home);
        assert_eq!(cfg, home.join("config"));
        assert_eq!(data, home.join("data"));
    }

    #[test]
    fn empty_hub_url_does_not_dial() {
        let mut cfg = Config {
            mode: Mode::Internet,
            hub: HubConfig {
                url: String::new(),
                ..Default::default()
            },
            telemetry: TelemetryConfig {
                url: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.normalize();
        assert!(!url_configured(&cfg.hub.url));
        assert!(!cfg.hub.enabled());
        assert!(!cfg.dials_hub());
        assert!(!cfg.reports_telemetry());
        cfg.hub.url = PUBLIC_HUB.into();
        assert!(cfg.dials_hub());
    }

    #[test]
    fn radio_reports_map_without_dialing_hub_chat() {
        let mut cfg = Config {
            mode: Mode::Radio,
            ..Default::default()
        };
        cfg.normalize();
        assert!(!cfg.dials_hub());
        assert!(cfg.reports_telemetry());
        cfg.telemetry.url.clear();
        assert!(!cfg.reports_telemetry());
    }

    #[test]
    fn wcr_home_redirects_config_and_data() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = LOCK.lock().unwrap();
        let prev = std::env::var_os("WCR_HOME");
        let root = std::env::temp_dir().join(format!("wcr-home-test-{}", std::process::id()));
        std::env::set_var("WCR_HOME", &root);
        let cfg = default_config_dir();
        let data = default_data_dir();
        let key = Config::key_path();
        match prev {
            Some(v) => std::env::set_var("WCR_HOME", v),
            None => std::env::remove_var("WCR_HOME"),
        }
        assert_eq!(cfg, root.join("config"));
        assert_eq!(data, root.join("data"));
        assert_eq!(key, root.join("data").join("identity.key"));
    }
}
