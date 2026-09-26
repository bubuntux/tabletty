//! Drawing a plugin's view tree, and moving focus around the things in it that can be
//! acted on.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use tabletty_sdk::{ActionSpec, Item, Layout as ZoneLayout, Prompt, View, Zone};

use crate::art::{self, clean};

/// Row cards: border, face, label, sublabel, border.
const CARD_HEIGHT: u16 = 5;
/// Grid cells: border (carrying the label), face, border.
const CELL_HEIGHT: u16 = 3;
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
/// but can't take focus.
pub fn targets(view: &View) -> Vec<Target> {
    let items = view
        .zones
        .iter()
        .enumerate()
        .flat_map(|(zone_index, zone)| {
            zone.items
                .iter()
                .enumerate()
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
pub fn render(frame: &mut Frame, area: Rect, view: &View, focused: Option<Place>) {
    let log_lines: Vec<&str> = view
        .log
        .iter()
        .rev()
        .take(MAX_LOG_LINES)
        .rev()
        .map(|line| line.text.as_str())
        .collect();

    let mut constraints = Vec::new();
    constraints.extend(
        view.zones
            .iter()
            .map(|zone| Constraint::Length(zone_height(zone))),
    );
    if !log_lines.is_empty() {
        constraints.push(Constraint::Length(log_lines.len() as u16 + 2));
    }
    if view.prompt.is_some() {
        constraints.push(Constraint::Length(PROMPT_HEIGHT));
    }
    constraints.push(Constraint::Min(0));
    let areas = Layout::vertical(constraints).split(area);
    let mut areas = areas.iter().copied();

    for (zone_index, zone) in view.zones.iter().enumerate() {
        render_zone(
            frame,
            areas.next().unwrap_or_default(),
            zone,
            zone_index,
            focused,
        );
    }
    if !log_lines.is_empty() {
        let lines: Vec<Line> = log_lines
            .into_iter()
            .map(|text| Line::raw(clean(text)))
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Log ")),
            areas.next().unwrap_or_default(),
        );
    }
    if let Some(prompt) = &view.prompt {
        let focused_choice = match focused {
            Some(Place::Choice(index)) => Some(index),
            _ => None,
        };
        render_prompt(
            frame,
            areas.next().unwrap_or_default(),
            prompt,
            focused_choice,
        );
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

fn zone_height(zone: &Zone) -> u16 {
    let borders = 2;
    match zone.layout {
        ZoneLayout::Row => CARD_HEIGHT + borders,
        ZoneLayout::Grid(columns) => {
            let rows = zone.items.len().div_ceil(usize::from(columns.max(1)));
            rows as u16 * CELL_HEIGHT + borders
        }
        ZoneLayout::Stack => zone.items.len() as u16 + borders,
    }
}

fn render_zone(
    frame: &mut Frame,
    area: Rect,
    zone: &Zone,
    zone_index: usize,
    focused: Option<Place>,
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
    match zone.layout {
        ZoneLayout::Row => {
            let widths = zone
                .items
                .iter()
                .map(|item| Constraint::Length(card_width(item)));
            let cells = Layout::horizontal(widths).spacing(1).split(inner);
            for (index, (item, cell)) in zone.items.iter().zip(cells.iter()).enumerate() {
                render_card(frame, *cell, item, is_focused(index));
            }
        }
        ZoneLayout::Grid(columns) => {
            let columns = usize::from(columns.max(1));
            let cell_width = zone.items.iter().map(cell_width).max().unwrap_or(0);
            let rows = Layout::vertical(
                zone.items
                    .chunks(columns)
                    .map(|_| Constraint::Length(CELL_HEIGHT)),
            )
            .split(inner);
            for (row_index, (row, row_area)) in
                zone.items.chunks(columns).zip(rows.iter()).enumerate()
            {
                let cells = Layout::horizontal(row.iter().map(|_| Constraint::Length(cell_width)))
                    .split(*row_area);
                for (column, (item, cell)) in row.iter().zip(cells.iter()).enumerate() {
                    render_cell(frame, *cell, item, is_focused(row_index * columns + column));
                }
            }
        }
        ZoneLayout::Stack => {
            let lines: Vec<Line> = zone
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| stack_line(item, is_focused(index)))
                .collect();
            frame.render_widget(Paragraph::new(lines), inner);
        }
    }
}

fn border(item: &Item, focused: bool) -> (BorderType, Style) {
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

fn card_width(item: &Item) -> u16 {
    let content = [
        art::face(&item.face).width(),
        Line::raw(clean(&item.label)).width(),
        Line::raw(detail(item)).width(),
    ];
    content.into_iter().max().unwrap_or(0).max(8) as u16 + 4
}

fn render_card(frame: &mut Frame, area: Rect, item: &Item, focused: bool) {
    let (border_type, border_style) = border(item, focused);
    let lines = vec![
        Line::from(art::face(&item.face)),
        Line::raw(clean(&item.label)),
        Line::styled(detail(item), Style::new().fg(Color::DarkGray)),
    ];
    let card = Paragraph::new(lines).alignment(Alignment::Center).block(
        Block::bordered()
            .border_type(border_type)
            .border_style(border_style),
    );
    frame.render_widget(card, area);
}

fn cell_width(item: &Item) -> u16 {
    let label = Line::raw(clean(&item.label)).width();
    let face = art::face(&item.face).width();
    label.max(face).max(3) as u16 + 4
}

fn render_cell(frame: &mut Frame, area: Rect, item: &Item, focused: bool) {
    let (border_type, border_style) = border(item, focused);
    let cell = Paragraph::new(Line::from(art::face(&item.face)))
        .alignment(Alignment::Center)
        .block(
            Block::bordered()
                .border_type(border_type)
                .border_style(border_style)
                .title(Span::styled(
                    clean(&item.label),
                    Style::new().fg(Color::DarkGray),
                )),
        );
    frame.render_widget(cell, area);
}

fn stack_line(item: &Item, focused: bool) -> Line<'static> {
    let (_, style) = border(item, focused);
    let marker = if focused { "▸ " } else { "  " };
    Line::from(vec![
        Span::styled(marker, style),
        art::face(&item.face),
        Span::raw(" "),
        Span::styled(clean(&item.label), style),
        Span::styled(
            format!("  {}", detail(item)),
            Style::new().fg(Color::DarkGray),
        ),
    ])
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
    use tabletty_sdk::{InitCtx, Player};
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
}
