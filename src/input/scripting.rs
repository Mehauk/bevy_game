//! The `input` Lua namespace: the only way menu scripts see or change keybinds.

use bevy::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use super::RemapStatus;
use super::hotkeys::HotkeyRegistry;
use super::keys::{current_modifier_flags, modifier_map, parse_mod_keys};
use super::remap::RemapBindingMessage;
use crate::utils::{script_map, script_str};

pub(super) fn register_input_namespace(app: &mut App) {
    NamespaceBuilder::in_namespace(app.world_mut(), "input")
        .register(
            "list_bindings",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|reg: &HotkeyRegistry| {
                    let list = reg
                        .entries
                        .iter()
                        .map(|entry| {
                            script_map([
                                ("key", script_str(&entry.key)),
                                ("label", script_str(&entry.label)),
                                ("context", script_str(&entry.context)),
                                ("active", ScriptValue::Bool(entry.active)),
                                ("is_movement", ScriptValue::Bool(entry.is_movement)),
                                ("ctrl", ScriptValue::Bool(entry.ctrl)),
                                ("shift", ScriptValue::Bool(entry.shift)),
                                ("alt", ScriptValue::Bool(entry.alt)),
                                ("super", ScriptValue::Bool(entry.super_key)),
                            ])
                        })
                        .collect();
                    ScriptValue::List(list)
                })
            },
        )
        .register(
            "active_modifiers",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?
                    .with_resource(|keys: &ButtonInput<KeyCode>| {
                        modifier_map(current_modifier_flags(keys))
                    })
            },
        )
        .register(
            "remap",
            |ctx: FunctionCallContext,
             source_key: String,
             source_mods: ScriptValue,
             target_key: String,
             target_mods: ScriptValue|
             -> Result<(), InteropError> {
                let source_m = parse_mod_keys(&source_mods);
                let target_m = parse_mod_keys(&target_mods);
                let _ = ctx.world()?.with_resource_mut(
                    |mut writer: Mut<Messages<RemapBindingMessage>>| {
                        writer.write(RemapBindingMessage {
                            source_key,
                            source_mods: source_m,
                            target_key,
                            target_mods: target_m,
                        });
                    },
                );
                Ok(())
            },
        )
        .register(
            "remap_status",
            |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|status: &RemapStatus| {
                    script_map([
                        ("success", ScriptValue::Bool(status.success)),
                        ("message", script_str(&status.message)),
                        ("version", ScriptValue::Integer(status.version as i64)),
                    ])
                })
            },
        );
}
