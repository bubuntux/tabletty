//! The terminal interface: a menu of installed games, and hotseat matches drawn from
//! each game's view tree.
//!
//! The TUI owns every glyph and colour. Plugins only describe what is on the table;
//! nothing they send reaches the terminal without going through [`art`].

mod art;
mod hotseat;
mod menu;
mod view;

use std::io;
use std::sync::Arc;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{DefaultTerminal, Frame};
use tabletty_host::Plugin;
use tabletty_sdk::Manifest;

use crate::hotseat::{Hotseat, HotseatAction};
use crate::menu::{Menu, MenuAction};

/// An installed game the menu can start.
pub struct GameEntry {
    pub manifest: Manifest,
    pub plugin: Arc<dyn Plugin>,
}

/// Where each match's 32-byte seed comes from. Randomness belongs to the caller, so
/// this crate never reaches for entropy and tests stay deterministic.
pub type SeedSource = Box<dyn FnMut() -> [u8; 32]>;

pub struct App {
    games: Vec<GameEntry>,
    seeds: SeedSource,
    menu: Menu,
    screen: Screen,
    /// Shown on the menu, e.g. why the last match couldn't start or had to stop.
    notice: Option<String>,
    quit: bool,
}

enum Screen {
    Menu,
    Hotseat(Box<Hotseat>),
}

impl App {
    pub fn new(games: Vec<GameEntry>, seeds: SeedSource) -> Self {
        Self {
            menu: Menu::new(&games),
            games,
            seeds,
            screen: Screen::Menu,
            notice: None,
            quit: false,
        }
    }

    /// Go straight into a match, skipping the menu. If it can't start, the menu
    /// opens with the reason.
    pub fn start(&mut self, game: usize, players: u8) {
        let Some(entry) = self.games.get(game) else {
            return;
        };
        match Hotseat::start(entry, players, (self.seeds)()) {
            Ok(hotseat) => {
                self.notice = None;
                self.screen = Screen::Hotseat(Box::new(hotseat));
            }
            Err(err) => {
                tracing::warn!(game = %entry.manifest.id, "could not start match: {err}");
                self.notice = Some(format!("Could not start {}: {err}", entry.manifest.name));
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match &mut self.screen {
            Screen::Menu => match self.menu.handle_key(&self.games, key) {
                MenuAction::None => {}
                MenuAction::Quit => self.quit = true,
                MenuAction::Start { game, players } => self.start(game, players),
            },
            Screen::Hotseat(hotseat) => match hotseat.handle_key(key) {
                Ok(HotseatAction::None) => {}
                Ok(HotseatAction::Leave) => self.screen = Screen::Menu,
                // A trapped or runaway plugin ends the match, not the app.
                Err(err) => {
                    tracing::error!("plugin failed mid-match: {err}");
                    self.notice = Some(format!("The game stopped: {err}"));
                    self.screen = Screen::Menu;
                }
            },
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        match &self.screen {
            Screen::Menu => self
                .menu
                .render(frame, area, &self.games, self.notice.as_deref()),
            Screen::Hotseat(hotseat) => hotseat.render(frame, area),
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }
}

/// Take over the terminal and run until the player quits. The terminal is restored
/// on the way out, including on panic.
pub fn run(mut app: App) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut app, &mut terminal);
    ratatui::restore();
    result
}

fn event_loop(app: &mut App, terminal: &mut DefaultTerminal) -> io::Result<()> {
    while !app.should_quit() {
        terminal.draw(|frame| app.render(frame))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}
