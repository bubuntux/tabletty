//! The match runtime: owns the state, decides what is legal to submit, and records
//! the log that replay and the post-game audit re-run.
//!
//! Legality needs no game rules. The engine only accepts a payload byte-identical to
//! an enabled action the plugin itself offered that player, so it enforces the rules
//! while understanding none of them. The plugin's `apply` returning `Err` is the
//! backstop for games whose action space is too large to enumerate.

use tabletty_host::Plugin;
use tabletty_sdk::{
    ActionSpec, Effect, InitCtx, Manifest, Outcome, Payload, Player, PlayerId, View,
};

pub struct Match<P> {
    plugin: P,
    manifest: Manifest,
    players: Vec<Player>,
    state: Vec<u8>,
    history: Vec<Submission>,
    outcomes: Option<Vec<Outcome>>,
}

/// One accepted action, in apply order. Initial context plus this log reproduces
/// every state of the match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    pub player: PlayerId,
    pub action: Payload,
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("{game} needs {min}–{max} players, got {got}")]
    PlayerCount {
        game: String,
        min: u8,
        max: u8,
        got: usize,
    },
    #[error(transparent)]
    Plugin(#[from] tabletty_host::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    #[error("the game is over")]
    GameOver,
    #[error("that action was not offered to this player")]
    NotOffered,
    #[error("{id} is not available: {reason}")]
    Disabled { id: String, reason: String },
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Plugin(#[from] tabletty_host::Error),
}

impl<P: Plugin> Match<P> {
    pub fn start(
        plugin: P,
        players: Vec<Player>,
        seed: [u8; 32],
        options: Vec<(String, String)>,
    ) -> Result<Self, StartError> {
        let manifest = plugin.manifest()?;
        let player_range = usize::from(manifest.min_players)..=usize::from(manifest.max_players);
        if !player_range.contains(&players.len()) {
            return Err(StartError::PlayerCount {
                game: manifest.name,
                min: manifest.min_players,
                max: manifest.max_players,
                got: players.len(),
            });
        }
        let state = plugin.init(&InitCtx {
            players: players.clone(),
            seed: seed.to_vec(),
            options,
        })?;
        Ok(Self {
            plugin,
            manifest,
            players,
            state,
            history: Vec::new(),
            outcomes: None,
        })
    }

    /// Submit `action` for `player`. Accepted only if it matches, byte for byte, an
    /// enabled action currently offered to that player.
    pub fn submit(&mut self, player: PlayerId, action: &[u8]) -> Result<Vec<Effect>, SubmitError> {
        if self.is_over() {
            return Err(SubmitError::GameOver);
        }
        let offered = self.plugin.actions(&self.state, player)?;
        let spec = offered
            .into_iter()
            .find(|spec| spec.payload == action)
            .ok_or(SubmitError::NotOffered)?;
        if !spec.enabled {
            return Err(SubmitError::Disabled {
                id: spec.id,
                reason: spec.reason.unwrap_or_default(),
            });
        }

        let transition = self
            .plugin
            .apply(&self.state, player, action)?
            .map_err(SubmitError::Rejected)?;
        self.state = transition.state;
        self.history.push(Submission {
            player,
            action: action.to_vec(),
        });
        for effect in &transition.effects {
            if let Effect::GameOver(outcomes) = effect {
                self.outcomes = Some(outcomes.clone());
            }
        }
        Ok(transition.effects)
    }

    pub fn actions(&self, player: PlayerId) -> Result<Vec<ActionSpec>, tabletty_host::Error> {
        self.plugin.actions(&self.state, player)
    }

    /// What `viewer` may see; `None` is a spectator.
    pub fn view(&self, viewer: Option<PlayerId>) -> Result<View, tabletty_host::Error> {
        self.plugin.view(&self.state, viewer)
    }

    pub fn is_over(&self) -> bool {
        self.outcomes.is_some()
    }

    pub fn outcomes(&self) -> Option<&[Outcome]> {
        self.outcomes.as_deref()
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn players(&self) -> &[Player] {
        &self.players
    }

    pub fn state(&self) -> &[u8] {
        &self.state
    }

    pub fn history(&self) -> &[Submission] {
        &self.history
    }

    pub fn plugin(&self) -> &P {
        &self.plugin
    }
}
