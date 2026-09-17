//! SPDX-License-Identifier: Apache-2.0
//! Terminal-style console helpers for CLI output.

use console::Style;
use dialoguer::theme::ColorfulTheme;

/// Approximate #7d9bff / #ff7a3d in xterm-256.
const ACCENT: u8 = 111;
const DIM: u8 = 240;
const WARN: u8 = 209;
const ERR: u8 = 196;

pub fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && console::colors_enabled()
}

fn style(code: u8) -> Style {
    if color_enabled() {
        Style::new().color256(code)
    } else {
        Style::new()
    }
}

pub fn main_style() -> Style {
    style(ACCENT)
}

pub fn dim() -> Style {
    style(DIM)
}

pub fn ok() -> Style {
    style(ACCENT)
}

pub fn warn() -> Style {
    style(WARN)
}

pub fn err() -> Style {
    if color_enabled() {
        Style::new().color256(ERR).bold()
    } else {
        Style::new()
    }
}

pub fn panel_line(title: &str, meta: &str) -> String {
    let left = if meta.is_empty() {
        format!("┌─ {title} ─")
    } else {
        format!("┌─ {title}  {meta} ─")
    };
    main_style().apply_to(left).to_string()
}

pub fn panel(title: &str, meta: &str) {
    println!();
    println!("{}", panel_line(title, meta));
}

pub fn rule() {
    println!(
        "{}",
        dim().apply_to("────────────────────────────────────────")
    );
}

pub fn step(n: u8, of: u8, prompt: &str) -> String {
    format!("STEP {n}/{of} — {prompt}")
}

pub fn prompt_theme() -> ColorfulTheme {
    let accent = main_style();
    ColorfulTheme {
        prompt_prefix: accent.clone().apply_to("▸".to_string()),
        prompt_style: accent.clone(),
        values_style: accent.clone(),
        active_item_style: accent.clone().bold(),
        success_prefix: accent.clone().apply_to("OK".to_string()),
        hint_style: dim(),
        ..ColorfulTheme::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_line_contains_title() {
        let s = panel_line("WEECHAT RADIO", "HELP");
        assert!(s.contains("WEECHAT RADIO"));
        assert!(s.contains("HELP"));
    }
}
