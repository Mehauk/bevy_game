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

/// Height-band boundaries, as normalized 0..1 upper edges. Each band's width is *derived*
/// from these edges so the three widths cannot drift out of sync with the boundaries they
/// span (a hand-copied width silently mis-colours every vertex in the band).
const VALLEY_BAND: f32 = 0.45;
const SLOPE_BAND: f32 = 0.72;
const HILLSIDE_BAND: f32 = SLOPE_BAND - VALLEY_BAND;
const RIDGE_BAND: f32 = 1.0 - SLOPE_BAND;

const VALLEY_COLOUR: Color = Color::srgb(0.20, 0.34, 0.16);
const HILLSIDE_COLOUR: Color = Color::srgb(0.28, 0.42, 0.20);
const SLOPE_ROCK_COLOUR: Color = Color::srgb(0.38, 0.36, 0.31);
const RIDGE_COLOUR: Color = Color::srgb(0.48, 0.46, 0.43);

/// Map a world-space height to 0..1 across the terrain's amplitude range.
fn normalized_height(y: f32, amplitude: f32) -> f32 {
    ((y / amplitude) * 0.5 + 0.5).clamp(0.0, 1.0)
}

/// Vertex colour for a normalized height in 0..1: valley floor blends into hillside, then
/// rock, then ridge. Each band blends across its own width, so there are no hard seams.
/// Blending happens in linear space, which is what keeps the mid-tones from going muddy.
fn height_colour(h: f32) -> LinearRgba {
    let (from, to, t) = if h < VALLEY_BAND {
        (VALLEY_COLOUR, HILLSIDE_COLOUR, h / VALLEY_BAND)
    } else if h < SLOPE_BAND {
        (
            HILLSIDE_COLOUR,
            SLOPE_ROCK_COLOUR,
            (h - VALLEY_BAND) / HILLSIDE_BAND,
        )
    } else {
        (
            SLOPE_ROCK_COLOUR,
            RIDGE_COLOUR,
            (h - SLOPE_BAND) / RIDGE_BAND,
        )
    };
    let (from, to) = (from.to_linear(), to.to_linear());
    from.mix(&to, t)
}

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

    let mut colors = Vec::new();
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        terrain.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        colors.reserve(positions.len());
        for pos in positions.iter_mut() {
            let y = terrain_map.height_at(pos[0], pos[2]);
            pos[1] = y;
            let h = normalized_height(y, terrain_map.amplitude);
            colors.push(height_colour(h).to_f32_array());
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
/// An alpha above this counts as "on" when toggling, so a faded-out value toggles back on.
const FOG_ON_THRESHOLD: f32 = 0.05;

/// Flip a fog colour between fully off and `on_alpha`.
fn toggle_fog_alpha(color: &mut Color, on_alpha: f32) {
    color.set_alpha(if color.alpha() > FOG_ON_THRESHOLD {
        0.0
    } else {
        on_alpha
    });
}

fn toggle_atmospheric_fog(mut terrain_fog: ResMut<TerrainFog>) {
    toggle_fog_alpha(&mut terrain_fog.0.color, DEFAULT_FOG_ALPHA);
}

fn toggle_fog_light_influence(mut terrain_fog: ResMut<TerrainFog>) {
    toggle_fog_alpha(&mut terrain_fog.0.directional_light_color, DEFAULT_FOG_LIGHT_ALPHA);
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_bands_tile_the_unit_range_exactly() {
        assert!(VALLEY_BAND < SLOPE_BAND, "band edges must be ordered");
        let total = VALLEY_BAND + HILLSIDE_BAND + RIDGE_BAND;
        assert!((total - 1.0).abs() < 1e-6, "bands must tile 0..1 exactly, got {total}");
    }

    #[test]
    fn normalized_height_spans_and_clamps_to_zero_one() {
        assert!((normalized_height(0.0, 22.0) - 0.5).abs() < 1e-6, "mid height is mid range");
        assert_eq!(normalized_height(22.0, 22.0), 1.0);
        assert_eq!(normalized_height(-22.0, 22.0), 0.0);
        assert_eq!(normalized_height(1000.0, 22.0), 1.0, "must clamp above");
        assert_eq!(normalized_height(-1000.0, 22.0), 0.0, "must clamp below");
    }

    #[test]
    fn height_colour_is_continuous_across_band_boundaries() {
        // A discontinuity here would show up as a hard ring painted on the terrain.
        for edge in [VALLEY_BAND, SLOPE_BAND] {
            let below = height_colour(edge - 1e-4).to_f32_array();
            let above = height_colour(edge + 1e-4).to_f32_array();
            let delta: f32 = (0..3).map(|i| (below[i] - above[i]).abs()).sum();
            assert!(delta < 0.01, "colour jumps at band edge {edge}: delta {delta}");
        }
    }

    #[test]
    fn height_colour_endpoints_are_the_outer_band_colours() {
        assert_eq!(
            height_colour(0.0).to_f32_array(),
            VALLEY_COLOUR.to_linear().to_f32_array()
        );
        assert_eq!(
            height_colour(1.0).to_f32_array(),
            RIDGE_COLOUR.to_linear().to_f32_array()
        );
    }

    #[test]
    fn terrain_height_is_deterministic_and_varied() {
        let map = TerrainMap::default();
        assert_eq!(
            map.height_at(12.5, -3.25),
            map.height_at(12.5, -3.25),
            "height must be a pure function of x,z"
        );
        assert!(map.height_at(12.5, -3.25).is_finite());
        assert_ne!(map.height_at(0.0, 0.0), map.height_at(40.0, 40.0), "terrain must vary");
    }

    #[test]
    fn fog_toggle_round_trips() {
        let mut color = Color::srgba(0.1, 0.2, 0.3, DEFAULT_FOG_ALPHA);
        toggle_fog_alpha(&mut color, DEFAULT_FOG_ALPHA);
        assert_eq!(color.alpha(), 0.0, "on -> off");
        toggle_fog_alpha(&mut color, DEFAULT_FOG_ALPHA);
        assert_eq!(color.alpha(), DEFAULT_FOG_ALPHA, "off -> on");
    }

    #[test]
    fn a_faded_fog_counts_as_off() {
        let mut color = Color::srgba(0.1, 0.2, 0.3, FOG_ON_THRESHOLD / 2.0);
        toggle_fog_alpha(&mut color, DEFAULT_FOG_ALPHA);
        assert_eq!(color.alpha(), DEFAULT_FOG_ALPHA, "a near-zero alpha must toggle back on");
    }
}
