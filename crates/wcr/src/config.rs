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
    pub host: String,
    pub kiss_port: u16,
    pub control_port: u16,
    pub manage: bool,
    pub binary: String,
    /// vox | digirig | cm108 | rigctl | none
    pub ptt: String,
    pub preset: String,
    pub com_port: String,
    pub com_line: String,
    pub vox_lead_ms: u32,
    pub vox_tail_ms: u32,
    pub cm108_gpio: u8,
    pub rigctl: String,
}

impl Default for ModemConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            kiss_port: 8001,
            control_port: 8073,
            manage: true,
            binary: "modem73".into(),
            ptt: "none".into(),
            preset: "vhf-fm".into(),
            com_port: String::new(),
            com_line: "rts".into(),
            vox_lead_ms: 500,
            vox_tail_ms: 150,
            cm108_gpio: 3,
            rigctl: "127.0.0.1:4532".into(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LanConfig {
    pub discovery: bool,
    pub port: u16,
}

impl Default for LanConfig {
    fn default() -> Self {
        Self {
            discovery: true,
            port: 7373,
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
    /// Data shards when erasure-coding a group / oversized frame.
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
        self.hub.url = crate::net::hub_client::websocket_url(&self.hub.url);
    }

    pub fn default_path() -> PathBuf {
        default_config_dir().join("wcr.toml")
    }

    pub fn key_path() -> PathBuf {
        default_data_dir().join("identity.key")
    }
}

pub fn default_config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "thaum-labs", "wcr")
        .map(|p| p.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join(".wcr"))
}

pub fn default_data_dir() -> PathBuf {
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
    }
}
