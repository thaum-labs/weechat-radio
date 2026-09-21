//! SPDX-License-Identifier: Apache-2.0
//! When a gateway may hand mail to the hub. Chat's third-party flag does not apply.

use crate::modes::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendPath {
    /// Pure radio cannot reach Resend.
    Blocked,
    /// Radio-plus always keys to the named gateway. No hub shortcut.
    Rf,
    /// Internet and internet-radio post when the hub is up.
    Hub,
}

pub fn local_send_path(mode: Mode) -> SendPath {
    match mode {
        Mode::Radio => SendPath::Blocked,
        Mode::RadioPlus => SendPath::Rf,
        Mode::Internet | Mode::InternetRadio => SendPath::Hub,
    }
}

/// Inbound RF mail at an internet-radio gateway may be posted even when
/// `[gateway] third_party = deny`. That flag is chat-only.
pub fn gateway_may_post(mode: Mode, third_party_allow: bool) -> bool {
    let _ = third_party_allow;
    matches!(mode, Mode::InternetRadio)
}

/// VOX mail is Sent only after Resend has accepted it. A hub error is not Sent.
pub fn resend_accepted(http_ok: bool) -> bool {
    http_ok
}
