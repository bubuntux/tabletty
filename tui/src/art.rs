//! Turning plugin output into terminal cells.
//!
//! Plugins name their art with keys; the glyphs and colours are ours. Every string a
//! plugin hands us is untrusted and goes through [`clean`] before it reaches a cell.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use tabletty_sdk::Face;

/// Strip control characters, so a plugin can't smuggle escape sequences into the
/// terminal through a label.
pub fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// How a face looks on screen.
pub fn face(face: &Face) -> Span<'static> {
    match face {
        Face::Up(key) => up(key),
        Face::Down => Span::styled("▒▒▒", Style::new().fg(Color::DarkGray)),
        Face::Empty => Span::styled("·", Style::new().fg(Color::DarkGray)),
    }
}

/// Known art keys get a glyph of their own; anything else is shown as its key.
fn up(key: &str) -> Span<'static> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    match key {
        "x" => Span::styled("X", bold.fg(Color::Red)),
        "o" => Span::styled("O", bold.fg(Color::Blue)),
        _ => Span::styled(clean(key).to_uppercase(), bold),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_sequences_are_stripped() {
        assert_eq!(clean("rock\x1b[2J\x1b]0;pwned\x07"), "rock[2J]0;pwned");
        assert_eq!(clean("two\nlines\ttab"), "twolinestab");
    }

    #[test]
    fn unknown_art_keys_are_cleaned_too() {
        assert_eq!(face(&Face::Up("gu\x1bard".into())).content, "GUARD");
    }
}
