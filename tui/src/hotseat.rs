//! A match played by everyone at one screen.
//!
//! Control passes to whoever can act next, and every change of player goes through a
//! handoff screen that shows nothing of the game — so a hidden throw or hand is never
//! on screen while the wrong person is looking.

use std::sync::Arc;

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use tabletty_engine::{Match, StartError, SubmitError};
use tabletty_host::{Error as PluginError, Plugin};
use tabletty_sdk::{Effect, Player, PlayerId, View};

use crate::GameEntry;
use crate::art::clean;
use crate::view::{self, Direction, Target};

pub struct Hotseat {
    game: Match<Arc<dyn Plugin>>,
    phase: Phase,
    /// Whose screen this is: the player acting, or about to after a handoff.
    viewer: PlayerId,
    /// The current screen's view, cached so drawing never calls the plugin.
    view: View,
    targets: Vec<Target>,
    focus: usize,
    events: Vec<Event>,
    /// Feedback on the last key, e.g. a rejected action.
    notice: Option<String>,
    confirming_leave: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Handoff,
    Acting,
    Over,
    /// Nobody can act but the game hasn't ended: a plugin bug, shown rather than hung.
    Stuck,
}

/// A logged effect, with who may see it.
struct Event {
    text: String,
    to: Option<PlayerId>,
}

pub enum HotseatAction {
    None,
    Leave,
}

impl Hotseat {
    pub fn start(entry: &GameEntry, players: u8, seed: [u8; 32]) -> Result<Self, StartError> {
        let players = (0..players)
            .map(|id| Player {
                id,
                name: format!("Player {}", id + 1),
            })
            .collect();
        let game = Match::start(Arc::clone(&entry.plugin), players, seed, Vec::new())?;
        let mut hotseat = Self {
            game,
            phase: Phase::Handoff,
            viewer: 0,
            view: empty_view(),
            targets: Vec::new(),
            focus: 0,
            events: Vec::new(),
            notice: None,
            confirming_leave: false,
        };
        hotseat.pass_control(None)?;
        Ok(hotseat)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<HotseatAction, PluginError> {
        if self.confirming_leave {
            self.confirming_leave = false;
            return Ok(match key.code {
                KeyCode::Char('y') => HotseatAction::Leave,
                _ => HotseatAction::None,
            });
        }
        self.notice = None;

        match (self.phase, key.code) {
            (Phase::Over | Phase::Stuck, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) => {
                return Ok(HotseatAction::Leave);
            }
            (_, KeyCode::Esc | KeyCode::Char('q')) => self.confirming_leave = true,
            (Phase::Handoff, KeyCode::Enter) => {
                self.phase = Phase::Acting;
                self.refresh()?;
            }
            (Phase::Acting, KeyCode::Enter | KeyCode::Char(' ')) => self.submit_focused()?,
            (Phase::Acting, code) => {
                let direction = match code {
                    KeyCode::Up | KeyCode::Char('k') => Some(Direction::Up),
                    KeyCode::Down | KeyCode::Char('j') => Some(Direction::Down),
                    KeyCode::Left | KeyCode::Char('h') => Some(Direction::Left),
                    KeyCode::Right | KeyCode::Char('l') => Some(Direction::Right),
                    KeyCode::Tab => Some(Direction::Next),
                    KeyCode::BackTab => Some(Direction::Previous),
                    _ => None,
                };
                if let Some(direction) = direction {
                    self.focus = view::step(&self.view, &self.targets, self.focus, direction);
                }
            }
            _ => {}
        }
        Ok(HotseatAction::None)
    }

    fn submit_focused(&mut self) -> Result<(), PluginError> {
        let Some(target) = self.targets.get(self.focus) else {
            return Ok(());
        };
        let payload = target.spec.payload.clone();
        match self.game.submit(self.viewer, &payload) {
            Ok(effects) => {
                self.record(effects);
                self.pass_control(Some(self.viewer))
            }
            Err(SubmitError::Plugin(err)) => Err(err),
            Err(rejected) => {
                self.notice = Some(rejected.to_string());
                Ok(())
            }
        }
    }

    fn record(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            let event = match effect {
                Effect::PublicLog(text) => Event { text, to: None },
                Effect::PrivateLog(msg) => Event {
                    text: msg.text,
                    to: Some(msg.to),
                },
                // Timers need engine support and canary games don't use them yet.
                Effect::SetTimer(_) | Effect::CancelTimer(_) | Effect::GameOver(_) => continue,
            };
            self.events.push(event);
        }
    }

    /// Hand control to the next player who can act, searching seats in order after
    /// `after`. Staying with the same player needs no handoff.
    fn pass_control(&mut self, after: Option<PlayerId>) -> Result<(), PluginError> {
        if self.game.is_over() {
            self.phase = Phase::Over;
            return self.refresh();
        }
        let seats: Vec<PlayerId> = self.game.players().iter().map(|player| player.id).collect();
        let start = after
            .and_then(|id| seats.iter().position(|&seat| seat == id))
            .map_or(0, |position| position + 1);
        let mut next = None;
        for offset in 0..seats.len() {
            let candidate = seats[(start + offset) % seats.len()];
            if self
                .game
                .actions(candidate)?
                .iter()
                .any(|spec| spec.enabled)
            {
                next = Some(candidate);
                break;
            }
        }

        match next {
            None => {
                tracing::warn!(game = %self.game.manifest().id, "no player can act, but the game is not over");
                self.phase = Phase::Stuck;
                self.refresh()
            }
            Some(player) if after == Some(player) => {
                self.phase = Phase::Acting;
                self.refresh()
            }
            Some(player) => {
                self.viewer = player;
                self.phase = Phase::Handoff;
                self.view = empty_view();
                self.targets.clear();
                self.focus = 0;
                Ok(())
            }
        }
    }

    /// Re-fetch the view for the current screen: the acting player's, or the
    /// spectator's once nobody is acting.
    fn refresh(&mut self) -> Result<(), PluginError> {
        match self.phase {
            Phase::Acting => {
                let view = self.game.view(Some(self.viewer))?;
                let actions = self.game.actions(self.viewer)?;
                self.view = view::with_fallback_prompt(view, actions);
                self.targets = view::targets(&self.view);
                self.focus = self.focus.min(self.targets.len().saturating_sub(1));
            }
            Phase::Over | Phase::Stuck => {
                self.view = self.game.view(None)?;
                self.targets.clear();
            }
            Phase::Handoff => {}
        }
        Ok(())
    }

    fn player_name(&self, id: PlayerId) -> String {
        self.game
            .players()
            .iter()
            .find(|player| player.id == id)
            .map_or_else(
                || format!("Player {}", id + 1),
                |player| player.name.clone(),
            )
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let [header, body, help] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);

        let title = clean(&self.game.manifest().name);
        let whose = match self.phase {
            Phase::Handoff => String::new(),
            Phase::Acting => format!(" — {}", self.player_name(self.viewer)),
            Phase::Over => " — game over".into(),
            Phase::Stuck => " — stuck".into(),
        };
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!("{title}{whose}"),
                Style::new().add_modifier(Modifier::BOLD),
            )),
            header,
        );

        if self.phase == Phase::Handoff {
            self.render_handoff(frame, body);
        } else {
            let [status, columns] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(body);
            frame.render_widget(view::status_line(&self.view), status);
            let [board, side] =
                Layout::horizontal([Constraint::Min(0), Constraint::Length(36)]).areas(columns);
            let focused = self.targets.get(self.focus).map(|target| target.place);
            view::render(frame, board, &self.view, focused);
            self.render_side(frame, side);
        }

        let help_text = if self.confirming_leave {
            "Leave this match? y / n".to_string()
        } else if let Some(notice) = &self.notice {
            clean(notice)
        } else {
            match self.phase {
                Phase::Handoff => "enter continue  q leave".into(),
                Phase::Acting => "←↑↓→ move  enter choose  q leave".into(),
                Phase::Over | Phase::Stuck => "enter back to menu".into(),
            }
        };
        let help_style = if self.confirming_leave || self.notice.is_some() {
            Style::new().fg(Color::Yellow)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        frame.render_widget(Paragraph::new(help_text).style(help_style), help);
    }

    fn render_handoff(&self, frame: &mut Frame, area: Rect) {
        let [_, message, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .areas(area);
        let lines = vec![
            Line::styled(
                format!("Pass to {}", self.player_name(self.viewer)),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::styled("Press enter when ready", Style::new().fg(Color::DarkGray)),
        ];
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), message);
    }

    /// Events the viewer may see, newest at the bottom; outcomes once the game is over.
    fn render_side(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();
        if self.phase == Phase::Over {
            let mut outcomes = self.game.outcomes().unwrap_or_default().to_vec();
            outcomes.sort_by_key(|outcome| outcome.rank);
            for outcome in outcomes {
                let mut line = format!(
                    "{}. {}  {}",
                    outcome.rank,
                    self.player_name(outcome.player),
                    outcome.score
                );
                if !outcome.note.is_empty() {
                    line.push_str(&format!("  {}", clean(&outcome.note)));
                }
                lines.push(Line::styled(
                    line,
                    Style::new().add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::raw(""));
        }
        if self.phase == Phase::Stuck {
            lines.push(Line::styled(
                "No player can act, but the game has not ended. This is a bug in the game.",
                Style::new().fg(Color::Red),
            ));
        }

        let visible_to = match self.phase {
            Phase::Acting => Some(self.viewer),
            _ => None,
        };
        let events: Vec<Line> = self
            .events
            .iter()
            .filter(|event| event.to.is_none() || event.to == visible_to)
            .map(|event| {
                let style = if event.to.is_some() {
                    Style::new().fg(Color::Magenta)
                } else {
                    Style::new()
                };
                Line::styled(clean(&event.text), style)
            })
            .collect();
        let room = usize::from(area.height.saturating_sub(2)).saturating_sub(lines.len());
        lines.extend(events.into_iter().rev().take(room).rev());

        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Events ")),
            area,
        );
    }
}

fn empty_view() -> View {
    View {
        title: String::new(),
        status: Vec::new(),
        zones: Vec::new(),
        log: Vec::new(),
        prompt: None,
    }
}
