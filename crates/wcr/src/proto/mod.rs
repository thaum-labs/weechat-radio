//! SPDX-License-Identifier: Apache-2.0
//! Compact binary envelope used on radio (KISS) and the internet.

pub mod callsign;
pub mod compress;
pub mod envelope;
pub mod flags;
pub mod frag;
pub mod ids;
pub mod sign;

pub use callsign::{is_plausible_callsign, Callsign};
pub use envelope::{
    clock_warn_after, now_ts, split_body_chunks, Envelope, MsgType, CLOCK_SKEW_SECS, HEADER_LEN,
    MAX_BODY, VERSION, VERSION_V1,
};
pub use flags::{
    Flags, Priority, FLAG_COMPRESSED, FLAG_GROUP, FLAG_GROUP_IDX, FLAG_INET_OK, FLAG_NO_INET,
    FLAG_REQ_ACK, FLAG_SIGNED, FLAG_THIRD_PARTY,
};
pub use ids::MsgId;
pub use sign::{load_or_create, verify_envelope, IdentityKeys};
