use bevy::prelude::*;
use bevy_script_menus::MenuUpdateSystems;

use crate::character::CharacterSystems;
use crate::input::InputUpdateSystems;
use crate::state::PlayState;

mod bridges;
mod debug_views;
mod distance_field;
mod input_wiring;
mod menu_callbacks;
mod menu_wiring;
mod ocean;
mod post_process;
mod state_pause;
mod submersion;
mod terrain_viewer;
mod water_commands;
mod water_interactions;

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

        debug_views::register(app);
        input_wiring::register(app);
        menu_callbacks::register(app);
        menu_wiring::register(app);
        ocean::register(app);
        post_process::register(app);
        bridges::register(app);
        distance_field::register(app);
        state_pause::register(app);
        submersion::register(app);
        terrain_viewer::register(app);
        water_commands::register(app);
        water_interactions::register(app);
    }
}
