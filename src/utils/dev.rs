//! Development-only utilities.
//!
//! The console command name is derived from the module path, so this file's location is
//! part of the user-facing name: `utils>dev>toggle_wireframe`.

use bevy::{
    dev_tools::fps_overlay::FpsOverlayConfig,
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    prelude::*,
};
use bevy_enhanced_input::prelude::*;

use crate::input::InputAppExt;
use crate::utils::registry::commands::{CommandAppExt, command};

/// Hotkey action that toggles the FPS overlay.
#[derive(InputAction)]
#[action_output(bool)]
struct ToggleFpsOverlay;

pub struct DevUtilsPlugin;

impl Plugin for DevUtilsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WireframePlugin::default());
        command!(app, "Toggle global wireframe rendering", toggle_wireframe);
        command!(app, "Toggle the FPS overlay", toggle_fps_overlay);
        app.register_gameplay_hotkey::<ToggleFpsOverlay, _, _, _>(
            "FPS Overlay",
            ActionSettings {
                require_reset: true,
                ..default()
            },
            [KeyCode::F2.into()],
            |_ev: On<Start<ToggleFpsOverlay>>, config: ResMut<FpsOverlayConfig>| {
                toggle_fps_overlay(config);
            },
        );
    }
}

fn toggle_wireframe(mut wireframe_config: ResMut<WireframeConfig>) {
    wireframe_config.global = !wireframe_config.global;
}

/// Flips both halves of the overlay: `enabled` drives the FPS text, while the
/// frame-time graph has its own flag that `toggle_display` reads independently.
fn toggle_fps_overlay(mut config: ResMut<FpsOverlayConfig>) {
    config.enabled = !config.enabled;
    config.frame_time_graph_config.enabled = config.enabled;
}
