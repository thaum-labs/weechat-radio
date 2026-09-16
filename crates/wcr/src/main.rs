//! SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use tracing_subscriber::EnvFilter;
use wcr::cli::{Cli, Command, HubAdmin, ServiceAction};
use wcr::config::Config;
use wcr::error::Result;

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init();
    if let Err(e) = real_main().await {
        eprintln!("{} {e}", wcr::ui_style::err().apply_to("ERR"));
        std::process::exit(1);
    }
}

async fn real_main() -> Result<()> {
    let cli = Cli::parse();
    let cfg_path = cli.config.clone().unwrap_or_else(Config::default_path);
    match cli.cmd {
        Command::Setup => {
            wcr::setup::run_wizard()?;
        }
        Command::Node => {
            let cfg = load_cfg(&cfg_path)?;
            wcr::node::run_node(cfg, false).await?;
        }
        Command::Tui => {
            let cfg = load_cfg(&cfg_path)?;
            wcr::tui::run(&cfg).await?;
        }
        Command::Hub { bind, admin } => {
            if let Some(HubAdmin::Admin { cmd }) = admin {
                match cmd {
                    wcr::cli::AdminCmd::Callsign { cmd } => match cmd {
                        wcr::cli::CallsignAdmin::Release { call } => {
                            let db = wcr::telemetry::TelemetryDb::open(&data_telemetry())?;
                            db.release_callsign(&call)?;
                            println!("released {call}");
                        }
                        wcr::cli::CallsignAdmin::Reassign { call, pubkey_hex } => {
                            let db = wcr::telemetry::TelemetryDb::open(&data_telemetry())?;
                            db.release_callsign(&call)?;
                            let pk = hex::decode(&pubkey_hex)
                                .map_err(|e| wcr::Error::Msg(e.to_string()))?;
                            if pk.len() != 32 {
                                return Err(wcr::Error::Msg(
                                    "public key must be 32 bytes hex".into(),
                                ));
                            }
                            let mut a = [0u8; 32];
                            a.copy_from_slice(&pk);
                            db.bind_callsign(&call, &a)?;
                            println!("reassigned {call}");
                        }
                    },
                }
                return Ok(());
            }
            let store = std::sync::Arc::new(wcr::store::Store::open(
                &wcr::config::default_data_dir().join("hub.db"),
                72,
                50_000,
            )?);
            let tel = std::sync::Arc::new(wcr::telemetry::TelemetryDb::open(&data_telemetry())?);
            let keys = wcr::proto::load_or_create(&Config::key_path())?;
            wcr::net::run_hub(&bind, store, tel, keys).await?;
        }
        Command::Service { action } => {
            wcr::ui_style::panel("WEECHAT RADIO", "SERVICE");
            match action {
                ServiceAction::Install => println!("{}", wcr::service::install()?),
                ServiceAction::Uninstall => println!("{}", wcr::service::uninstall()?),
                ServiceAction::Status => println!("{}", wcr::service::status()?),
            }
        }
        Command::Update => {
            wcr::ui_style::panel("WEECHAT RADIO", "UPDATE");
            println!("{}", wcr::update::apply().await?);
        }
        Command::Help { topic } => {
            print!("{}", wcr::help::render(topic.as_deref()));
        }
        Command::Weechat { configure } => {
            wcr::ui_style::panel("WEECHAT RADIO", "WEECHAT");
            wcr::weechat_app::run(configure)?;
        }
    }
    Ok(())
}

fn load_cfg(path: &std::path::Path) -> Result<Config> {
    if !path.exists() {
        return Err(wcr::Error::config(format!(
            "no config at {}. Run `wcr setup` first.",
            path.display()
        )));
    }
    Config::load(path)
}

fn data_telemetry() -> std::path::PathBuf {
    wcr::config::default_data_dir().join("telemetry.db")
}
