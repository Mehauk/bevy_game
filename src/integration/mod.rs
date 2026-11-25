use bevy::prelude::*;
use bevy_script_menus::MenuUpdateSystems;

use crate::character::CharacterSystems;
use crate::input::InputUpdateSystems;
use crate::state::PlayState;

mod bridges;
mod distance_field;
mod menu_callbacks;
mod menu_wiring;
mod post_process;
mod ripples;
mod state_pause;

pub struct IntegrationPlugin;

impl Plugin for IntegrationPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                InputUpdateSystems::SyncActionContexts,
                InputUpdateSystems::ApplyContextSync,
                InputUpdateSystems::ProcessRemap,
                InputUpdateSystems::ApplyRemap,
                InputUpdateSystems::SyncHotkeyRegistry,
            )
                .before(MenuUpdateSystems),
        );

        app.configure_sets(
            FixedUpdate,
            CharacterSystems::Playing.run_if(in_state(PlayState::Running)),
        )
        .configure_sets(
            Update,
            CharacterSystems::Playing.run_if(in_state(PlayState::Running)),
        );

        menu_callbacks::register(app);
        menu_wiring::register(app);
        post_process::register(app);
        bridges::register(app);
        distance_field::register(app);
        ripples::register(app);
        state_pause::register(app);
    }
}
