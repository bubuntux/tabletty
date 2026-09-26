//! The TUI driven by key presses and rendered to an in-memory terminal. Games run
//! natively here; the component path is covered by the engine's conformance tests.

use std::sync::Arc;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use rps::RockPaperScissors;
use tabletty_host::{Error, NativePlugin, Plugin, Transition};
use tabletty_sdk::{ActionSpec, InitCtx, Manifest, PlayerId, View};
use tabletty_tui::{App, GameEntry};
use tic_tac_toe::TicTacToe;

fn entry(plugin: impl Plugin + 'static) -> GameEntry {
    GameEntry {
        manifest: plugin.manifest().unwrap(),
        plugin: Arc::new(plugin),
    }
}

fn app() -> App {
    let games = vec![
        entry(NativePlugin::<RockPaperScissors>::new()),
        entry(NativePlugin::<TicTacToe>::new()),
    ];
    App::new(games, Box::new(|| [0; 32]))
}

fn press(app: &mut App, keys: &[KeyCode]) {
    for &key in keys {
        app.handle_key(KeyEvent::from(key));
    }
}

fn screen(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().to_string()
}

use KeyCode::{Down, Enter, Right};

#[test]
fn menu_lists_installed_games() {
    insta::assert_snapshot!(screen(&app()));
}

#[test]
fn handoff_screen_shows_nothing_of_the_game() {
    let mut app = app();
    app.start(0, 2);
    // Player 1 throws rock; control passes to player 2.
    press(&mut app, &[Enter, Enter]);

    let handoff = screen(&app);
    assert!(handoff.contains("Pass to Player 2"));
    assert!(!handoff.contains("thrown"), "{handoff}");
    assert!(!handoff.contains("You throw"), "{handoff}");
    insta::assert_snapshot!(handoff);
}

#[test]
fn opponents_throw_is_face_down_until_the_reveal() {
    let mut app = app();
    app.start(0, 2);
    press(&mut app, &[Enter, Enter, Enter]);

    let player_two = screen(&app);
    assert!(player_two.contains("Rock-Paper-Scissors — Player 2"));
    assert!(!player_two.contains("ROCK"), "{player_two}");
    assert!(
        !player_two.contains("You throw"),
        "player 1's private log leaked:\n{player_two}"
    );
    insta::assert_snapshot!(player_two);
}

#[test]
fn tic_tac_toe_plays_to_a_win_with_the_arrow_keys() {
    let mut app = app();
    press(&mut app, &[Down, Enter]); // pick tic-tac-toe from the menu

    // Each turn is Enter past the handoff, moves, Enter to place. Focus starts on the
    // first free square in reading order, and Right steps through free squares.
    press(&mut app, &[Enter, Enter]); // X a1
    press(&mut app, &[Enter, Enter]); // O b1
    press(&mut app, &[Enter, Right, Enter]); // X a2 (focus starts on c1)
    press(&mut app, &[Enter, Right, Enter]); // O b2 (focus starts on c1)
    press(&mut app, &[Enter]);
    insta::assert_snapshot!("tic_tac_toe_before_winning_move", screen(&app));

    press(&mut app, &[Right, Right, Enter]); // X a3: c1 → c2 → a3, three in a column
    let over = screen(&app);
    assert!(over.contains("game over"), "{over}");
    insta::assert_snapshot!("tic_tac_toe_over", over);
}

/// A plugin that fails the moment anyone acts, standing in for a trap or a runaway
/// loop inside a real component.
struct Crashing(NativePlugin<RockPaperScissors>);

impl Plugin for Crashing {
    fn manifest(&self) -> Result<Manifest, Error> {
        self.0.manifest()
    }
    fn init(&self, ctx: &InitCtx) -> Result<Vec<u8>, Error> {
        self.0.init(ctx)
    }
    fn actions(&self, state: &[u8], player: PlayerId) -> Result<Vec<ActionSpec>, Error> {
        self.0.actions(state, player)
    }
    fn apply(&self, _: &[u8], _: PlayerId, _: &[u8]) -> Result<Result<Transition, String>, Error> {
        Err(Error::OutOfFuel)
    }
    fn view(&self, state: &[u8], viewer: Option<PlayerId>) -> Result<View, Error> {
        self.0.view(state, viewer)
    }
}

#[test]
fn a_failing_plugin_ends_the_match_not_the_app() {
    let mut app = App::new(
        vec![entry(Crashing(NativePlugin::new()))],
        Box::new(|| [0; 32]),
    );
    app.start(0, 2);
    press(&mut app, &[Enter, Enter]);

    assert!(!app.should_quit());
    let menu = screen(&app);
    assert!(
        menu.contains("The game stopped: plugin ran out of fuel"),
        "{menu}"
    );
}
