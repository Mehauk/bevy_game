use bevy::prelude::*;

use crate::menus::ToggleMenu;

const MENU_NAME: &str = "command_palette";

pub struct CommandMenuPlugin;

impl Plugin for CommandMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, toggle_command_menu_shortcut);
    }
}

fn toggle_command_menu_shortcut(
    keys: Res<ButtonInput<KeyCode>>,
    mut toggle_writer: MessageWriter<ToggleMenu>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);

    if ctrl && alt && keys.just_pressed(KeyCode::Space) {
        toggle_writer.write(ToggleMenu(MENU_NAME.to_string()));
    }
}