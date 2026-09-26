//! The hotseat harness: one machine, every player typing into the same stdin.
//!
//! Headless on purpose: it exercises the plugin boundary and the engine with nothing
//! else in the way, and a script piped into it gives a reproducible transcript.
//! Stdout is this command's whole interface, so it writes there directly.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use tabletty_engine::{Match, SubmitError};
use tabletty_host::{Plugin, Runtime};
use tabletty_sdk::{Effect, Face, Item, Layout, Player, PlayerId, View};

use crate::{games, paths};

#[derive(clap::Args)]
pub struct Args {
    /// Game id from its manifest (e.g. `rps`), or a path to a `.wasm` component
    #[arg(long)]
    game: String,

    #[arg(long, default_value_t = 2)]
    players: u8,

    /// Any string; hashed into the match's 32-byte seed
    #[arg(long, default_value = "0")]
    seed: String,

    /// Also search this directory for games, before ./dist/games and the install dir
    #[arg(long = "games-dir", value_name = "DIR")]
    games_dirs: Vec<PathBuf>,
}

const HELP: &str = "\
commands:
  <player> <action>   submit an action, e.g. `0 rock`
  actions <player>    list what a player may do
  view [<player>]     show a player's view; no player is the spectator view
  help                this text
blank lines and lines starting with # are ignored";

pub fn main(args: Args) -> anyhow::Result<()> {
    let runtime = Runtime::new(paths::cwasm_cache_dir())?;
    let plugin = games::load(&runtime, &args.game, &args.games_dirs)?;
    let players = (0..args.players)
        .map(|id| Player {
            id,
            name: format!("P{id}"),
        })
        .collect();
    let seed = *blake3::hash(args.seed.as_bytes()).as_bytes();
    let mut game = Match::start(plugin, players, seed, Vec::new())?;
    run(&mut game, io::stdin().lock(), &mut io::stdout().lock())
}

/// Play `game` from `input` until it ends or input runs out, writing a transcript.
fn run<P: Plugin>(
    game: &mut Match<P>,
    input: impl BufRead,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let manifest = game.manifest();
    writeln!(out, "{} {} — hotseat", manifest.name, manifest.version)?;
    let seats: Vec<String> = game
        .players()
        .iter()
        .map(|player| format!("{} {}", player.id, player.name))
        .collect();
    writeln!(out, "players: {}", seats.join(", "))?;
    writeln!(out, "type `help` for commands")?;

    for line in input.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        writeln!(out, "> {line}")?;
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["help"] => writeln!(out, "{HELP}")?,
            ["view"] => write_view(out, &game.view(None)?)?,
            ["view", player] => match parse_player(game, player) {
                Ok(player) => write_view(out, &game.view(Some(player))?)?,
                Err(problem) => writeln!(out, "  ! {problem}")?,
            },
            ["actions", player] => match parse_player(game, player) {
                Ok(player) => write_actions(out, game, player)?,
                Err(problem) => writeln!(out, "  ! {problem}")?,
            },
            [player, action_id] => match parse_player(game, player) {
                Ok(player) => submit(out, game, player, action_id)?,
                Err(problem) => writeln!(out, "  ! {problem}")?,
            },
            _ => writeln!(out, "  ! unrecognised command; try `help`")?,
        }
        if game.is_over() {
            return write_outcomes(out, game);
        }
    }
    writeln!(out, "(input ended before the game finished)")?;
    Ok(())
}

fn parse_player<P: Plugin>(game: &Match<P>, word: &str) -> Result<PlayerId, String> {
    word.parse()
        .ok()
        .filter(|&id| game.players().iter().any(|player| player.id == id))
        .ok_or_else(|| format!("no player {word:?}"))
}

fn player_name<P: Plugin>(game: &Match<P>, id: PlayerId) -> String {
    game.players()
        .iter()
        .find(|player| player.id == id)
        .map_or_else(|| format!("player {id}"), |player| player.name.clone())
}

fn submit<P: Plugin>(
    out: &mut impl Write,
    game: &mut Match<P>,
    player: PlayerId,
    action_id: &str,
) -> anyhow::Result<()> {
    let offered = game.actions(player)?;
    let Some(spec) = offered.iter().find(|spec| spec.id == action_id) else {
        let name = player_name(game, player);
        let ids: Vec<&str> = offered.iter().map(|spec| spec.id.as_str()).collect();
        if ids.is_empty() {
            writeln!(out, "  ! {name} has nothing to do right now")?;
        } else {
            writeln!(
                out,
                "  ! {name} has no action {action_id:?}; offered: {}",
                ids.join(", ")
            )?;
        }
        return Ok(());
    };

    match game.submit(player, &spec.payload.clone()) {
        Ok(effects) => {
            for effect in effects {
                write_effect(out, game, effect)?;
            }
        }
        Err(SubmitError::Plugin(err)) => return Err(err.into()),
        Err(rejected) => writeln!(out, "  ! {rejected}")?,
    }
    Ok(())
}

fn write_effect<P: Plugin>(
    out: &mut impl Write,
    game: &Match<P>,
    effect: Effect,
) -> io::Result<()> {
    match effect {
        Effect::PublicLog(text) => writeln!(out, "  {text}"),
        Effect::PrivateLog(msg) => {
            writeln!(out, "  (to {}) {}", player_name(game, msg.to), msg.text)
        }
        Effect::SetTimer(timer) => writeln!(out, "  timer {} set: {}s", timer.id, timer.seconds),
        Effect::CancelTimer(id) => writeln!(out, "  timer {id} cancelled"),
        // Reported once the match ends, by `write_outcomes`.
        Effect::GameOver(_) => Ok(()),
    }
}

fn write_outcomes<P: Plugin>(out: &mut impl Write, game: &Match<P>) -> anyhow::Result<()> {
    writeln!(out, "game over")?;
    let mut outcomes = game.outcomes().unwrap_or_default().to_vec();
    outcomes.sort_by_key(|outcome| outcome.rank);
    for outcome in outcomes {
        write!(
            out,
            "  {}. {} (score {})",
            outcome.rank,
            player_name(game, outcome.player),
            outcome.score
        )?;
        if !outcome.note.is_empty() {
            write!(out, " — {}", outcome.note)?;
        }
        writeln!(out)?;
    }
    Ok(())
}

fn write_actions<P: Plugin>(
    out: &mut impl Write,
    game: &Match<P>,
    player: PlayerId,
) -> anyhow::Result<()> {
    let offered = game.actions(player)?;
    if offered.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for spec in offered {
        write!(out, "  {} — {}", spec.id, spec.label)?;
        if !spec.enabled {
            write!(
                out,
                " (disabled: {})",
                spec.reason.as_deref().unwrap_or("no reason given")
            )?;
        }
        writeln!(out)?;
    }
    Ok(())
}

/// A plain-text rendering of a view tree. Deliberately dumb: the real renderer is
/// the TUI, this only has to make a transcript readable.
fn write_view(out: &mut impl Write, view: &View) -> io::Result<()> {
    writeln!(out, "  == {} ==", view.title)?;
    if !view.status.is_empty() {
        let status: Vec<String> = view
            .status
            .iter()
            .map(|(key, value)| format!("{key}: {value}"))
            .collect();
        writeln!(out, "  {}", status.join(" · "))?;
    }
    for zone in &view.zones {
        writeln!(out, "  [{}]", zone.label)?;
        match zone.layout {
            Layout::Grid(columns) => write_grid(out, columns, &zone.items)?,
            Layout::Row | Layout::Stack => {
                for item in &zone.items {
                    write_item(out, item)?;
                }
            }
        }
    }
    for line in &view.log {
        writeln!(out, "  log: {}", line.text)?;
    }
    if let Some(prompt) = &view.prompt {
        let choices: Vec<&str> = prompt.choices.iter().map(|spec| spec.id.as_str()).collect();
        writeln!(out, "  prompt: {} [{}]", prompt.text, choices.join(", "))?;
    }
    Ok(())
}

fn face_text(face: &Face) -> &str {
    match face {
        Face::Up(art) => art,
        Face::Down => "##",
        Face::Empty => ".",
    }
}

fn write_item(out: &mut impl Write, item: &Item) -> io::Result<()> {
    write!(out, "    {}: {}", item.label, face_text(&item.face))?;
    if !item.sublabel.is_empty() {
        write!(out, " ({})", item.sublabel)?;
    }
    if !item.badges.is_empty() {
        write!(out, " [{}]", item.badges.join(", "))?;
    }
    if let Some(spec) = &item.selectable {
        write!(out, " -> {}", spec.id)?;
    }
    writeln!(out)
}

/// Lay items out in rows of `columns`. A plugin asking for zero columns gets one.
fn write_grid(out: &mut impl Write, columns: u8, items: &[Item]) -> io::Result<()> {
    let columns = usize::from(columns.max(1));
    let width = items
        .iter()
        .map(|item| face_text(&item.face).len())
        .max()
        .unwrap_or(1);
    for row in items.chunks(columns) {
        let cells: Vec<String> = row
            .iter()
            .map(|item| format!("{:width$}", face_text(&item.face)))
            .collect();
        writeln!(out, "    {}", cells.join(" "))?;
    }
    let selectable: Vec<&str> = items
        .iter()
        .filter_map(|item| Some(item.selectable.as_ref()?.id.as_str()))
        .collect();
    if !selectable.is_empty() {
        writeln!(out, "    selectable: {}", selectable.join(" "))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Golden transcripts, played through the real components. Needs `game-build` first.

    use super::*;

    fn transcript(game_crate: &str, script: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../dist/games")
            .join(format!("{game_crate}.wasm"));
        assert!(
            path.exists(),
            "{} is missing; run `game-build` first",
            path.display()
        );
        let plugin = Runtime::new(None).unwrap().load_file(&path).unwrap();
        let players = (0..2)
            .map(|id| Player {
                id,
                name: format!("P{id}"),
            })
            .collect();
        let seed = *blake3::hash(b"42").as_bytes();
        let mut game = Match::start(plugin, players, seed, Vec::new()).unwrap();

        let mut out = Vec::new();
        run(&mut game, script.as_bytes(), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn rps_full_match() {
        insta::assert_snapshot!(transcript(
            "rps",
            "\
# round 1: a draw
0 rock
view 1
1 rock
# round 2: P0 has already thrown, so their second throw is refused
0 paper
0 scissors
view 0
1 scissors
# round 3
actions 1
1 paper
0 scissors
# round 4 decides it
0 rock
1 scissors
",
        ));
    }

    #[test]
    fn tic_tac_toe_full_match() {
        insta::assert_snapshot!(transcript(
            "tic-tac-toe",
            "\
0 b2
1 b2
1 a1
1 c1
0 c1
view 1
actions 1
1 a3
0 a2
1 c2
0 b3
1 b1
0 c3
",
        ));
    }
}
