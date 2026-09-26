//! The types a game moves across the boundary, mirroring `wit/game.wit`.
//!
//! Anything that carries an action is generic over it. A game works with its own
//! typed `Action`; the default, [`Payload`], is the encoded form the host sees.

use serde::{Deserialize, Serialize};

pub type PlayerId = u8;

/// An encoded action: what actually crosses the boundary.
pub type Payload = Vec<u8>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Stable identifier used for discovery, e.g. `"love-letter"`.
    pub id: String,
    /// Shown to humans, e.g. `"Love Letter"`.
    pub name: String,
    pub version: String,
    pub min_players: u8,
    pub max_players: u8,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitCtx {
    pub players: Vec<Player>,
    /// 32 bytes from commit–reveal. The only entropy a game ever gets.
    pub seed: Vec<u8>,
    /// Per-game settings from the lobby.
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSpec<A = Payload> {
    /// Stable key for logs, tests and golden transcripts.
    pub id: String,
    /// What the TUI shows.
    pub label: String,
    /// Submitted verbatim; the host checks it byte-for-byte against what it offered.
    pub payload: A,
    pub enabled: bool,
    /// Why the action is disabled, shown as a hint.
    pub reason: Option<String>,
}

impl<A> ActionSpec<A> {
    /// An enabled action.
    pub fn new(id: impl Into<String>, label: impl Into<String>, payload: A) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            payload,
            enabled: true,
            reason: None,
        }
    }

    /// Offer the action greyed out, with a hint saying why.
    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.enabled = false;
        self.reason = Some(reason.into());
        self
    }

    pub fn map_payload<B>(self, f: impl FnOnce(A) -> B) -> ActionSpec<B> {
        ActionSpec {
            id: self.id,
            label: self.label,
            payload: f(self.payload),
            enabled: self.enabled,
            reason: self.reason,
        }
    }
}

/// The only way a game affects anything outside its own state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    PublicLog(String),
    PrivateLog(PrivateMsg),
    SetTimer(Timer),
    CancelTimer(String),
    GameOver(Vec<Outcome>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateMsg {
    pub to: PlayerId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    pub id: String,
    pub seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub player: PlayerId,
    pub rank: u8,
    pub score: i32,
    pub note: String,
}

/// The declarative render tree. Games emit it; the TUI draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View<A = Payload> {
    pub title: String,
    /// Key/value pairs such as `("Turn", "Alice")`.
    pub status: Vec<(String, String)>,
    pub zones: Vec<Zone<A>>,
    pub log: Vec<LogLine>,
    pub prompt: Option<Prompt<A>>,
}

impl<A> View<A> {
    pub fn map_payloads<B>(self, mut f: impl FnMut(A) -> B) -> View<B> {
        View {
            title: self.title,
            status: self.status,
            zones: self
                .zones
                .into_iter()
                .map(|zone| zone.map_payloads(&mut f))
                .collect(),
            log: self.log,
            prompt: self.prompt.map(|prompt| prompt.map_payloads(&mut f)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone<A = Payload> {
    pub label: String,
    pub layout: Layout,
    pub items: Vec<Item<A>>,
}

impl<A> Zone<A> {
    pub fn map_payloads<B>(self, mut f: impl FnMut(A) -> B) -> Zone<B> {
        Zone {
            label: self.label,
            layout: self.layout,
            items: self
                .items
                .into_iter()
                .map(|item| item.map_payloads(&mut f))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    Row,
    Grid,
    Stack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item<A = Payload> {
    pub face: Face,
    pub label: String,
    pub sublabel: String,
    pub badges: Vec<String>,
    /// Makes the item a click/enter target.
    pub selectable: Option<ActionSpec<A>>,
}

impl<A> Item<A> {
    /// An item with just a face and a label.
    pub fn new(face: Face, label: impl Into<String>) -> Self {
        Self {
            face,
            label: label.into(),
            sublabel: String::new(),
            badges: Vec::new(),
            selectable: None,
        }
    }

    pub fn map_payloads<B>(self, f: impl FnOnce(A) -> B) -> Item<B> {
        Item {
            face: self.face,
            label: self.label,
            sublabel: self.sublabel,
            badges: self.badges,
            selectable: self.selectable.map(|spec| spec.map_payload(f)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Face {
    /// An art key, e.g. `"guard"`. The renderer owns the glyphs.
    Up(String),
    Down,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    pub text: String,
    pub kind: LogKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogKind {
    Public,
    Private,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt<A = Payload> {
    pub text: String,
    pub choices: Vec<ActionSpec<A>>,
    /// Display only; the real clock is a timer effect.
    pub deadline_seconds: Option<u32>,
}

impl<A> Prompt<A> {
    pub fn map_payloads<B>(self, mut f: impl FnMut(A) -> B) -> Prompt<B> {
        Prompt {
            text: self.text,
            choices: self
                .choices
                .into_iter()
                .map(|spec| spec.map_payload(&mut f))
                .collect(),
            deadline_seconds: self.deadline_seconds,
        }
    }
}
