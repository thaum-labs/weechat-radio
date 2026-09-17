//! SPDX-License-Identifier: Apache-2.0
//! Map kHz to an amateur or licence-free band name.

/// Band name for a frequency in kHz, if it falls in a known allocation.
pub fn band_for_khz(khz: u32) -> Option<&'static str> {
    if khz == 0 {
        return None;
    }
    // Licence-free first where they sit inside or beside amateur allocations.
    // 11m CB: NZ 26 MHz extra, German extra, FCC/CEPT mid-band, UK 27/81 extra.
    if (26330..=27992).contains(&khz) {
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

/// A suggested calling frequency from the published list (`wcr help calling`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallingFreq {
    pub region: &'static str,
    pub khz: u32,
    /// Radio mode: `USB`, `LSB`, `FM`, or `AM`.
    pub mode: &'static str,
    pub preset: &'static str,
    /// Extra dial note, e.g. CB `ch 19`. Empty when unused.
    pub note: &'static str,
}

impl CallingFreq {
    pub fn band(self) -> &'static str {
        band_for_khz(self.khz).unwrap_or("?")
    }

    /// Compact label for the station panel: `144.950 · 2m USB`.
    pub fn short_label(self) -> String {
        let mut s = format!("{} · {} {}", fmt_mhz(self.khz), self.band(), self.mode);
        if !self.note.is_empty() {
            s.push(' ');
            s.push_str(self.note);
        }
        s
    }

    /// Hover / list detail: `UK/EU 2m FM · preset vhf-fm`.
    pub fn detail(self) -> String {
        let mut s = format!(
            "{} {} {} · preset {}",
            self.region,
            self.band(),
            self.mode,
            self.preset
        );
        if !self.note.is_empty() {
            s.push_str(" · ");
            s.push_str(self.note);
        }
        if self.band() == "CB" {
            s.push_str(" · verify your national CB plan");
        }
        if self.mode == "USB" && self.khz < 10_000 && self.band() != "60m" {
            s.push_str(" · USB for data (voice on this band is LSB)");
        }
        s
    }

    /// Compact combo button: `7.045 40m USB` or `UK 27.135 USB`.
    pub fn closed_label(self) -> String {
        if self.band() == "CB" {
            format!("{} {} {}", self.region, fmt_mhz(self.khz), self.mode)
        } else if self.region == "WW" {
            format!("{} {} {}", fmt_mhz(self.khz), self.band(), self.mode)
        } else {
            format!(
                "{} {} {} {}",
                fmt_mhz(self.khz),
                self.band(),
                self.mode,
                self.region
            )
        }
    }

    /// Frequency-first list row (band/region live in section headers).
    pub fn list_label(self) -> String {
        if self.band() == "CB" {
            format!("{:>8}  {:<3}  {}", fmt_mhz(self.khz), self.mode, self.note)
        } else {
            let region = if self.region == "WW" { "" } else { self.region };
            format!(
                "{:>8}  {:<3}  {:<6}  {region}",
                fmt_mhz(self.khz),
                self.mode,
                self.band()
            )
        }
    }
}

const fn cf(
    region: &'static str,
    khz: u32,
    mode: &'static str,
    preset: &'static str,
) -> CallingFreq {
    CallingFreq {
        region,
        khz,
        mode,
        preset,
        note: "",
    }
}

const fn cf_note(
    region: &'static str,
    khz: u32,
    mode: &'static str,
    preset: &'static str,
    note: &'static str,
) -> CallingFreq {
    CallingFreq {
        region,
        khz,
        mode,
        preset,
        note,
    }
}

/// Suggested data/packet simplex spots covering amateur 160m–23cm plus licence-free
/// allocations from the original band table. Verify your band plan before TX.
/// Not APRS: skip 144.800 (UK/EU) and 144.390 (US) for chat.
const CALLING_FREQS: &[CallingFreq] = &[
    // HF USB data centres (IARU-style).
    cf("WW", 1_838, "USB", "hf-poor"),
    cf("WW", 3_580, "USB", "hf-poor"),
    cf("WW", 5_357, "USB", "hf-poor"),
    cf("UK/EU", 7_045, "USB", "hf-poor"),
    cf("US/AU", 7_090, "USB", "hf-poor"),
    cf("WW", 10_140, "USB", "hf-poor"),
    cf("WW", 14_070, "USB", "hf-poor"),
    cf("WW", 18_100, "USB", "hf-poor"),
    cf("WW", 21_070, "USB", "hf-poor"),
    cf("WW", 24_920, "USB", "hf-poor"),
    // 11m CB by region. AM/FM = typical handheld; USB/LSB = SSB-capable sets.
    cf_note("DE", 26_565, "FM", "vhf-fm", "ch 41"), // German extra 80ch, FM only
    cf_note("NZ", 26_720, "LSB", "hf-poor", "26ch 35"),
    cf_note("DE", 26_965, "FM", "vhf-fm", "ch 1"), // German-speaking FM calling
    cf_note("AU", 27_085, "AM", "vox-safe", "ch 11"),
    cf_note("NZ", 27_085, "AM", "vox-safe", "ch 11"),
    cf_note("EU", 27_135, "USB", "hf-poor", "ch 15"),
    cf_note("UK", 27_135, "USB", "hf-poor", "CEPT ch 15"),
    cf_note("AU", 27_155, "LSB", "hf-poor", "ch 16"), // local SSB calling
    cf_note("US/CA", 27_185, "AM", "vox-safe", "ch 19"),
    cf_note("EU", 27_185, "FM", "vhf-fm", "ch 19"),
    cf_note("UK", 27_185, "FM", "vhf-fm", "CEPT ch 19"),
    cf_note("AU", 27_355, "LSB", "hf-poor", "ch 35"), // DX SSB calling
    cf_note("AU", 27_355, "USB", "hf-poor", "ch 35"),
    cf_note("NZ", 27_355, "USB", "hf-poor", "ch 35"),
    cf_note("US/CA", 27_365, "USB", "hf-poor", "ch 36"),
    cf_note("US/CA", 27_385, "LSB", "hf-poor", "ch 38"),
    cf_note("UK", 27_781, "FM", "vhf-fm", "UK ch 19"), // 27.78125 27/81 band
    cf("WW", 28_120, "USB", "hf-poor"),
    cf("WW", 29_250, "FM", "vhf-fm"), // 10m FM packet
    // VHF packet / data simplex.
    cf("WW", 50_230, "USB", "hf-poor"),
    cf("UK/EU", 50_620, "FM", "vhf-fm"),
    cf("UK/EU", 70_488, "FM", "vhf-fm"), // 70.4875
    cf("UK/EU", 144_950, "FM", "vhf-fm"),
    cf("US", 145_010, "FM", "vhf-fm"),
    cf("US", 145_530, "FM", "vhf-fm"),
    cf("AU", 146_550, "FM", "vhf-fm"),
    cf("US", 151_820, "FM", "vhf-fm"), // MURS
    cf("US", 223_400, "FM", "vhf-fm"),
    // UHF packet / licence-free.
    cf("UK/EU", 433_625, "FM", "vhf-fm"),
    cf("AU", 439_950, "FM", "vhf-fm"),
    cf("US", 441_000, "FM", "vhf-fm"),
    cf("UK/EU", 446_006, "FM", "vhf-fm"), // PMR446 ch 1
    cf("US", 462_562, "FM", "vhf-fm"),    // FRS ch 1
    cf("WW", 1_296_800, "FM", "vhf-fm"),
];

pub fn calling_freqs() -> &'static [CallingFreq] {
    CALLING_FREQS
}

/// Plain-text calling card (`wcr help calling` twin, wizard printout).
pub fn calling_card_text() -> String {
    let mut s = String::from(
        "WeeChat Radio — suggested data/packet calling presets\n\
         Verify your band plan before you transmit. You are the control operator.\n\n",
    );
    for c in calling_freqs() {
        let extra = if c.note.is_empty() {
            String::new()
        } else {
            format!(" {}", c.note)
        };
        s.push_str(&format!(
            "{:<5} {:<6} {:>8} MHz {:<3}{extra}  preset {}\n",
            c.region,
            c.band(),
            fmt_mhz(c.khz),
            c.mode,
            c.preset
        ));
    }
    s.push_str(
        "\nHF data is USB, including 160/80/40 (voice on those bands is LSB). 60m is USB.\n\
         CB is listed per region (US/CA, EU, UK, DE, AU, NZ). AM/FM for typical sets; USB/LSB for SSB-capable radios.\n\
         Avoid APRS chat on 144.800 (UK/EU) and 144.390 (US).\n\
         The wizard can set frequency via rigctl only if you confirm.\n",
    );
    s
}

/// Unique calling rows (same kHz + region + mode collapses; CB regions stay separate).
pub fn calling_freqs_unique() -> Vec<CallingFreq> {
    let mut out = Vec::new();
    for c in calling_freqs() {
        if !out
            .iter()
            .any(|x: &CallingFreq| x.khz == c.khz && x.region == c.region && x.mode == c.mode)
        {
            out.push(*c);
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
        assert_eq!(band_for_khz(26330), Some("CB"));
        assert_eq!(band_for_khz(26565), Some("CB"));
        assert_eq!(band_for_khz(27185), Some("CB"));
        assert_eq!(band_for_khz(27781), Some("CB"));
        assert_eq!(band_for_khz(27991), Some("CB"));
        assert_eq!(band_for_khz(26329), None);
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

    #[test]
    fn calling_list_covers_plan_bands() {
        assert!(calling_freqs()
            .iter()
            .any(|c| c.region.contains("UK") && c.khz == 144_950));
        let uniq = calling_freqs_unique();
        assert_eq!(uniq.len(), calling_freqs().len());
        assert_eq!(uniq[0].short_label(), "1.838 · 160m USB");
        let hf40 = uniq.iter().find(|c| c.khz == 7_045).unwrap();
        assert_eq!(hf40.mode, "USB");
        assert!(hf40.detail().contains("voice on this band is LSB"));
        assert!(!calling_freqs()
            .iter()
            .any(|c| c.region == "WW" && c.band() == "CB"));
        let cb_usb: Vec<_> = calling_freqs()
            .iter()
            .filter(|c| c.band() == "CB" && c.mode == "USB")
            .collect();
        assert!(cb_usb
            .iter()
            .any(|c| c.region == "US/CA" && c.khz == 27_365));
        assert!(cb_usb.iter().any(|c| c.region == "EU" && c.khz == 27_135));
        assert!(cb_usb.iter().any(|c| c.region == "UK" && c.khz == 27_135));
        assert!(cb_usb.iter().any(|c| c.region == "AU" && c.khz == 27_355));
        let cb19: Vec<_> = calling_freqs().iter().filter(|c| c.khz == 27_185).collect();
        assert!(cb19.iter().any(|c| c.region == "US/CA" && c.mode == "AM"));
        assert!(cb19.iter().any(|c| c.region == "EU" && c.mode == "FM"));
        assert!(cb19.iter().any(|c| c.region == "UK" && c.mode == "FM"));
        let uk_fm = calling_freqs()
            .iter()
            .find(|c| c.region == "UK" && c.khz == 27_781)
            .unwrap();
        assert_eq!(uk_fm.mode, "FM");
        assert_eq!(uk_fm.list_label(), "  27.781  FM   UK ch 19");
        assert_eq!(uk_fm.closed_label(), "UK 27.781 FM");
        assert_eq!(uniq[0].closed_label(), "1.838 160m USB");
        assert!(uk_fm.detail().contains("UK ch 19"));
        assert!(uk_fm.detail().contains("verify your national CB plan"));
        let bands: Vec<&str> = uniq.iter().map(|c| c.band()).collect();
        for expected in [
            "160m", "80m", "60m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m", "4m",
            "2m", "1.25m", "70cm", "23cm", "CB", "MURS", "PMR446", "FRS",
        ] {
            assert!(
                bands.contains(&expected),
                "calling list missing band {expected}"
            );
        }
        assert!(!uniq.iter().any(|c| c.khz == 144_800 || c.khz == 144_390));
        assert!(uniq.iter().any(|c| c.khz == 14_070));
        assert!(uniq.iter().any(|c| c.khz == 433_625));
        let two_m = uniq.iter().find(|c| c.khz == 144_950).unwrap();
        assert!(two_m.detail().contains("preset vhf-fm"));
    }
}
