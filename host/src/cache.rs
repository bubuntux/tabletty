//! Precompiled `.cwasm` cache, keyed by the component's `blake3` hash.

use std::fs;
use std::path::Path;

use wasmtime::Engine;
use wasmtime::component::Component;

use crate::Error;

pub(crate) fn load_or_compile(
    engine: &Engine,
    dir: &Path,
    hash: &blake3::Hash,
    bytes: &[u8],
) -> Result<Component, Error> {
    let path = dir.join(format!("{}.cwasm", hash.to_hex()));

    if path.exists() {
        // SAFETY: deserialising runs no checks on the machine code itself, so the
        // file must come from `precompile_component`. Only this function writes into
        // the cache directory, and wasmtime rejects artifacts built by a different
        // version or config — in which case we fall through and recompile.
        match unsafe { Component::deserialize_file(engine, &path) } {
            Ok(component) => return Ok(component),
            Err(err) => {
                tracing::debug!(path = %path.display(), "stale cwasm, recompiling: {err:#}")
            }
        }
    }

    let compiled = engine.precompile_component(bytes).map_err(Error::Load)?;
    if let Err(err) = write_atomically(dir, &path, &compiled) {
        tracing::warn!(path = %path.display(), "could not cache compiled component: {err}");
    }
    // SAFETY: `compiled` came straight from `precompile_component` on this engine.
    unsafe { Component::deserialize(engine, &compiled) }.map_err(Error::Load)
}

/// Write via a temp file and rename, so a concurrent reader never sees a torn file.
fn write_atomically(dir: &Path, path: &Path, contents: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let temp = path.with_extension(format!("cwasm.{}.tmp", std::process::id()));
    fs::write(&temp, contents)?;
    fs::rename(&temp, path)
}
