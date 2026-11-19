//! Development-only utilities.
//!
//! The console command name is derived from the module path, so this file's location is
//! part of the user-facing name: `utils>dev>toggle_wireframe`.

use bevy::{pbr::wireframe::{WireframeConfig, WireframePlugin}, prelude::*};

use super::commands::{CommandAppExt, command};

pub struct DevUtilsPlugin;

impl Plugin for DevUtilsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WireframePlugin::default());
        command!(app, "Toggle global wireframe rendering", toggle_wireframe);
    }
}

fn toggle_wireframe(mut wireframe_config: ResMut<WireframeConfig>) {
    wireframe_config.global = !wireframe_config.global;
}
