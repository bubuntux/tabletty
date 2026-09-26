use std::marker::PhantomData;

use tabletty_sdk::{ActionSpec, Game, InitCtx, Manifest, PlayerId, View, raw};

use crate::{Error, Plugin, Transition};

/// A game linked in as a Rust library rather than loaded as a component.
///
/// Goes through the same `tabletty_sdk::raw` layer the component glue does, so any
/// difference from the [`WasmPlugin`](crate::WasmPlugin) is an ABI bug, not a rules
/// bug. Not sandboxed: for tests only.
pub struct NativePlugin<G>(PhantomData<G>);

impl<G> NativePlugin<G> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<G> Default for NativePlugin<G> {
    fn default() -> Self {
        Self::new()
    }
}

impl<G: Game> Plugin for NativePlugin<G> {
    fn manifest(&self) -> Result<Manifest, Error> {
        Ok(raw::manifest::<G>())
    }

    fn init(&self, ctx: &InitCtx) -> Result<Vec<u8>, Error> {
        Ok(raw::init::<G>(ctx.clone()))
    }

    fn actions(&self, state: &[u8], player: PlayerId) -> Result<Vec<ActionSpec>, Error> {
        Ok(raw::actions::<G>(state, player))
    }

    fn apply(
        &self,
        state: &[u8],
        player: PlayerId,
        action: &[u8],
    ) -> Result<Result<Transition, String>, Error> {
        Ok(raw::apply::<G>(state, player, action)
            .map(|(state, effects)| Transition { state, effects }))
    }

    fn view(&self, state: &[u8], viewer: Option<PlayerId>) -> Result<View, Error> {
        Ok(raw::view::<G>(state, viewer))
    }
}
