//! Terrain: a procedurally generated heightfield, its collider/mesh, and the atmosphere
//! (fog, sun, skybox) that renders it.
//!
//! The pure heightfield model (Voronoi cells + FBM noise, `height_at`) lives in [`map`] and
//! has no rendering concern; this module is the Bevy side that consumes it — mesh/collider
//! building, fog, lighting, and the console commands that poke the model.

use avian3d::prelude::*;
use bevy::{
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    mesh::VertexAttributeValues,
    prelude::*,
};

use crate::utils::registry::commands::{CommandAppExt, CommandArgs, command};
use crate::utils::SpawnGroundHeight;

mod map;

pub use map::TerrainMap;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainMap>()
            .init_resource::<TerrainFog>();

        // Publish the origin surface height now (at build, before any Startup runs) so the
        // player spawns at the right height even though `character` never reads `TerrainMap`.
        let origin_height = app.world().resource::<TerrainMap>().height_at(0.0, 0.0);
        app.insert_resource(SpawnGroundHeight(origin_height))
            .add_systems(Startup, (setup_terrain, setup_atmosphere))
            .add_systems(Update, (sync_terrain_fog, rebuild_terrain_on_change));

        command!(app, "Toggle atmospheric fog", toggle_atmospheric_fog);
        command!(
            app,
            "Toggle directional light fog influence",
            toggle_fog_light_influence
        );
        command!(
            app,
            "Set a Voronoi cell's base height (can be negative): <cell_id> <height>",
            set_cell_height
        );
        command!(
            app,
            "Inspect Voronoi terrain cells (optional: <x> <z>)",
            cell_info
        );
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
struct TerrainMesh;

#[derive(Component)]
pub(crate) struct SkyBox;

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

fn build_terrain_mesh(terrain_map: &TerrainMap) -> Mesh {
    let mut terrain = Mesh::from(
        Plane3d::default()
            .mesh()
            .size(terrain_map.size, terrain_map.size)
            .subdivisions(terrain_map.subdivisions),
    );

    let max_extent = terrain_map.max_height_extent();
    let mut colors = Vec::new();
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        terrain.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        colors.reserve(positions.len());
        for pos in positions.iter_mut() {
            let y = terrain_map.height_at(pos[0], pos[2]);
            pos[1] = y;
            let h = normalized_height(y, max_extent);
            colors.push(height_colour(h).to_f32_array());
        }
    }

    terrain.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    terrain.compute_normals();
    terrain
}

fn setup_terrain(
    mut commands: Commands,
    terrain_map: Res<TerrainMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let terrain = build_terrain_mesh(&terrain_map);
    let collider = Collider::trimesh_from_mesh(&terrain)
        .expect("Terrain mesh must create a trimesh collider");

    commands.spawn((
        Name::new("Terrain"),
        TerrainMesh,
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

fn rebuild_terrain_on_change(
    terrain_map: Res<TerrainMap>,
    mut last_generation: Local<Option<u64>>,
    mut spawn_height: ResMut<SpawnGroundHeight>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(&Mesh3d, &mut Collider), With<TerrainMesh>>,
) {
    if *last_generation == Some(terrain_map.generation()) {
        return;
    }
    let first_run = last_generation.is_none();
    *last_generation = Some(terrain_map.generation());
    // Keep the published origin height in sync with the (possibly edited) heightfield.
    spawn_height.0 = terrain_map.height_at(0.0, 0.0);
    if first_run {
        return;
    }

    let mesh = build_terrain_mesh(&terrain_map);
    let collider =
        Collider::trimesh_from_mesh(&mesh).expect("Terrain mesh must create a trimesh collider");

    for (mesh_handle, mut col) in &mut query {
        if let Some(mut existing) = meshes.get_mut(&mesh_handle.0) {
            *existing = mesh.clone();
        }
        *col = collider.clone();
    }
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

fn set_cell_height(args: Res<CommandArgs>, mut terrain_map: ResMut<TerrainMap>) {
    let Some(cell_id) = args.args.first().and_then(|s| s.parse::<usize>().ok()) else {
        warn!("Usage: set_cell_height <cell_id> <height>");
        return;
    };
    let Some(height) = args.get_f32(1) else {
        warn!("Usage: set_cell_height <cell_id> <height>");
        return;
    };

    if terrain_map.set_cell_base_height(cell_id, height) {
        info!("Set Voronoi cell {} base_height to {:.2}", cell_id, height);
    } else {
        warn!(
            "Voronoi cell {} not found (valid ids: 0..{})",
            cell_id,
            terrain_map.cells().len()
        );
    }
}

fn cell_info(args: Res<CommandArgs>, terrain_map: Res<TerrainMap>) {
    let x = args.get_f32(0).unwrap_or(0.0);
    let z = args.get_f32(1).unwrap_or(0.0);
    let cells = terrain_map.cells();
    let min_h = cells
        .iter()
        .map(|c| c.base_height)
        .fold(f32::INFINITY, f32::min);
    let max_h = cells
        .iter()
        .map(|c| c.base_height)
        .fold(f32::NEG_INFINITY, f32::max);

    info!(
        "Voronoi cells: {} | base_height range: [{:.2}, {:.2}]",
        cells.len(),
        min_h,
        max_h
    );
    if let Some(cell) = terrain_map.cell_at(x, z) {
        info!(
            "Cell at ({:.1}, {:.1}) -> id={}, center=({:.1}, {:.1}), base_height={:.2}",
            x, z, cell.id, cell.center.x, cell.center.y, cell.base_height
        );
    }
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