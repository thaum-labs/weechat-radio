//! SPDX-License-Identifier: Apache-2.0
//! WeeChat Radio library.

pub mod air;
pub mod audio_meter;
pub mod band;
pub mod cli;
pub mod config;
pub mod discover;
pub mod e2e;
pub mod emcomm;
pub mod error;
pub mod grid;
#[cfg(feature = "desktop")]
pub mod gui;
pub mod help;
pub mod ircd;
pub mod mail;
pub mod modem;
pub mod modes;
pub mod net;
pub mod node;
pub mod presets;
pub mod proto;
pub mod rate_limit;
pub mod relay;
pub mod service;
pub mod setup;
pub mod sim;
pub mod slash;
pub mod status;
pub mod store;
pub mod telemetry;
pub mod tnc;
pub mod tui;
pub mod ui_style;
pub mod update;
pub mod weechat_app;

pub use error::{Error, Result};
pub use modes::Mode;
