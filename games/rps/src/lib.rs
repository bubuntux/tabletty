//! Rock-Paper-Scissors, first to two round wins.
//!
//! A conformance fixture, not a game anyone should play. It covers the half of the
//! contract tic-tac-toe doesn't: simultaneous hidden submission, advancing only once
//! every player is in, and redaction — nobody sees the other throw before the reveal.

use serde::{Deserialize, Serialize};
use tabletty_sdk::{
    ActionSpec, Art, Effect, Face, Game, InitCtx, Item, Layout, Manifest, Outcome, Player,
    PlayerId, PrivateMsg, Shape, Size, Tone, View, Zone,
};

const WINS_NEEDED: u8 = 2;

const ROCK_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <path d="M20 70 Q10 48 28 32 Q44 16 64 24 Q86 32 88 54 Q90 76 66 82 Q40 88 20 70 Z"
        fill="#8d8d8d" stroke="#4a4a4a" stroke-width="4"/>
  <path d="M36 40 Q46 32 58 37" fill="none" stroke="#bdbdbd" stroke-width="5"
        stroke-linecap="round"/>
</svg>"##;

const PAPER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <path d="M24 10 H62 L80 28 V90 H24 Z" fill="#f4f1e8" stroke="#6b6b6b" stroke-width="4"
        stroke-linejoin="round"/>
  <path d="M62 10 V28 H80" fill="none" stroke="#6b6b6b" stroke-width="4"
        stroke-linejoin="round"/>
  <path d="M34 46 H70 M34 58 H70 M34 70 H58" stroke="#9a9a9a" stroke-width="4"
        stroke-linecap="round"/>
</svg>"##;

const SCISSORS_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <path d="M38 64 L74 12" stroke="#7f8c8d" stroke-width="7" stroke-linecap="round"/>
  <path d="M62 64 L26 12" stroke="#95a5a6" stroke-width="7" stroke-linecap="round"/>
  <circle cx="30" cy="76" r="12" fill="none" stroke="#c0392b" stroke-width="6"/>
  <circle cx="70" cy="76" r="12" fill="none" stroke="#c0392b" stroke-width="6"/>
  <circle cx="50" cy="46" r="4" fill="#2c3e50"/>
</svg>"##;

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

    /// For tight spaces: one single-width character.
    fn glyph(self) -> &'static str {
        match self {
            Throw::Rock => "●",
            Throw::Paper => "▯",
            Throw::Scissors => "✂",
        }
    }

    fn svg(self) -> &'static str {
        match self {
            Throw::Rock => ROCK_SVG,
            Throw::Paper => PAPER_SVG,
            Throw::Scissors => SCISSORS_SVG,
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

        // Your own throw shows; the other stays face down until both are in.
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
                .shape(Shape::Card)
                .size(Size::Medium)
                .tone(Tone::Player(state.players[seat].id))
            })
            .collect();
        let mut zones = vec![Zone {
            label: "This round".into(),
            layout: Layout::Row,
            items: throws,
        }];

        if let Some(seat) = viewer_seat.filter(|&seat| state.can_throw(seat)) {
            let choices = Self::actions(state, state.players[seat].id)
                .into_iter()
                .zip(Throw::ALL)
                .map(|(spec, throw)| Item {
                    selectable: Some(spec),
                    ..Item::new(Face::Up(throw.key().into()), throw.label())
                        .shape(Shape::Icon)
                        .size(Size::Large)
                })
                .collect();
            zones.push(Zone {
                label: "Your throw".into(),
                layout: Layout::Row,
                items: choices,
            });
        }

        if let Some(last_round) = state.last_round {
            zones.push(Zone {
                label: "Last round".into(),
                layout: Layout::Row,
                items: (0..2)
                    .map(|seat| {
                        Item::new(Face::Up(last_round[seat].key().into()), state.name(seat))
                            .shape(Shape::Card)
                    })
                    .collect(),
            });
        }

        View {
            title: "Rock-Paper-Scissors".into(),
            status,
            zones,
            log: Vec::new(),
            prompt: None,
        }
    }

    fn art() -> Vec<Art> {
        Throw::ALL
            .into_iter()
            .map(|throw| Art {
                key: throw.key().into(),
                label: throw.label().into(),
                glyph: Some(throw.glyph().into()),
                svg: Some(throw.svg().into()),
                tone: None,
            })
            .collect()
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
