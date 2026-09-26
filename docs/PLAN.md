# tabletty — P2P TUI board games with WASM plugins

## Context

A terminal app for playing hidden-information board games with friends over the internet —
Love Letter and One Night Ultimate Werewolf to begin with. Peer-to-peer, so there is no
server to run or pay for, and games ship as plugins so new ones can be added without
touching the core.

Starting from nothing. The shape of the project is set by two requirements that pull
against each other: both games turn on information only some players may see, and a
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

`wit/game.wit`:

```wit
package tabletty:game@0.1.0;

world game-plugin {
  export manifest: func() -> manifest;
  export init:     func(ctx: init-ctx) -> state;
  export actions:  func(st: state, p: player-id) -> list<action-spec>;
  export apply:    func(st: state, p: player-id, a: action)
                     -> result<tuple<state, list<effect>>, string>;
  export view:     func(st: state, p: option<player-id>) -> view;
}
```

All `export`, **no `import`**. A component with zero imports cannot reach the clock,
entropy, the filesystem, or the network — determinism becomes a property of the module
rather than a rule authors must follow.

To keep it that way, build plugins for **`wasm32-unknown-unknown`** and componentise with
`wasm-tools component new` (no WASI adapter). `wasm32-wasip2` emits components directly but
Rust std pulls in `wasi:cli/environment` and friends, which means satisfying clock/random
imports or fighting instantiation. Bonus: on `wasm32-unknown-unknown`, `getrandom` won't
link without an explicit backend, so a plugin *can't* accidentally reach for OS entropy.

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

`Effect` = `PublicLog(String)` | `PrivateLog { to, text }` | `SetTimer { id, secs }` |
`GameOver(Vec<Result>)`. No host callbacks — that's the invariant that keeps the sandbox
intact. Timer firings come back in as ordinary actions so they land in the log and replay
stays deterministic.

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
wit/game.wit          the contract
crates/
  tabletty-sdk/       plugin-side: wit-bindgen glue, postcard, seeded-RNG helper,
                      derive macros + a Game trait so authors write idiomatic Rust
  tabletty-host/      wasmtime Engine/Linker, fuel + limits, .cwasm cache, discovery
  tabletty-engine/    match runtime: lobby→match, commit-reveal seed, effect processing,
                      timers, per-player view projection, audit log
  tabletty-net/       iroh endpoint, tickets, framing, iroh-blobs transfer, seats
  tabletty-tui/       ratatui View renderer + screens
  tabletty/           bin: clap, hotseat, replay, --spectate
games/
  love-letter/        builds to a component AND a plain lib target (see Verification)
  onuw/
```

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

## Build order

0. **devenv shell — done.** Next: workspace skeleton + `wit/game.wit` first draft.
1. `tabletty-host` + `tabletty-sdk` + **hotseat harness** (headless, stdin-driven, file-watch hot
   reload). Build **Love Letter** against it. Validates the ABI before any networking exists,
   and stays the fastest way to develop a game forever. State is bytes, so a reload can often
   keep a match running.
2. `tabletty-tui` renders `View` in hotseat. Playable on one machine.
3. **ONUW rules in hotseat.** The API stress test: timers, sequential night wakes where nobody
   knows who's acting, simultaneous voting (`apply` accepts partial submissions and only
   transitions on quorum), Doppelgänger acting twice. Expect `View` and `Effect` to grow here.
4. `tabletty-net` + engine split: iroh star, tickets, seats, `--spectate`, commit-reveal seed,
   component transfer. Now it's multiplayer.
5. Deferred: replay-audit UI, reconnect-on-drop, host failover via per-turn signed snapshots,
   text chat, mouse selection.

**Sequencing judgment call:** ONUW lands at step 3, before the net layer, because `View` and
`Effect` *are* the wire format — letting ONUW reshape them first means the protocol is
designed against final types. Cheap to reorder while there are no deployed peers; if you'd
rather see it working over the network sooner, swap 3 and 4 and accept a protocol rev.

## Verification

Games set `crate-type = ["cdylib", "rlib"]` so each builds as **both** a component and a
native lib. That dual target is what makes the rest of this testable:

- **Native rule tests** — table-driven on `apply`, fast and debuggable, no WASM in the loop.
- **Invariant props** (`proptest`, native): card conservation (Love Letter's 16 cards always
  accounted for); legal-action closure; no state reachable where `actions()` is empty but the
  game isn't over.
- **Redaction prop** — for every state in a fuzz run, assert `view(st, p)` contains no bytes
  of another player's hidden cards. The one security-critical test; runs natively, so it's
  cheap enough for every commit.
- **Conformance suite** — replay scripted matches natively *and* through the component,
  assert identical state hashes and effect sequences. Native tests catch rule bugs; this
  catches ABI bugs.
- **Fuel determinism** — same match twice, assert identical fuel consumed.
- **Golden transcripts** — `tabletty hotseat --game love-letter --players 3 --seed 42` against
  scripted stdin, snapshotted with `insta`.
- **Replay** — `tabletty replay match.log` re-derives a byte-identical final state.
- **Manual end-to-end** — three terminals: one `tabletty host --spectate`, two joining by
  ticket, one of them without the component installed so the `iroh-blobs` transfer is
  exercised.
