//! Write a tabletty game.
//!
//! Implement [`Game`] with your own typed state and action, then export it:
//!
//! ```ignore
//! tabletty_sdk::export_game!(MyGame);
//! ```
//!
//! That is the whole boundary. State and actions are serialised with postcard behind
//! your back, and the wit-bindgen glue is generated here, not in your crate.
//!
//! A game must be a pure function of its inputs: all entropy arrives through
//! [`InitCtx::seed`], and the only way out is the returned [`Effect`] list.

mod guest;
pub mod raw;
mod types;

pub use types::*;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub trait Game {
    type State: Serialize + DeserializeOwned;
    type Action: Serialize + DeserializeOwned;

    fn manifest() -> Manifest;

    fn init(ctx: InitCtx) -> Self::State;

    /// Everything `player` may do right now. Disabled actions are shown but can't
    /// be submitted; the host only accepts a payload it offered here.
    fn actions(state: &Self::State, player: PlayerId) -> Vec<ActionSpec<Self::Action>>;

    /// Apply `player`'s action. On `Err` the host discards `state`, so a half-applied
    /// mutation is harmless.
    fn apply(
        state: &mut Self::State,
        player: PlayerId,
        action: Self::Action,
    ) -> Result<Vec<Effect>, String>;

    /// What `viewer` may see; `None` is a spectator. This is the redaction point:
    /// never put another player's hidden information in here.
    fn view(state: &Self::State, viewer: Option<PlayerId>) -> View<Self::Action>;
}

/// Export a [`Game`] as the plugin's component interface.
///
/// Expands to nothing off wasm32, so the same crate also builds as a native library
/// for tests.
#[macro_export]
macro_rules! export_game {
    ($game:ty) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            type TablettyGame = $crate::__private::Adapter<$game>;
            $crate::__private::export_game_plugin!(
                TablettyGame with_types_in $crate::__private::bindings
            );
        };
    };
}

#[doc(hidden)]
pub mod __private {
    pub use crate::guest::bindings::export_game_plugin;
    pub use crate::guest::{Adapter, bindings};
}
