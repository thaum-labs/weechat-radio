//! SPDX-License-Identifier: Apache-2.0
//! Error types for WeeChat Radio.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),

    #[error("config: {0}")]
    Config(String),

    #[error("protocol: {0}")]
    Protocol(String),

    #[error("store: {0}")]
    Store(String),

    #[error("modem: {0}")]
    Modem(String),

    #[error("network: {0}")]
    Net(String),

    #[error("identity: {0}")]
    Identity(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl From<dialoguer::Error> for Error {
    fn from(e: dialoguer::Error) -> Self {
        Self::Msg(e.to_string())
    }
}

impl Error {
    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn identity(msg: impl Into<String>) -> Self {
        Self::Identity(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
