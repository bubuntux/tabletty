//! Pick a game and a player count.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::GameEntry;
use crate::art::clean;

#[derive(Debug, Default)]
pub struct Menu {
    selected: usize,
    players: u8,
}

pub enum MenuAction {
    None,
    Start { game: usize, players: u8 },
    Quit,
}

impl Menu {
    pub fn new(games: &[GameEntry]) -> Self {
        let mut menu = Self::default();
        menu.select(games, 0);
        menu
    }

    fn select(&mut self, games: &[GameEntry], index: usize) {
        self.selected = index;
        self.players = games.get(index).map_or(0, |game| game.manifest.min_players);
    }

    pub fn handle_key(&mut self, games: &[GameEntry], key: KeyEvent) -> MenuAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return MenuAction::Quit,
            _ if games.is_empty() => {}
            KeyCode::Up | KeyCode::Char('k') => {
                self.select(games, (self.selected + games.len() - 1) % games.len());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select(games, (self.selected + 1) % games.len());
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let min = games[self.selected].manifest.min_players;
                self.players = self.players.saturating_sub(1).max(min);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let max = games[self.selected].manifest.max_players;
                self.players = self.players.saturating_add(1).min(max);
            }
            KeyCode::Enter => {
                return MenuAction::Start {
                    game: self.selected,
                    players: self.players,
                };
            }
            _ => {}
        }
        MenuAction::None
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, games: &[GameEntry], notice: Option<&str>) {
        let [title, list, detail, notice_area, help] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(games.len().max(1) as u16 + 2),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);

        frame.render_widget(
            Paragraph::new(Line::styled(
                "tabletty",
                Style::new().add_modifier(Modifier::BOLD),
            )),
            title,
        );

        if games.is_empty() {
            let text = "No games found. Build them with `game-build`, or drop a .wasm into \
                        ~/.local/share/tabletty/games.";
            frame.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: true })
                    .block(Block::bordered().title(" Games ")),
                list,
            );
        } else {
            let lines: Vec<Line> = games
                .iter()
                .enumerate()
                .map(|(index, game)| {
                    let manifest = &game.manifest;
                    let style = if index == self.selected {
                        Style::new().fg(Color::Black).bg(Color::Yellow)
                    } else {
                        Style::new()
                    };
                    Line::from(vec![
                        Span::styled(format!(" {} ", clean(&manifest.name)), style),
                        Span::styled(
                            format!(" {}", clean(&manifest.version)),
                            Style::new().fg(Color::DarkGray),
                        ),
                    ])
                })
                .collect();
            frame.render_widget(
                Paragraph::new(lines).block(Block::bordered().title(" Games ")),
                list,
            );

            let manifest = &games[self.selected].manifest;
            let players = if manifest.min_players == manifest.max_players {
                format!("Players: {}", self.players)
            } else {
                format!("Players: ‹ {} ›", self.players)
            };
            let detail_lines = vec![
                Line::raw(clean(&manifest.summary)),
                Line::raw(""),
                Line::raw(players),
            ];
            frame.render_widget(
                Paragraph::new(detail_lines).wrap(Wrap { trim: true }),
                detail,
            );
        }

        if let Some(notice) = notice {
            frame.render_widget(
                Paragraph::new(clean(notice))
                    .style(Style::new().fg(Color::Red))
                    .wrap(Wrap { trim: true }),
                notice_area,
            );
        }

        frame.render_widget(
            Paragraph::new("↑↓ game  ←→ players  enter start  q quit")
                .style(Style::new().fg(Color::DarkGray)),
            help,
        );
    }
}
