# tabletty

A TUI for playing hidden-information board games peer-to-peer. Games are not built in —
each is a sandboxed WebAssembly component loaded at runtime.

**Read `docs/PLAN.md` before changing anything.** It holds the architecture, the decisions
and *why* they were made, the crate layout, the build order, and the verification strategy.
This file is only the operational summary.

Status: Phase 1 done (contract, SDK, host, engine `Match`, hotseat harness, and the
rps / tic-tac-toe fixtures). Phase 2 is the ratatui renderer.

## Commands

Everything runs inside the dev shell:

```
devenv shell                    # enter the environment
devenv test                     # runs the tabletty:check-toolchain task
devenv shell -- ci              # everything CI runs: build games, import check, fmt, clippy, tests

game-build <crate>              # build a game crate -> dist/games/<crate>.wasm
game-imports <component.wasm>   # dump a component's world; imports MUST be empty

cargo nextest run               # needs `game-build rps` and `game-build tic-tac-toe` first
cargo clippy --all-targets
cargo fmt

cargo run -- hotseat --game rps --players 2 --seed 42   # stdin-driven; `help` lists commands
```

The conformance, fuel and golden-transcript tests load the components from `dist/games/`.
Rebuild them after changing a game or the SDK, or the tests run stale WASM against fresh
native code (conformance will usually catch this and fail).

## Invariants

Load-bearing. Breaking any one of these silently breaks the sandbox, replay, or the
post-game audit — the failure shows up much later and far from the cause.

- **Plugins import nothing.** Build games for `wasm32-unknown-unknown` and componentise
  with `wasm-tools component new`, no WASI adapter. A plugin must not be able to reach a
  clock, entropy, the filesystem, or the network. `game-imports` is how you check.
- **Plugins are pure.** `apply` is a deterministic function of `(state, player, action)`.
  All entropy arrives through `init-ctx.seed`. The only channel out is the returned
  `effect` list — never add a host callback.
- **Fuel, not epochs.** `epoch_interruption` is wall-clock and therefore non-deterministic;
  two peers auditing the same log would disagree about a timeout. Use `consume_fuel`.
- **No floats in game logic**, or turn on `nan_canonicalization`. NaN bit patterns are the
  one non-deterministic corner of WASM arithmetic.
- **`tabletty-sdk` stays dependency-thin.** No tokio, no iroh, no ratatui. It is the only
  crate a plugin author depends on.
- **Plugins never render.** They emit the declarative `view` tree; the TUI owns all glyphs
  and styling. `face.up` carries an art key, never terminal output.
- **Never `println!`.** stdout is the UI. Log through `tracing` to a file.

## Layout

`sdk/` (with the contract in `sdk/wit/game.wit`), `host/`, `engine/`, `cli/` (the
`tabletty` binary), later `net/` and `tui/`, and `games/*` for the plugins. Directories
drop the `tabletty-` prefix; package names keep it. See the Layout section of
`docs/PLAN.md` for what each one owns.

## Docs

`docs/PLAN.md` is deliberately one file — at roughly 4.5k tokens it is cheaper to read
whole than to chase across split files, and per-phase specs written in advance get
rewritten by what the previous phase teaches.

Two rules keep it that way:

- When code replaces prose, delete the prose. `sdk/wit/game.wit` supersedes the type surface
  section the moment it exists; a duplicated contract drifts.
- Write `docs/phases/N-name.md` just-in-time, at most one phase ahead, and only when a
  phase needs more detail than the plan carries.

## Version control

`jj`, colocated with git.
