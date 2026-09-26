# tabletty

Play tabletop games in your terminal, with friends, over the internet.

Peer-to-peer — there's no server to run and nothing to pay for. You share a ticket, people
join, you play.

Games aren't built in. Each one is a sandboxed WebAssembly component, so new games can be
added without touching the core — and if you join a game you don't have, the host hands it
to you over the wire.

Games run from trivial — Rock-Paper-Scissors and tic-tac-toe, which exist only to prove
the plugin boundary — up through Liar's Dice, and on to the likes of Love Letter and
One Night Ultimate Werewolf.

## Status

Early. Games load as sandboxed components and play hotseat on one machine, in the terminal
interface or through a headless stdin harness. No networking yet. See
[docs/PLAN.md](docs/PLAN.md) for the architecture and build order.

## Layout

```
sdk/       tabletty-sdk     what a game author depends on; includes the plugin contract (wit/)
host/      tabletty-host    runs one plugin safely: sandbox, fuel, memory limits, discovery
engine/    tabletty-engine  runs one match: state, legal actions, effects, the action log
net/       tabletty-net     peer-to-peer lobbies and game transfer (planned)
tui/       tabletty-tui     the terminal interface: menu, board rendering, hotseat play
cli/       tabletty         the binary; wires the pieces above into modes like `hotseat`
games/     the games, each one a plugin built against the SDK
```

Dependencies only point one way: games need nothing but the SDK, and nothing depends on
the binary. The `host` knows how to run a plugin, the `engine` knows how a match works,
and the `cli` decides which pieces to combine.

## Building

Requires [devenv](https://devenv.sh):

```
devenv shell
game-build rps && game-build tic-tac-toe
cargo run
```

## License

tabletty is licensed under the [GNU General Public License v3.0 or later](LICENSE).

The SDK ([`sdk/`](sdk)), including the plugin contract in
[`sdk/wit/game.wit`](sdk/wit/game.wit), is instead dual-licensed under
[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. The SDK is compiled
into every game plugin, so you can license your own games however you like.
