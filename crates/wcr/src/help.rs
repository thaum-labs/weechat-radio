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
            "internet / internet-radio / radio / radio-plus",
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
        (
            "hops",
            "relay on one dial when not everyone is in range",
            include_str!("../../../docs/HOPS.md"),
        ),
        (
            "email",
            "mail beside live chat",
            include_str!("../../../docs/EMAIL.md"),
        ),
        (
            "deploy",
            "run the public hub",
            include_str!("../../../docs/DEPLOY.md"),
        ),
        (
            "try",
            "two computers, speaker to radio",
            include_str!("../../../docs/TRY.md"),
        ),
    ]
}

fn canonical_topic(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "test" | "lan" | "e2e-lan" | "e2e-radio" | "sim" => "e2e".into(),
        other => other.to_string(),
    }
}

pub fn topic_found(topic: Option<&str>) -> bool {
    match topic {
        None | Some("help") | Some("") => true,
        Some(t) => {
            let key = canonical_topic(t);
            topics().iter().any(|(k, _, _)| *k == key.as_str())
        }
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
                "{}\n\nTopics:\n{}\n\nAlso: wcr help test  (same as wcr help e2e)\n\n\
                 Commands:\n\
                 \x20 wcr tnc scan                 paired Bluetooth devices\n\
                 \x20 wcr tnc scan --inquiry       look for new radios (~8 s; put the radio in pairing mode)\n\
                 \x20 wcr hub admin callsign release CALL\n\
                 \x20 wcr hub admin callsign reassign CALL PUBKEY_HEX\n\
                 \x20 wcr service install|uninstall|status\n",
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
        assert!(idx.contains("internet-radio"));
        assert!(idx.contains("tnc scan --inquiry"));
        assert!(idx.contains("hub admin callsign release"));
        assert!(idx.contains("wcr service install"));
        assert!(idx.contains("wcr help email"));
        assert!(idx.contains("wcr help deploy"));
        assert!(idx.contains("wcr help try"));
        assert!(render(Some("email")).contains("mail.weechatradio.com"));
        assert!(render(Some("try")).contains("hf-poor"));
        assert!(!topic_found(Some("nope")));
        assert!(topic_found(Some("modes")));
        assert!(idx.contains("simulated RF") || idx.contains("LAN hub"));
        assert!(idx.contains("wcr help test"));
    }

    #[test]
    fn windows_install_needs_no_administrator() {
        let body = render(Some("windows"));
        let step1 = body.split("\n2. ").next().unwrap_or("");
        assert!(step1.contains("install.ps1"), "{step1}");
        assert!(!step1.to_lowercase().contains("administrator"), "{step1}");
        // Admin is only for the service.
        let rest = &body[step1.len()..];
        assert!(rest.contains("Administrator") && rest.contains("wcr service install"));
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

    #[test]
    fn hops_topic_covers_20m_and_2m() {
        let idx = render(None);
        assert!(idx.contains("wcr help hops"));
        let body = render(Some("hops"));
        assert!(body.contains("14.070"));
        assert!(body.contains("144.950"));
        assert!(body.contains("Bucharest"));
        assert!(body.contains("Boryspil"));
        assert!(body.contains("3 hops"));
        assert!(body.contains("Kyiv"));
        assert!(body.contains("internet-radio"));
        assert!(body.contains("Bucharest reads the line from the hub"));
    }

    /// Numbered steps in every guide run 1, 2, 3 with no repeats or gaps.
    /// A heading or a step 1 starts a new list.
    #[test]
    fn guide_steps_are_numbered_in_order() {
        for (name, _, body) in topics() {
            let mut next = 1u32;
            for line in body.lines() {
                if line.starts_with('#') {
                    next = 1;
                    continue;
                }
                let Some((num, _)) = line.split_once(". ") else {
                    continue;
                };
                let Ok(n) = num.parse::<u32>() else {
                    continue;
                };
                if n != 1 {
                    assert_eq!(
                        n, next,
                        "wcr help {name}: step {n} where {next} was due: {line}"
                    );
                }
                next = n + 1;
            }
        }
    }
}
