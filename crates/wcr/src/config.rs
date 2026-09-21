//! SPDX-License-Identifier: Apache-2.0
//! Node configuration (`wcr.toml`).

use crate::error::{Error, Result};
use crate::modes::Mode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PUBLIC_HUB: &str = "wss://hub.weechatradio.com/ws";
pub const PUBLIC_TELEMETRY: &str = "https://hub.weechatradio.com/api/v1/report";

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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MailConfig {
    /// Verified subdomain for WCR addresses (`mail.weechatradio.com`).
    pub domain: String,
    /// RF mail dest: callsign of an `internet-radio` gateway on your dial (required for Email).
    pub gateway: String,
}

impl Default for MailConfig {
    fn default() -> Self {
        Self {
            domain: "mail.weechatradio.com".into(),
            gateway: String::new(),
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
    /// vox | digirig | cm108 | rigctl | tnc | none
    pub ptt: String,
    pub preset: String,
    pub com_port: String,
    pub com_line: String,
    pub vox_lead_ms: u32,
    pub vox_tail_ms: u32,
    pub cm108_gpio: u8,
    pub rigctl: String,
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
            preset: "vhf-fm".into(),
            com_port: String::new(),
            com_line: "rts".into(),
            vox_lead_ms: 900,
            vox_tail_ms: 300,
            cm108_gpio: 3,
            rigctl: "127.0.0.1:4532".into(),
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
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            rf_egress: true,
            third_party: "deny".into(),
        }
    }
}

impl GatewayConfig {
    pub fn third_party_allow(&self) -> bool {
        self.third_party.eq_ignore_ascii_case("allow")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    pub path: PathBuf,
    pub max_age_hours: u64,
    pub max_msgs: u64,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
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
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "tron".into(),
            unicode: true,
            activity_panel: true,
            bell: false,
            boot: true,
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
    /// Beacon interval is 60 s ± this many seconds.
    pub beacon_jitter_s: u32,
    /// Randomise ARQ retry hold (±25%).
    pub retry_jitter: bool,
    /// Dial frequency in kHz (0 = unknown). Manual; rigctl overrides when CAT is up.
    #[serde(default)]
    pub frequency_khz: u32,
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
            frequency_khz: 0,
        }
    }
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

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut cfg: Config = toml::from_str(&text)?;
        cfg.normalize();
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| Error::config(e.to_string()))?;
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
        if self.modem.uses_tnc() {
            // The radio's TNC is fixed 1200 bd AFSK; modem73 is not involved.
            self.modem.manage = false;
            self.modem.preset = crate::presets::Preset::Afsk1200.as_str().into();
            if self.modem.ptt == "none" || self.modem.ptt.is_empty() {
                self.modem.ptt = "tnc".into();
            }
        }
    }

    pub fn default_path() -> PathBuf {
        default_config_dir().join("wcr.toml")
    }

    pub fn key_path() -> PathBuf {
        default_data_dir().join("identity.key")
    }

    /// True when this station will open a hub WebSocket.
    pub fn dials_hub(&self) -> bool {
        self.mode.uses_internet() && self.hub.enabled()
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
