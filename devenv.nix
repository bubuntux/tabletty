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

  # Build game crates and componentise them in one step: `game-build` builds every
  # crate under games/, `game-build rps tic-tac-toe` just those. One cargo call for
  # all of them, so shared dependencies compile once. The dash/underscore swap
  # bites every time: cargo writes love_letter.wasm for crate love-letter. Assumes
  # each games/<dir> holds a crate named <dir>.
  scripts.game-build.exec = ''
    set -euo pipefail
    cd "$DEVENV_ROOT"
    if [ "$#" -gt 0 ]; then
      crates=("$@")
    else
      crates=()
      for dir in games/*/; do
        crates+=("$(basename "$dir")")
      done
    fi
    packages=()
    for crate in "''${crates[@]}"; do
      packages+=(-p "$crate")
    done
    cargo build --release --target wasm32-unknown-unknown "''${packages[@]}"
    mkdir -p dist/games
    for crate in "''${crates[@]}"; do
      module="''${crate//-/_}"
      wasm-tools component new \
        "target/wasm32-unknown-unknown/release/$module.wasm" \
        -o "dist/games/$crate.wasm"
      echo "dist/games/$crate.wasm"
    done
  '';

  # Confirm a built plugin really has no imports. If this prints anything under
  # `import`, the sandbox guarantee is broken and replay can diverge.
  scripts.game-imports.exec = ''
    set -euo pipefail
    wasm-tools component wit "''${1:?usage: game-imports <component.wasm>}"
  '';

  # Everything CI checks, runnable locally with `devenv shell -- ci`. Components
  # are built first because the conformance and golden-transcript tests load them
  # from dist/games.
  scripts.ci.exec = ''
    set -euo pipefail
    cd "$DEVENV_ROOT"
    game-build
    for dir in games/*/; do
      crate="$(basename "$dir")"
      if wasm-tools component wit "dist/games/$crate.wasm" | grep -E '^\s*import'; then
        echo "error: $crate imports something; plugins must import nothing" >&2
        exit 1
      fi
    done
    cargo fmt --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo nextest run --locked
  '';

  enterShell = ''
    echo "tabletty devenv ready:"
    echo "  $(rustc --version)"
    echo "  $(wasm-tools --version)"
    echo "  targets: $(rustc --print target-list | grep -c wasm32) wasm variants available"
    echo
    echo "  game-build [crate...]   build + componentise game plugins (all if none given)"
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
