use std::borrow::Cow;

use bevy::{
    ecs::relationship::Relationship,
    platform::collections::HashMap,
    prelude::*,
};
use bevy_enhanced_input::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::{
    menus::{MenuManager, ToggleMenu},
    utils::commands::CommandAppExt,
};

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EnhancedInputPlugin)
            .add_input_context::<GameplayInput>()
            .add_input_context::<GlobalInput>()
            .init_resource::<HotkeyRegistry>()
            .register_command("Toggle virtual keyboard overlay", toggle_keyboard_overlay_cmd)
            .add_systems(Startup, setup_global_input)
            .add_systems(
                PreUpdate,
                sync_gameplay_context_activity.before(EnhancedInputSystems::Update),
            )
            .add_systems(Update, sync_hotkey_registry)
            .add_observer(on_toggle_command_palette)
            .add_observer(on_toggle_keyboard_overlay);

        register_input_namespace(app);
    }
}

#[derive(Component, Default, Clone)]
pub struct GameplayInput;

#[derive(Component, Default, Clone)]
pub struct GlobalInput;

#[derive(Component, Clone, Copy, Default)]
pub struct HotkeyLabel(pub &'static str);

#[derive(InputAction)]
#[action_output(bool)]
pub struct ToggleCommandPalette;

#[derive(InputAction)]
#[action_output(bool)]
pub struct ToggleKeyboardOverlay;

#[derive(Clone, Debug)]
pub struct HotkeyEntry {
    pub key: String,
    pub label: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Resource, Default)]
pub struct HotkeyRegistry {
    pub entries: Vec<HotkeyEntry>,
}

fn setup_global_input(world: &mut World) {
    let cmd_palette: Binding = KeyCode::Space.with_mod_keys(ModKeys::CONTROL).into();
    let kb_overlay: Binding = KeyCode::KeyK
        .with_mod_keys(ModKeys::CONTROL | ModKeys::SHIFT)
        .into();

    world
        .spawn_scene(bsn! {
            GlobalInput
            Actions<GlobalInput> [
                Action::<ToggleCommandPalette>::new()
                HotkeyLabel("Command Palette")
                Bindings [
                    cmd_palette
                ]
                --
                Action::<ToggleKeyboardOverlay>::new()
                HotkeyLabel("Keyboard Overlay")
                Bindings [
                    Binding(KeyCode::F1)
                    --
                    kb_overlay
                ]
            ]
        })
        .unwrap();
}

fn toggle_keyboard_overlay_cmd(mut toggle_writer: MessageWriter<ToggleMenu>) {
    toggle_writer.write(ToggleMenu("keyboard".to_string()));
}

fn on_toggle_command_palette(
    _event: On<Start<ToggleCommandPalette>>,
    mut toggle_writer: MessageWriter<ToggleMenu>,
) {
    toggle_writer.write(ToggleMenu("command_palette".to_string()));
}

fn on_toggle_keyboard_overlay(
    _event: On<Start<ToggleKeyboardOverlay>>,
    mut toggle_writer: MessageWriter<ToggleMenu>,
) {
    toggle_writer.write(ToggleMenu("keyboard".to_string()));
}

fn sync_gameplay_context_activity(
    menu_manager: Res<MenuManager>,
    mut prev_open: Local<Option<bool>>,
    q_contexts: Query<Entity, With<GameplayInput>>,
    mut commands: Commands,
) {
    let open = menu_manager.any_open();
    if *prev_open == Some(open) {
        return;
    }
    *prev_open = Some(open);

    let target = if open {
        ContextActivity::<GameplayInput>::INACTIVE
    } else {
        ContextActivity::<GameplayInput>::ACTIVE
    };
    for entity in &q_contexts {
        commands.entity(entity).insert(target);
    }
}

fn sync_hotkey_registry(
    q_changed: Query<(), Or<(Added<Binding>, Changed<Binding>, Added<HotkeyLabel>, Changed<HotkeyLabel>)>>,
    mut removed_bindings: RemovedComponents<Binding>,
    q_bindings: Query<(&Binding, &BindingOf, Option<&HotkeyLabel>)>,
    q_actions: Query<&HotkeyLabel>,
    mut registry: ResMut<HotkeyRegistry>,
) {
    if q_changed.is_empty() && removed_bindings.read().next().is_none() && !registry.entries.is_empty() {
        return;
    }

    let mut entries = Vec::new();
    for (binding, binding_of, binding_label) in &q_bindings {
        let Binding::Keyboard { key, mod_keys } = *binding else {
            continue;
        };
        let label = binding_label
            .or_else(|| q_actions.get(binding_of.get()).ok())
            .map(|l| l.0)
            .unwrap_or("Bound");

        entries.push(HotkeyEntry {
            key: format!("{key:?}"),
            label: label.to_string(),
            ctrl: mod_keys.contains(ModKeys::CONTROL),
            shift: mod_keys.contains(ModKeys::SHIFT),
            alt: mod_keys.contains(ModKeys::ALT),
            super_key: mod_keys.contains(ModKeys::SUPER),
        });
    }
    registry.entries = entries;
}

pub fn current_modifier_flags(keys: &ButtonInput<KeyCode>) -> (bool, bool, bool, bool) {
    (
        keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
        keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight]),
    )
}

pub fn modifier_map(ctrl: bool, shift: bool, alt: bool, super_key: bool) -> ScriptValue {
    let mut map = HashMap::default();
    map.insert("ctrl".to_string(), ScriptValue::Bool(ctrl));
    map.insert("shift".to_string(), ScriptValue::Bool(shift));
    map.insert("alt".to_string(), ScriptValue::Bool(alt));
    map.insert("super".to_string(), ScriptValue::Bool(super_key));
    ScriptValue::Map(map)
}

fn register_input_namespace(app: &mut App) {
    NamespaceBuilder::in_namespace(app.world_mut(), "input")
        .register(
            "list_bindings",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|reg: &HotkeyRegistry| {
                    let list = reg
                        .entries
                        .iter()
                        .map(|entry| {
                            let mut map = HashMap::default();
                            map.insert("key".to_string(), ScriptValue::String(Cow::Owned(entry.key.clone())));
                            map.insert("label".to_string(), ScriptValue::String(Cow::Owned(entry.label.clone())));
                            map.insert("ctrl".to_string(), ScriptValue::Bool(entry.ctrl));
                            map.insert("shift".to_string(), ScriptValue::Bool(entry.shift));
                            map.insert("alt".to_string(), ScriptValue::Bool(entry.alt));
                            map.insert("super".to_string(), ScriptValue::Bool(entry.super_key));
                            ScriptValue::Map(map)
                        })
                        .collect();
                    ScriptValue::List(list)
                })
            },
        )
        .register(
            "active_modifiers",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|keys: &ButtonInput<KeyCode>| {
                    let (ctrl, shift, alt, super_key) = current_modifier_flags(keys);
                    modifier_map(ctrl, shift, alt, super_key)
                })
            },
        );
}