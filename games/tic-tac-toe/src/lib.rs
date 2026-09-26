//! Tic-tac-toe.
//!
//! A conformance fixture covering the half of the contract rock-paper-scissors
//! doesn't: turn alternation, a grid layout, selectable items, legal-action
//! enumeration with disabled actions, and win/draw outcomes.

use serde::{Deserialize, Serialize};
use tabletty_sdk::{
    ActionSpec, Effect, Face, Game, InitCtx, Item, Layout, Manifest, Outcome, Player, PlayerId,
    View, Zone,
};

const LINES: [[usize; 3]; 8] = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8],
    [0, 3, 6],
    [1, 4, 7],
    [2, 5, 8],
    [0, 4, 8],
    [2, 4, 6],
];

pub struct TicTacToe;

tabletty_sdk::export_game!(TicTacToe);

/// A board square, 0..9 in reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Square(pub u8);

impl Square {
    /// Chess-style name: column `a`–`c`, row `1`–`3` from the top.
    fn name(self) -> String {
        let column = char::from(b'a' + self.0 % 3);
        let row = self.0 / 3 + 1;
        format!("{column}{row}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Finish {
    Won { seat: u8 },
    Draw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Seat 0 plays X and moves first.
    players: [Player; 2],
    /// The seat whose mark is on each square.
    board: [Option<u8>; 9],
    to_move: u8,
    finish: Option<Finish>,
}

impl State {
    fn seat(&self, player: PlayerId) -> Option<u8> {
        (0..2).find(|&seat| self.players[usize::from(seat)].id == player)
    }

    fn is_to_move(&self, seat: u8) -> bool {
        self.finish.is_none() && self.to_move == seat
    }

    fn name(&self, seat: u8) -> &str {
        &self.players[usize::from(seat)].name
    }

    fn line_completed_by(&self, seat: u8) -> bool {
        LINES
            .iter()
            .any(|line| line.iter().all(|&square| self.board[square] == Some(seat)))
    }
}

fn mark(seat: u8) -> &'static str {
    if seat == 0 { "x" } else { "o" }
}

fn place(square: Square) -> ActionSpec<Square> {
    let name = square.name();
    ActionSpec::new(name.clone(), name, square)
}

impl Game for TicTacToe {
    type State = State;
    type Action = Square;

    fn manifest() -> Manifest {
        Manifest {
            id: "tic-tac-toe".into(),
            name: "Tic-tac-toe".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            min_players: 2,
            max_players: 2,
            summary: "Three in a row on a 3×3 grid.".into(),
        }
    }

    fn init(ctx: InitCtx) -> State {
        let players: [Player; 2] = ctx
            .players
            .try_into()
            .expect("the host enforces the manifest's player count");
        State {
            players,
            board: [None; 9],
            to_move: 0,
            finish: None,
        }
    }

    /// Every square, with taken ones offered disabled.
    fn actions(state: &State, player: PlayerId) -> Vec<ActionSpec<Square>> {
        if !state
            .seat(player)
            .is_some_and(|seat| state.is_to_move(seat))
        {
            return Vec::new();
        }
        (0..9)
            .map(|index| {
                let spec = place(Square(index));
                if state.board[usize::from(index)].is_some() {
                    spec.disabled("taken")
                } else {
                    spec
                }
            })
            .collect()
    }

    fn apply(state: &mut State, player: PlayerId, square: Square) -> Result<Vec<Effect>, String> {
        let seat = state.seat(player).ok_or("you are not in this match")?;
        if state.finish.is_some() {
            return Err("the game is over".into());
        }
        if state.to_move != seat {
            return Err("it is not your turn".into());
        }
        let cell = state
            .board
            .get_mut(usize::from(square.0))
            .ok_or("no such square")?;
        if cell.is_some() {
            return Err(format!("{} is taken", square.name()));
        }

        *cell = Some(seat);
        let mut effects = vec![Effect::PublicLog(format!(
            "{} ({}) plays {}.",
            state.name(seat),
            mark(seat).to_uppercase(),
            square.name()
        ))];

        let finish = if state.line_completed_by(seat) {
            Some(Finish::Won { seat })
        } else if state.board.iter().all(Option::is_some) {
            Some(Finish::Draw)
        } else {
            None
        };
        match finish {
            None => state.to_move = 1 - seat,
            Some(finish) => {
                state.finish = Some(finish);
                effects.push(Effect::GameOver(outcomes(state, finish)));
            }
        }
        Ok(effects)
    }

    fn view(state: &State, viewer: Option<PlayerId>) -> View<Square> {
        let viewer_to_move = viewer
            .and_then(|player| state.seat(player))
            .is_some_and(|seat| state.is_to_move(seat));

        let mut status = vec![
            ("X".to_string(), state.name(0).to_string()),
            ("O".to_string(), state.name(1).to_string()),
        ];
        status.push(match state.finish {
            None => ("Turn".into(), state.name(state.to_move).into()),
            Some(Finish::Won { seat }) => ("Winner".into(), state.name(seat).into()),
            Some(Finish::Draw) => ("Result".into(), "Draw".into()),
        });

        let squares = (0..9)
            .map(|index| {
                let square = Square(index);
                match state.board[usize::from(index)] {
                    Some(seat) => Item::new(Face::Up(mark(seat).into()), square.name()),
                    None => Item {
                        selectable: viewer_to_move.then(|| place(square)),
                        ..Item::new(Face::Empty, square.name())
                    },
                }
            })
            .collect();

        View {
            title: "Tic-tac-toe".into(),
            status,
            zones: vec![Zone {
                label: "Board".into(),
                layout: Layout::Grid,
                items: squares,
            }],
            log: Vec::new(),
            prompt: None,
        }
    }
}

fn outcomes(state: &State, finish: Finish) -> Vec<Outcome> {
    (0..2)
        .map(|seat| {
            let (rank, score, note) = match finish {
                Finish::Won { seat: winner } if winner == seat => (1, 1, "three in a row"),
                Finish::Won { .. } => (2, 0, ""),
                Finish::Draw => (1, 0, "draw"),
            };
            Outcome {
                player: state.players[usize::from(seat)].id,
                rank,
                score,
                note: note.into(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> State {
        TicTacToe::init(InitCtx {
            players: vec![
                Player {
                    id: 0,
                    name: "Ann".into(),
                },
                Player {
                    id: 1,
                    name: "Bob".into(),
                },
            ],
            seed: vec![0; 32],
            options: Vec::new(),
        })
    }

    /// Play alternating moves starting with X, returning the last move's effects.
    fn play(state: &mut State, squares: &[u8]) -> Vec<Effect> {
        let mut effects = Vec::new();
        for &square in squares {
            let player = state.to_move;
            effects = TicTacToe::apply(state, player, Square(square)).unwrap();
        }
        effects
    }

    #[test]
    fn only_the_player_to_move_has_actions() {
        let mut state = start();
        assert_eq!(TicTacToe::actions(&state, 0).len(), 9);
        assert!(TicTacToe::actions(&state, 1).is_empty());

        play(&mut state, &[4]);
        assert!(TicTacToe::actions(&state, 0).is_empty());
        assert!(TicTacToe::apply(&mut state, 0, Square(0)).is_err());
    }

    #[test]
    fn taken_squares_are_offered_disabled() {
        let mut state = start();
        play(&mut state, &[4]);
        let actions = TicTacToe::actions(&state, 1);
        let centre = &actions[4];
        assert_eq!(centre.id, "b2");
        assert!(!centre.enabled);
        assert_eq!(centre.reason.as_deref(), Some("taken"));
        assert!(TicTacToe::apply(&mut state, 1, Square(4)).is_err());
    }

    #[test]
    fn three_in_a_row_wins() {
        let mut state = start();
        // X: a1 b1 c1, O: a2 b2
        let effects = play(&mut state, &[0, 3, 1, 4, 2]);
        assert_eq!(state.finish, Some(Finish::Won { seat: 0 }));
        let Some(Effect::GameOver(outcomes)) = effects.last() else {
            panic!("expected game over, got {effects:?}");
        };
        assert_eq!((outcomes[0].rank, outcomes[1].rank), (1, 2));
        assert!(TicTacToe::actions(&state, 1).is_empty());
    }

    #[test]
    fn full_board_without_a_line_is_a_draw() {
        let mut state = start();
        // X O X / X O O / O X X
        play(&mut state, &[0, 1, 2, 4, 3, 5, 7, 6, 8]);
        assert_eq!(state.finish, Some(Finish::Draw));
    }

    #[test]
    fn only_the_player_to_move_sees_selectable_squares() {
        let mut state = start();
        play(&mut state, &[4]);
        let selectable = |viewer| {
            TicTacToe::view(&state, viewer).zones[0]
                .items
                .iter()
                .filter(|item| item.selectable.is_some())
                .count()
        };
        assert_eq!(selectable(Some(1)), 8);
        assert_eq!(selectable(Some(0)), 0);
        assert_eq!(selectable(None), 0);
    }
}
