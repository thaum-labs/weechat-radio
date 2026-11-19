//! Emoji as `:shortcode:` names in the desktop app (#365, Tobias's final
//! call on comment 7018434427): the app's font cannot draw colour emoji, so
//! chat lines and the input box show `🙂` as `:slightly_smiling_face:`.
//! What is sent, stored and put on the air stays the real emoji: the input
//! box turns names back into emoji when a line is sent. The Pi web app is
//! not touched (browsers draw emoji fine).

use std::collections::HashMap;
use std::sync::OnceLock;

/// The longest emoji sequence looked for, in chars (family and flag ZWJ
/// sequences are up to 10).
const MAX_CHARS: usize = 12;

/// Text with every emoji written as its `:name:`. A skin tone is kept as
/// a second name, `:+1::skin-tone-3:`, the way Slack writes it, so the
/// line sent from the input box carries the same emoji back.
pub fn to_names(text: &str) -> String {
    if text.is_ascii() {
        return text.to_string();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut i = 0;
    while i < chars.len() {
        if let Some((n, e)) = emoji_at(text, &chars, i) {
            out.push_str(&name_of(e));
            i += n;
            continue;
        }
        let c = chars[i].1;
        // A stray variation selector or joiner left over: nothing to show.
        if !matches!(c as u32, 0xFE0E | 0xFE0F | 0x200D) {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Text with every known `:name:` turned back into its emoji: what the
/// input box sends. Unknown names (`:30:` in a time) stay as typed.
pub fn from_names(text: &str) -> String {
    if !text.contains(':') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(':') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match emoji_named(name) {
            Some(e) => {
                let mut tail = &after[end + 1..];
                let mut e = e;
                if let Some((tone, t)) = skin_tone_suffix(tail) {
                    if let Some(toned) = e.with_skin_tone(tone) {
                        e = toned;
                        tail = t;
                    }
                }
                out.push_str(e.as_str());
                rest = tail;
            }
            None => {
                // Not a name: keep the colon, and let the closing one start
                // the next try (`10:30 :blush:`).
                out.push(':');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The longest emoji starting at char `i`, and how many chars it takes.
fn emoji_at(
    text: &str,
    chars: &[(usize, char)],
    i: usize,
) -> Option<(usize, &'static emojis::Emoji)> {
    let c = chars[i].1;
    // Plain ASCII only starts an emoji as a keycap (`1️⃣`, `#️⃣`).
    if c.is_ascii()
        && !chars
            .get(i + 1)
            .is_some_and(|&(_, n)| matches!(n as u32, 0xFE0F | 0x20E3))
    {
        return None;
    }
    let start = chars[i].0;
    let last = (i + MAX_CHARS).min(chars.len());
    for j in (i + 1..=last).rev() {
        let end = chars.get(j).map(|&(b, _)| b).unwrap_or(text.len());
        let s = &text[start..end];
        if let Some(e) = emojis::get(s) {
            return Some((j - i, e));
        }
        // A text-style symbol written without its selector (`❤` for `❤️`).
        if j == i + 1 {
            if let Some(e) = emojis::get(&format!("{s}\u{FE0F}")) {
                return Some((1, e));
            }
        }
    }
    None
}

/// `:name:` for one emoji, with `:skin-tone-N:` after it when it has one.
fn name_of(e: &'static emojis::Emoji) -> String {
    use emojis::SkinTone;
    let tone = e.skin_tone().filter(|t| *t != SkinTone::Default);
    let base = match tone {
        Some(_) => e.with_skin_tone(SkinTone::Default).unwrap_or(e),
        None => e,
    };
    let mut s = format!(":{}:", code_of(base));
    // Slack's numbers: 2 is the lightest, 6 the darkest. Two-person mixed
    // tones have no such name; the base name shows.
    if let Some(n) = tone.map(|t| t as u8).filter(|n| (1..=5).contains(n)) {
        s.push_str(&format!(":skin-tone-{}:", n + 1));
    }
    s
}

/// The GitHub shortcode, or the Unicode name in the same shape for the few
/// emoji that have none.
fn code_of(e: &'static emojis::Emoji) -> String {
    match e.shortcode() {
        Some(c) => c.to_string(),
        None => name_code(e.name()),
    }
}

fn name_code(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('_') {
            s.push('_');
        }
    }
    s.trim_matches('_').to_string()
}

/// The emoji a `:name:` stands for: a GitHub shortcode, or one of the
/// Unicode-name codes [`code_of`] makes for emoji without one.
fn emoji_named(name: &str) -> Option<&'static emojis::Emoji> {
    if name.is_empty() || name.len() > 64 {
        return None;
    }
    if let Some(e) = emojis::get_by_shortcode(name) {
        return Some(e);
    }
    static BY_NAME: OnceLock<HashMap<String, &'static emojis::Emoji>> = OnceLock::new();
    BY_NAME
        .get_or_init(|| {
            emojis::iter()
                .filter(|e| e.shortcode().is_none())
                .map(|e| (name_code(e.name()), e))
                .collect()
        })
        .get(name)
        .copied()
}

/// `:skin-tone-N:` at the start of `s` (N 2 to 6), and what follows it.
fn skin_tone_suffix(s: &str) -> Option<(emojis::SkinTone, &str)> {
    use emojis::SkinTone;
    let rest = s.strip_prefix(":skin-tone-")?;
    let (n, tail) = rest.split_once(':')?;
    let tone = match n {
        "2" => SkinTone::Light,
        "3" => SkinTone::MediumLight,
        "4" => SkinTone::Medium,
        "5" => SkinTone::MediumDark,
        "6" => SkinTone::Dark,
        _ => return None,
    };
    Some((tone, tail))
}

/// Puts `draft` in name form in place, keeping the cursor after what it
/// was after. `cursor` is a char index; the new one is returned.
pub fn names_in_draft(draft: &mut String, cursor: usize) -> usize {
    let before: String = draft.chars().take(cursor).collect();
    let new_cursor = to_names(&before).chars().count();
    *draft = to_names(draft);
    new_cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tobias_examples_show_as_names() {
        assert_eq!(to_names("hi 🙂"), "hi :slightly_smiling_face:");
        assert_eq!(to_names("😊 ok"), ":blush: ok");
        assert_eq!(to_names("🔥🔥"), ":fire::fire:");
    }

    #[test]
    fn ascii_and_app_text_are_left_alone() {
        assert_eq!(to_names("plain text 10:30"), "plain text 10:30");
        assert_eq!(to_names("café ü"), "café ü");
        assert_eq!(to_names("13 dB"), "13 dB");
    }

    #[test]
    fn names_go_back_to_the_same_emoji() {
        for s in [
            "hi 🙂",
            "😊 and ❤️",
            "👍🏽 thanks",
            "👨‍👩‍👧 family",
            "flag 🇬🇧",
            "keycap 1️⃣",
            "🧑‍💻 at 10:30",
        ] {
            let shown = to_names(s);
            assert!(shown.is_ascii(), "{s} shown as {shown}");
            assert_eq!(from_names(&shown), s, "round trip of {s} via {shown}");
        }
    }

    #[test]
    fn a_text_style_heart_still_gets_a_name() {
        assert_eq!(to_names("❤"), ":heart:");
    }

    #[test]
    fn skin_tones_read_like_slack() {
        assert_eq!(to_names("👍🏽"), ":+1::skin-tone-4:");
    }

    #[test]
    fn unknown_names_and_times_stay_as_typed() {
        assert_eq!(from_names("at 10:30:00 :nope: ok"), "at 10:30:00 :nope: ok");
        assert_eq!(from_names("10:30 :blush:"), "10:30 😊");
        assert_eq!(from_names("::fire::"), ":🔥:");
        assert_eq!(from_names("odd : colon"), "odd : colon");
    }

    #[test]
    fn the_cursor_stays_after_the_typed_emoji() {
        let mut d = "ab🙂cd".to_string();
        let c = names_in_draft(&mut d, 3);
        assert_eq!(d, "ab:slightly_smiling_face:cd");
        assert_eq!(&d[..c], "ab:slightly_smiling_face:");
    }
}
