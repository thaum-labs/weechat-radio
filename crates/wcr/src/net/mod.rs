//! SPDX-License-Identifier: Apache-2.0
//! Internet leg: hub WebSocket, direct peers, mDNS LAN.

pub mod hub_client;
pub mod hub_server;
pub mod lan;

pub use hub_client::HubClient;
pub use hub_server::run_hub;
pub use lan::LanMesh;
