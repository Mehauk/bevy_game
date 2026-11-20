use std::{borrow::Cow, collections::VecDeque};

use bevy::{platform::collections::HashMap, prelude::*};
use bevy_mod_scripting::bindings::script_value::ScriptValue;

pub fn script_str(s: &str) -> ScriptValue {
    ScriptValue::String(Cow::Owned(s.to_string()))
}

pub fn script_str_list<'a>(
    items: impl IntoIterator<Item = impl AsRef<str> + 'a>,
) -> ScriptValue {
    ScriptValue::List(
        items
            .into_iter()
            .map(|s| script_str(s.as_ref()))
            .collect::<VecDeque<_>>(),
    )
}

pub fn script_map(entries: impl IntoIterator<Item = (&'static str, ScriptValue)>) -> ScriptValue {
    let mut map = HashMap::default();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    ScriptValue::Map(map)
}
