//! SPDX-License-Identifier: Apache-2.0
//! Long chat messages sent as several pieces, shown as one message (#365).
//!
//! A line longer than one frame's body is cut into pieces. Each piece starts
//! with a short text tag, `[1/3] `, so the receiver knows from the first
//! piece how many follow (Tobias's option B on #365). Older versions show the
//! tag as text; this version groups the pieces into one entry and never shows
//! the tag.
//!
//! A user who types `[1/3] ` at the start of a line is not a piece: the
//! sender escapes it with a backslash (`\[1/3] `) and the receiver removes
//! that backslash again.
//!
//! The wire format is untouched: the tag is ordinary body text, and the cut
//! is made in the sender before the envelope is built.

use std::borrow::Cow;

/// A group whose next piece has not come for this long is shown as
/// incomplete. A late retry still completes it.
pub const INCOMPLETE_AFTER_S: u64 = 300;
/// Pieces in one message at most (the tag has two digits).
pub const MAX_PARTS: usize = 99;

/// The tag in front of piece `i` of `n`.
pub fn tag(i: usize, n: usize) -> String {
    format!("[{i}/{n}] ")
}

/// Reads a leading `[i/n] ` piece tag: `(i, n, rest)`. Only the exact form
/// the sender writes counts (no spaces or leading zeros, 1 <= i <= n,
/// 2 <= n <= 99).
pub fn parse(body: &str) -> Option<(usize, usize, &str)> {
    let rest = body.strip_prefix('[')?;
    let close = rest.find("] ")?;
    let (a, b) = rest[..close].split_once('/')?;
    let num = |s: &str| -> Option<usize> {
        if s.is_empty()
            || s.len() > 2
            || s.starts_with('0')
            || !s.bytes().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        s.parse().ok()
    };
    let (i, n) = (num(a)?, num(b)?);
    if !(2..=MAX_PARTS).contains(&n) || i == 0 || i > n {
        return None;
    }
    Some((i, n, &rest[close + 2..]))
}

/// Escapes a line a user typed that starts like a piece tag (`[1/3] `, or
/// such a tag behind backslashes), so the receiver does not take it for one.
pub fn escape(text: &str) -> Cow<'_, str> {
    if parse(text.trim_start_matches('\\')).is_some() {
        Cow::Owned(format!("\\{text}"))
    } else {
        Cow::Borrowed(text)
    }
}

/// Undoes [`escape`] for display.
pub fn unescape(text: &str) -> &str {
    match text.strip_prefix('\\') {
        Some(rest) if parse(rest.trim_start_matches('\\')).is_some() => rest,
        _ => text,
    }
}

/// What a received chat body is: one piece of a long message, or a whole line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body<'a> {
    Piece { i: usize, n: usize, text: &'a str },
    Whole(&'a str),
}

pub fn read(body: &str) -> Body<'_> {
    match parse(body) {
        Some((i, n, text)) => Body::Piece { i, n, text },
        None => Body::Whole(unescape(body)),
    }
}

/// The text to show for a body: the tag removed, an escape undone.
pub fn plain(body: &str) -> &str {
    match read(body) {
        Body::Piece { text, .. } => text,
        Body::Whole(t) => t,
    }
}

/// True for a character that must stay with the one before it (emoji
/// modifiers, joiners, variation selectors, combining marks), so a hard cut
/// never splits an emoji such as 👍🏽 or 👨‍👩‍👧 (#365).
fn sticks_to_previous(c: char) -> bool {
    matches!(c as u32,
        0x200D            // zero-width joiner
        | 0xFE00..=0xFE0F // variation selectors
        | 0x1F3FB..=0x1F3FF // skin tones
        | 0x0300..=0x036F // combining marks
        | 0x20D0..=0x20FF
        | 0xE0020..=0xE007F // tag sequences (flags)
    )
}

/// The largest cut at or below `max` bytes that is on a char boundary and
/// does not split an emoji sequence. At least one whole character.
fn safe_cut(s: &str, max: usize) -> usize {
    if s.len() <= max {
        return s.len();
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    let first = s.chars().next().map(char::len_utf8).unwrap_or(1);
    loop {
        if cut <= first {
            return first.max(cut);
        }
        let next = s[cut..].chars().next();
        let prev = s[..cut].chars().next_back();
        let bad = next.is_some_and(sticks_to_previous) || prev == Some('\u{200D}');
        if !bad {
            return cut;
        }
        cut -= prev.map(char::len_utf8).unwrap_or(1);
    }
}

/// Cuts `text` into pieces of at most `max` bytes, at a space when it can,
/// never inside a character or an emoji sequence. A piece that follows a
/// space starts with that one space, so the receiver joins the pieces as
/// they are and a hard cut (a long URL) gets no space added.
fn cut(text: &str, max: usize) -> Vec<String> {
    let max = max.max(8);
    let mut out = Vec::new();
    let mut rest: String = text.trim().to_string();
    while !rest.is_empty() {
        if rest.len() <= max {
            out.push(std::mem::take(&mut rest));
            break;
        }
        let take = safe_cut(&rest, max);
        let slice = &rest[..take];
        let at_space = slice
            .char_indices()
            .rev()
            .find(|&(i, c)| c.is_whitespace() && !slice[..i].trim().is_empty())
            .map(|(i, _)| i);
        match at_space {
            Some(i) => {
                out.push(slice[..i].trim_end().to_string());
                rest = format!(" {}", rest[i..].trim_start());
            }
            None => {
                out.push(slice.to_string());
                rest = rest[take..].to_string();
            }
        }
    }
    out
}

/// Splits a chat line for the air: one piece as it is (escaped if it looks
/// like a tag), or several, each with its `[i/n] ` tag and each at most
/// `max` bytes with the tag.
pub fn split(text: &str, max: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let text = escape(text);
    if text.len() <= max {
        return vec![text.into_owned()];
    }
    for reserve in [tag(9, 9).len(), tag(99, 99).len()] {
        let pieces = cut(&text, max.saturating_sub(reserve));
        let n = pieces.len();
        if tag(n, n).len() <= reserve || reserve == tag(99, 99).len() {
            let n = n.min(MAX_PARTS);
            return pieces
                .into_iter()
                .take(MAX_PARTS)
                .enumerate()
                .map(|(i, p)| format!("{}{p}", tag(i + 1, n)))
                .collect();
        }
    }
    unreachable!("the last reserve always returns")
}

/// How a grouped message stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// More pieces are expected.
    Arriving,
    /// Every piece is here.
    Complete,
    /// A piece is still missing long after the last one came.
    Incomplete,
}

/// The pieces of one long message received so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Parts {
    got: Vec<Option<String>>,
    /// Unix seconds the last piece came.
    pub last_at: u64,
}

impl Parts {
    pub fn new(n: usize, now: u64) -> Self {
        Self {
            got: vec![None; n.clamp(1, MAX_PARTS)],
            last_at: now,
        }
    }

    pub fn total(&self) -> usize {
        self.got.len()
    }

    pub fn received(&self) -> usize {
        self.got.iter().filter(|p| p.is_some()).count()
    }

    pub fn has(&self, i: usize) -> bool {
        i >= 1 && self.got.get(i - 1).is_some_and(|p| p.is_some())
    }

    /// Adds piece `i` (1-based). False when it does not fit this group.
    pub fn add(&mut self, i: usize, n: usize, text: &str, now: u64) -> bool {
        if n != self.total() || i == 0 || i > n || self.has(i) {
            return false;
        }
        self.got[i - 1] = Some(text.to_string());
        self.last_at = now;
        true
    }

    /// Missing piece numbers (1-based).
    pub fn missing(&self) -> Vec<usize> {
        self.got
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_none())
            .map(|(i, _)| i + 1)
            .collect()
    }

    pub fn status(&self, now: u64) -> Status {
        if self.received() == self.total() {
            Status::Complete
        } else if now.saturating_sub(self.last_at) >= INCOMPLETE_AFTER_S {
            Status::Incomplete
        } else {
            Status::Arriving
        }
    }

    /// The text so far: the pieces joined as sent, a gap shown as ` … `.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let mut gap = false;
        for p in &self.got {
            match p {
                Some(t) => {
                    if gap && !t.starts_with(' ') {
                        out.push(' ');
                    }
                    out.push_str(t);
                    gap = false;
                }
                None if !gap => {
                    out.push_str(" …");
                    gap = true;
                }
                None => {}
            }
        }
        unescape(out.trim_start()).to_string()
    }

    /// The footer under the message, as in the agreed mockup.
    pub fn footer(&self, now: u64) -> String {
        let (n, total) = (self.received(), self.total());
        match self.status(now) {
            Status::Complete => format!("complete · {total}/{total}"),
            Status::Arriving => format!("⋯ more coming · {n}/{total} received"),
            Status::Incomplete => {
                let miss = self.missing();
                let which = miss
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let word = if miss.len() == 1 { "part" } else { "parts" };
                format!("⚠ incomplete · {n}/{total} received · {word} {which} missing")
            }
        }
    }
}

/// The progress line under a long message we are sending: `sending 2/3 ■■□`
/// while pieces go out, `sent 3/3 ■■■` once all are on the air.
pub fn sending_line(done: usize, total: usize) -> String {
    let done = done.min(total);
    let bar: String = (0..total.min(20))
        .map(|i| if i < done { '■' } else { '□' })
        .collect();
    if done >= total {
        format!("sent {total}/{total}  {bar}")
    } else {
        format!("sending {done}/{total}  {bar}")
    }
}

/// Reads an IRC `+radio/parts=k/n` progress tag value.
pub fn parse_progress(v: &str) -> Option<(usize, usize)> {
    let (a, b) = v.split_once('/')?;
    let (k, n) = (a.parse().ok()?, b.parse().ok()?);
    (n >= 2 && k <= n).then_some((k, n))
}

/// Finds the open group that piece `i` of `n` from `nick` belongs to:
/// the newest grouped line from that station, if it still waits for this
/// piece. `lines` yields `(index, nick, parts)` oldest first. None means
/// the piece starts a new grouped line.
pub fn find_group<'a>(
    lines: impl DoubleEndedIterator<Item = (usize, &'a str, Option<&'a Parts>)>,
    nick: &str,
    i: usize,
    n: usize,
) -> Option<usize> {
    for (idx, who, parts) in lines.rev().take(200) {
        if !who.eq_ignore_ascii_case(nick) {
            continue;
        }
        if let Some(p) = parts {
            if p.total() == n && !p.has(i) && p.received() < p.total() {
                return Some(idx);
            }
            return None;
        }
    }
    None
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOREM: &str = "Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?";

    #[test]
    fn a_short_line_is_one_untagged_piece() {
        assert_eq!(split("hello radio", 300), vec!["hello radio"]);
        assert!(split("   ", 300).is_empty());
    }

    #[test]
    fn a_long_line_is_tagged_and_every_piece_fits_with_its_tag() {
        let pieces = split(LOREM, 300);
        assert_eq!(pieces.len(), 3, "{pieces:?}");
        for (k, p) in pieces.iter().enumerate() {
            assert!(p.len() <= 300, "piece {k} is {} bytes", p.len());
            assert_eq!(parse(p).map(|(i, n, _)| (i, n)), Some((k + 1, 3)));
        }
        let mut parts = Parts::new(3, 0);
        for p in &pieces {
            let (i, n, t) = parse(p).unwrap();
            assert!(parts.add(i, n, t, 0));
        }
        assert_eq!(parts.text(), LOREM);
        assert_eq!(parts.footer(0), "complete · 3/3");
    }

    #[test]
    fn ten_or_more_pieces_reserve_room_for_a_two_digit_tag() {
        let long = LOREM.repeat(4);
        let pieces = split(&long, 300);
        assert!(pieces.len() >= 10);
        assert!(pieces.iter().all(|p| p.len() <= 300));
        assert!(pieces[0].starts_with(&format!("[1/{}] ", pieces.len())));
    }

    #[test]
    fn a_typed_tag_is_escaped_and_shown_as_typed() {
        for typed in ["[1/3] of the cake", "\\[2/2] x", "\\\\[1/2] y"] {
            let sent = split(typed, 300);
            assert_eq!(sent.len(), 1);
            assert!(
                parse(&sent[0]).is_none(),
                "{typed} must not read as a piece"
            );
            assert_eq!(read(&sent[0]), Body::Whole(typed));
        }
        // Not tag-like: left alone.
        assert_eq!(split("[1/1] hi", 300), vec!["[1/1] hi"]);
        assert_eq!(split("[a/3] hi", 300), vec!["[a/3] hi"]);
    }

    #[test]
    fn only_the_exact_tag_form_counts() {
        assert!(parse("[1/3] x").is_some());
        assert!(parse("[0/3] x").is_none());
        assert!(parse("[4/3] x").is_none());
        assert!(parse("[01/3] x").is_none());
        assert!(parse("[1/1] x").is_none());
        assert!(parse("[1/3]x").is_none());
        assert!(parse("[1 /3] x").is_none());
        assert!(parse("[1/100] x").is_none());
    }

    #[test]
    fn emoji_are_never_cut_in_half() {
        // No spaces, so every cut is a hard cut.
        let line = "👍🏽👨‍👩‍👧📻✅".repeat(40);
        for max in [40, 41, 42, 43, 57, 100, 300] {
            let pieces = split(&line, max);
            assert!(pieces.len() > 1);
            let mut parts = Parts::new(pieces.len(), 0);
            for p in &pieces {
                assert!(p.len() <= max, "{} > {max}", p.len());
                let (i, n, t) = parse(p).unwrap();
                // Each piece is whole emoji: it never starts with a joiner
                // or modifier, and never ends on a joiner.
                let first = t.chars().next().unwrap();
                assert!(!first.is_whitespace());
                assert!(
                    !sticks_to_previous(first),
                    "max {max}: piece starts with {first:?}"
                );
                assert!(
                    !t.ends_with('\u{200D}'),
                    "max {max}: piece ends on a joiner"
                );
                parts.add(i, n, t, 0);
            }
            assert_eq!(parts.text(), line, "max {max}");
        }
    }

    #[test]
    fn footers_follow_the_agreed_design() {
        let mut p = Parts::new(3, 100);
        p.add(1, 3, "a", 100);
        assert_eq!(p.footer(110), "⋯ more coming · 1/3 received");
        assert_eq!(p.status(110), Status::Arriving);
        p.add(2, 3, " b", 120);
        assert_eq!(
            p.footer(120 + INCOMPLETE_AFTER_S),
            "⚠ incomplete · 2/3 received · part 3 missing"
        );
        assert_eq!(p.text(), "a b …");
        // A late retry completes it.
        p.add(3, 3, " c", 1000);
        assert_eq!(p.status(1000), Status::Complete);
        assert_eq!(p.text(), "a b c");
        let mut gap = Parts::new(3, 0);
        gap.add(1, 3, "a", 0);
        gap.add(3, 3, " c", 0);
        assert_eq!(gap.text(), "a … c");
        assert_eq!(
            gap.footer(INCOMPLETE_AFTER_S),
            "⚠ incomplete · 2/3 received · part 2 missing"
        );
    }

    #[test]
    fn the_sending_line_fills_its_bar() {
        assert_eq!(sending_line(2, 3), "sending 2/3  ■■□");
        assert_eq!(sending_line(3, 3), "sent 3/3  ■■■");
        assert_eq!(parse_progress("2/3"), Some((2, 3)));
        assert_eq!(parse_progress("4/3"), None);
    }
}
