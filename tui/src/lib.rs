//! The terminal interface: a menu of installed games, and hotseat matches drawn from
//! each game's view tree.
//!
//! Games describe what is on the table and supply their own art; this crate decides
//! how to draw it. Nothing a plugin sends reaches the terminal without going through
//! [`art`] or [`picture`].

mod art;
mod hotseat;
mod item;
mod menu;
mod picture;
mod probe;
mod view;

use std::io;
use std::sync::Arc;

use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{DefaultTerminal, Frame};
use ratatui_image::picker::Picker;
use tabletty_host::Plugin;
use tabletty_sdk::Manifest;

use crate::hotseat::{Hotseat, HotseatAction};
use crate::menu::{Menu, MenuAction};
use crate::picture::Pictures;

/// An installed game the menu can start.
pub struct GameEntry {
    pub manifest: Manifest,
    pub plugin: Arc<dyn Plugin>,
}

/// Where each match's 32-byte seed comes from. Randomness belongs to the caller, so
/// this crate never reaches for entropy and tests stay deterministic.
pub type SeedSource = Box<dyn FnMut() -> [u8; 32]>;

/// How to draw game art.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Graphics {
    /// Ask the terminal: Kitty, iTerm2 or Sixel images where supported, coloured
    /// half-blocks otherwise.
    #[default]
    Auto,
    /// Coloured half-blocks: works in any truecolor terminal, two pixels per cell.
    Halfblocks,
    /// No images at all; art shows as glyphs and labels.
    Text,
}

pub struct App {
    games: Vec<GameEntry>,
    seeds: SeedSource,
    menu: Menu,
    screen: Screen,
    /// Shown on the menu, e.g. why the last match couldn't start or had to stop.
    notice: Option<String>,
    graphics: Graphics,
    /// Resolved from `graphics` once the terminal is ours; `None` means text only.
    picker: Option<Picker>,
    full_redraw: bool,
    quit: bool,
}

enum Screen {
    Menu,
    Hotseat(Box<Hotseat>),
}

impl App {
    pub fn new(games: Vec<GameEntry>, seeds: SeedSource, graphics: Graphics) -> Self {
        let picker = match graphics {
            Graphics::Halfblocks => Some(Picker::halfblocks()),
            // `Auto` has to query the terminal, which only `run` can do.
            Graphics::Auto | Graphics::Text => None,
        };
        Self {
            menu: Menu::new(&games),
            games,
            seeds,
            screen: Screen::Menu,
            notice: None,
            graphics,
            picker,
            full_redraw: true,
            quit: false,
        }
    }

    /// Go straight into a match, skipping the menu. If it can't start, the menu
    /// opens with the reason.
    pub fn start(&mut self, game: usize, players: u8) {
        let Some(entry) = self.games.get(game) else {
            return;
        };
        let pictures = Pictures::new(self.picker.clone());
        match Hotseat::start(entry, players, (self.seeds)(), pictures) {
            Ok(hotseat) => {
                self.notice = None;
                self.show(Screen::Hotseat(Box::new(hotseat)));
            }
            Err(err) => {
                tracing::warn!(game = %entry.manifest.id, "could not start match: {err}");
                self.notice = Some(format!("Could not start {}: {err}", entry.manifest.name));
            }
        }
    }

    fn show(&mut self, screen: Screen) {
        self.screen = screen;
        self.full_redraw = true;
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
                Ok(HotseatAction::Leave) => self.show(Screen::Menu),
                // A trapped or runaway plugin ends the match, not the app.
                Err(err) => {
                    tracing::error!("plugin failed mid-match: {err}");
                    self.notice = Some(format!("The game stopped: {err}"));
                    self.show(Screen::Menu);
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

    /// Whether the next frame must repaint everything rather than diff; clears the
    /// request.
    fn take_full_redraw(&mut self) -> bool {
        let hotseat = match &mut self.screen {
            Screen::Hotseat(hotseat) => hotseat.take_full_redraw(),
            Screen::Menu => false,
        };
        std::mem::take(&mut self.full_redraw) || hotseat
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }
}

/// Take over the terminal and run until the player quits. The terminal is restored
/// on the way out, including on panic.
pub fn run(mut app: App) -> io::Result<()> {
    let mut terminal = ratatui::init();
    if app.graphics == Graphics::Auto {
        // Must happen after entering raw mode and before reading any input.
        app.picker = Some(probe::detect_graphics());
    }
    let result = event_loop(&mut app, &mut terminal);
    ratatui::restore();
    result
}

fn event_loop(app: &mut App, terminal: &mut DefaultTerminal) -> io::Result<()> {
    while !app.should_quit() {
        if app.take_full_redraw() {
            // Not `terminal.clear()`: that asks the terminal for the cursor position,
            // which blocks for seconds on a terminal that doesn't answer.
            terminal.backend_mut().clear()?;
            // Empty the previous-frame buffer too, so the next draw repaints every cell.
            terminal.swap_buffers();
        }
        terminal.draw(|frame| app.render(frame))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}
