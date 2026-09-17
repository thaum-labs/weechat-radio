//! SPDX-License-Identifier: Apache-2.0
//! Map kHz to an amateur or licence-free band name.

/// Band name for a frequency in kHz, if it falls in a known allocation.
pub fn band_for_khz(khz: u32) -> Option<&'static str> {
    if khz == 0 {
        return None;
    }
    // Licence-free first where they sit inside or beside amateur allocations.
    if (26965..=27405).contains(&khz) {
        return Some("CB");
    }
    if (151800..=151950).contains(&khz) || (154550..=154650).contains(&khz) {
        return Some("MURS");
    }
    if (446000..=446200).contains(&khz) {
        return Some("PMR446");
    }
    if (462550..=462725).contains(&khz) || (467550..=467725).contains(&khz) {
        return Some("FRS");
    }
    Some(match khz {
        1800..=2000 => "160m",
        3500..=4000 => "80m",
        5250..=5450 => "60m",
        7000..=7300 => "40m",
        10100..=10150 => "30m",
        14000..=14350 => "20m",
        18068..=18168 => "17m",
        21000..=21450 => "15m",
        24890..=24990 => "12m",
        28000..=29700 => "10m",
        50000..=54000 => "6m",
        70000..=70500 => "4m",
        144000..=148000 => "2m",
        222000..=225000 => "1.25m",
        420000..=450000 => "70cm",
        1240000..=1300000 => "23cm",
        _ => return None,
    })
}

/// `"inet"` when there is no RF frequency, otherwise the band name or `"?"`.
pub fn band_label(khz: u32) -> String {
    if khz == 0 {
        return "inet".into();
    }
    band_for_khz(khz).unwrap_or("?").into()
}

/// Parse an operator-typed MHz string (`144.950`, `7.045 MHz`) into kHz.
pub fn parse_mhz(s: &str) -> Option<u32> {
    let t = s
        .trim()
        .trim_end_matches(|c: char| {
            c.is_ascii_whitespace() || matches!(c, 'm' | 'M' | 'h' | 'H' | 'z' | 'Z')
        })
        .trim();
    if t.is_empty() {
        return None;
    }
    let mhz: f64 = t.parse().ok()?;
    if !(0.1..=10_000.0).contains(&mhz) {
        return None;
    }
    Some((mhz * 1000.0).round() as u32)
}

/// Format kHz as MHz with three decimal places (`144.950`, `7.045`).
pub fn fmt_mhz(khz: u32) -> String {
    format!("{:.3}", khz as f64 / 1000.0)
}

/// `144.950 MHz (2m)` or `unknown`.
pub fn describe(khz: u32) -> String {
    if khz == 0 {
        return "unknown".into();
    }
    match band_for_khz(khz) {
        Some(b) => format!("{} MHz ({b})", fmt_mhz(khz)),
        None => format!("{} MHz", fmt_mhz(khz)),
    }
}

/// Parse hamlib `f` / `rigctl` text (Hz, sometimes labelled) into kHz.
pub fn parse_rigctl_hz(s: &str) -> Option<u32> {
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else if !num.is_empty() {
            break;
        }
    }
    if num.is_empty() {
        return None;
    }
    let n: u64 = num.parse().ok()?;
    let khz = if n >= 1_000_000 { n / 1000 } else { n };
    if khz == 0 || khz > 10_000_000 {
        return None;
    }
    Some(khz as u32)
}

/// kHz from a beacon body `B|<mode>|<khz>` (legacy `B|<mode>` returns None).
pub fn beacon_khz(body: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(body).ok()?.trim();
    let mut parts = text.split('|');
    let head = parts.next()?;
    if !head.eq_ignore_ascii_case("B") {
        return None;
    }
    let _mode = parts.next()?;
    let khz = parts.next()?.parse::<u32>().ok()?;
    if khz == 0 {
        None
    } else {
        Some(khz)
    }
}

/// Distinct band labels for hub `to_bands` (empty freq → `inet`).
pub fn to_bands(freq_khzs: impl IntoIterator<Item = u32>) -> Vec<String> {
    let mut out = Vec::new();
    for khz in freq_khzs {
        let label = band_label(khz);
        if !out.iter().any(|b| b == &label) {
            out.push(label);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amateur_edges() {
        assert_eq!(band_for_khz(144000), Some("2m"));
        assert_eq!(band_for_khz(148000), Some("2m"));
        assert_eq!(band_for_khz(143999), None);
        assert_eq!(band_for_khz(7045), Some("40m"));
        assert_eq!(band_for_khz(7000), Some("40m"));
        assert_eq!(band_for_khz(7300), Some("40m"));
        assert_eq!(band_for_khz(1800), Some("160m"));
        assert_eq!(band_for_khz(14000), Some("20m"));
        assert_eq!(band_for_khz(50000), Some("6m"));
        assert_eq!(band_for_khz(430000), Some("70cm"));
        assert_eq!(band_for_khz(1296000), Some("23cm"));
        assert_eq!(band_for_khz(0), None);
    }

    #[test]
    fn licence_free() {
        assert_eq!(band_for_khz(27185), Some("CB"));
        assert_eq!(band_for_khz(151820), Some("MURS"));
        assert_eq!(band_for_khz(154570), Some("MURS"));
        assert_eq!(band_for_khz(446006), Some("PMR446"));
        assert_eq!(band_for_khz(462562), Some("FRS"));
        assert_eq!(band_for_khz(467712), Some("FRS"));
        // PMR446 wins over 70cm
        assert_eq!(band_for_khz(446100), Some("PMR446"));
        assert_eq!(band_for_khz(433000), Some("70cm"));
    }

    #[test]
    fn parse_and_fmt() {
        assert_eq!(parse_mhz("144.950"), Some(144950));
        assert_eq!(parse_mhz("7.045 MHz"), Some(7045));
        assert_eq!(parse_mhz(" 145.530mhz "), Some(145530));
        assert_eq!(fmt_mhz(144950), "144.950");
        assert_eq!(fmt_mhz(7045), "7.045");
        assert_eq!(describe(144950), "144.950 MHz (2m)");
        assert_eq!(describe(0), "unknown");
        assert_eq!(band_label(0), "inet");
        assert_eq!(band_label(144950), "2m");
    }

    #[test]
    fn rigctl_hz() {
        assert_eq!(parse_rigctl_hz("144950000"), Some(144950));
        assert_eq!(parse_rigctl_hz("Frequency: 7045000 Hz\n"), Some(7045));
        assert_eq!(parse_rigctl_hz("144950"), Some(144950));
        assert_eq!(parse_rigctl_hz("nope"), None);
    }

    #[test]
    fn beacon_body() {
        assert_eq!(beacon_khz(b"B|internet-radio|144950"), Some(144950));
        assert_eq!(beacon_khz(b"B|radio"), None);
        assert_eq!(beacon_khz(b"B|radio|0"), None);
        assert_eq!(beacon_khz(b"hello"), None);
    }

    #[test]
    fn to_bands_dedupes() {
        assert_eq!(
            to_bands([144950, 145530, 0, 7045]),
            vec!["2m", "inet", "40m"]
        );
    }
}
