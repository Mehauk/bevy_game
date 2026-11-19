//! Helpers for building [`ScriptValue`]s on the Rust → Lua boundary.
//!
//! Every Rust → Lua payload in the engine is assembled from these, which keeps the
//! value shapes consistent across the `dom`, `input`, `commands` and `menus` namespaces.

use std::{borrow::Cow, collections::VecDeque};

use bevy::{platform::collections::HashMap, prelude::*};
use bevy_mod_scripting::bindings::script_value::ScriptValue;

/// [`ScriptValue::String`] from a `&str`.
pub fn script_str(s: &str) -> ScriptValue {
    ScriptValue::String(Cow::Owned(s.to_string()))
}

/// [`ScriptValue::String`] list from any iterator of string-convertible items.
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

/// Build a [`ScriptValue::Map`] from static-keyed entries.
pub fn script_map(entries: impl IntoIterator<Item = (&'static str, ScriptValue)>) -> ScriptValue {
    let mut map = HashMap::default();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    ScriptValue::Map(map)
}
