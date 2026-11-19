pub mod commands;

use std::borrow::Cow;

use bevy::{pbr::wireframe::WireframePlugin, platform::collections::HashMap, prelude::*};
use bevy_mod_scripting::bindings::script_value::ScriptValue;

use commands::{CommandAppExt, command};

/// [`ScriptValue::String`] from a `&str`.
pub fn script_str(s: &str) -> ScriptValue {
    ScriptValue::String(Cow::Owned(s.to_string()))
}

/// Build a [`ScriptValue::Map`] from static-keyed entries.
pub fn script_map(entries: impl IntoIterator<Item = (&'static str, ScriptValue)>) -> ScriptValue {
    let mut map = HashMap::default();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    ScriptValue::Map(map)
}

pub struct DevUtilsPlugin;

impl Plugin for DevUtilsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WireframePlugin::default());
        command!(
            app,
            "Toggle global wireframe rendering",
            dev::toggle_wireframe
        );
    }
}

pub mod dev {
    use bevy::{pbr::wireframe::WireframeConfig, prelude::*};

    pub fn toggle_wireframe(mut wireframe_config: ResMut<WireframeConfig>) {
        wireframe_config.global = !wireframe_config.global;
    }
}