//! SPDX-License-Identifier: Apache-2.0
//! In-terminal docs: `wcr help <topic>`.

pub fn topics() -> &'static [(&'static str, &'static str)] {
    &[
        ("setup", include_str!("../../../docs/QUICKSTART.md")),
        ("radio", include_str!("../../../docs/RADIO-SETUP.md")),
        ("vr-n76", include_str!("../../../docs/VR-N76.md")),
        ("windows", include_str!("../../../docs/SETUP-windows.md")),
        ("macos", include_str!("../../../docs/SETUP-macos.md")),
        ("linux", include_str!("../../../docs/SETUP-linux.md")),
        ("modes", include_str!("../../../docs/MODES.md")),
        ("protocol", include_str!("../../../docs/PROTOCOL.md")),
        ("api", include_str!("../../../docs/API.md")),
        ("style", include_str!("../../../docs/STYLE.md")),
        ("calling", include_str!("../../../docs/CALLING.md")),
        ("weak", include_str!("../../../docs/WEAK.md")),
        ("bridging", include_str!("../../../docs/BRIDGING.md")),
    ]
}

pub fn render(topic: Option<&str>) -> String {
    match topic {
        None | Some("help") | Some("") => {
            let list: Vec<String> = topics()
                .iter()
                .map(|(k, _)| format!("  wcr help {k}"))
                .collect();
            format!(
                "{}\n\nTopics:\n{}\n",
                crate::ui_style::panel_line("WEECHAT RADIO", "HELP"),
                list.join("\n")
            )
        }
        Some(t) => topics()
            .iter()
            .find(|(k, _)| *k == t)
            .map(|(_, body)| body.to_string())
            .unwrap_or_else(|| format!("unknown topic '{t}'. Try `wcr help`.")),
    }
}
