//! Drawing a plugin's view tree, and moving focus around the things in it that can be
//! acted on.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use tabletty_sdk::{ActionSpec, Layout as ZoneLayout, Prompt, Size, View, Zone};

use crate::art::clean;
use crate::item::{self, Ctx, Look};

const PROMPT_HEIGHT: u16 = 3;
const MAX_LOG_LINES: usize = 5;

/// Something the viewer can act on: an enabled selectable item or prompt choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub place: Place,
    pub spec: ActionSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Item { zone: usize, item: usize },
    Choice(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
    Next,
    Previous,
}

/// A game may offer actions without surfacing them anywhere in its view. Rather than
/// leave the player stuck, show them as a plain prompt.
pub fn with_fallback_prompt(mut view: View, actions: Vec<ActionSpec>) -> View {
    let actions_enabled = actions.iter().any(|spec| spec.enabled);
    if actions_enabled && targets(&view).is_empty() {
        view.prompt = Some(Prompt {
            text: "Actions".into(),
            choices: actions,
            deadline_seconds: None,
        });
    }
    view
}

/// Everything the viewer can act on, in reading order. Disabled actions are drawn
/// but can't take focus, and neither can anything under the top of a pile.
pub fn targets(view: &View) -> Vec<Target> {
    let items = view
        .zones
        .iter()
        .enumerate()
        .flat_map(|(zone_index, zone)| {
            let top_of_pile = zone.items.len().saturating_sub(1);
            zone.items
                .iter()
                .enumerate()
                .filter(move |&(item_index, _)| {
                    zone.layout != ZoneLayout::Pile || item_index == top_of_pile
                })
                .filter_map(move |(item_index, item)| {
                    let spec = item.selectable.as_ref().filter(|spec| spec.enabled)?;
                    Some(Target {
                        place: Place::Item {
                            zone: zone_index,
                            item: item_index,
                        },
                        spec: spec.clone(),
                    })
                })
        });
    let choices = view
        .prompt
        .iter()
        .flat_map(|prompt| prompt.choices.iter().enumerate())
        .filter(|(_, spec)| spec.enabled)
        .map(|(index, spec)| Target {
            place: Place::Choice(index),
            spec: spec.clone(),
        });
    items.chain(choices).collect()
}

/// Where focus lands after moving from `focus` in `direction`. Up and down move by
/// column inside a grid, skipping cells that can't be acted on; everything else
/// steps through targets in reading order, wrapping at the ends.
pub fn step(view: &View, targets: &[Target], focus: usize, direction: Direction) -> usize {
    if targets.is_empty() {
        return 0;
    }
    let next = (focus + 1) % targets.len();
    let previous = (focus + targets.len() - 1) % targets.len();

    let grid_position = match targets.get(focus).map(|target| target.place) {
        Some(Place::Item { zone, item }) => match view.zones[zone].layout {
            ZoneLayout::Grid(columns) => Some((zone, item, usize::from(columns.max(1)))),
            _ => None,
        },
        _ => None,
    };

    match (direction, grid_position) {
        (Direction::Up | Direction::Down, Some((zone, item, columns))) => {
            // Same column, nearest first.
            let column: Vec<usize> = if direction == Direction::Up {
                (item % columns..item).step_by(columns).rev().collect()
            } else {
                (item + columns..view.zones[zone].items.len())
                    .step_by(columns)
                    .collect()
            };
            column
                .into_iter()
                .filter_map(|candidate| {
                    targets.iter().position(|target| {
                        target.place
                            == Place::Item {
                                zone,
                                item: candidate,
                            }
                    })
                })
                .next()
                .unwrap_or(focus)
        }
        (Direction::Up | Direction::Left | Direction::Previous, _) => previous,
        (Direction::Down | Direction::Right | Direction::Next, _) => next,
    }
}

/// Draw `view`'s zones, log and prompt into `area`, highlighting the focused target
/// if there is one. The status line is drawn separately by [`status_line`], so the
/// caller can give it the full width.
///
/// Items are drawn at the size the game hinted if everything fits, and shrunk
/// step by step until it does.
pub fn render(frame: &mut Frame, area: Rect, view: &View, focused: Option<Place>, ctx: &Ctx) {
    let log_lines: Vec<&str> = view
        .log
        .iter()
        .rev()
        .take(MAX_LOG_LINES)
        .rev()
        .map(|line| line.text.as_str())
        .collect();
    let log_height = if log_lines.is_empty() {
        0
    } else {
        log_lines.len() as u16 + 2
    };
    let prompt_height = if view.prompt.is_some() {
        PROMPT_HEIGHT
    } else {
        0
    };

    let zones_height = |cap| -> u16 {
        view.zones
            .iter()
            .map(|zone| zone_height(zone, area.width, cap, ctx))
            .sum()
    };
    // Without images, large art is just a glyph in a big empty box.
    let caps: &[Size] = if ctx.pictures.enabled() {
        &[Size::Large, Size::Medium]
    } else {
        &[Size::Medium]
    };
    let cap = caps
        .iter()
        .copied()
        .find(|&cap| zones_height(cap) + log_height + prompt_height <= area.height)
        .unwrap_or(Size::Small);

    let mut constraints: Vec<Constraint> = view
        .zones
        .iter()
        .map(|zone| Constraint::Length(zone_height(zone, area.width, cap, ctx)))
        .collect();
    constraints.push(Constraint::Length(log_height));
    constraints.push(Constraint::Length(prompt_height));
    constraints.push(Constraint::Min(0));
    let areas = Layout::vertical(constraints).split(area);

    for (zone_index, zone) in view.zones.iter().enumerate() {
        render_zone(
            frame,
            areas[zone_index],
            zone,
            zone_index,
            focused,
            cap,
            ctx,
        );
    }
    let after_zones = view.zones.len();
    if !log_lines.is_empty() {
        let lines: Vec<Line> = log_lines
            .into_iter()
            .map(|text| Line::raw(clean(text)))
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Log ")),
            areas[after_zones],
        );
    }
    if let Some(prompt) = &view.prompt {
        let focused_choice = match focused {
            Some(Place::Choice(index)) => Some(index),
            _ => None,
        };
        render_prompt(frame, areas[after_zones + 1], prompt, focused_choice);
    }
}

pub fn status_line(view: &View) -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (index, (key, value)) in view.status.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ·  ", Style::new().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(
            format!("{}: ", clean(key)),
            Style::new().fg(Color::DarkGray),
        ));
        spans.push(Span::raw(clean(value)));
    }
    Paragraph::new(Line::from(spans))
}

/// Rows a zone needs at `width`, borders included, with item sizes capped at `cap`.
fn zone_height(zone: &Zone, width: u16, cap: Size, ctx: &Ctx) -> u16 {
    let sizes: Vec<(u16, u16)> = zone
        .items
        .iter()
        .map(|item| item::size(item, Look::of(item, zone.layout, cap), ctx))
        .collect();
    let tallest = |indices: &[usize]| {
        indices
            .iter()
            .map(|&index| sizes[index].1)
            .max()
            .unwrap_or(0)
    };
    let all: Vec<usize> = (0..sizes.len()).collect();
    let content = match zone.layout {
        ZoneLayout::Row => row_lines(&sizes, width.saturating_sub(2))
            .iter()
            .map(|line| tallest(line))
            .sum(),
        ZoneLayout::Hand => tallest(&all),
        ZoneLayout::Pile => sizes.last().map_or(0, |&(_, height)| height),
        ZoneLayout::Grid(columns) => {
            let rows = sizes.len().div_ceil(usize::from(columns.max(1)));
            rows as u16 * tallest(&all)
        }
        ZoneLayout::Stack => sizes.iter().map(|&(_, height)| height).sum(),
    };
    content.max(1) + 2
}

/// Pack items left to right into lines no wider than `width`, one cell apart.
fn row_lines(sizes: &[(u16, u16)], width: u16) -> Vec<Vec<usize>> {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut used = 0;
    for (index, &(item_width, _)) in sizes.iter().enumerate() {
        match lines.last_mut() {
            Some(line) if used + 1 + item_width <= width => {
                line.push(index);
                used += 1 + item_width;
            }
            _ => {
                lines.push(vec![index]);
                used = item_width;
            }
        }
    }
    lines
}

fn render_zone(
    frame: &mut Frame,
    area: Rect,
    zone: &Zone,
    zone_index: usize,
    focused: Option<Place>,
    cap: Size,
    ctx: &Ctx,
) {
    let block = Block::bordered()
        .border_style(Style::new().fg(Color::DarkGray))
        .title(format!(" {} ", clean(&zone.label)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let is_focused = |item_index: usize| {
        focused
            == Some(Place::Item {
                zone: zone_index,
                item: item_index,
            })
    };
    let looks: Vec<Look> = zone
        .items
        .iter()
        .map(|item| Look::of(item, zone.layout, cap))
        .collect();
    let sizes: Vec<(u16, u16)> = zone
        .items
        .iter()
        .zip(&looks)
        .map(|(item, &look)| item::size(item, look, ctx))
        .collect();
    let draw = |frame: &mut Frame, index: usize, x: u16, y: u16, width: u16| {
        let rect = Rect::new(x, y, width, sizes[index].1).intersection(inner);
        if !rect.is_empty() {
            item::draw(
                frame,
                rect,
                &zone.items[index],
                looks[index],
                is_focused(index),
                ctx,
            );
        }
    };

    match zone.layout {
        ZoneLayout::Row => {
            let mut y = inner.y;
            for line in row_lines(&sizes, inner.width) {
                let mut x = inner.x;
                for &index in &line {
                    draw(frame, index, x, y, sizes[index].0);
                    x = x.saturating_add(sizes[index].0 + 1);
                }
                let height = line.iter().map(|&index| sizes[index].1).max().unwrap_or(0);
                y = y.saturating_add(height);
            }
        }
        ZoneLayout::Hand => {
            // Side by side if they fit; otherwise fanned, each card overlapping the
            // last, with the focused card drawn on top.
            let total: u16 = sizes.iter().map(|&(width, _)| width + 1).sum();
            let last_width = sizes.last().map_or(0, |&(width, _)| width);
            let step = |index: usize| -> u16 {
                if total <= inner.width + 1 || sizes.len() < 2 {
                    sizes[..index].iter().map(|&(width, _)| width + 1).sum()
                } else {
                    let room = inner.width.saturating_sub(last_width);
                    (room / (sizes.len() as u16 - 1)).max(1) * index as u16
                }
            };
            let mut order: Vec<usize> = (0..sizes.len()).collect();
            if let Some(position) = order.iter().position(|&index| is_focused(index)) {
                let focused = order.remove(position);
                order.push(focused);
            }
            for index in order {
                draw(frame, index, inner.x + step(index), inner.y, sizes[index].0);
            }
        }
        ZoneLayout::Pile => {
            if let Some(top) = sizes.len().checked_sub(1) {
                let width = sizes[top].0;
                draw(frame, top, inner.x, inner.y, width);
                if zone.items.len() > 1 {
                    let count = Paragraph::new(format!(" ×{}", zone.items.len()))
                        .style(Style::new().fg(Color::DarkGray));
                    let rect = Rect::new(inner.x + width, inner.y, 6, 1).intersection(inner);
                    frame.render_widget(count, rect);
                }
            }
        }
        ZoneLayout::Grid(columns) => {
            let columns = usize::from(columns.max(1));
            let cell_width = sizes.iter().map(|&(width, _)| width).max().unwrap_or(0);
            let cell_height = sizes.iter().map(|&(_, height)| height).max().unwrap_or(0);
            for index in 0..sizes.len() {
                let x = inner.x + (index % columns) as u16 * cell_width;
                let y = inner.y + (index / columns) as u16 * cell_height;
                draw(frame, index, x, y, cell_width);
            }
        }
        ZoneLayout::Stack => {
            let mut y = inner.y;
            for (index, &(width, height)) in sizes.iter().enumerate() {
                let width = if looks[index].shape == tabletty_sdk::Shape::Plain {
                    inner.width
                } else {
                    width
                };
                draw(frame, index, inner.x, y, width);
                y = y.saturating_add(height);
            }
        }
    }
}

fn render_prompt(frame: &mut Frame, area: Rect, prompt: &Prompt, focused_choice: Option<usize>) {
    let mut title = format!(" {} ", clean(&prompt.text));
    if let Some(seconds) = prompt.deadline_seconds {
        title.push_str(&format!("({seconds}s) "));
    }
    let mut spans = Vec::new();
    for (index, spec) in prompt.choices.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let label = format!(" {} ", clean(&spec.label));
        let style = if focused_choice == Some(index) {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if spec.enabled {
            Style::new().fg(Color::Cyan)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        spans.push(Span::styled(label, style));
        if let Some(reason) = spec.reason.as_ref().filter(|_| !spec.enabled) {
            spans.push(Span::styled(
                format!("({})", clean(reason)),
                Style::new().fg(Color::DarkGray),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::bordered().title(title)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use tabletty_host::{NativePlugin, Plugin};
    use tabletty_sdk::{InitCtx, Item, Player};
    use tic_tac_toe::TicTacToe;

    use super::*;

    /// X's view after X took b2 and O took a1, with X to move.
    fn tic_tac_toe_view() -> View {
        let plugin = NativePlugin::<TicTacToe>::new();
        let players = (0..2)
            .map(|id| Player {
                id,
                name: format!("P{id}"),
            })
            .collect();
        let mut state = plugin
            .init(&InitCtx {
                players,
                seed: vec![0; 32],
                options: Vec::new(),
            })
            .unwrap();
        for (player, square) in [(0, "b2"), (1, "a1")] {
            let spec = plugin
                .actions(&state, player)
                .unwrap()
                .into_iter()
                .find(|spec| spec.id == square)
                .unwrap();
            state = plugin
                .apply(&state, player, &spec.payload)
                .unwrap()
                .unwrap()
                .state;
        }
        plugin.view(&state, Some(0)).unwrap()
    }

    fn focused_id(targets: &[Target], focus: usize) -> &str {
        &targets[focus].spec.id
    }

    #[test]
    fn only_free_squares_are_targets() {
        let view = tic_tac_toe_view();
        let targets = targets(&view);
        let ids: Vec<&str> = targets.iter().map(|t| t.spec.id.as_str()).collect();
        assert_eq!(ids, ["b1", "c1", "a2", "c2", "a3", "b3", "c3"]);
    }

    #[test]
    fn up_and_down_move_within_a_grid_column_skipping_taken_squares() {
        let view = tic_tac_toe_view();
        let targets = targets(&view);
        let b1 = 0;
        let down = step(&view, &targets, b1, Direction::Down);
        assert_eq!(focused_id(&targets, down), "b3", "b2 is taken, so skip it");
        let up = step(&view, &targets, down, Direction::Up);
        assert_eq!(focused_id(&targets, up), "b1");
        assert_eq!(
            step(&view, &targets, b1, Direction::Up),
            b1,
            "nothing above b1"
        );
    }

    #[test]
    fn left_and_right_step_through_targets_and_wrap() {
        let view = tic_tac_toe_view();
        let targets = targets(&view);
        assert_eq!(
            focused_id(&targets, step(&view, &targets, 0, Direction::Right)),
            "c1"
        );
        assert_eq!(
            focused_id(&targets, step(&view, &targets, 0, Direction::Left)),
            "c3"
        );
    }

    #[test]
    fn actions_missing_from_the_view_become_a_prompt() {
        let mut view = tic_tac_toe_view();
        view.zones.clear();
        let actions = vec![ActionSpec::new("go", "Go", vec![1])];
        let view = with_fallback_prompt(view, actions);
        assert_eq!(targets(&view).len(), 1);
        assert_eq!(view.prompt.unwrap().text, "Actions");
    }

    fn draw(view: &View, width: u16, height: u16, focused: Option<Place>) -> String {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let art = crate::art::ArtBook::default();
        let pictures = crate::picture::Pictures::new(None);
        let ctx = Ctx {
            art: &art,
            pictures: &pictures,
        };
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), view, focused, &ctx))
            .unwrap();
        terminal.backend().to_string()
    }

    fn zone(label: &str, layout: ZoneLayout, items: Vec<Item>) -> Zone {
        Zone {
            label: label.into(),
            layout,
            items,
        }
    }

    /// Every semantic component the renderer draws without game art.
    fn components() -> View {
        use tabletty_sdk::{Face, Shape, Tone, Track};

        let dice = (1..=6)
            .map(|pips| Item::new(Face::Die(pips), format!("d{pips}")).size(Size::Medium))
            .collect();
        let hand = (1..=8)
            .map(|card| {
                let item = Item::new(Face::Up(format!("c{card}")), format!("Card {card}"));
                Item {
                    selectable: Some(ActionSpec::new(format!("c{card}"), "", vec![card])),
                    ..item
                }
            })
            .collect();
        let deck = (0..5).map(|_| Item::new(Face::Down, "Deck")).collect();
        let tokens = vec![
            Item::new(
                Face::Track(Track {
                    filled: 3,
                    total: 7,
                }),
                "Ann",
            )
            .shape(Shape::Plain)
            .tone(Tone::Player(0)),
            Item::new(
                Face::Track(Track {
                    filled: 5,
                    total: 7,
                }),
                "Bob",
            )
            .shape(Shape::Plain)
            .tone(Tone::Player(1)),
        ];
        View {
            title: "Components".into(),
            status: Vec::new(),
            zones: vec![
                zone("Dice", ZoneLayout::Row, dice),
                zone("Hand", ZoneLayout::Hand, hand),
                zone("Deck", ZoneLayout::Pile, deck),
                zone("Affection", ZoneLayout::Stack, tokens),
            ],
            log: Vec::new(),
            prompt: None,
        }
    }

    #[test]
    fn semantic_components_render() {
        let view = components();
        let focused = targets(&view).first().map(|target| target.place);
        insta::assert_snapshot!(draw(&view, 70, 36, focused));
    }

    #[test]
    fn items_shrink_when_the_screen_is_short() {
        let view = components();
        let roomy = draw(&view, 70, 36, None);
        let cramped = draw(&view, 70, 22, None);
        // Medium dice draw pips on three rows; squeezed, they fall back to ⚀–⚅.
        assert!(roomy.contains("●   ●"), "{roomy}");
        assert!(cramped.contains('⚅'), "{cramped}");
    }

    #[test]
    fn only_the_top_of_a_pile_can_take_focus() {
        let mut view = components();
        for item in &mut view.zones[2].items {
            item.selectable = Some(ActionSpec::new("draw", "Draw", vec![0]));
        }
        let pile_targets: Vec<Place> = targets(&view)
            .into_iter()
            .map(|target| target.place)
            .filter(|place| matches!(place, Place::Item { zone: 2, .. }))
            .collect();
        assert_eq!(pile_targets, [Place::Item { zone: 2, item: 4 }]);
    }
}
