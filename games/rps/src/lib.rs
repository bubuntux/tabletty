//! Rock-Paper-Scissors, first to two round wins.
//!
//! A conformance fixture, not a game anyone should play. It covers the half of the
//! contract tic-tac-toe doesn't: simultaneous hidden submission, advancing only once
//! every player is in, and redaction — nobody sees the other throw before the reveal.

use serde::{Deserialize, Serialize};
use tabletty_sdk::{
    ActionSpec, Effect, Face, Game, InitCtx, Item, Layout, Manifest, Outcome, Player, PlayerId,
    PrivateMsg, Prompt, View, Zone,
};

const WINS_NEEDED: u8 = 2;

pub struct RockPaperScissors;

tabletty_sdk::export_game!(RockPaperScissors);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Throw {
    Rock,
    Paper,
    Scissors,
}

impl Throw {
    const ALL: [Throw; 3] = [Throw::Rock, Throw::Paper, Throw::Scissors];

    /// Action id and art key.
    fn key(self) -> &'static str {
        match self {
            Throw::Rock => "rock",
            Throw::Paper => "paper",
            Throw::Scissors => "scissors",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Throw::Rock => "Rock",
            Throw::Paper => "Paper",
            Throw::Scissors => "Scissors",
        }
    }

    fn beats(self, other: Throw) -> bool {
        matches!(
            (self, other),
            (Throw::Rock, Throw::Scissors)
                | (Throw::Paper, Throw::Rock)
                | (Throw::Scissors, Throw::Paper)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    players: [Player; 2],
    /// This round's throws. Hidden from everyone but the thrower until both are in.
    throws: [Option<Throw>; 2],
    last_round: Option<[Throw; 2]>,
    wins: [u8; 2],
    round: u8,
}

impl State {
    fn seat(&self, player: PlayerId) -> Option<usize> {
        self.players.iter().position(|seated| seated.id == player)
    }

    fn winner(&self) -> Option<usize> {
        self.wins.iter().position(|&wins| wins >= WINS_NEEDED)
    }

    fn can_throw(&self, seat: usize) -> bool {
        self.winner().is_none() && self.throws[seat].is_none()
    }

    fn name(&self, seat: usize) -> &str {
        &self.players[seat].name
    }
}

impl Game for RockPaperScissors {
    type State = State;
    type Action = Throw;

    fn manifest() -> Manifest {
        Manifest {
            id: "rps".into(),
            name: "Rock-Paper-Scissors".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            min_players: 2,
            max_players: 2,
            summary: format!("Simultaneous throws, first to {WINS_NEEDED} round wins."),
        }
    }

    fn init(ctx: InitCtx) -> State {
        let players: [Player; 2] = ctx
            .players
            .try_into()
            .expect("the host enforces the manifest's player count");
        State {
            players,
            throws: [None, None],
            last_round: None,
            wins: [0, 0],
            round: 1,
        }
    }

    fn actions(state: &State, player: PlayerId) -> Vec<ActionSpec<Throw>> {
        match state.seat(player) {
            Some(seat) if state.can_throw(seat) => Throw::ALL
                .into_iter()
                .map(|throw| ActionSpec::new(throw.key(), throw.label(), throw))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn apply(state: &mut State, player: PlayerId, throw: Throw) -> Result<Vec<Effect>, String> {
        let seat = state.seat(player).ok_or("you are not in this match")?;
        if state.winner().is_some() {
            return Err("the match is over".into());
        }
        if state.throws[seat].is_some() {
            return Err("you have already thrown this round".into());
        }

        state.throws[seat] = Some(throw);
        let private = Effect::PrivateLog(PrivateMsg {
            to: player,
            text: format!("You throw {}.", throw.key()),
        });

        let [Some(first), Some(second)] = state.throws else {
            let public = Effect::PublicLog(format!("{} has thrown.", state.name(seat)));
            return Ok(vec![public, private]);
        };

        let result = if first.beats(second) {
            state.wins[0] += 1;
            format!("{} takes the round", state.name(0))
        } else if second.beats(first) {
            state.wins[1] += 1;
            format!("{} takes the round", state.name(1))
        } else {
            "a draw".to_string()
        };
        let reveal = Effect::PublicLog(format!(
            "Round {}: {} throws {}, {} throws {} — {result}.",
            state.round,
            state.name(0),
            first.key(),
            state.name(1),
            second.key(),
        ));
        state.throws = [None, None];
        state.last_round = Some([first, second]);
        state.round += 1;

        let mut effects = vec![private, reveal];
        if let Some(winner) = state.winner() {
            let outcomes = (0..2)
                .map(|seat| Outcome {
                    player: state.players[seat].id,
                    rank: if seat == winner { 1 } else { 2 },
                    score: state.wins[seat].into(),
                    note: String::new(),
                })
                .collect();
            effects.push(Effect::GameOver(outcomes));
        }
        Ok(effects)
    }

    fn view(state: &State, viewer: Option<PlayerId>) -> View<Throw> {
        let viewer_seat = viewer.and_then(|player| state.seat(player));

        let mut status = vec![
            ("Round".to_string(), state.round.to_string()),
            (
                "Score".to_string(),
                format!(
                    "{} {} – {} {}",
                    state.name(0),
                    state.wins[0],
                    state.wins[1],
                    state.name(1)
                ),
            ),
        ];
        if let Some(winner) = state.winner() {
            status.push(("Winner".to_string(), state.name(winner).to_string()));
        }

        let throws = (0..2)
            .map(|seat| {
                let (face, sublabel) = match state.throws[seat] {
                    Some(throw) if viewer_seat == Some(seat) => {
                        (Face::Up(throw.key().into()), "thrown")
                    }
                    Some(_) => (Face::Down, "thrown"),
                    None => (Face::Empty, "thinking"),
                };
                Item {
                    sublabel: sublabel.into(),
                    ..Item::new(face, state.name(seat))
                }
            })
            .collect();
        let mut zones = vec![Zone {
            label: "This round".into(),
            layout: Layout::Row,
            items: throws,
        }];
        if let Some(last_round) = state.last_round {
            zones.push(Zone {
                label: "Last round".into(),
                layout: Layout::Row,
                items: (0..2)
                    .map(|seat| {
                        Item::new(Face::Up(last_round[seat].key().into()), state.name(seat))
                    })
                    .collect(),
            });
        }

        let prompt = viewer_seat
            .filter(|&seat| state.can_throw(seat))
            .map(|seat| Prompt {
                text: "Your throw".into(),
                choices: Self::actions(state, state.players[seat].id),
                deadline_seconds: None,
            });

        View {
            title: "Rock-Paper-Scissors".into(),
            status,
            zones,
            log: Vec::new(),
            prompt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Throw::*;

    fn start() -> State {
        RockPaperScissors::init(InitCtx {
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

    fn play(state: &mut State, player: PlayerId, throw: Throw) -> Vec<Effect> {
        RockPaperScissors::apply(state, player, throw).unwrap()
    }

    #[test]
    fn round_resolves_only_once_both_have_thrown() {
        let mut state = start();
        play(&mut state, 0, Rock);
        assert_eq!(state.wins, [0, 0]);
        assert!(RockPaperScissors::actions(&state, 0).is_empty());
        assert_eq!(RockPaperScissors::actions(&state, 1).len(), 3);

        play(&mut state, 1, Scissors);
        assert_eq!(state.wins, [1, 0]);
        assert_eq!(state.throws, [None, None]);
    }

    #[test]
    fn opponent_throw_is_face_down_before_reveal() {
        let mut state = start();
        play(&mut state, 0, Paper);

        let faces = |viewer| {
            RockPaperScissors::view(&state, viewer).zones[0]
                .items
                .iter()
                .map(|item| item.face.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(faces(Some(0)), [Face::Up("paper".into()), Face::Empty]);
        assert_eq!(faces(Some(1)), [Face::Down, Face::Empty]);
        assert_eq!(faces(None), [Face::Down, Face::Empty]);
    }

    #[test]
    fn throwing_twice_in_a_round_is_rejected() {
        let mut state = start();
        play(&mut state, 0, Rock);
        assert!(RockPaperScissors::apply(&mut state, 0, Paper).is_err());
    }

    #[test]
    fn draws_do_not_score_and_two_wins_end_the_match() {
        let mut state = start();
        play(&mut state, 0, Rock);
        play(&mut state, 1, Rock);
        assert_eq!(state.wins, [0, 0]);

        play(&mut state, 0, Paper);
        play(&mut state, 1, Scissors);
        play(&mut state, 1, Rock);
        let effects = play(&mut state, 0, Scissors);

        assert_eq!(state.wins, [0, 2]);
        assert!(matches!(effects.last(), Some(Effect::GameOver(_))));
        assert!(RockPaperScissors::actions(&state, 0).is_empty());
        assert!(RockPaperScissors::actions(&state, 1).is_empty());
    }
}
