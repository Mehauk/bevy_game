use bevy::prelude::*;

use crate::character::CharacterSystems;
use crate::input::InputUpdateSystems;
use crate::menus::MenuUpdateSystems;
use crate::state::PlayState;

mod bridges;
mod menu_wiring;
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

        menu_wiring::register(app);
        bridges::register(app);
        state_pause::register(app);
    }
}
