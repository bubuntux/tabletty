//! A game's own art, and the colours we give its tones.
//!
//! Everything a plugin hands us is untrusted: strings go through [`clean`] before
//! they reach a cell, glyphs are held to single-width characters, and oversized
//! art is dropped rather than rendered.

use std::collections::HashMap;

use ratatui::style::Color;
use tabletty_sdk::{Art, Tone};
use unicode_width::UnicodeWidthChar;

/// More than any card game needs; keeps a hostile plugin from ballooning memory.
const MAX_ART_ENTRIES: usize = 512;
const MAX_SVG_BYTES: usize = 64 * 1024;
/// A glyph is for tight spaces: at most this many single-width characters.
const MAX_GLYPH_CHARS: usize = 2;

/// Strip control characters, so a plugin can't smuggle escape sequences into the
/// terminal through a label.
pub fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// Keep a glyph to single-width characters. Wide characters (most emoji) render at
/// different widths in different terminals and would break alignment.
fn single_width(glyph: &str) -> Option<String> {
    let kept: String = glyph
        .chars()
        .filter(|c| !c.is_control() && c.width() == Some(1))
        .take(MAX_GLYPH_CHARS)
        .collect();
    (!kept.is_empty()).then_some(kept)
}

pub fn tone_color(tone: Tone) -> Color {
    const PLAYERS: [Color; 6] = [
        Color::Red,
        Color::Blue,
        Color::Green,
        Color::Yellow,
        Color::Magenta,
        Color::Cyan,
    ];
    match tone {
        Tone::Neutral => Color::Gray,
        Tone::Accent => Color::Magenta,
        Tone::Good => Color::Green,
        Tone::Bad => Color::Red,
        Tone::Player(player) => PLAYERS[usize::from(player) % PLAYERS.len()],
    }
}

/// One game's art, cleaned up and looked up by key.
#[derive(Debug, Default)]
pub struct ArtBook {
    entries: HashMap<String, Entry>,
}

#[derive(Debug)]
pub struct Entry {
    pub label: String,
    pub glyph: Option<String>,
    pub svg: Option<String>,
    pub tone: Option<Tone>,
}

impl ArtBook {
    pub fn new(art: Vec<Art>) -> Self {
        if art.len() > MAX_ART_ENTRIES {
            tracing::warn!(
                count = art.len(),
                "game declares too much art; keeping the first {MAX_ART_ENTRIES}"
            );
        }
        let entries = art
            .into_iter()
            .take(MAX_ART_ENTRIES)
            .map(|art| {
                let svg = art.svg.filter(|svg| {
                    let fits = svg.len() <= MAX_SVG_BYTES;
                    if !fits {
                        tracing::warn!(key = %art.key, bytes = svg.len(), "dropping oversized svg");
                    }
                    fits
                });
                let entry = Entry {
                    label: clean(&art.label),
                    glyph: art.glyph.as_deref().and_then(single_width),
                    svg,
                    tone: art.tone,
                };
                (art.key, entry)
            })
            .collect();
        Self { entries }
    }

    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.entries.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(key: &str, glyph: Option<&str>, svg: Option<String>) -> Art {
        Art {
            key: key.into(),
            label: format!("{key}\x1b[31m"),
            glyph: glyph.map(Into::into),
            svg,
            tone: None,
        }
    }

    #[test]
    fn escape_sequences_are_stripped() {
        assert_eq!(clean("rock\x1b[2J\x1b]0;pwned\x07"), "rock[2J]0;pwned");
        assert_eq!(clean("two\nlines\ttab"), "twolinestab");
    }

    #[test]
    fn glyphs_keep_only_single_width_characters() {
        let book = ArtBook::new(vec![
            art("fist", Some("✊R"), None),
            art("wide", Some("✊"), None),
            art("long", Some("ABCD"), None),
        ]);
        assert_eq!(book.get("fist").unwrap().glyph.as_deref(), Some("R"));
        assert_eq!(book.get("wide").unwrap().glyph, None);
        assert_eq!(book.get("long").unwrap().glyph.as_deref(), Some("AB"));
        assert_eq!(book.get("fist").unwrap().label, "fist[31m");
    }

    #[test]
    fn oversized_svg_is_dropped() {
        let huge = "x".repeat(MAX_SVG_BYTES + 1);
        let book = ArtBook::new(vec![art("big", None, Some(huge))]);
        assert!(book.get("big").unwrap().svg.is_none());
    }
}
