use bevy::input::mouse::MouseWheel;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::menus::MenuManager;
use crate::terrain::atmosphere::sync_underwater_view;
use crate::terrain::sky::{SkyBox, sync_terrain_atmosphere, sync_terrain_fog};
use crate::terrain::topology::wrapped_copies;
use crate::terrain::TerrainMap;
use crate::utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput};

const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 64.0;
const CAMERA_CLEARANCE: f32 = 80.0;
const DEPTH_MARGIN: f32 = 40.0;
const WHEEL_ZOOM_STEP: f32 = 1.15;
const KEY_ZOOM_RATE: f32 = 1.8;
const PAN_SPEED_FACTOR: f32 = 0.45;
const SHIFT_PAN_BOOST: f32 = 2.5;

#[derive(Component)]
pub struct WorldShotCamera;

#[derive(Resource, Default)]
pub struct WorldShotRequest {
    pub pending: Option<WorldShotMode>,
}

#[derive(Resource, Reflect, Clone, Debug)]
#[reflect(Resource)]
pub struct WorldShotView {
    pub center: Vec2,
    pub zoom: f32,
    pub tilt_deg: f32,
    pub yaw_deg: f32,
}

impl Default for WorldShotView {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
            tilt_deg: 0.0,
            yaw_deg: 0.0,
        }
    }
}

impl WorldShotView {
    pub fn clamped_zoom(&self) -> f32 {
        if self.zoom.is_finite() {
            self.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Resource, Default)]
struct WorldShotBackup {
    cascades: Option<CascadeShadowConfig>,
    clear_color: Option<Color>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorldShotMode {
    Off,
    Full,
    FullSplines,
    NoWater,
    RiversOnly,
    RiversSplines,
    RiversLakes,
}

impl WorldShotMode {
    pub fn parse(label: &str) -> Option<Self> {
        match label {
            "off" => Some(Self::Off),
            "full" => Some(Self::Full),
            "full_splines" => Some(Self::FullSplines),
            "no_water" => Some(Self::NoWater),
            "rivers" => Some(Self::RiversOnly),
            "rivers_splines" => Some(Self::RiversSplines),
            "rivers_lakes" => Some(Self::RiversLakes),
            _ => None,
        }
    }

    fn shows_terrain(self) -> bool {
        matches!(self, Self::Full | Self::FullSplines | Self::NoWater)
    }

    fn shows_sea(self) -> bool {
        matches!(self, Self::Full | Self::FullSplines)
    }

    fn shows_lakes(self) -> bool {
        matches!(self, Self::Full | Self::FullSplines | Self::RiversLakes)
    }

    fn shows_rivers(self) -> bool {
        matches!(
            self,
            Self::Full
                | Self::FullSplines
                | Self::RiversOnly
                | Self::RiversSplines
                | Self::RiversLakes
        )
    }

    fn shows_cascades(self) -> bool {
        matches!(self, Self::Full | Self::FullSplines | Self::RiversLakes)
    }

    pub fn splines(self) -> bool {
        matches!(self, Self::RiversSplines | Self::FullSplines)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Full => "full",
            Self::FullSplines => "full_splines",
            Self::NoWater => "no_water",
            Self::RiversOnly => "rivers",
            Self::RiversSplines => "rivers_splines",
            Self::RiversLakes => "rivers_lakes",
        }
    }

    fn backdrop(self) -> Color {
        match self {
            Self::RiversOnly | Self::RiversSplines | Self::RiversLakes => {
                Color::srgb(0.04, 0.06, 0.09)
            }
            _ => Color::srgb(0.08, 0.12, 0.17),
        }
    }
}

#[derive(Resource, Default)]
pub struct WorldShotActive {
    pub mode: Option<WorldShotMode>,
}

pub fn world_shot(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut request: ResMut<WorldShotRequest>,
    mut view: ResMut<WorldShotView>,
) {
    let Some(label) = args.args.first() else {
        output.set(
            "error: usage: world_shot <off|full|full_splines|no_water|rivers|rivers_splines|rivers_lakes> [x z [zoom]]",
        );
        return;
    };
    let Some(mode) = WorldShotMode::parse(label) else {
        output.set(format!(
            "error: unknown mode '{label}' (off | full | full_splines | no_water | rivers | rivers_splines | rivers_lakes)"
        ));
        return;
    };
    if args.args.len() >= 3 {
        let (Some(x), Some(z)) = (args.get_f32(1), args.get_f32(2)) else {
            output.set("error: coordinates must be numbers: world_shot <mode> [x z [zoom]]");
            return;
        };
        view.center = Vec2::new(x, z);
        if let Some(zoom) = args.get_f32(3) {
            view.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }
    request.pending = Some(mode);
    let text = format!(
        "world shot queued: {} (center=({:.1}, {:.1}) zoom={:.2})",
        mode.label(),
        view.center.x,
        view.center.y,
        view.clamped_zoom()
    );
    output.set(text.clone());
    info!("{text}");
}

pub fn world_shot_focus(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut request: ResMut<WorldShotRequest>,
    active: Res<WorldShotActive>,
    mut view: ResMut<WorldShotView>,
) {
    let (Some(x), Some(z)) = (args.get_f32(0), args.get_f32(1)) else {
        output.set("error: usage: world_shot_focus <x> <z> [zoom]");
        return;
    };
    view.center = Vec2::new(x, z);
    if let Some(zoom) = args.get_f32(2) {
        view.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }
    if active.mode.is_none() {
        request.pending = Some(WorldShotMode::Full);
    }
    let text = format!(
        "world shot focus: ({:.1}, {:.1}) zoom={:.2}",
        view.center.x,
        view.center.y,
        view.clamped_zoom()
    );
    output.set(text.clone());
    info!("{text}");
}

pub fn world_shot_zoom(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut request: ResMut<WorldShotRequest>,
    active: Res<WorldShotActive>,
    mut view: ResMut<WorldShotView>,
) {
    let Some(zoom) = args.get_f32(0) else {
        output.set("error: usage: world_shot_zoom <zoom>");
        return;
    };
    view.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    if active.mode.is_none() {
        request.pending = Some(WorldShotMode::Full);
    }
    let text = format!("world shot zoom: {:.2}", view.zoom);
    output.set(text.clone());
    info!("{text}");
}

fn disabled_fog() -> DistanceFog {
    let mut fog = DistanceFog {
        falloff: FogFalloff::Linear {
            start: 1_000_000.0,
            end: 2_000_000.0,
        },
        ..default()
    };
    fog.color.set_alpha(0.0);
    fog.directional_light_color.set_alpha(0.0);
    fog
}

fn world_height_span(map: &TerrainMap) -> (f32, f32) {
    let min_y = map.heights().min().min(map.sea_level());
    let max_y = map.heights().max().max(map.sea_level());
    (min_y, max_y)
}

fn camera_geometry(map: &TerrainMap, view: &WorldShotView) -> (Transform, OrthographicProjection) {
    let size = map.size().max(1.0);
    let (min_y, max_y) = world_height_span(map);
    let target_y = (min_y + max_y) * 0.5;
    let altitude = max_y + CAMERA_CLEARANCE;
    let yaw = view.yaw_deg.to_radians();
    let tilt = view.tilt_deg.clamp(0.0, 60.0).to_radians();
    let up = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    let horiz = Vec3::new(yaw.sin(), 0.0, yaw.cos()) * ((altitude - target_y) * tilt.tan());
    let target = Vec3::new(view.center.x, target_y, view.center.y);
    let eye = Vec3::new(view.center.x, altitude, view.center.y) + horiz;
    let distance_to_min = eye.distance(Vec3::new(view.center.x, min_y, view.center.y));
    let far = (distance_to_min + size * tilt.sin() + DEPTH_MARGIN).max(120.0);
    let zoom = view.clamped_zoom();

    let projection = OrthographicProjection {
        near: 1.0,
        far,
        viewport_origin: Vec2::splat(0.5),
        scaling_mode: bevy::camera::ScalingMode::FixedVertical {
            viewport_height: size,
        },
        scale: 1.0 / zoom,
        area: Rect::default(),
    };
    let transform = Transform::from_translation(eye).looking_at(target, up);
    (transform, projection)
}

fn topdown_cascades(near: f32, far: f32, zoom: f32) -> CascadeShadowConfig {
    let span = (far - near).max(20.0);
    let focus_frac = (0.35 / zoom.max(1.0).sqrt()).clamp(0.12, 0.45);
    CascadeShadowConfigBuilder {
        first_cascade_far_bound: near + span * focus_frac,
        maximum_distance: far,
        ..default()
    }
    .build()
}

fn apply_world_shot(
    mut commands: Commands,
    mut request: ResMut<WorldShotRequest>,
    mut active: ResMut<WorldShotActive>,
    mut view: ResMut<WorldShotView>,
    mut backup: ResMut<WorldShotBackup>,
    mut clear_color: ResMut<ClearColor>,
    map: Res<TerrainMap>,
    mut main_cams: Query<&mut Camera, With<MainCamera>>,
    mut shot_cameras: Query<(Entity, Option<&mut DistanceFog>), With<WorldShotCamera>>,
    mut lights: Query<&mut CascadeShadowConfig, With<DirectionalLight>>,
    mut skies: Query<&mut Visibility, With<SkyBox>>,
    mut targets: Query<(&mut Visibility, &Name), Without<SkyBox>>,
) {
    let Some(mode) = request.pending.take() else {
        return;
    };

    if mode == WorldShotMode::Off {
        for mut camera in &mut main_cams {
            camera.is_active = true;
        }
        for (entity, _) in &shot_cameras {
            commands.entity(entity).despawn();
        }
        for mut sky in &mut skies {
            *sky = Visibility::Visible;
        }
        for (mut visibility, _) in &mut targets {
            *visibility = Visibility::Visible;
        }
        if let Some(saved_cascades) = backup.cascades.take() {
            for mut cascades in &mut lights {
                *cascades = saved_cascades.clone();
            }
        }
        if let Some(saved_clear) = backup.clear_color.take() {
            clear_color.0 = saved_clear;
        }
        active.mode = None;
        info!("world shot off");
        return;
    }

    if active.mode.is_none() {
        backup.clear_color = Some(clear_color.0);
        if backup.cascades.is_none() {
            backup.cascades = lights.iter().next().cloned();
        }
    }

    let (transform, projection) = camera_geometry(&map, &view);
    let cascade_config = topdown_cascades(projection.near, projection.far, view.clamped_zoom());

    if shot_cameras.is_empty() {
        commands.spawn((
            Camera3d::default(),
            Projection::from(projection),
            transform,
            WorldShotCamera,
            disabled_fog(),
        ));
    }

    for mut camera in &mut main_cams {
        camera.is_active = false;
    }

    for (entity, fog) in &mut shot_cameras {
        match fog {
            Some(mut fog) => *fog = disabled_fog(),
            None => {
                commands.entity(entity).insert(disabled_fog());
            }
        }
    }

    for mut cascades in &mut lights {
        *cascades = cascade_config.clone();
    }

    for mut sky in &mut skies {
        *sky = Visibility::Hidden;
    }

    clear_color.0 = mode.backdrop();
    active.mode = Some(mode);
    apply_target_visibility(mode, &mut targets);
    if !view.center.is_finite() {
        view.reset();
    }
    info!("world shot applied: {}", mode.label());
}

fn entity_visible_in_mode(label: &str, mode: WorldShotMode) -> Option<bool> {
    if label.starts_with("Terrain Chunk") || label.starts_with("Stream Chunk") {
        Some(mode.shows_terrain())
    } else if label == "Sea Surface" {
        Some(mode.shows_sea())
    } else if label == "River Network" {
        Some(mode.shows_rivers())
    } else if label == "Lake Surface" {
        Some(mode.shows_lakes())
    } else if label == "Waterfall Cascade" {
        Some(mode.shows_cascades())
    } else {
        None
    }
}

fn apply_target_visibility(
    mode: WorldShotMode,
    targets: &mut Query<(&mut Visibility, &Name), Without<SkyBox>>,
) {
    for (mut visibility, name) in targets {
        let Some(visible) = entity_visible_in_mode(name.as_str(), mode) else {
            continue;
        };
        let desired = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != desired {
            *visibility = desired;
        }
    }
}

fn sync_world_shot_visibility(
    active: Res<WorldShotActive>,
    mut skies: Query<&mut Visibility, With<SkyBox>>,
    mut targets: Query<(&mut Visibility, &Name), Without<SkyBox>>,
) {
    let Some(mode) = active.mode else {
        return;
    };
    for mut sky in &mut skies {
        if *sky != Visibility::Hidden {
            *sky = Visibility::Hidden;
        }
    }
    apply_target_visibility(mode, &mut targets);
}

fn control_world_shot_camera(
    time: Res<Time<Real>>,
    active: Res<WorldShotActive>,
    menu_manager: Option<Res<MenuManager>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    map: Res<TerrainMap>,
    mut view: ResMut<WorldShotView>,
) {
    if active.mode.is_none() || menu_manager.as_ref().is_some_and(|m| m.any_open()) {
        for _ in mouse_wheel.read() {}
        return;
    }

    let dt = time.delta_secs().min(0.1);
    let mut wheel_y = 0.0f32;
    for ev in mouse_wheel.read() {
        wheel_y += ev.y;
    }
    if wheel_y.abs() > 1e-4 {
        view.zoom = (view.zoom * WHEEL_ZOOM_STEP.powf(wheel_y)).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    let zoom_in = keys.pressed(KeyCode::Equal) || keys.pressed(KeyCode::NumpadAdd);
    let zoom_out = keys.pressed(KeyCode::Minus) || keys.pressed(KeyCode::NumpadSubtract);
    if zoom_in != zoom_out {
        let sign = if zoom_in { 1.0 } else { -1.0 };
        view.zoom =
            (view.zoom * (1.0 + KEY_ZOOM_RATE * dt * sign).max(0.2)).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowUp) {
        pan.y -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        pan.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowLeft) {
        pan.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        pan.x += 1.0;
    }
    if pan != Vec2::ZERO {
        let dir = Vec2::from_angle(-view.yaw_deg.to_radians()).rotate(pan.normalize());
        let boost = if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            SHIFT_PAN_BOOST
        } else {
            1.0
        };
        let speed = map.size() * PAN_SPEED_FACTOR / view.clamped_zoom() * boost;
        let limit = map.size() * 1.5;
        view.center =
            (view.center + dir * speed * dt).clamp(Vec2::splat(-limit), Vec2::splat(limit));
    }

    if keys.just_pressed(KeyCode::Home) || keys.just_pressed(KeyCode::Digit0) {
        view.reset();
    }
}

fn sync_world_shot_camera(
    active: Res<WorldShotActive>,
    map: Res<TerrainMap>,
    view: Res<WorldShotView>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<WorldShotCamera>>,
    mut lights: Query<&mut CascadeShadowConfig, With<DirectionalLight>>,
) {
    if active.mode.is_none() {
        return;
    }
    let (next_transform, next_ortho) = camera_geometry(&map, &view);
    let next_cascades = topdown_cascades(next_ortho.near, next_ortho.far, view.clamped_zoom());
    for (mut transform, mut projection) in &mut cameras {
        *transform = next_transform;
        if let Projection::Orthographic(ortho) = projection.as_mut() {
            *ortho = next_ortho.clone();
        } else {
            *projection = Projection::from(next_ortho.clone());
        }
    }
    for mut cascades in &mut lights {
        *cascades = next_cascades.clone();
    }
}

fn draw_river_splines(map: Res<TerrainMap>, active: Res<WorldShotActive>, mut gizmos: Gizmos) {
    if !active.mode.is_some_and(|mode| mode.splines()) {
        return;
    }
    let heights = map.heights();
    let size = map.size();
    let copies = wrapped_copies(map.topology(), size);
    let color = Color::srgb(1.0, 0.25, 0.1);
    for path in map.river_paths() {
        if path.len() < 2 {
            continue;
        }
        let mut points: Vec<Vec3> = Vec::with_capacity(path.len());
        let (fx, fz) = heights.coords(path[0]);
        let mut cursor = heights.world_of(fx, fz);
        for (i, &index) in path.iter().enumerate() {
            if i > 0 {
                let (ax, az) = heights.coords(path[i - 1]);
                let (bx, bz) = heights.coords(index);
                let a = heights.world_of(ax, az);
                let b = heights.world_of(bx, bz);
                cursor += heights.topology.delta_xz(a, b, size);
            }
            let wrapped = heights.topology.wrap_xz(cursor, size);
            let ground_y = map.height_at(wrapped.x, wrapped.y);
            let water_y = map
                .water_surface_at(wrapped.x, wrapped.y)
                .map(|sample| sample.surface_y)
                .unwrap_or(ground_y);
            points.push(Vec3::new(cursor.x, ground_y.max(water_y) + 0.5, cursor.y));
        }
        for offset in &copies {
            let shift = Vec3::new(offset.x, 0.0, offset.y);
            for window in points.windows(2) {
                gizmos.line(window[0] + shift, window[1] + shift, color);
            }
        }
    }
}

fn enforce_shot_atmosphere_off(
    active: Res<WorldShotActive>,
    mut clear_color: ResMut<ClearColor>,
    mut shot_cameras: Query<&mut DistanceFog, With<WorldShotCamera>>,
) {
    let Some(mode) = active.mode else {
        return;
    };
    clear_color.0 = mode.backdrop();
    for mut fog in &mut shot_cameras {
        *fog = disabled_fog();
    }
}

pub fn register(app: &mut App) {
    app.register_type::<WorldShotView>()
        .init_resource::<WorldShotRequest>()
        .init_resource::<WorldShotActive>()
        .init_resource::<WorldShotView>()
        .init_resource::<WorldShotBackup>()
        .register_command(
            "world_shot",
            "Capture the world top-down: <off|full|full_splines|no_water|rivers|rivers_splines|rivers_lakes> [x z [zoom]]",
            world_shot,
        )
        .register_command(
            "world_shot_focus",
            "Focus top-down camera at coordinates: <x> <z> [zoom]",
            world_shot_focus,
        )
        .register_command(
            "world_shot_zoom",
            "Set top-down camera zoom: <zoom>",
            world_shot_zoom,
        )
        .add_systems(
            Update,
            (
                apply_world_shot,
                control_world_shot_camera,
                sync_world_shot_camera,
                sync_world_shot_visibility,
                draw_river_splines,
            )
                .chain(),
        )
        .add_systems(
            Update,
            enforce_shot_atmosphere_off
                .after(sync_terrain_atmosphere)
                .after(sync_underwater_view)
                .after(sync_terrain_fog),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TerrainConfig;
    use crate::utils::registry::commands::CommandRegistryPlugin;

    #[test]
    fn modes_control_terrain_and_water_visibility() {
        assert!(WorldShotMode::NoWater.shows_terrain());
        assert!(!WorldShotMode::NoWater.shows_sea());
        assert!(!WorldShotMode::NoWater.shows_lakes());
        assert!(!WorldShotMode::NoWater.shows_rivers());
        assert!(!WorldShotMode::NoWater.shows_cascades());

        assert!(!WorldShotMode::RiversOnly.shows_terrain());
        assert!(!WorldShotMode::RiversOnly.shows_sea());
        assert!(!WorldShotMode::RiversOnly.shows_lakes());
        assert!(WorldShotMode::RiversOnly.shows_rivers());
        assert!(!WorldShotMode::RiversOnly.shows_cascades());

        assert!(WorldShotMode::RiversLakes.shows_rivers());
        assert!(WorldShotMode::RiversLakes.shows_lakes());
        assert!(WorldShotMode::RiversLakes.shows_cascades());

        assert!(WorldShotMode::Full.shows_terrain());
        assert!(WorldShotMode::Full.shows_sea());
        assert!(WorldShotMode::Full.shows_lakes());
        assert!(WorldShotMode::Full.shows_rivers());
        assert!(WorldShotMode::Full.shows_cascades());
    }

    #[test]
    fn camera_geometry_and_cascades_cover_the_entire_relief() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let (min_y, max_y) = world_height_span(&map);
        let view = WorldShotView {
            center: Vec2::new(12.0, -8.0),
            zoom: 4.0,
            ..default()
        };
        let (transform, projection) = camera_geometry(&map, &view);
        assert!(
            transform.translation.y > max_y,
            "camera must sit above the highest summit"
        );
        assert_eq!(transform.translation.x, 12.0);
        assert_eq!(transform.translation.z, -8.0);
        assert!(projection.near >= 1.0);
        assert!(
            projection.far > (transform.translation.y - min_y),
            "far plane must reach past the deepest seabed"
        );
        assert!((projection.scale - 0.25).abs() < 1e-5);

        let cascades = topdown_cascades(projection.near, projection.far, view.zoom);
        assert!(
            *cascades.bounds.last().unwrap() >= projection.far - 1e-3,
            "shadow cascades must span the whole top-down depth range"
        );
    }

    #[test]
    fn world_shot_spawns_and_restores_cameras_and_cascades() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandRegistryPlugin))
            .insert_resource(TerrainMap::compact())
            .init_resource::<ClearColor>();
        register(&mut app);

        let original_cascades = CascadeShadowConfigBuilder {
            first_cascade_far_bound: 25.0,
            maximum_distance: 200.0,
            ..default()
        }
        .build();

        let main_cam = app
            .world_mut()
            .spawn((MainCamera, Camera3d::default()))
            .id();
        let light = app
            .world_mut()
            .spawn((DirectionalLight::default(), original_cascades.clone()))
            .id();
        let sky = app
            .world_mut()
            .spawn((SkyBox, Visibility::Visible))
            .id();
        let chunk = app
            .world_mut()
            .spawn((Name::new("Terrain Chunk 0 0"), Visibility::Visible))
            .id();
        let sea = app
            .world_mut()
            .spawn((Name::new("Sea Surface"), Visibility::Visible))
            .id();
        let cascade = app
            .world_mut()
            .spawn((Name::new("Waterfall Cascade"), Visibility::Visible))
            .id();

        app.world_mut().resource_mut::<WorldShotRequest>().pending =
            Some(WorldShotMode::NoWater);
        app.update();

        assert!(!app.world().get::<Camera>(main_cam).unwrap().is_active);
        assert_eq!(
            app.world()
                .query_filtered::<Entity, With<WorldShotCamera>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(*app.world().get::<Visibility>(sky).unwrap(), Visibility::Hidden);
        assert_eq!(*app.world().get::<Visibility>(chunk).unwrap(), Visibility::Visible);
        assert_eq!(*app.world().get::<Visibility>(sea).unwrap(), Visibility::Hidden);
        assert_eq!(*app.world().get::<Visibility>(cascade).unwrap(), Visibility::Hidden);
        assert_ne!(
            app.world().get::<CascadeShadowConfig>(light).unwrap().bounds,
            original_cascades.bounds
        );

        let late_river = app
            .world_mut()
            .spawn((Name::new("River Network"), Visibility::Visible))
            .id();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(late_river).unwrap(),
            Visibility::Hidden,
            "asynchronously spawned water must inherit active no_water hiding"
        );

        app.world_mut().resource_mut::<WorldShotRequest>().pending = Some(WorldShotMode::Off);
        app.update();

        assert!(app.world().get::<Camera>(main_cam).unwrap().is_active);
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<WorldShotCamera>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(*app.world().get::<Visibility>(sky).unwrap(), Visibility::Visible);
        assert_eq!(*app.world().get::<Visibility>(sea).unwrap(), Visibility::Visible);
        assert_eq!(*app.world().get::<Visibility>(cascade).unwrap(), Visibility::Visible);
        assert_eq!(
            app.world()
                .get::<CascadeShadowConfig>(light)
                .unwrap()
                .bounds,
            original_cascades.bounds
        );
    }
}