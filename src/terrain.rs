use avian3d::prelude::*;
use bevy::{
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    mesh::VertexAttributeValues,
    prelude::*,
    transform::TransformSystems,
};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::{
    camera::{CameraSystems, MainCamera},
    utils::commands::{CommandAppExt, command},
};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainMap>()
            .init_resource::<TerrainFog>()
            .add_systems(Startup, (setup_terrain, setup_atmosphere))
            .add_systems(Update, sync_terrain_fog)
            .add_systems(
                PostUpdate,
                sync_skybox_to_camera
                    .after(CameraSystems::UpdateTransform)
                    .before(TransformSystems::Propagate),
            );

        command!(app, "Toggle atmospheric fog", toggle_atmospheric_fog);
        command!(
            app,
            "Toggle directional light fog influence",
            toggle_fog_light_influence
        );
    }
}

#[derive(Resource, Clone)]
pub struct TerrainMap {
    pub size: f32,
    pub subdivisions: u32,
    pub amplitude: f32,
    fbm: Fbm<Perlin>,
}

impl Default for TerrainMap {
    fn default() -> Self {
        let fbm = Fbm::<Perlin>::new(42)
            .set_octaves(4)
            .set_frequency(0.009)
            .set_persistence(0.42)
            .set_lacunarity(2.0);

        Self {
            size: 400.0,
            subdivisions: 200,
            amplitude: 22.0,
            fbm,
        }
    }
}

impl TerrainMap {
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let broad = self.fbm.get([x as f64, z as f64]) as f32;
        let swell = ((x * 0.012).sin() + (z * 0.012).cos()) * 0.15;
        (broad + swell) * self.amplitude
    }
}

#[derive(Resource, Clone)]
pub struct TerrainFog(pub DistanceFog);

impl Default for TerrainFog {
    fn default() -> Self {
        Self(DistanceFog {
            color: Color::srgba(0.35, 0.48, 0.66, 1.0),
            directional_light_color: Color::srgba(1.0, 0.95, 0.85, 0.5),
            directional_light_exponent: 30.0,
            falloff: FogFalloff::from_visibility_colors(
                110.0,
                Color::srgb(0.35, 0.5, 0.66),
                Color::srgb(0.8, 0.844, 1.0),
            ),
        })
    }
}

#[derive(Component)]
struct SkyBox;

fn setup_terrain(
    mut commands: Commands,
    terrain_map: Res<TerrainMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut terrain = Mesh::from(
        Plane3d::default()
            .mesh()
            .size(terrain_map.size, terrain_map.size)
            .subdivisions(terrain_map.subdivisions),
    );

    let valley = Color::srgb(0.20, 0.34, 0.16).to_linear();
    let hillside = Color::srgb(0.28, 0.42, 0.20).to_linear();
    let slope_rock = Color::srgb(0.38, 0.36, 0.31).to_linear();
    let ridge = Color::srgb(0.48, 0.46, 0.43).to_linear();

    // Height bands (normalized 0..1) and their widths; widths must equal the band gaps.
    const VALLEY_BAND: f32 = 0.45;
    const SLOPE_BAND: f32 = 0.72;
    const HILLSIDE_BAND: f32 = 0.27; // SLOPE_BAND - VALLEY_BAND
    const RIDGE_BAND: f32 = 0.28; // 1.0 - SLOPE_BAND

    let mut colors = Vec::new();
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        terrain.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        colors.reserve(positions.len());
        for pos in positions.iter_mut() {
            let y = terrain_map.height_at(pos[0], pos[2]);
            pos[1] = y;

            let h = ((y / terrain_map.amplitude) * 0.5 + 0.5).clamp(0.0, 1.0);
            let color = if h < VALLEY_BAND {
                valley.mix(&hillside, h / VALLEY_BAND)
            } else if h < SLOPE_BAND {
                hillside.mix(&slope_rock, (h - VALLEY_BAND) / HILLSIDE_BAND)
            } else {
                slope_rock.mix(&ridge, (h - SLOPE_BAND) / RIDGE_BAND)
            };
            colors.push(color.to_f32_array());
        }
    }

    terrain.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    terrain.compute_normals();

    let collider = Collider::trimesh_from_mesh(&terrain)
        .expect("Terrain mesh must create a trimesh collider");

    commands.spawn((
        Name::new("Terrain"),
        RigidBody::Static,
        collider,
        Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
        Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
        Transform::default(),
        Mesh3d(meshes.add(terrain)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.92,
            reflectance: 0.15,
            ..default()
        })),
    ));
}

fn setup_atmosphere(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cascade_shadow_config = CascadeShadowConfigBuilder {
        first_cascade_far_bound: 25.0,
        maximum_distance: 200.0,
        ..default()
    }
    .build();

    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.98, 0.95, 0.82),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(Vec3::new(-0.15, -0.05, 0.25), Vec3::Y),
        cascade_shadow_config,
    ));

    commands.spawn((
        SkyBox,
        Mesh3d(meshes.add(Cuboid::new(2.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Srgba::hex("888888").unwrap().into(),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(600.0)),
        NotShadowCaster,
    ));
}

fn sync_terrain_fog(
    terrain_fog: Res<TerrainFog>,
    mut commands: Commands,
    mut cameras: Query<(Entity, Option<&mut DistanceFog>), With<Camera3d>>,
) {
    for (entity, existing_fog) in &mut cameras {
        if let Some(mut fog) = existing_fog {
            if terrain_fog.is_changed() {
                *fog = terrain_fog.0.clone();
            }
        } else {
            commands.entity(entity).insert(terrain_fog.0.clone());
        }
    }
}

fn sync_skybox_to_camera(
    camera: Query<&Transform, (With<MainCamera>, Without<SkyBox>)>,
    mut skybox: Single<&mut Transform, (With<SkyBox>, Without<MainCamera>)>,
) {
    if let Some(cam_transform) = camera.iter().next() {
        skybox.translation = cam_transform.translation;
    }
}

const DEFAULT_FOG_ALPHA: f32 = 1.0;
const DEFAULT_FOG_LIGHT_ALPHA: f32 = 0.5;

fn toggle_atmospheric_fog(mut terrain_fog: ResMut<TerrainFog>) {
    let cur = terrain_fog.0.color.alpha();
    let target = if cur > 0.05 { 0.0 } else { DEFAULT_FOG_ALPHA };
    terrain_fog.0.color.set_alpha(target);
}

fn toggle_fog_light_influence(mut terrain_fog: ResMut<TerrainFog>) {
    let cur = terrain_fog.0.directional_light_color.alpha();
    let target = if cur > 0.05 { 0.0 } else { DEFAULT_FOG_LIGHT_ALPHA };
    terrain_fog.0.directional_light_color.set_alpha(target);
}