use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tabletty_sdk::{ActionSpec, InitCtx, Manifest, PlayerId, View};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap};

use crate::{Error, Plugin, Transition, cache};

mod bindings {
    wasmtime::component::bindgen!({
        path: "../sdk/wit",
        world: "game-plugin",
    });
}

use bindings as wit;

/// Fuel per call. Generous for any turn-based rules engine, and still stops an
/// infinite loop in well under a second.
const DEFAULT_FUEL_PER_CALL: u64 = 100_000_000;

const MEMORY_LIMIT_BYTES: usize = 64 << 20;

/// A configured wasmtime engine that loads game components.
pub struct Runtime {
    engine: Engine,
    /// Deliberately empty: a component that imports anything fails to instantiate.
    linker: Linker<Sandbox>,
    cache_dir: Option<PathBuf>,
    fuel_per_call: u64,
}

impl Runtime {
    /// `cache_dir` holds precompiled `.cwasm` files, so each component pays for
    /// Cranelift once rather than on every launch.
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self, Error> {
        let mut config = Config::new();
        config
            // Fuel, not epoch interruption: fuel is counted identically on every run,
            // epochs are wall-clock. Anything that can reach the replay log must be
            // bounded deterministically, or two peers auditing it can disagree.
            .consume_fuel(true)
            // NaN bit patterns are WASM arithmetic's one non-deterministic corner.
            .cranelift_nan_canonicalization(true)
            // Relaxed SIMD results are allowed to vary by CPU.
            .wasm_relaxed_simd(false)
            .wasm_threads(false);
        let engine = Engine::new(&config).map_err(Error::Load)?;
        Ok(Self {
            linker: Linker::new(&engine),
            engine,
            cache_dir,
            fuel_per_call: DEFAULT_FUEL_PER_CALL,
        })
    }

    pub fn with_fuel_per_call(mut self, fuel: u64) -> Self {
        self.fuel_per_call = fuel;
        self
    }

    pub fn load_file(&self, path: &Path) -> Result<WasmPlugin, Error> {
        let bytes = fs::read(path).map_err(|source| Error::Read {
            path: path.to_owned(),
            source,
        })?;
        self.load(&bytes)
    }

    pub fn load(&self, bytes: &[u8]) -> Result<WasmPlugin, Error> {
        let hash = blake3::hash(bytes);
        let component = match &self.cache_dir {
            Some(dir) => cache::load_or_compile(&self.engine, dir, &hash, bytes)?,
            None => Component::new(&self.engine, bytes).map_err(Error::Load)?,
        };
        let instance_pre = self
            .linker
            .instantiate_pre(&component)
            .map_err(Error::Load)?;
        Ok(WasmPlugin {
            pre: wit::GamePluginPre::new(instance_pre).map_err(Error::Load)?,
            engine: self.engine.clone(),
            hash,
            fuel_per_call: self.fuel_per_call,
            fuel_consumed: AtomicU64::new(0),
        })
    }
}

struct Sandbox {
    limits: StoreLimits,
}

/// A loaded game component.
pub struct WasmPlugin {
    pre: wit::GamePluginPre<Sandbox>,
    engine: Engine,
    hash: blake3::Hash,
    fuel_per_call: u64,
    fuel_consumed: AtomicU64,
}

impl WasmPlugin {
    /// The component's `blake3` hash; peers must agree on it before a match starts.
    pub fn hash(&self) -> blake3::Hash {
        self.hash
    }

    /// Fuel burned across every call so far. Identical inputs must burn identical
    /// fuel, which is what makes a fuel-based timeout safe to audit.
    pub fn fuel_consumed(&self) -> u64 {
        self.fuel_consumed.load(Ordering::Relaxed)
    }

    /// Run one export in a fresh store and instance.
    ///
    /// A new instance per call means nothing survives in linear memory between calls,
    /// so a plugin can't keep hidden mutable state behind the host's back — `apply`
    /// really is a function of its arguments. Instantiating a pre-linked component is
    /// cheap next to a human taking a turn.
    fn call<R>(
        &self,
        export: impl FnOnce(&wit::GamePlugin, &mut Store<Sandbox>) -> wasmtime::Result<R>,
    ) -> Result<R, Error> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(MEMORY_LIMIT_BYTES)
            .build();
        let mut store = Store::new(&self.engine, Sandbox { limits });
        store.limiter(|sandbox| &mut sandbox.limits);
        store
            .set_fuel(self.fuel_per_call)
            .expect("fuel is enabled on the engine");

        let result = self
            .pre
            .instantiate(&mut store)
            .and_then(|plugin| export(&plugin, &mut store));

        let remaining = store.get_fuel().expect("fuel is enabled on the engine");
        self.fuel_consumed
            .fetch_add(self.fuel_per_call - remaining, Ordering::Relaxed);

        result.map_err(|err| match err.downcast_ref::<Trap>() {
            Some(Trap::OutOfFuel) => Error::OutOfFuel,
            _ => Error::Trap(err),
        })
    }
}

impl Plugin for WasmPlugin {
    fn manifest(&self) -> Result<Manifest, Error> {
        self.call(|plugin, store| plugin.call_manifest(store))
            .map(Into::into)
    }

    fn init(&self, ctx: &InitCtx) -> Result<Vec<u8>, Error> {
        let ctx = wit::InitCtx::from(ctx.clone());
        self.call(|plugin, store| plugin.call_init(store, &ctx))
    }

    fn actions(&self, state: &[u8], player: PlayerId) -> Result<Vec<ActionSpec>, Error> {
        let specs =
            self.call(|plugin, store| plugin.call_actions(store, &state.to_vec(), player))?;
        Ok(specs.into_iter().map(Into::into).collect())
    }

    fn apply(
        &self,
        state: &[u8],
        player: PlayerId,
        action: &[u8],
    ) -> Result<Result<Transition, String>, Error> {
        let applied = self.call(|plugin, store| {
            plugin.call_apply(store, &state.to_vec(), player, &action.to_vec())
        })?;
        Ok(applied.map(|(state, effects)| Transition {
            state,
            effects: effects.into_iter().map(Into::into).collect(),
        }))
    }

    fn view(&self, state: &[u8], viewer: Option<PlayerId>) -> Result<View, Error> {
        self.call(|plugin, store| plugin.call_view(store, &state.to_vec(), viewer))
            .map(Into::into)
    }
}

impl From<InitCtx> for wit::InitCtx {
    fn from(ctx: InitCtx) -> Self {
        Self {
            players: ctx
                .players
                .into_iter()
                .map(|player| wit::Player {
                    id: player.id,
                    name: player.name,
                })
                .collect(),
            seed: ctx.seed,
            options: ctx.options,
        }
    }
}

impl From<wit::Manifest> for Manifest {
    fn from(manifest: wit::Manifest) -> Self {
        Self {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            min_players: manifest.min_players,
            max_players: manifest.max_players,
            summary: manifest.summary,
        }
    }
}

impl From<wit::ActionSpec> for ActionSpec {
    fn from(spec: wit::ActionSpec) -> Self {
        Self {
            id: spec.id,
            label: spec.label,
            payload: spec.payload,
            enabled: spec.enabled,
            reason: spec.reason,
        }
    }
}

impl From<wit::Effect> for tabletty_sdk::Effect {
    fn from(effect: wit::Effect) -> Self {
        use tabletty_sdk::{Outcome, PrivateMsg, Timer};
        match effect {
            wit::Effect::PublicLog(text) => Self::PublicLog(text),
            wit::Effect::PrivateLog(msg) => Self::PrivateLog(PrivateMsg {
                to: msg.to,
                text: msg.text,
            }),
            wit::Effect::SetTimer(timer) => Self::SetTimer(Timer {
                id: timer.id,
                seconds: timer.seconds,
            }),
            wit::Effect::CancelTimer(id) => Self::CancelTimer(id),
            wit::Effect::GameOver(outcomes) => Self::GameOver(
                outcomes
                    .into_iter()
                    .map(|outcome| Outcome {
                        player: outcome.player,
                        rank: outcome.rank,
                        score: outcome.score,
                        note: outcome.note,
                    })
                    .collect(),
            ),
        }
    }
}

impl From<wit::View> for View {
    fn from(view: wit::View) -> Self {
        Self {
            title: view.title,
            status: view.status,
            zones: view.zones.into_iter().map(Into::into).collect(),
            log: view.log.into_iter().map(Into::into).collect(),
            prompt: view.prompt.map(Into::into),
        }
    }
}

impl From<wit::Zone> for tabletty_sdk::Zone {
    fn from(zone: wit::Zone) -> Self {
        use tabletty_sdk::Layout;
        Self {
            label: zone.label,
            layout: match zone.layout {
                wit::Layout::Row => Layout::Row,
                wit::Layout::Grid => Layout::Grid,
                wit::Layout::Stack => Layout::Stack,
            },
            items: zone.items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<wit::Item> for tabletty_sdk::Item {
    fn from(item: wit::Item) -> Self {
        use tabletty_sdk::Face;
        Self {
            face: match item.face {
                wit::Face::Up(art) => Face::Up(art),
                wit::Face::Down => Face::Down,
                wit::Face::Empty => Face::Empty,
            },
            label: item.label,
            sublabel: item.sublabel,
            badges: item.badges,
            selectable: item.selectable.map(Into::into),
        }
    }
}

impl From<wit::LogLine> for tabletty_sdk::LogLine {
    fn from(line: wit::LogLine) -> Self {
        use tabletty_sdk::LogKind;
        Self {
            text: line.text,
            kind: match line.kind {
                wit::LogKind::Public => LogKind::Public,
                wit::LogKind::Private => LogKind::Private,
                wit::LogKind::System => LogKind::System,
            },
        }
    }
}

impl From<wit::Prompt> for tabletty_sdk::Prompt {
    fn from(prompt: wit::Prompt) -> Self {
        Self {
            text: prompt.text,
            choices: prompt.choices.into_iter().map(Into::into).collect(),
            deadline_seconds: prompt.deadline_seconds,
        }
    }
}
