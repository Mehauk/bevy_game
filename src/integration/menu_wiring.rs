use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_script_menus::ToggleMenu;

use crate::input::InputAppExt;
use crate::utils::registry::commands::CommandAppExt;

#[derive(InputAction)]
#[action_output(bool)]
struct ToggleKeyboardOverlay;

#[derive(InputAction)]
#[action_output(bool)]
struct ToggleCommandPalette;

pub struct MenuSpec {
    pub id: &'static str,
    pub hotkey_label: &'static str,
    pub command: Option<(&'static str, &'static str)>,
    pub bindings: Vec<Binding>,
}

pub(super) fn register(app: &mut App) {
    wire_menu::<ToggleKeyboardOverlay>(
        app,
        MenuSpec {
            id: "keyboard",
            hotkey_label: "Keyboard Overlay",
            command: Some(("menu>keyboard", "Toggle virtual keyboard overlay")),
            bindings: vec![
                Binding::from(KeyCode::F1),
                KeyCode::KeyK
                    .with_mod_keys(ModKeys::CONTROL | ModKeys::SHIFT)
                    .into(),
            ],
        },
    );
    wire_menu::<ToggleCommandPalette>(
        app,
        MenuSpec {
            id: "command_palette",
            hotkey_label: "Command Palette",
            command: None,
            bindings: vec![Binding::from(KeyCode::Backquote)],
        },
    );
}

fn wire_menu<A: InputAction<Output = bool> + Send + Sync + 'static>(app: &mut App, spec: MenuSpec) {
    let menu_id = spec.id;

    if let Some((cmd_name, desc)) = spec.command {
        app.register_command(cmd_name, desc, move |mut writer: MessageWriter<ToggleMenu>| {
            writer.write(ToggleMenu(menu_id.to_string()));
        });
    }

    if !spec.bindings.is_empty() {
        app.register_gameplay_hotkey::<A, _, _, _>(
            spec.hotkey_label,
            ActionSettings {
                consume_input: true,
                require_reset: true,
                ..default()
            },
            spec.bindings,
            move |_ev: On<Start<A>>, mut writer: MessageWriter<ToggleMenu>| {
                writer.write(ToggleMenu(menu_id.to_string()));
            },
        );
    }
}
