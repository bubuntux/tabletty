//! The terminal interface: pick a game from the menu, or jump straight into one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::bail;
use tabletty_host::{Plugin, Runtime, discover};
use tabletty_tui::{App, GameEntry, SeedSource};

use crate::{games, paths};

#[derive(clap::Args, Default)]
pub struct Args {
    /// Skip the menu and start this game: a manifest id (e.g. `rps`) or a path to a
    /// `.wasm` component
    #[arg(long)]
    game: Option<String>,

    /// Players for `--game`; defaults to the game's minimum
    #[arg(long, requires = "game")]
    players: Option<u8>,

    /// Any string, hashed into every match's 32-byte seed so games replay identically.
    /// Without it each match gets a fresh random seed
    #[arg(long)]
    seed: Option<String>,

    /// Also search this directory for games, before ./dist/games and the install dir
    #[arg(long = "games-dir", value_name = "DIR")]
    games_dirs: Vec<PathBuf>,
}

pub fn main(args: Args) -> anyhow::Result<()> {
    let runtime = Runtime::new(paths::cwasm_cache_dir())?;
    let dirs = games::search_dirs(&args.games_dirs);
    let found = discover(&runtime, &dirs);
    let available = games::ids(&found);
    let mut entries: Vec<GameEntry> = found
        .into_iter()
        .map(|game| GameEntry {
            manifest: game.manifest,
            plugin: Arc::new(game.plugin),
        })
        .collect();

    let start = match &args.game {
        None => None,
        Some(path) if games::is_component_path(path) => {
            let plugin = games::load_component(&runtime, Path::new(path))?;
            entries.insert(
                0,
                GameEntry {
                    manifest: plugin.manifest()?,
                    plugin: Arc::new(plugin),
                },
            );
            Some(0)
        }
        Some(id) => match entries.iter().position(|entry| entry.manifest.id == *id) {
            Some(index) => Some(index),
            None => bail!("{}", games::not_found(id, &available, &dirs)),
        },
    };

    let players = start.map(|index| args.players.unwrap_or(entries[index].manifest.min_players));
    let mut app = App::new(entries, seed_source(args.seed.as_deref()));
    if let (Some(index), Some(players)) = (start, players) {
        app.start(index, players);
    }
    tabletty_tui::run(app)?;
    Ok(())
}

fn seed_source(seed: Option<&str>) -> SeedSource {
    match seed {
        Some(seed) => {
            let seed = *blake3::hash(seed.as_bytes()).as_bytes();
            Box::new(move || seed)
        }
        // The host may use OS entropy; only plugins are cut off from it.
        None => Box::new(|| {
            let mut seed = [0; 32];
            getrandom::fill(&mut seed).expect("the OS entropy source is available");
            seed
        }),
    }
}
