use std::borrow::Cow;

use bevy::platform::collections::HashMap;
use bevy_mod_scripting::bindings::script_value::ScriptValue;

pub(crate) fn script_str(s: &str) -> ScriptValue {
    ScriptValue::String(Cow::Owned(s.to_string()))
}

pub(crate) fn script_map(
    entries: impl IntoIterator<Item = (&'static str, ScriptValue)>,
) -> ScriptValue {
    let mut map = HashMap::default();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    ScriptValue::Map(map)
}
