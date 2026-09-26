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

Early. The plugin boundary works end to end: games load as sandboxed components and play
through a headless hotseat harness. No TUI or networking yet. See
[docs/PLAN.md](docs/PLAN.md) for the architecture and build order.

## Building

Requires [devenv](https://devenv.sh):

```
devenv shell
cargo build
```

## License

tabletty is licensed under the [GNU General Public License v3.0 or later](LICENSE).

The plugin contract ([`wit/game.wit`](wit/game.wit)) and the SDK
([`crates/tabletty-sdk`](crates/tabletty-sdk)) are instead dual-licensed under
[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. The SDK is compiled
into every game plugin, so you can license your own games however you like.
