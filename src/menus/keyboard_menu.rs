use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;

use super::{MenuAppExt, MenuSpec};

#[derive(InputAction)]
#[action_output(bool)]
struct ToggleKeyboardOverlay;

pub struct KeyboardMenuPlugin;

impl Plugin for KeyboardMenuPlugin {
    fn build(&self, app: &mut App) {
        app.register_menu_with_hotkey::<ToggleKeyboardOverlay>(MenuSpec {
            id: "keyboard",
            hotkey_label: "Keyboard Overlay",
            command: Some(("menu>keyboard", "Toggle virtual keyboard overlay")),
            bindings: vec![
                Binding::from(KeyCode::F1),
                KeyCode::KeyK
                    .with_mod_keys(ModKeys::CONTROL | ModKeys::SHIFT)
                    .into(),
            ],
        });
    }
}