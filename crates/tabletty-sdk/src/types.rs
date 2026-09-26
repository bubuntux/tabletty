//! The types a game moves across the boundary.
//!
//! Most are generated from `wit/game.wit` and re-exported as-is. The five that carry
//! an action — [`ActionSpec`], [`View`], [`Zone`], [`Item`], [`Prompt`] — are written
//! out here so they can be generic over it: a game works with its own typed `Action`,
//! and the default, [`Payload`], is the encoded form the host sees. `guest.rs`
//! converts them to the generated wire types, so a WIT change that isn't mirrored
//! here fails to compile.

use serde::{Deserialize, Serialize};

pub use crate::guest::bindings::{
    Effect, Face, InitCtx, Layout, LogKind, LogLine, Manifest, Outcome, Player, PlayerId,
    PrivateMsg, Timer,
};

/// An encoded action: what actually crosses the boundary.
pub type Payload = Vec<u8>;

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
