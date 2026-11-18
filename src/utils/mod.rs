pub mod commands;

use bevy::{pbr::wireframe::WireframePlugin, prelude::*};

use commands::CommandAppExt;

pub struct DevUtilsPlugin;

impl Plugin for DevUtilsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WireframePlugin::default())
            .register_command(
                "utils>dev_utils>toggle_wireframe",
                "Toggle global wireframe rendering",
                dev_utils::toggle_wireframe,
            );
    }
}

pub mod dev_utils {
    use bevy::{pbr::wireframe::WireframeConfig, prelude::*};

    pub fn toggle_wireframe(mut wireframe_config: ResMut<WireframeConfig>) {
        wireframe_config.global = !wireframe_config.global;
    }
}