//! The wit-bindgen side of the boundary. Authors never see this: `export_game!`
//! points the generated exports at [`Adapter`], which routes each call through
//! [`crate::raw`] and converts between the generated types and the SDK's own.

use std::marker::PhantomData;

use crate::{Game, raw};

pub mod bindings {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "game-plugin",
        pub_export_macro: true,
        export_macro_name: "export_game_plugin",
    });
}

use bindings as wit;

pub struct Adapter<G>(PhantomData<G>);

impl<G: Game> wit::Guest for Adapter<G> {
    fn manifest() -> wit::Manifest {
        raw::manifest::<G>().into()
    }

    fn init(ctx: wit::InitCtx) -> wit::State {
        raw::init::<G>(ctx.into())
    }

    fn actions(state: wit::State, player: wit::PlayerId) -> Vec<wit::ActionSpec> {
        raw::actions::<G>(&state, player)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    fn apply(
        state: wit::State,
        player: wit::PlayerId,
        action: wit::Action,
    ) -> Result<(wit::State, Vec<wit::Effect>), String> {
        let (state, effects) = raw::apply::<G>(&state, player, &action)?;
        Ok((state, effects.into_iter().map(Into::into).collect()))
    }

    fn view(state: wit::State, viewer: Option<wit::PlayerId>) -> wit::View {
        raw::view::<G>(&state, viewer).into()
    }
}

impl From<wit::InitCtx> for crate::InitCtx {
    fn from(ctx: wit::InitCtx) -> Self {
        Self {
            players: ctx
                .players
                .into_iter()
                .map(|player| crate::Player {
                    id: player.id,
                    name: player.name,
                })
                .collect(),
            seed: ctx.seed,
            options: ctx.options,
        }
    }
}

impl From<crate::Manifest> for wit::Manifest {
    fn from(manifest: crate::Manifest) -> Self {
        Self {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            min_players: manifest.min_players,
            max_players: manifest.max_players,
            summary: manifest.summary,
        }
    }
}

impl From<crate::ActionSpec> for wit::ActionSpec {
    fn from(spec: crate::ActionSpec) -> Self {
        Self {
            id: spec.id,
            label: spec.label,
            payload: spec.payload,
            enabled: spec.enabled,
            reason: spec.reason,
        }
    }
}

impl From<crate::Effect> for wit::Effect {
    fn from(effect: crate::Effect) -> Self {
        match effect {
            crate::Effect::PublicLog(text) => Self::PublicLog(text),
            crate::Effect::PrivateLog(msg) => Self::PrivateLog(wit::PrivateMsg {
                to: msg.to,
                text: msg.text,
            }),
            crate::Effect::SetTimer(timer) => Self::SetTimer(wit::Timer {
                id: timer.id,
                seconds: timer.seconds,
            }),
            crate::Effect::CancelTimer(id) => Self::CancelTimer(id),
            crate::Effect::GameOver(outcomes) => Self::GameOver(
                outcomes
                    .into_iter()
                    .map(|outcome| wit::Outcome {
                        player: outcome.player,
                        rank: outcome.rank,
                        score: outcome.score,
                        note: outcome.note,
                    })
                    .collect(),
            ),
        }
    }
}

impl From<crate::View> for wit::View {
    fn from(view: crate::View) -> Self {
        Self {
            title: view.title,
            status: view.status,
            zones: view.zones.into_iter().map(Into::into).collect(),
            log: view.log.into_iter().map(Into::into).collect(),
            prompt: view.prompt.map(Into::into),
        }
    }
}

impl From<crate::Zone> for wit::Zone {
    fn from(zone: crate::Zone) -> Self {
        Self {
            label: zone.label,
            layout: zone.layout.into(),
            items: zone.items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::Layout> for wit::Layout {
    fn from(layout: crate::Layout) -> Self {
        match layout {
            crate::Layout::Row => Self::Row,
            crate::Layout::Grid => Self::Grid,
            crate::Layout::Stack => Self::Stack,
        }
    }
}

impl From<crate::Item> for wit::Item {
    fn from(item: crate::Item) -> Self {
        Self {
            face: item.face.into(),
            label: item.label,
            sublabel: item.sublabel,
            badges: item.badges,
            selectable: item.selectable.map(Into::into),
        }
    }
}

impl From<crate::Face> for wit::Face {
    fn from(face: crate::Face) -> Self {
        match face {
            crate::Face::Up(art) => Self::Up(art),
            crate::Face::Down => Self::Down,
            crate::Face::Empty => Self::Empty,
        }
    }
}

impl From<crate::LogLine> for wit::LogLine {
    fn from(line: crate::LogLine) -> Self {
        Self {
            text: line.text,
            kind: match line.kind {
                crate::LogKind::Public => wit::LogKind::Public,
                crate::LogKind::Private => wit::LogKind::Private,
                crate::LogKind::System => wit::LogKind::System,
            },
        }
    }
}

impl From<crate::Prompt> for wit::Prompt {
    fn from(prompt: crate::Prompt) -> Self {
        Self {
            text: prompt.text,
            choices: prompt.choices.into_iter().map(Into::into).collect(),
            deadline_seconds: prompt.deadline_seconds,
        }
    }
}
