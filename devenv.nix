{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

{
  # devenv.cachix.org is already in the system /etc/nix/nix.conf, so tell
  # devenv not to auto-configure it. Silences the "Substituter ... is already
  # present" warning on shell entry; the cache still works via the system config.
  cachix.enable = false;

  # Rust via rust-overlay rather than nixpkgs, because game plugins are built for
  # wasm32-unknown-unknown and only a channel toolchain can add that target.
  #
  # Deliberately NOT wasm32-wasip2: that target emits components directly, but
  # Rust std drags in wasi:cli/environment and friends, so plugins would import
  # a clock and entropy. Building bare wasm32 and componentising with
  # `wasm-tools component new` (no WASI adapter) leaves a plugin with zero
  # imports, which is what makes determinism structural instead of a convention.
  languages.rust = {
    enable = true;
    channel = "stable";
    targets = [ "wasm32-unknown-unknown" ];
  };

  packages = [
    pkgs.wasm-tools # `component new` — core module -> component
    pkgs.wasmtime # CLI, for poking at built components by hand
    pkgs.watchexec # hot reload for the hotseat harness (cargo-watch is archived)
    pkgs.cargo-nextest # test runner
    pkgs.cargo-deny # enforces that tabletty-sdk stays dependency-thin
  ];

  # Build a game crate and componentise it in one step. The dash/underscore
  # swap bites every time: cargo writes love_letter.wasm for crate love-letter.
  scripts.game-build.exec = ''
    set -euo pipefail
    crate="''${1:?usage: game-build <crate-name>}"
    module="''${crate//-/_}"
    cargo build --release --target wasm32-unknown-unknown -p "$crate"
    mkdir -p dist/games
    wasm-tools component new \
      "target/wasm32-unknown-unknown/release/$module.wasm" \
      -o "dist/games/$crate.wasm"
    echo "dist/games/$crate.wasm"
  '';

  # Confirm a built plugin really has no imports. If this prints anything under
  # `import`, the sandbox guarantee is broken and replay can diverge.
  scripts.game-imports.exec = ''
    set -euo pipefail
    wasm-tools component wit "''${1:?usage: game-imports <component.wasm>}"
  '';

  enterShell = ''
    echo "tabletty devenv ready:"
    echo "  $(rustc --version)"
    echo "  $(wasm-tools --version)"
    echo "  targets: $(rustc --print target-list | grep -c wasm32) wasm variants available"
    echo
    echo "  game-build <crate>      build + componentise a game plugin"
    echo "  game-imports <wasm>     dump a component's world (imports must be empty)"
  '';

  # `devenv test` runs tasks, not the `enterTest` string option (that one only
  # fires under `devenv shell`), so the toolchain check has to be a task to
  # actually run in CI.
  tasks."tabletty:check-toolchain" = {
    before = [ "devenv:enterTest" ];
    exec = ''
      set -euo pipefail
      rustc --print target-list | grep -qx wasm32-unknown-unknown
      test -d "$(rustc --print target-libdir --target wasm32-unknown-unknown)"
      wasm-tools --version >/dev/null
      wasmtime --version >/dev/null
      echo "toolchain ok: $(rustc --version), wasm32 target present"
    '';
  };
}
