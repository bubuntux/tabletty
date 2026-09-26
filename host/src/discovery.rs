//! Find installed games by asking each component for its manifest — there is no
//! sidecar file to keep in sync. Drop a `.wasm` into a games directory and it appears.

use std::fs;
use std::path::{Path, PathBuf};

use tabletty_sdk::Manifest;

use crate::{Plugin, Runtime, WasmPlugin};

pub struct Discovered {
    pub path: PathBuf,
    pub manifest: Manifest,
    pub plugin: WasmPlugin,
}

/// Load every `*.wasm` in `dirs`. Missing directories and broken components are
/// skipped with a log line; when two components claim the same id, the one from the
/// earlier directory wins.
pub fn discover(runtime: &Runtime, dirs: &[PathBuf]) -> Vec<Discovered> {
    let mut found: Vec<Discovered> = Vec::new();
    for path in dirs.iter().flat_map(|dir| components_in(dir)) {
        let loaded = runtime
            .load_file(&path)
            .and_then(|plugin| Ok((plugin.manifest()?, plugin)));
        let (manifest, plugin) = match loaded {
            Ok(loaded) => loaded,
            Err(err) => {
                tracing::warn!(path = %path.display(), "skipping game: {err}");
                continue;
            }
        };
        if let Some(existing) = found.iter().find(|game| game.manifest.id == manifest.id) {
            tracing::warn!(
                path = %path.display(),
                shadowed_by = %existing.path.display(),
                "skipping duplicate game id {:?}",
                manifest.id,
            );
            continue;
        }
        found.push(Discovered {
            path,
            manifest,
            plugin,
        });
    }
    found
}

fn components_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "wasm"))
        .collect();
    paths.sort();
    paths
}
