//! Loading and sandboxing. Needs `game-build rps` first.

use std::path::PathBuf;

use rps::RockPaperScissors;
use tabletty_host::{Error, NativePlugin, Plugin, Runtime};

fn rps_component_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/games/rps.wasm");
    assert!(
        path.exists(),
        "{} is missing; run `game-build rps` first",
        path.display()
    );
    path
}

#[test]
fn component_with_an_import_is_refused() {
    let runtime = Runtime::new(None).unwrap();
    let clock = br#"(component (import "clock" (func)))"#;
    assert!(matches!(runtime.load(clock), Err(Error::Load(_))));
}

#[test]
fn running_out_of_fuel_is_reported_as_such() {
    let runtime = Runtime::new(None).unwrap().with_fuel_per_call(1_000);
    let plugin = runtime.load_file(&rps_component_path()).unwrap();
    assert!(matches!(plugin.manifest(), Err(Error::OutOfFuel)));
}

#[test]
fn precompiled_cache_is_written_and_reused() {
    let cache_dir =
        std::env::temp_dir().join(format!("tabletty-cwasm-test-{}", std::process::id()));
    let runtime = Runtime::new(Some(cache_dir.clone())).unwrap();

    let compiled = runtime.load_file(&rps_component_path()).unwrap();
    let cached_file = cache_dir.join(format!("{}.cwasm", compiled.hash().to_hex()));
    assert!(cached_file.exists());

    let reloaded = runtime.load_file(&rps_component_path()).unwrap();
    let native = NativePlugin::<RockPaperScissors>::new().manifest().unwrap();
    assert_eq!(reloaded.manifest().unwrap(), native);

    std::fs::remove_dir_all(cache_dir).unwrap();
}
