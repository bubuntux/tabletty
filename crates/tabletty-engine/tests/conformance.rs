//! Conformance: the same scripted match, run natively and through the component,
//! must agree step for step — state bytes, effects, offered actions, and every
//! player's view. Native tests catch rule bugs; this catches ABI bugs.
//!
//! Needs the components built first: `game-build rps && game-build tic-tac-toe`.

use std::path::PathBuf;

use rps::RockPaperScissors;
use tabletty_engine::Match;
use tabletty_host::{NativePlugin, Plugin, Runtime, WasmPlugin};
use tabletty_sdk::{Game, Player, PlayerId};
use tic_tac_toe::TicTacToe;

const SEED: [u8; 32] = [7; 32];

/// A draw, then a throw from a player who has already thrown this round.
const RPS_SCRIPT: &[(PlayerId, &str)] = &[
    (0, "rock"),
    (1, "rock"),
    (1, "paper"),
    (0, "scissors"),
    (0, "paper"),
    (0, "rock"),
    (1, "rock"),
];

/// Includes an attempt on a taken square, which is offered but disabled.
const TIC_TAC_TOE_SCRIPT: &[(PlayerId, &str)] = &[
    (0, "b2"),
    (1, "b2"),
    (1, "a1"),
    (0, "c1"),
    (1, "a3"),
    (0, "a2"),
    (1, "c2"),
    (0, "b3"),
    (1, "b1"),
    (0, "c3"),
];

fn component(crate_name: &str) -> WasmPlugin {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dist/games")
        .join(format!("{crate_name}.wasm"));
    assert!(
        path.exists(),
        "{} is missing; run `game-build {crate_name}` first",
        path.display()
    );
    Runtime::new(None)
        .unwrap()
        .load_file(&path)
        .expect("component loads")
}

fn players() -> Vec<Player> {
    ["Ann", "Bob"]
        .into_iter()
        .zip(0..)
        .map(|(name, id)| Player {
            id,
            name: name.into(),
        })
        .collect()
}

fn start<P: Plugin>(plugin: P) -> Match<P> {
    Match::start(plugin, players(), SEED, Vec::new()).expect("match starts")
}

/// Everything observable about a match at one point in time.
fn snapshot<P: Plugin>(game: &Match<P>) -> String {
    let viewers = [None, Some(0), Some(1)];
    format!(
        "state={:?}\nactions={:?}\nviews={:?}",
        game.state(),
        [0, 1].map(|player| game.actions(player).unwrap()),
        viewers.map(|viewer| game.view(viewer).unwrap()),
    )
}

/// Submit `action_id` for `player`, returning the result in a comparable form.
fn submit<P: Plugin>(game: &mut Match<P>, player: PlayerId, action_id: &str) -> String {
    let offered = game.actions(player).unwrap();
    let Some(spec) = offered.iter().find(|spec| spec.id == action_id) else {
        return format!("not offered: {action_id}");
    };
    format!("{:?}", game.submit(player, &spec.payload.clone()))
}

fn assert_conforms<G: Game>(crate_name: &str, script: &[(PlayerId, &str)]) {
    let mut native = start(NativePlugin::<G>::new());
    let mut wasm = start(component(crate_name));
    assert_eq!(native.manifest(), wasm.manifest());
    assert_eq!(snapshot(&native), snapshot(&wasm), "after init");

    for (step, &(player, action_id)) in script.iter().enumerate() {
        let native_result = submit(&mut native, player, action_id);
        let wasm_result = submit(&mut wasm, player, action_id);
        assert_eq!(
            native_result, wasm_result,
            "step {step}: {player} {action_id}"
        );
        assert_eq!(snapshot(&native), snapshot(&wasm), "after step {step}");
    }

    assert!(wasm.is_over(), "script should play the match to the end");
    assert_eq!(native.outcomes(), wasm.outcomes());
    assert_eq!(
        blake3::hash(native.state()),
        blake3::hash(wasm.state()),
        "final state hash"
    );
}

#[test]
fn rps_component_matches_native() {
    assert_conforms::<RockPaperScissors>("rps", RPS_SCRIPT);
}

#[test]
fn tic_tac_toe_component_matches_native() {
    assert_conforms::<TicTacToe>("tic-tac-toe", TIC_TAC_TOE_SCRIPT);
}

#[test]
fn same_match_burns_same_fuel() {
    let fuel_for_one_match = || {
        let mut game = start(component("tic-tac-toe"));
        for &(player, action_id) in TIC_TAC_TOE_SCRIPT {
            submit(&mut game, player, action_id);
            snapshot(&game);
        }
        game.plugin().fuel_consumed()
    };
    let first = fuel_for_one_match();
    assert!(first > 0);
    assert_eq!(first, fuel_for_one_match());
}
