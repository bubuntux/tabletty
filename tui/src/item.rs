//! Framing and drawing one item: its shape, its size, and its face.
//!
//! The game's hints pick the frame (card, tile, token, icon, plain line) and how
//! big the art is; the zone's layout fills in whatever the game left out. The face
//! is then drawn as well as the space and terminal allow: an image, a glyph, or
//! the art's label.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use tabletty_sdk::{Face, Item, Layout as ZoneLayout, Shape, Size, Tone, Track};

use crate::art::{ArtBook, clean, tone_color};
use crate::picture::Pictures;

/// What drawing needs beyond the item itself.
pub struct Ctx<'a> {
    pub art: &'a ArtBook,
    pub pictures: &'a Pictures,
}

/// The hints, resolved: what the game asked for, filled in and capped to fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub shape: Shape,
    pub size: Size,
}

impl Look {
    /// Resolve an item's hints. Anything the game didn't say comes from the zone's
    /// layout, and the size never exceeds `cap`, which shrinks when screen space
    /// runs short.
    pub fn of(item: &Item, layout: ZoneLayout, cap: Size) -> Self {
        let default_shape = match layout {
            ZoneLayout::Grid(_) => Shape::Tile,
            ZoneLayout::Stack => Shape::Plain,
            ZoneLayout::Row | ZoneLayout::Hand | ZoneLayout::Pile => Shape::Card,
        };
        let size = item.size.unwrap_or(Size::Small);
        Self {
            shape: item.shape.unwrap_or(default_shape),
            size: if rank(size) > rank(cap) { cap } else { size },
        }
    }
}

fn rank(size: Size) -> u8 {
    match size {
        Size::Small => 0,
        Size::Medium => 1,
        Size::Large => 2,
    }
}

/// The face's own area in cells, per size.
fn art_area(size: Size) -> (u16, u16) {
    match size {
        Size::Small => (3, 1),
        Size::Medium => (9, 3),
        Size::Large => (16, 6),
    }
}

fn text_width(text: &str) -> u16 {
    Line::raw(text).width() as u16
}

/// Sublabel and badges, as one dim line.
fn detail(item: &Item) -> String {
    let mut detail = clean(&item.sublabel);
    if !item.badges.is_empty() {
        let badges: Vec<String> = item.badges.iter().map(|badge| clean(badge)).collect();
        if !detail.is_empty() {
            detail.push(' ');
        }
        detail.push_str(&format!("[{}]", badges.join(", ")));
    }
    detail
}

/// How many cells an item wants.
pub fn size(item: &Item, look: Look, ctx: &Ctx) -> (u16, u16) {
    let (art_width, art_height) = art_area(look.size);
    let face_width = art_width.max(compact_face(&item.face, item.tone, ctx).width() as u16);
    let label = text_width(&clean(&item.label));
    let detail = detail(item);
    let detail_width = text_width(&detail);
    let detail_rows = u16::from(!detail.is_empty());
    match look.shape {
        Shape::Card => (
            face_width.max(label).max(detail_width).max(6) + 4,
            2 + art_height + 1 + detail_rows,
        ),
        Shape::Tile | Shape::Token => (face_width.max(label) + 4, 2 + art_height),
        Shape::Icon => (
            face_width.max(label).max(detail_width) + 2,
            art_height + 1 + detail_rows,
        ),
        Shape::Plain => {
            let detail_part = if detail.is_empty() {
                0
            } else {
                detail_width + 2
            };
            (2 + face_width + 1 + label + detail_part, 1)
        }
    }
}

/// Border by interaction state: focused, selectable, or inert.
fn frame_style(item: &Item, focused: bool) -> (BorderType, Style) {
    let selectable = item.selectable.as_ref().is_some_and(|spec| spec.enabled);
    if focused {
        (
            BorderType::Thick,
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )
    } else if selectable {
        (BorderType::Rounded, Style::new().fg(Color::Cyan))
    } else {
        (BorderType::Rounded, Style::new().fg(Color::DarkGray))
    }
}

fn label_style(item: &Item, focused: bool) -> Style {
    let selectable = item.selectable.as_ref().is_some_and(|spec| spec.enabled);
    if focused {
        Style::new()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if selectable {
        Style::new().fg(Color::Cyan)
    } else {
        Style::new()
    }
}

/// Draw `item` into `area`, which should be the size [`size`] asked for.
pub fn draw(frame: &mut Frame, area: Rect, item: &Item, look: Look, focused: bool, ctx: &Ctx) {
    // Items can overlap (a fanned hand), so each one paints its whole area.
    frame.render_widget(Clear, area);
    let (_, art_height) = art_area(look.size);
    let (border_type, border_style) = frame_style(item, focused);
    let label = clean(&item.label);
    let detail = detail(item);
    let dim = Style::new().fg(Color::DarkGray);

    match look.shape {
        Shape::Card => {
            let block = Block::bordered()
                .border_type(border_type)
                .border_style(border_style);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let [art, label_area, detail_area] = Layout::vertical([
                Constraint::Length(art_height),
                Constraint::Length(1),
                Constraint::Length(u16::from(!detail.is_empty())),
            ])
            .areas(inner);
            draw_face(frame, art, &item.face, item.tone, ctx);
            frame.render_widget(
                Paragraph::new(label).alignment(Alignment::Center),
                label_area,
            );
            frame.render_widget(
                Paragraph::new(detail)
                    .style(dim)
                    .alignment(Alignment::Center),
                detail_area,
            );
        }
        Shape::Tile | Shape::Token => {
            let border_type = match (look.shape, border_type) {
                (_, BorderType::Thick) => BorderType::Thick,
                (Shape::Tile, _) => BorderType::Plain,
                _ => BorderType::Rounded,
            };
            let block = Block::bordered()
                .border_type(border_type)
                .border_style(border_style)
                .title(Span::styled(label, dim));
            let inner = block.inner(area);
            frame.render_widget(block, area);
            draw_face(frame, inner, &item.face, item.tone, ctx);
        }
        Shape::Icon => {
            let [art, label_area, detail_area] = Layout::vertical([
                Constraint::Length(art_height),
                Constraint::Length(1),
                Constraint::Length(u16::from(!detail.is_empty())),
            ])
            .areas(area);
            draw_face(frame, art, &item.face, item.tone, ctx);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {label} "),
                    label_style(item, focused),
                ))
                .alignment(Alignment::Center),
                label_area,
            );
            frame.render_widget(
                Paragraph::new(detail)
                    .style(dim)
                    .alignment(Alignment::Center),
                detail_area,
            );
        }
        Shape::Plain => {
            let marker = if focused { "▸ " } else { "  " };
            let mut spans = vec![
                Span::styled(marker, border_style),
                compact_face(&item.face, item.tone, ctx),
                Span::raw(" "),
                Span::styled(label, label_style(item, focused)),
            ];
            if !detail.is_empty() {
                spans.push(Span::styled(format!("  {detail}"), dim));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), area);
        }
    }
}

/// A face in one line: for plain items, small sizes, and width estimates.
fn compact_face(face: &Face, tone: Option<Tone>, ctx: &Ctx) -> Span<'static> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let colored = |style: Style, tone: Option<Tone>| match tone {
        Some(tone) => style.fg(tone_color(tone)),
        None => style,
    };
    match face {
        Face::Up(key) => match ctx.art.get(key) {
            Some(entry) => Span::styled(
                entry.glyph.clone().unwrap_or_else(|| entry.label.clone()),
                colored(bold, tone.or(entry.tone)),
            ),
            None => Span::styled(clean(key).to_uppercase(), colored(bold, tone)),
        },
        Face::Down => Span::styled("▒▒▒", Style::new().fg(Color::DarkGray)),
        Face::Empty => Span::styled("·", Style::new().fg(Color::DarkGray)),
        Face::Die(pips) => {
            let text = match pips {
                1..=6 => char::from_u32(0x2680 + u32::from(pips - 1))
                    .map_or_else(|| pips.to_string(), String::from),
                _ => pips.to_string(),
            };
            Span::styled(text, colored(bold, tone))
        }
        Face::Track(track) => Span::styled(track_text(*track, u32::MAX), colored(bold, tone)),
    }
}

/// `●●●○○○○` when it fits in `room` cells, `3/7` otherwise.
fn track_text(track: Track, room: u32) -> String {
    const MAX_DOTS: u32 = 12;
    let filled = track.filled.min(track.total);
    if track.total <= MAX_DOTS && track.total <= room {
        let empty = track.total - filled;
        "●".repeat(filled as usize) + &"○".repeat(empty as usize)
    } else {
        format!("{}/{}", track.filled, track.total)
    }
}

/// Die pips on a 5×3 grid: columns 0, 2, 4 and rows 0, 1, 2.
fn pip_rows(pips: u8) -> Option<[String; 3]> {
    let spots: &[(usize, usize)] = match pips {
        1 => &[(2, 1)],
        2 => &[(0, 0), (4, 2)],
        3 => &[(0, 0), (2, 1), (4, 2)],
        4 => &[(0, 0), (4, 0), (0, 2), (4, 2)],
        5 => &[(0, 0), (4, 0), (2, 1), (0, 2), (4, 2)],
        6 => &[(0, 0), (4, 0), (0, 1), (4, 1), (0, 2), (4, 2)],
        _ => return None,
    };
    let mut rows = [[' '; 5]; 3];
    for &(column, row) in spots {
        rows[row][column] = '●';
    }
    Some(rows.map(|row| row.iter().collect()))
}

/// Lines centred vertically and horizontally in `area`.
fn centered(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    let height = (lines.len() as u16).min(area.height);
    let top = area.y + (area.height - height) / 2;
    let area = Rect::new(area.x, top, area.width, height);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn draw_face(frame: &mut Frame, area: Rect, face: &Face, tone: Option<Tone>, ctx: &Ctx) {
    if area.is_empty() {
        return;
    }
    match face {
        Face::Up(key) => {
            let svg = ctx.art.get(key).and_then(|entry| entry.svg.as_deref());
            if let Some(svg) = svg
                && ctx.pictures.worth_drawing(area)
                && ctx.pictures.draw(frame, area, key, svg)
            {
                return;
            }
            centered(frame, area, vec![Line::from(compact_face(face, tone, ctx))]);
        }
        Face::Down => {
            let back = "▒".repeat(usize::from(area.width));
            let lines: Vec<Line> = (0..area.height)
                .map(|_| Line::styled(back.clone(), Style::new().fg(Color::DarkGray)))
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        }
        Face::Die(pips) if area.height >= 3 && area.width >= 5 => match pip_rows(*pips) {
            Some(rows) => {
                let style = match tone {
                    Some(tone) => Style::new().fg(tone_color(tone)),
                    None => Style::new(),
                };
                let lines = rows
                    .into_iter()
                    .map(|row| Line::styled(row, style))
                    .collect();
                centered(frame, area, lines);
            }
            None => centered(frame, area, vec![Line::from(compact_face(face, tone, ctx))]),
        },
        Face::Track(track) => {
            let style = match tone {
                Some(tone) => Style::new().fg(tone_color(tone)),
                None => Style::new(),
            };
            let text = track_text(*track, u32::from(area.width));
            centered(frame, area, vec![Line::styled(text, style)]);
        }
        Face::Empty | Face::Die(_) => {
            centered(frame, area, vec![Line::from(compact_face(face, tone, ctx))]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pips_are_laid_out_like_a_real_die() {
        assert_eq!(pip_rows(5).unwrap(), ["●   ●", "  ●  ", "●   ●"]);
        assert_eq!(pip_rows(6).unwrap(), ["●   ●", "●   ●", "●   ●"]);
        assert!(pip_rows(7).is_none());
    }

    #[test]
    fn tracks_fall_back_to_a_count_when_too_long() {
        let track = Track {
            filled: 3,
            total: 7,
        };
        assert_eq!(track_text(track, 20), "●●●○○○○");
        assert_eq!(track_text(track, 5), "3/7");
        let overfilled = Track {
            filled: 9,
            total: 4,
        };
        assert_eq!(track_text(overfilled, 20), "●●●●");
    }

    #[test]
    fn hints_are_capped_and_defaulted_from_the_layout() {
        let item = tabletty_sdk::Item::<Vec<u8>>::new(Face::Empty, "x").size(Size::Large);
        let look = Look::of(&item, ZoneLayout::Grid(3), Size::Medium);
        assert_eq!(
            look,
            Look {
                shape: Shape::Tile,
                size: Size::Medium
            }
        );
    }
}
