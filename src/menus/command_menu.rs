use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;

use super::{MenuAppExt, MenuSpec};

#[derive(InputAction)]
#[action_output(bool)]
struct ToggleCommandPalette;

pub struct CommandMenuPlugin;

impl Plugin for CommandMenuPlugin {
    fn build(&self, app: &mut App) {
        app.register_menu_with_hotkey::<ToggleCommandPalette>(MenuSpec {
            id: "command_palette",
            hotkey_label: "Command Palette",
            command: None,
            bindings: vec![KeyCode::Space.with_mod_keys(ModKeys::CONTROL).into()],
        });
    }
}