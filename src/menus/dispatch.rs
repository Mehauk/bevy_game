//! Pushing engine state changes out to open menu scripts.
//!
//! Every dispatcher here has the same shape — "if any menu is open and loaded, send this
//! payload to each" — so that shape lives in [`broadcast_to_menus`] once, and the
//! per-dispatcher code is only the change detection that decides *whether* to send.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_mod_scripting::{
    bindings::script_value::ScriptValue,
    core::event::{CallbackLabel, IntoCallbackLabel, ScriptCallbackMessage},
};

use crate::input::{RemapStatus, current_modifier_flags, modifier_map, mouse_button_name};
use crate::menus::OnMenuModifiersChanged;
use crate::menus::OnMenuRemapResult;
use crate::menus::lifecycle::MenuRoot;
use crate::menus::state::MenuManager;
use crate::utils::{script_str, script_str_list};

/// Change-detection cache for [`dispatch_modifier_changes`].
///
/// Lives here rather than on [`MenuManager`] because no one else cares about it.
#[derive(Resource, Default)]
pub(crate) struct ModifierTracking {
    last_mods: ModKeys,
    last_pressed: Vec<String>,
}

/// Send `label`/`payload` to every menu whose script is ready.
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

/// Tell open menus which modifiers and keys are held, so the keyboard overlay can show
/// the bindings for the currently-pinned modifier set.
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
    // We intentionally drain mouse motion so the message reader does not accumulate,
    // but do not add MouseMotion to pressed keys (Aim/Look should never glow).
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

    broadcast_to_menus(
        &q_roots,
        &mut callbacks,
        OnMenuModifiersChanged::into_callback_label(),
        &[
            modifier_map(mods),
            script_str_list(&pressed),
            script_str_list(&just_pressed),
        ],
    );
}

/// Relay the outcome of the last `input.remap` to open menus.
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
