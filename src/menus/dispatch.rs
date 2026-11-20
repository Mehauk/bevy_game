use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_mod_scripting::{
    bindings::script_value::ScriptValue,
    core::event::{CallbackLabel, IntoCallbackLabel, ScriptCallbackMessage},
};

use crate::menus::OnHistoryChanged;
use crate::menus::OnMenuModifiersChanged;
use crate::menus::OnMenuRemapResult;
use crate::menus::lifecycle::MenuRoot;
use crate::menus::state::MenuManager;
use crate::utils::RemapStatus;
use crate::utils::keys::{current_modifier_flags, modifier_map, mouse_button_name};
use crate::utils::registry::commands::CommandHistory;
use crate::utils::{script_str, script_str_list};

#[derive(Resource, Default)]
pub(crate) struct ModifierTracking {
    last_mods: ModKeys,
    last_pressed: Vec<String>,
}

#[derive(Resource, Default)]
pub(crate) struct HistoryTracking {
    last_version: usize,
}

fn broadcast_to_menus(
    roots: &Query<(Entity, &MenuRoot)>,
    callbacks: &mut MessageWriter<ScriptCallbackMessage>,
    label: CallbackLabel,
    payload: &[ScriptValue],
) {
    for (entity, root) in roots.iter() {
        let Some(script) = root.ready_script() else {
            continue;
        };
        callbacks.write(ScriptCallbackMessage::new_for_script_entity(
            label.clone(),
            payload.to_vec(),
            script,
            entity,
        ));
    }
}

pub(crate) fn dispatch_modifier_changes(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    manager: Res<MenuManager>,
    mut tracking: ResMut<ModifierTracking>,
    q_roots: Query<(Entity, &MenuRoot)>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    if !manager.any_open() {
        return;
    }

    let mods = current_modifier_flags(&keys);
    let mut pressed: Vec<String> = keys.get_pressed().map(|k| format!("{k:?}")).collect();
    pressed.extend(
        mouse_buttons
            .get_pressed()
            .filter_map(|mb| mouse_button_name(*mb).map(str::to_string)),
    );

    if mouse_wheel.read().count() > 0 {
        pressed.push("MouseWheel".to_string());
    }
    let _ = mouse_motion.read().count();

    pressed.sort();
    pressed.dedup();

    let mut just_pressed: Vec<String> = keys
        .get_just_pressed()
        .map(|k| format!("{k:?}"))
        .collect();
    just_pressed.extend(
        mouse_buttons
            .get_just_pressed()
            .filter_map(|mb| mouse_button_name(*mb).map(str::to_string)),
    );

    if mods == tracking.last_mods && pressed == tracking.last_pressed && just_pressed.is_empty() {
        return;
    }

    tracking.last_mods = mods;
    tracking.last_pressed = pressed.clone();

    let label = OnMenuModifiersChanged::into_callback_label();
    let mods_val = modifier_map(mods);
    let pressed_val = script_str_list(&pressed);
    let just_pressed_val = script_str_list(&just_pressed);
    let empty_list = ScriptValue::List(Default::default());
    let top_entity = manager.top_entity();

    for (entity, root) in q_roots.iter() {
        let Some(script) = root.ready_script() else {
            continue;
        };
        let just_pressed_for_menu = if Some(entity) == top_entity {
            just_pressed_val.clone()
        } else {
            empty_list.clone()
        };
        callbacks.write(ScriptCallbackMessage::new_for_script_entity(
            label.clone(),
            vec![mods_val.clone(), pressed_val.clone(), just_pressed_for_menu],
            script,
            entity,
        ));
    }
}

pub(crate) fn dispatch_history_changes(
    history: Res<CommandHistory>,
    manager: Res<MenuManager>,
    mut tracking: ResMut<HistoryTracking>,
    q_roots: Query<(Entity, &MenuRoot)>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    if history.version == tracking.last_version || !manager.any_open() {
        return;
    }
    tracking.last_version = history.version;
    broadcast_to_menus(
        &q_roots,
        &mut callbacks,
        OnHistoryChanged::into_callback_label(),
        &[],
    );
}

pub(crate) fn dispatch_remap_results(
    remap_status: Res<RemapStatus>,
    manager: Res<MenuManager>,
    q_roots: Query<(Entity, &MenuRoot)>,
    mut callbacks: MessageWriter<ScriptCallbackMessage>,
) {
    if !remap_status.is_changed() || !manager.any_open() || remap_status.version == 0 {
        return;
    }
    broadcast_to_menus(
        &q_roots,
        &mut callbacks,
        OnMenuRemapResult::into_callback_label(),
        &[
            ScriptValue::Bool(remap_status.success),
            script_str(&remap_status.message),
        ],
    );
}
