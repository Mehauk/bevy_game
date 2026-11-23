use bevy::prelude::*;
use bevy_mod_scripting::bindings::script_value::ScriptValue;

const MODIFIER_KEYS: [(&str, [KeyCode; 2]); 4] = [
    ("ctrl", [KeyCode::ControlLeft, KeyCode::ControlRight]),
    ("shift", [KeyCode::ShiftLeft, KeyCode::ShiftRight]),
    ("alt", [KeyCode::AltLeft, KeyCode::AltRight]),
    ("super", [KeyCode::SuperLeft, KeyCode::SuperRight]),
];

pub(crate) fn modifier_entries(
    keys: &ButtonInput<KeyCode>,
) -> impl Iterator<Item = (&'static str, ScriptValue)> + Clone {
    MODIFIER_KEYS
        .iter()
        .map(move |(name, codes)| (*name, ScriptValue::Bool(keys.any_pressed(*codes))))
}
