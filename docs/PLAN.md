# tabletty — P2P TUI board games with WASM plugins

## Context

A terminal app for playing hidden-information board games with friends over the internet.
Peer-to-peer, so there is no server to run or pay for, and games ship as plugins so new
ones can be added without touching the core.

No particular game is special. The build works up from throwaway-simple ones that exist
to prove the plugin boundary, through real games chosen for the machinery each exercises.
See Phases.

Starting from nothing. The shape of the project is set by two requirements that pull
against each other: these games turn on information only some players may see, and a
peer-to-peer design means there is no trusted party to hold it. Nearly every decision
below is downstream of resolving that.

### Environment

The toolchain lives in `devenv.nix` rather than being installed globally: a rust-overlay
stable toolchain carrying the `wasm32-unknown-unknown` target, plus `wasm-tools`,
`wasmtime`, `watchexec`, `cargo-nextest` and `cargo-deny`. Pin the `wasmtime` crate to
whatever version the CLI in the shell reports (currently 48.x) so the host and the
command-line tooling agree.

## Decisions

| Area | Choice |
|---|---|
| Authority | One peer is authoritative. Star topology through the host — the host's apply-order **is** the canonical order, so no consensus, no CRDTs, no vector clocks. |
| Fairness | `--spectate` host (authoritative peer isn't playing) + commit–reveal shuffle seed + deterministic replay audit. |
| Plugins | WASM component model from day one: `wasmtime` + `wit-bindgen`. |
| Transport | `iroh` — QUIC, NodeId is an ed25519 pubkey, relay-backed hole punching, ticket string doubles as a lobby code. |
| Rendering | Plugins emit a declarative `View` tree; the TUI draws it. Plugins never touch the terminal. |
| Plugin dist | Host serves components to peers over `iroh-blobs`, gated by a hash-accept prompt. |
| SDK | Thick — authors implement a trait and never see wit-bindgen, postcard, or the byte boundary. |

### Why hidden information drives the architecture

Dealerless hidden information is *mental poker* — commutative encryption plus ZK shuffle
proofs. Project-sized, and it doesn't generalise across plugins. Hence an authoritative host.

But a peeking host matters more than it first looks: in Love Letter it's a strong advantage;
in **ONUW, knowing every role deletes the game**. The ladder that fixes this without any
zero-knowledge work:

1. **`--spectate`** — same binary, host doesn't play. Werewolf always had a narrator.
2. **Commit–reveal seed** — everyone publishes `blake3(seed_i)`, then reveals;
   shared seed is the hash of the concatenation, so a *playing* host can't rig the deal (~50 lines).
3. **Replay audit** — host publishes initial state + action log at game end; peers re-run the
   plugin deterministically and check it matches what they saw. Catches lying about outcomes,
   not peeking. Nearly free given pure plugins.

## The WIT world is the contract

[`sdk/wit/game.wit`](../sdk/wit/game.wit) is the source of truth for the plugin interface: five
exports (`manifest`, `init`, `actions`, `apply`, `view`) and the types they move.

All `export`, **no `import`**. A component with zero imports cannot reach the clock,
entropy, the filesystem, or the network — determinism becomes a property of the module
rather than a rule authors must follow. The types are declared inside the world rather
than in an `interface`, because a world that `use`s an interface imports it, and even a
types-only import would break that.

To keep it that way, build plugins for **`wasm32-unknown-unknown`** and componentise with
`wasm-tools component new` (no WASI adapter). `wasm32-wasip2` emits components directly but
Rust std pulls in `wasi:cli/environment` and friends, which means satisfying clock/random
imports or fighting instantiation. Bonus: on `wasm32-unknown-unknown`, `getrandom` won't
link without an explicit backend, so a plugin *can't* accidentally reach for OS entropy.

Two things about the types that WIT can't say:

`item.selectable` and `prompt.choices` both carry `action-spec` on purpose — the same
action is reachable by selecting a card or by picking from an explicit list, and the
TUI decides which affordance to show. They are the same value, not two encodings.

`face.up` carries an art *key*, never glyphs. Keeping the renderer in charge of visuals
is what makes every game look like one application, and it means a plugin can't smuggle
escape sequences into the terminal.

### State and actions are opaque bytes

`state` and `action` are `list<u8>`; the host never interprets them. Plugins serialise with
postcard internally, hidden by the SDK.

Component-model `resource` handles would avoid re-serialisation per call, but passing state
explicitly is what makes **replay, the post-game audit, snapshot-to-peers, and host failover
fall out for free** — each needs state-as-bytes anyway. These states are well under a
kilobyte. Take the slow design; it buys four features.

Consequence worth exploiting: `action-spec` is `{ id, label, payload: list<u8> }`. The host
offers specs to the TUI for rendering and validates a submission by checking byte-equality
against one it offered — so **the host enforces legality while understanding zero rules**.
`apply` returning `Err` is the backstop for games whose action space is too large to enumerate.

### Effects are the only channel out

The `effect` variant above is the only way a plugin affects anything. No host callbacks —
that's the invariant that keeps the sandbox intact and `apply` a pure function. Timer
firings come back in as ordinary actions, so they land in the log and replay stays
deterministic.

### wasmtime config that matters

- `consume_fuel(true)` with a per-call budget, so a buggy or hostile plugin can't hang the
  event loop. **Fuel, not `epoch_interruption`** — fuel consumption is identical every run,
  epochs are wall-clock and therefore non-deterministic. Anything whose outcome reaches the
  replay log must be fuel-bounded or two peers auditing the same log can disagree.
- `nan_canonicalization(true)`, or ban floats in game logic. NaN bit patterns are WASM
  arithmetic's one non-deterministic corner.
- `StoreLimits` for memory; no threads.
- Precompile to `.cwasm` and cache, or every lobby pays for Cranelift.

### Plugins over the wire

Peers must run byte-identical components or state and views diverge, so the lobby handshake
exchanges a `blake3` hash and refuses to start on mismatch. Since the hash is already in
flight, serving the bytes is a small step: host offers the component over `iroh-blobs`
(a few hundred KB), peer accepts via prompt (`host is offering love-letter 0.2, blake3:a3f1… — accept?`).
Paste a ticket, get the game, play — nobody installs anything. Safe only because the plugin
is sandboxed; this is the thing dylibs could never have done.

Local discovery: scan `~/.local/share/tabletty/games/*.wasm`, call `manifest()` on each.
Drop in a file, the game appears — no sidecar manifest to keep in sync with the binary.

## Layout

```
devenv.nix            rust (wasm32-unknown-unknown) + wasm-tools + game-build script
devenv.yaml           nixpkgs + rust-overlay inputs
docs/PLAN.md          this file
sdk/                  tabletty-sdk, plugin-side: wit-bindgen glue, postcard, seeded-RNG
                      helper, a Game trait so authors write idiomatic Rust
  wit/game.wit        the contract — inside the SDK so it ships with the crate
host/                 tabletty-host: wasmtime Engine/Linker, fuel + limits, .cwasm cache,
                      discovery
engine/               tabletty-engine: match runtime — lobby→match, commit-reveal seed,
                      effect processing, timers, per-player view projection, audit log
net/                  tabletty-net: iroh endpoint, tickets, framing, iroh-blobs, seats
tui/                  tabletty-tui: ratatui View renderer + screens
cli/                  tabletty, the binary: clap, hotseat, replay, --spectate
games/
  rps/                canaries — trivial, permanent conformance fixtures
  tic-tac-toe/
  liars-dice/         each builds as a component AND a native lib (see Verification)
  .../               further games are just more directories here
```

Directories drop the `tabletty-` prefix; package names keep it. Everything under `sdk/` is
MIT OR Apache-2.0 so plugin authors can license games freely; the rest is GPL-3.0-or-later.

A single Cargo workspace: shared `[workspace.dependencies]`, members referring to each other
by path and taking deps with `foo.workspace = true`, so versions are pinned in one place.

`tabletty-sdk` must stay dependency-thin — no tokio, no iroh, no ratatui. Worth a `cargo-deny`
rule or a test asserting it.

## Libraries

| Layer | Crate | Note |
|---|---|---|
| TUI | `ratatui` + `crossterm` | `crossterm::event::EventStream` for async input |
| Runtime | `tokio` | one `select!` over input / net / timers → `Msg` → update → render |
| Transport | `iroh`, `iroh-blobs` | star topology; no gossip needed for ≤10 players |
| Plugins | `wasmtime`, `wit-bindgen` | `wasmtime` is the host crate, `wit-bindgen` the plugin-side proc macro. `wasm-tools` is a CLI from the shell, not a dependency |
| Codec | `postcard` + `serde` | compact, `no_std`, WASM-friendly; JSON behind a debug flag |
| Determinism | `rand_chacha` | `ChaCha20Rng` explicitly — `StdRng` isn't version-stable, which would silently break replay |
| Commitments | `blake3` | seed commitments + component hashes |
| Errors / CLI | `thiserror` in libs, `anyhow` + `clap` in the bin | typed errors where callers branch on them, opaque ones where they are only printed |
| Logs | `tracing` + `tracing-appender` | **to a file** — stdout is the UI, `println!` is not available |
| Tests | `proptest`, `insta` | run under `cargo-nextest` — per-test process isolation matters once tests instantiate wasmtime stores |

Signing note: iroh's QUIC connection is already authenticated to a NodeId pubkey, so
per-frame signatures buy nothing point-to-point. Sign only what gets *relayed and attributed* —
the audit log, where a peer must later prove "the host told me X."

## Phases

Each phase is defined by the **capability** it proves, not by the game that proves it.
The games below are vehicles, chosen because each one exercises the next piece of
machinery with the least rules code; swap any of them for something else that covers the
same ground. Every phase ends in something you can actually run.

### Phase 0 — Dev environment · **done**

`devenv.nix` with the toolchain and the `game-build` / `game-imports` helpers.

### Phase 1 — The contract and the boundary · **done**

`sdk/wit/game.wit`, `tabletty-sdk` (a `Game` trait plus `export_game!`), `tabletty-host`
(wasmtime, fuel, limits, `.cwasm` cache, discovery, and a `NativePlugin` for tests),
a minimal `tabletty-engine` `Match`, and `tabletty hotseat` — headless, stdin-driven.

Rock-Paper-Scissors and tic-tac-toe stay in the repo permanently as conformance
fixtures: between them they cover simultaneous hidden submission, quorum advance,
redaction, turn alternation, grid layout, `selectable` items, disabled actions, and
win/draw outcomes.

### Phase 2 — The renderer

`tabletty-tui`: the `view` tree rendered with ratatui, screens for menu and match, the
event loop (`tokio::select!` over input / timers, network later).

*Done when:* both canaries are playable on one machine with a real interface.

### Phase 3 — First real game

**Liar's Dice.** Simple rules, but it lands the machinery nothing so far has touched:
seeded RNG for the roll (all entropy from `init-ctx.seed`, so the same seed replays
identically), genuinely private per-player state, an escalating and enumerable action
space, challenge resolution, and player elimination.

*Done when:* a full multi-round game is playable in hotseat, and the redaction property
test passes — no serialised `view` for player A ever contains player B's dice.

### Phase 4 — Second real game

A game with **targeting and a real deck** — one player acting on another, card draw and
discard, per-card effects. Love Letter fits, so does Go Fish if you want less rules code.

The purpose is type stability: `view`, `effect` and `action-spec` are the wire format, so
they should stop changing before a protocol is built on them. Two real games plus two
canaries is enough signal that they've settled.

*Done when:* nothing in `sdk/wit/game.wit` has changed to accommodate this game that wasn't
already there for the last one.

### Phase 5 — Networking

`tabletty-net`: iroh endpoint, tickets as lobby codes, seats, the host/peer split,
`--spectate`, commit-reveal seed, and component transfer over `iroh-blobs` with the
hash-accept prompt.

*Done when:* three terminals on different machines — one `tabletty host --spectate`, two
joining by ticket, one of them without the component installed — play a game end to end.

### Phase 6 — The stress test

A game that pushes the API past where it's comfortable: **One Night Ultimate Werewolf**
is the obvious candidate — timed phases, sequential night wakes where nobody knows who is
acting, simultaneous voting, and a role that acts twice. It also only makes sense over
the network, which is why it lands after Phase 5 rather than before.

Expect to extend `effect` and `view` here. If Phase 4 did its job, expect to extend them
*only a little*.

### Phase 7 — Hardening

Replay-audit UI, reconnect-on-drop (cheap — NodeId is stable, so it's seat lookup plus a
view resend), host failover via per-turn signed snapshots, text chat, mouse selection.


## Verification

Games set `crate-type = ["cdylib", "rlib"]` so each builds as **both** a component and a
native lib. That dual target is what makes the rest of this testable:

- **Native rule tests** — table-driven on `apply`, fast and debuggable, no WASM in the loop.
- **Invariant props** (`proptest`, native): component conservation (every card or die
  accounted for in every reachable state); legal-action closure; no state where `actions()`
  is empty but the game has not ended.
- **Redaction prop** — for every state in a fuzz run, assert `view(st, p)` contains no bytes
  of another player's hidden cards. The one security-critical test; runs natively, so it's
  cheap enough for every commit.
- **Conformance suite** — replay scripted matches natively *and* through the component,
  assert identical state hashes and effect sequences. Native tests catch rule bugs; this
  catches ABI bugs.
- **Fuel determinism** — same match twice, assert identical fuel consumed.
- **Golden transcripts** — `tabletty hotseat --game rps --players 2 --seed 42` against
  scripted stdin, snapshotted with `insta`.
- **Replay** — `tabletty replay match.log` re-derives a byte-identical final state.
- **Manual end-to-end** — three terminals: one `tabletty host --spectate`, two joining by
  ticket, one of them without the component installed so the `iroh-blobs` transfer is
  exercised.
