//! Runs game plugins.
//!
//! [`Plugin`] is a game at the byte level, as the rest of tabletty sees it. There are
//! two implementations: [`WasmPlugin`], the sandboxed component that real matches
//! use, and [`NativePlugin`], the same rules linked in as a Rust library so tests can
//! check that the two agree.

mod cache;
mod discovery;
mod native;
mod wasm;

pub use discovery::{Discovered, discover};
pub use native::NativePlugin;
pub use wasm::{Runtime, WasmPlugin};

use std::io;
use std::path::PathBuf;

use tabletty_sdk::{ActionSpec, Effect, InitCtx, Manifest, PlayerId, View};

pub trait Plugin {
    fn manifest(&self) -> Result<Manifest, Error>;

    fn init(&self, ctx: &InitCtx) -> Result<Vec<u8>, Error>;

    fn actions(&self, state: &[u8], player: PlayerId) -> Result<Vec<ActionSpec>, Error>;

    /// The outer `Result` is the plugin failing; the inner one is the game
    /// rejecting the action.
    fn apply(
        &self,
        state: &[u8],
        player: PlayerId,
        action: &[u8],
    ) -> Result<Result<Transition, String>, Error>;

    fn view(&self, state: &[u8], viewer: Option<PlayerId>) -> Result<View, Error>;
}

/// The result of an accepted action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub state: Vec<u8>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("not a loadable game component: {0:#}")]
    Load(wasmtime::Error),
    #[error("plugin ran out of fuel")]
    OutOfFuel,
    #[error("plugin trapped: {0:#}")]
    Trap(wasmtime::Error),
}
