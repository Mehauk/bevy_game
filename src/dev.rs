use bevy::{
    dev_tools::fps_overlay::{FPS_OVERLAY_ZINDEX, FpsOverlayConfig},
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    prelude::*,
};
use bevy_enhanced_input::prelude::*;

use crate::input::InputAppExt;
use crate::utils::registry::commands::{CommandAppExt, CommandOutput, command};
use crate::utils::registry::debug_info::DebugInfoRegistry;

#[derive(InputAction)]
#[action_output(bool)]
struct ToggleFpsOverlay;

#[derive(Resource, Default)]
pub struct DebugOverlayConfig {
    pub enabled: bool,
}

#[derive(Component)]
struct DebugOverlayRoot;

#[derive(Component)]
struct DebugOverlayText;

pub struct DevUtilsPlugin;

impl Plugin for DevUtilsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WireframePlugin::default());
        app.init_resource::<DebugOverlayConfig>()
            .init_resource::<DebugInfoRegistry>()
            .add_systems(Startup, spawn_debug_overlay)
            .add_systems(
                Update,
                (sync_debug_overlay_layout, update_debug_overlay_text),
            );
        command!(app, "Toggle global wireframe rendering", toggle_wireframe);
        command!(app, "Toggle the FPS overlay", toggle_fps_overlay);
        command!(app, "Toggle debugging info overlay", toggle_debug_overlay);
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

fn toggle_fps_overlay(mut config: ResMut<FpsOverlayConfig>) {
    config.enabled = !config.enabled;
    config.frame_time_graph_config.enabled = config.enabled;
}

fn toggle_debug_overlay(
    mut config: ResMut<DebugOverlayConfig>,
    mut output: ResMut<CommandOutput>,
) {
    config.enabled = !config.enabled;
    output.set(if config.enabled {
        "Debug overlay: on"
    } else {
        "Debug overlay: off"
    });
}

fn spawn_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            DebugOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(4.0),
                top: Val::Px(4.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            GlobalZIndex(FPS_OVERLAY_ZINDEX),
            Pickable::IGNORE,
            Visibility::Hidden,
        ))
        .with_child((
            DebugOverlayText,
            Text::default(),
            TextFont::from_font_size(16.0),
            TextColor(Color::WHITE),
            Pickable::IGNORE,
        ));
}

fn sync_debug_overlay_layout(
    config: Res<DebugOverlayConfig>,
    fps_config: Res<FpsOverlayConfig>,
    overlay: Single<(&mut Node, &mut Visibility), With<DebugOverlayRoot>>,
    fps_roots: Query<(&ComputedNode, &GlobalZIndex), Without<DebugOverlayRoot>>,
) {
    let (mut node, mut visibility) = overlay.into_inner();
    let desired = if config.enabled {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if *visibility != desired {
        *visibility = desired;
    }

    let fps_height = fps_config
        .enabled
        .then(|| fps_roots.iter().find(|(_, z)| z.0 == FPS_OVERLAY_ZINDEX))
        .flatten()
        .map(|(computed, _)| computed.size().y * computed.inverse_scale_factor())
        .filter(|height| *height > 0.0)
        .unwrap_or(0.0);
    let top = Val::Px(fps_height + 4.0);
    if node.top != top {
        node.top = top;
    }
}

fn update_debug_overlay_text(world: &mut World) {
    if !world.resource::<DebugOverlayConfig>().enabled {
        return;
    }

    let entries = world.resource::<DebugInfoRegistry>().entries.clone();
    let mut text = String::new();
    let mut category_shown: Option<&str> = None;
    for entry in &entries {
        let value = match world.run_system(entry.system_id) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(err) => format!("error: {err}"),
        };
        if category_shown != Some(entry.category) {
            if category_shown.is_some() {
                text.push('\n');
            }
            text.push_str(&format!("[{}]\n", entry.category));
            category_shown = Some(entry.category);
        }
        text.push_str(&format!("  {}: {}\n", entry.label, value));
    }

    let Some(text_entity) = world
        .iter_entities()
        .find(|entity| entity.contains::<DebugOverlayText>())
        .map(|entity| entity.id())
    else {
        return;
    };
    let mut text_node = world.entity_mut(text_entity);
    let Some(mut current) = text_node.get_mut::<Text>() else {
        return;
    };
    if current.as_str() != text {
        *current = Text::from(text);
    }
}
