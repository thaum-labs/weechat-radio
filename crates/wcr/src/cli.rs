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
    /// Launch the real WeeChat client (after `wcr node`)
    Weechat {
        /// Write the local radio server and load radio.py, then exit
        #[arg(long)]
        configure: bool,
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
