//! SPDX-License-Identifier: Apache-2.0
//! Compact binary envelope used on radio (KISS) and the internet.

pub mod callsign;
pub mod envelope;
pub mod flags;
pub mod ids;
pub mod sign;

pub use callsign::{is_plausible_callsign, Callsign};
pub use envelope::{now_ts, Envelope, MsgType, HEADER_LEN, MAX_BODY, VERSION};
pub use flags::{
    Flags, Priority, FLAG_GROUP, FLAG_INET_OK, FLAG_NO_INET, FLAG_REQ_ACK, FLAG_SIGNED,
    FLAG_THIRD_PARTY,
};
pub use ids::MsgId;
pub use sign::{load_or_create, verify_envelope, IdentityKeys};
