use bevy::{input_focus::AutoFocus, prelude::*, text::EditableText};
use bevy_hui::prelude::*;

use crate::menus::{ToggleMenu, resolve_menu_file};

const MENU_NAME: &str = "command_palette";

pub struct CommandMenuPlugin;

impl Plugin for CommandMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_command_menu)
            .add_systems(Update, toggle_command_menu_shortcut);
    }
}

#[derive(Component, Clone, Default)]
pub struct CommandSearchInput;

fn setup_command_menu(server: Res<AssetServer>, mut html_comps: HtmlComponents) {
    html_comps.register(
        "command_row",
        server.load(resolve_menu_file(MENU_NAME, "components/command_row.html")),
    );

    html_comps.register_with_spawn_fn(
        "search_input",
        server.load(resolve_menu_file(MENU_NAME, "components/search_input.html")),
        |mut entity_commands| {
            entity_commands.insert((EditableText::default(), CommandSearchInput, AutoFocus));
        },
    );
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