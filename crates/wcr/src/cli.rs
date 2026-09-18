//! SPDX-License-Identifier: Apache-2.0
//! Command-line interface.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "wcr",
    version,
    about = "WeeChat Radio — chat over internet and HF/VHF radio",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
    /// Config file (default: platform config dir / wcr.toml)
    #[arg(global = true, short, long)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// First-run wizard
    Setup,
    /// Run the station daemon
    Node,
    /// Built-in terminal chat (starts the node if needed)
    Tui,
    /// Run the public hub + telemetry API
    Hub {
        /// Bind address
        #[arg(long, default_value = "0.0.0.0:7373")]
        bind: String,
        #[command(subcommand)]
        admin: Option<HubAdmin>,
    },
    /// Background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Check or apply an update
    Update,
    /// Offline help
    Help { topic: Option<String> },
    /// Desktop window: setup, station, and chat
    Gui,
    /// Launch the real WeeChat client (after `wcr node`)
    Weechat {
        /// Write the local radio server and load radio.py, then exit
        #[arg(long)]
        configure: bool,
    },
    /// Radios with a built-in KISS TNC (VR-N76, UV-PRO, GA-5WB) over Bluetooth
    Tnc {
        #[command(subcommand)]
        action: TncAction,
    },
    /// Isolated paired tests (no public hub, no user config)
    E2e {
        #[command(subcommand)]
        kind: E2eCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum TncAction {
    /// List paired Bluetooth devices; add --inquiry to scan for new ones
    Scan {
        #[arg(long)]
        inquiry: bool,
    },
    /// Find a VR-N76 / UV-PRO / GA-5WB, pair it if needed, and save it to wcr.toml
    Find {
        /// Bluetooth name to match (default: any known radio)
        #[arg(long, default_value = "")]
        name: String,
    },
    /// Connect to the configured radio and print decoded KISS frames for a while
    Test {
        /// Seconds to listen
        #[arg(long, default_value_t = 20)]
        seconds: u64,
        /// Also transmit one short test frame (identifies with your callsign)
        #[arg(long)]
        tx: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ServiceAction {
    Install,
    Uninstall,
    Status,
}

#[derive(Subcommand, Debug)]
pub enum HubAdmin {
    Admin {
        #[command(subcommand)]
        cmd: AdminCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum AdminCmd {
    Callsign {
        #[command(subcommand)]
        cmd: CallsignAdmin,
    },
}

#[derive(Subcommand, Debug)]
pub enum CallsignAdmin {
    Release { call: String },
    Reassign { call: String, pubkey_hex: String },
}

#[derive(Subcommand, Debug)]
pub enum E2eCmd {
    /// Two machines on the same LAN: #bulletin plus a private hub/map
    Lan {
        /// Seconds to wait for the other station
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// LAN TCP/UDP port (keep off 7373 so a live station can stay up)
        #[arg(long, default_value_t = 7375)]
        port: u16,
        /// Private hub listen port (keep off 7373)
        #[arg(long, default_value_t = 7376)]
        hub_port: u16,
    },
}
