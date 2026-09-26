//! Finding games: `--game` is either a path to a component or a manifest id looked up
//! across the games directories.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use tabletty_host::{Discovered, Runtime, WasmPlugin, discover};

use crate::paths;

/// Where to look for installed games, in precedence order: `--games-dir` flags, then
/// `./dist/games` (what `game-build` writes), then the install directory.
pub fn search_dirs(extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = extra_dirs.to_vec();
    dirs.push(PathBuf::from("dist/games"));
    dirs.extend(paths::games_dir());
    dirs
}

/// `--game` names a file rather than an id.
pub fn is_component_path(game: &str) -> bool {
    Path::new(game).extension().is_some_and(|ext| ext == "wasm")
}

pub fn load_component(runtime: &Runtime, path: &Path) -> anyhow::Result<WasmPlugin> {
    runtime
        .load_file(path)
        .with_context(|| format!("loading {}", path.display()))
}

/// Resolve `--game` to a single loaded plugin.
pub fn load(runtime: &Runtime, game: &str, extra_dirs: &[PathBuf]) -> anyhow::Result<WasmPlugin> {
    if is_component_path(game) {
        return load_component(runtime, Path::new(game));
    }
    let dirs = search_dirs(extra_dirs);
    let found = discover(runtime, &dirs);
    let available = ids(&found);
    match found.into_iter().find(|found| found.manifest.id == game) {
        Some(found) => Ok(found.plugin),
        None => bail!("{}", not_found(game, &available, &dirs)),
    }
}

pub fn ids(found: &[Discovered]) -> String {
    let ids: Vec<&str> = found.iter().map(|game| game.manifest.id.as_str()).collect();
    ids.join(", ")
}

pub fn not_found(game: &str, available: &str, dirs: &[PathBuf]) -> String {
    if available.is_empty() {
        format!("no game {game:?}: no games found in {dirs:?}")
    } else {
        format!("no game {game:?}; available: {available}")
    }
}
