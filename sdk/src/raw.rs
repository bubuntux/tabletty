//! A [`Game`] at the byte level: state and actions as encoded payloads, exactly as
//! the host sees them.
//!
//! Both the component glue and the host's native test harness go through here, so a
//! game behaves identically whether it runs as WASM or as a native library.

use crate::{ActionSpec, Effect, Game, InitCtx, Manifest, Payload, PlayerId, View};

pub fn manifest<G: Game>() -> Manifest {
    G::manifest()
}

pub fn init<G: Game>(ctx: InitCtx) -> Vec<u8> {
    encode(&G::init(ctx))
}

pub fn actions<G: Game>(state: &[u8], player: PlayerId) -> Vec<ActionSpec> {
    G::actions(&decode_state::<G>(state), player)
        .into_iter()
        .map(|spec| spec.map_payload(|action| encode(&action)))
        .collect()
}

pub fn apply<G: Game>(
    state: &[u8],
    player: PlayerId,
    action: &[u8],
) -> Result<(Vec<u8>, Vec<Effect>), String> {
    let mut state = decode_state::<G>(state);
    let action: G::Action =
        postcard::from_bytes(action).map_err(|err| format!("malformed action: {err}"))?;
    let effects = G::apply(&mut state, player, action)?;
    Ok((encode(&state), effects))
}

pub fn view<G: Game>(state: &[u8], viewer: Option<PlayerId>) -> View<Payload> {
    G::view(&decode_state::<G>(state), viewer).map_payloads(|action| encode(&action))
}

fn encode(value: &impl serde::Serialize) -> Vec<u8> {
    postcard::to_allocvec(value).expect("postcard encoding into a Vec cannot fail")
}

/// State only ever comes from this game's own `init` or `apply`, so failing to decode
/// it is a bug or a tampered host — either way, trap rather than guess.
fn decode_state<G: Game>(state: &[u8]) -> G::State {
    postcard::from_bytes(state).expect("state was not produced by this game")
}
