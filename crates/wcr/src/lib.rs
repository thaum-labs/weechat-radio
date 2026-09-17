//! SPDX-License-Identifier: Apache-2.0
//! WeeChat Radio library.

pub mod cli;
pub mod config;
pub mod emcomm;
pub mod error;
pub mod grid;
#[cfg(feature = "desktop")]
pub mod gui;
pub mod help;
pub mod ircd;
pub mod modem;
pub mod modes;
pub mod net;
pub mod node;
pub mod presets;
pub mod proto;
pub mod relay;
pub mod service;
pub mod setup;
pub mod sim;
pub mod slash;
pub mod status;
pub mod store;
pub mod telemetry;
pub mod tui;
pub mod ui_style;
pub mod update;
pub mod weechat_app;

pub use error::{Error, Result};
pub use modes::Mode;
