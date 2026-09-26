//! XDG base directories, each with a `tabletty` subdirectory.

use std::env;
use std::path::PathBuf;

/// Installed game components: drop a `.wasm` in here and it shows up.
pub fn games_dir() -> Option<PathBuf> {
    base_dir("XDG_DATA_HOME", ".local/share").map(|dir| dir.join("games"))
}

/// Precompiled `.cwasm` files.
pub fn cwasm_cache_dir() -> Option<PathBuf> {
    base_dir("XDG_CACHE_HOME", ".cache").map(|dir| dir.join("cwasm"))
}

/// Logs.
pub fn state_dir() -> Option<PathBuf> {
    base_dir("XDG_STATE_HOME", ".local/state")
}

fn base_dir(xdg_var: &str, home_fallback: &str) -> Option<PathBuf> {
    env::var_os(xdg_var)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(home_fallback)))
        .map(|dir| dir.join("tabletty"))
}
