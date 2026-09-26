//! Everything the SDK depends on ends up inside every plugin, and plugins must stay
//! sandboxable. Keep async runtimes, networking and terminal crates out of it.

use std::process::Command;

const FORBIDDEN: &[&str] = &[
    "tokio",
    "async-std",
    "iroh",
    "iroh-blobs",
    "ratatui",
    "crossterm",
    "wasmtime",
    "getrandom",
];

#[test]
fn sdk_dependency_tree_stays_thin() {
    let output = Command::new(env!("CARGO"))
        .args(["tree", "--package", "tabletty-sdk", "--edges", "normal"])
        .args([
            "--target",
            "wasm32-unknown-unknown",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let tree = String::from_utf8(output.stdout).unwrap();
    let crate_names: Vec<&str> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    let offenders: Vec<&&str> = FORBIDDEN
        .iter()
        .filter(|forbidden| crate_names.contains(forbidden))
        .collect();
    assert!(
        offenders.is_empty(),
        "tabletty-sdk depends on {offenders:?}"
    );
}
