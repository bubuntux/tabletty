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

Early. The development environment and the design are settled; there's no code yet.
See [docs/PLAN.md](docs/PLAN.md) for the architecture and build order.

## Building

Requires [devenv](https://devenv.sh):

```
devenv shell
cargo build
```
