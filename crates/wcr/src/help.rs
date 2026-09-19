//! SPDX-License-Identifier: Apache-2.0
//! In-terminal docs: `wcr help <topic>`.

/// `(name, one-line index blurb, markdown body)`.
pub fn topics() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "setup",
            "5-minute start",
            include_str!("../../../docs/QUICKSTART.md"),
        ),
        (
            "e2e",
            "LAN hub/map test, or simulated RF on one PC",
            include_str!("../../../docs/E2E.md"),
        ),
        (
            "radio",
            "Digirig, VOX, HF CAT, KISS TNC",
            include_str!("../../../docs/RADIO-SETUP.md"),
        ),
        (
            "vr-n76",
            "Bluetooth handheld TNC",
            include_str!("../../../docs/VR-N76.md"),
        ),
        (
            "windows",
            "Windows install",
            include_str!("../../../docs/SETUP-windows.md"),
        ),
        (
            "macos",
            "macOS install",
            include_str!("../../../docs/SETUP-macos.md"),
        ),
        (
            "linux",
            "Linux install",
            include_str!("../../../docs/SETUP-linux.md"),
        ),
        (
            "modes",
            "internet / radio / radio-plus",
            include_str!("../../../docs/MODES.md"),
        ),
        (
            "protocol",
            "on-air and hub frames",
            include_str!("../../../docs/PROTOCOL.md"),
        ),
        (
            "api",
            "public hub HTTP API",
            include_str!("../../../docs/API.md"),
        ),
        (
            "style",
            "how the guides are written",
            include_str!("../../../docs/STYLE.md"),
        ),
        (
            "calling",
            "suggested frequencies",
            include_str!("../../../docs/CALLING.md"),
        ),
        (
            "weak",
            "HF retries and presets",
            include_str!("../../../docs/WEAK.md"),
        ),
        (
            "compare",
            "vs JS8, FT8, Olivia, APRS",
            include_str!("../../../docs/COMPARE.md"),
        ),
        (
            "bridging",
            "cross-frequency via a gateway",
            include_str!("../../../docs/BRIDGING.md"),
        ),
    ]
}

fn canonical_topic(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "test" | "lan" | "e2e-lan" | "e2e-radio" | "sim" => "e2e".into(),
        other => other.to_string(),
    }
}

pub fn render(topic: Option<&str>) -> String {
    match topic {
        None | Some("help") | Some("") => {
            let width = topics().iter().map(|(k, _, _)| k.len()).max().unwrap_or(8);
            let list: Vec<String> = topics()
                .iter()
                .map(|(k, blurb, _)| format!("  wcr help {k:<width$}  {blurb}"))
                .collect();
            format!(
                "{}\n\nTopics:\n{}\n\nAlso: wcr help test  (same as wcr help e2e)\n",
                crate::ui_style::panel_line("WEECHAT RADIO", "HELP"),
                list.join("\n")
            )
        }
        Some(t) => {
            let key = canonical_topic(t);
            topics()
                .iter()
                .find(|(k, _, _)| *k == key.as_str())
                .map(|(_, _, body)| body.to_string())
                .unwrap_or_else(|| format!("unknown topic '{t}'. Try `wcr help`."))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_lists_e2e_with_blurb() {
        let idx = render(None);
        assert!(idx.contains("wcr help e2e"));
        assert!(idx.contains("simulated RF") || idx.contains("LAN hub"));
        assert!(idx.contains("wcr help test"));
    }

    #[test]
    fn e2e_aliases_load_the_guide() {
        let body = render(Some("e2e"));
        assert!(body.contains("wcr e2e lan"));
        assert!(body.contains("wcr e2e radio"));
        assert!(body.contains("7375"));
        assert!(body.contains("starts the map page"));
        assert!(body.contains("18077"));
        assert_eq!(render(Some("test")), body);
        assert_eq!(render(Some("lan")), body);
        assert_eq!(render(Some("sim")), body);
        assert!(render(Some("nope")).contains("unknown topic"));
    }
}
