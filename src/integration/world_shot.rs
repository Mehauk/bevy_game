use bevy::pbr::DistanceFog;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::terrain::TerrainMap;
use crate::terrain::sky::sync_terrain_fog;
use crate::utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput};

#[derive(Component)]
pub struct WorldShotCamera;

#[derive(Resource, Default)]
pub struct WorldShotRequest {
    pub pending: Option<WorldShotMode>,
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
}

#[derive(Resource, Default)]
pub struct WorldShotActive {
    pub mode: Option<WorldShotMode>,
}

pub fn world_shot(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut request: ResMut<WorldShotRequest>,
) {
    let Some(label) = args.args.first() else {
        output.set(
            "error: usage: world_shot <off|full|full_splines|no_water|rivers|rivers_splines|rivers_lakes>",
        );
        return;
    };
    let Some(mode) = WorldShotMode::parse(label) else {
        output.set(format!(
            "error: unknown mode '{label}' (off | full | full_splines | no_water | rivers | rivers_splines | rivers_lakes)"
        ));
        return;
    };
    request.pending = Some(mode);
    let text = format!("world shot queued: {}", mode.label());
    output.set(text.clone());
    info!("{text}");
}

fn disabled_fog() -> DistanceFog {
    let mut fog = DistanceFog::default();
    fog.color.set_alpha(0.0);
    fog
}

fn apply_world_shot(
    mut commands: Commands,
    mut request: ResMut<WorldShotRequest>,
    mut active: ResMut<WorldShotActive>,
    map: Res<TerrainMap>,
    mut main_cams: Query<&mut Camera, With<MainCamera>>,
    mut shot_cameras: Query<(Entity, Option<&mut DistanceFog>), With<WorldShotCamera>>,
    mut targets: Query<(&mut Visibility, &Name)>,
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
        for (mut visibility, _) in &mut targets {
            *visibility = Visibility::Visible;
        }
        active.mode = None;
        info!("world shot off");
        return;
    }

    if shot_cameras.is_empty() {
        let size = map.size();
        let altitude = size * 1.5;
        let projection = OrthographicProjection {
            near: 1.0,
            far: altitude * 2.0,
            viewport_origin: Vec2::splat(0.5),
            scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                viewport_height: size,
            },
            scale: 1.0,
            area: Rect::default(),
        };
        commands.spawn((
            Camera3d::default(),
            Projection::from(projection),
            Transform::from_xyz(0.0, altitude, 0.0)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            WorldShotCamera,
            disabled_fog(),
        ));
    }

    for mut camera in &mut main_cams {
        camera.is_active = false;
    }

    for (entity, fog) in &mut shot_cameras {
        match fog {
            Some(mut fog) => fog.color.set_alpha(0.0),
            None => {
                commands.entity(entity).insert(disabled_fog());
            }
        }
    }

    for (mut visibility, name) in &mut targets {
        let label = name.as_str();
        let visible = if label.starts_with("Terrain Chunk") {
            mode.shows_terrain()
        } else if label == "Sea Surface" {
            mode.shows_sea()
        } else if label == "River Network" {
            mode.shows_rivers()
        } else if label == "Lake Surface" {
            mode.shows_lakes()
        } else {
            continue;
        };
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    active.mode = Some(mode);
    info!("world shot applied: {}", mode.label());
}

fn draw_river_splines(map: Res<TerrainMap>, active: Res<WorldShotActive>, mut gizmos: Gizmos) {
    if !active.mode.is_some_and(|mode| mode.splines()) {
        return;
    }
    let heights = map.heights();
    let color = Color::srgb(1.0, 0.25, 0.1);
    for path in map.river_paths() {
        let points: Vec<Vec3> = path
            .iter()
            .filter_map(|&index| {
                let (gx, gz) = heights.coords(index);
                let p = heights.world_of(gx, gz);
                let y = map.height_at(p.x, p.y) + 0.5;
                Some(Vec3::new(p.x, y, p.y))
            })
            .collect();
        for window in points.windows(2) {
            gizmos.line(window[0], window[1], color);
        }
    }
}

fn enforce_shot_fog_off(
    active: Res<WorldShotActive>,
    mut shot_cameras: Query<&mut DistanceFog, With<WorldShotCamera>>,
) {
    if active.mode.is_none() {
        return;
    }
    for mut fog in &mut shot_cameras {
        fog.color.set_alpha(0.0);
        fog.directional_light_color.set_alpha(0.0);
    }
}

pub fn register(app: &mut App) {
    app.init_resource::<WorldShotRequest>()
        .init_resource::<WorldShotActive>()
        .register_command(
            "world_shot",
            "Capture the world top-down: <off|full|full_splines|no_water|rivers|rivers_splines|rivers_lakes>",
            world_shot,
        )
        .add_systems(Update, apply_world_shot)
        .add_systems(Update, draw_river_splines)
        .add_systems(Update, enforce_shot_fog_off.after(sync_terrain_fog));
}
