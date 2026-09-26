//! The wit-bindgen side of the boundary. Authors never see this: `export_game!`
//! points the generated exports at [`Adapter`], which routes each call through
//! [`crate::raw`] and converts the action-carrying types to their wire form.

use std::marker::PhantomData;

use crate::{Game, raw};

pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "game-plugin",
        pub_export_macro: true,
        export_macro_name: "export_game_plugin",
        // Most generated types are re-exported as the SDK's own, and games keep them
        // in their serialised state.
        additional_derives: [serde::Serialize, serde::Deserialize, PartialEq, Eq],
    });
}

use bindings as wit;

pub struct Adapter<G>(PhantomData<G>);

impl<G: Game> wit::Guest for Adapter<G> {
    fn manifest() -> wit::Manifest {
        raw::manifest::<G>()
    }

    fn init(ctx: wit::InitCtx) -> wit::State {
        raw::init::<G>(ctx)
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
        raw::apply::<G>(&state, player, &action)
    }

    fn view(state: wit::State, viewer: Option<wit::PlayerId>) -> wit::View {
        raw::view::<G>(&state, viewer).into()
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

impl From<crate::View> for wit::View {
    fn from(view: crate::View) -> Self {
        Self {
            title: view.title,
            status: view.status,
            zones: view.zones.into_iter().map(Into::into).collect(),
            log: view.log,
            prompt: view.prompt.map(Into::into),
        }
    }
}

impl From<crate::Zone> for wit::Zone {
    fn from(zone: crate::Zone) -> Self {
        Self {
            label: zone.label,
            layout: zone.layout,
            items: zone.items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::Item> for wit::Item {
    fn from(item: crate::Item) -> Self {
        Self {
            face: item.face,
            label: item.label,
            sublabel: item.sublabel,
            badges: item.badges,
            selectable: item.selectable.map(Into::into),
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
