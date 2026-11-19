pub mod commands;

use bevy::{pbr::wireframe::WireframePlugin, prelude::*};

use commands::{CommandAppExt, command};

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