use bevy::{
    dev_tools::fps_overlay::{FPS_OVERLAY_ZINDEX, FpsOverlayConfig},
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    prelude::*,
};

use crate::utils::registry::commands::{CommandAppExt, CommandOutput, command};
use crate::utils::registry::debug_info::{
    DebugInfoEnabled, DebugInfoSnapshot, DebugInfoSystems,
};

#[derive(Resource)]
pub struct DebugOverlayConfig {
    pub enabled: bool,
}

impl Default for DebugOverlayConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
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
            .init_resource::<DebugInfoEnabled>()
            .add_systems(Startup, spawn_debug_overlay)
            .add_systems(
                Update,
                sync_debug_info_enabled.in_set(DebugInfoSystems::Sync),
            )
            .add_systems(
                Update,
                (sync_debug_overlay_text, sync_debug_overlay_layout)
                    .chain()
                    .in_set(DebugInfoSystems::Render),
            )
            .configure_sets(
                Update,
                (
                    DebugInfoSystems::Sync,
                    DebugInfoSystems::Collect,
                    DebugInfoSystems::Render,
                )
                    .chain(),
            );
        command!(app, "Toggle global wireframe rendering", toggle_wireframe);
        command!(app, "Toggle the FPS overlay", toggle_fps_overlay);
        command!(app, "Toggle debugging info overlay", toggle_debug_overlay);
    }
}

fn toggle_wireframe(mut wireframe_config: ResMut<WireframeConfig>) {
    wireframe_config.global = !wireframe_config.global;
}

pub(crate) fn toggle_fps_overlay(mut config: ResMut<FpsOverlayConfig>) {
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
    let text_entity = commands
        .spawn((
            DebugOverlayText,
            Text::default(),
            TextFont::from_font_size(16.0),
            TextColor(Color::WHITE),
            Pickable::IGNORE,
        ))
        .id();
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
        ))
        .add_child(text_entity);
}

fn sync_debug_overlay_layout(
    config: Res<DebugOverlayConfig>,
    fps_config: Option<Res<FpsOverlayConfig>>,
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
        .as_deref()
        .is_some_and(|config| config.enabled)
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

fn sync_debug_info_enabled(config: Res<DebugOverlayConfig>, mut enabled: ResMut<DebugInfoEnabled>) {
    enabled.0 = config.enabled;
}

fn sync_debug_overlay_text(
    config: Res<DebugOverlayConfig>,
    mut snapshot: ResMut<DebugInfoSnapshot>,
    mut text: Single<&mut Text, With<DebugOverlayText>>,
) {
    if !config.enabled || !snapshot.take_dirty() {
        return;
    }
    let rendered = snapshot.render();
    if text.as_str() != rendered {
        **text = Text::from(rendered);
    }
}
