use avian3d::prelude::*;
use bevy::{
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    mesh::VertexAttributeValues,
    prelude::*,
    transform::TransformSystems,
};
use glam27::DVec3 as VoroDVec3;
use meshless_voronoi::VoronoiIntegrator;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::{
    camera::{CameraSystems, MainCamera},
    utils::registry::commands::{CommandAppExt, CommandArgs, command},
};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainMap>()
            .init_resource::<TerrainFog>()
            .add_systems(Startup, (setup_terrain, setup_atmosphere))
            .add_systems(Update, (sync_terrain_fog, rebuild_terrain_on_change))
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

#[derive(Clone, Debug)]
pub struct TerrainCell {
    pub id: usize,
    pub center: Vec2,
    pub base_height: f32,
}

#[derive(Resource, Clone)]
pub struct TerrainMap {
    pub size: f32,
    pub subdivisions: u32,
    pub amplitude: f32,
    pub cell_grid_dim: u32,
    pub cell_base_height_range: f32,
    pub cell_blend_width: f32,
    pub cell_seed: u64,
    cells: Vec<TerrainCell>,
    generation: u64,
    fbm: Fbm<Perlin>,
}

impl Default for TerrainMap {
    fn default() -> Self {
        let fbm = Fbm::<Perlin>::new(42)
            .set_octaves(4)
            .set_frequency(0.009)
            .set_persistence(0.42)
            .set_lacunarity(2.0);

        let mut map = Self {
            size: 400.0,
            subdivisions: 200,
            amplitude: 22.0,
            cell_grid_dim: 8,
            cell_base_height_range: 12.0,
            cell_blend_width: 8.0,
            cell_seed: 1337,
            cells: Vec::new(),
            generation: 0,
            fbm,
        };
        map.rebuild_voronoi_cells();
        map
    }
}

impl TerrainMap {
    /// Split the terrain map into 2D Voronoi cells using `meshless_voronoi` and
    /// assign each cell an initial base height (positive or negative).
    pub fn rebuild_voronoi_cells(&mut self) {
        let dim = self.cell_grid_dim.max(1);
        let half = self.size * 0.5;
        let cell_span = self.size / dim as f32;

        let mut generators = Vec::with_capacity((dim * dim) as usize);
        for gz in 0..dim {
            for gx in 0..dim {
                let idx = (gz * dim + gx) as u64;
                let jx = 0.15 + 0.70 * hash_unit_f32(self.cell_seed, idx, 0);
                let jz = 0.15 + 0.70 * hash_unit_f32(self.cell_seed, idx, 1);
                let x = -half + (gx as f32 + jx) * cell_span;
                let z = -half + (gz as f32 + jz) * cell_span;
                generators.push(VoroDVec3::new(x as f64, z as f64, 0.0));
            }
        }

        let anchor = VoroDVec3::new(-half as f64, -half as f64, -0.5);
        let width = VoroDVec3::new(self.size as f64, self.size as f64, 1.0);
        let voronoi = VoronoiIntegrator::build(
            &generators,
            None,
            anchor,
            width,
            2.try_into().unwrap(),
            false,
        );

        let mut cells = Vec::with_capacity(generators.len());
        for i in 0..generators.len() {
            if let Some(cell) = voronoi.get_cell_at(i) {
                let base_height = (hash_unit_f32(self.cell_seed, cell.idx as u64, 2) * 2.0 - 1.0)
                    * self.cell_base_height_range;
                cells.push(TerrainCell {
                    id: cell.idx,
                    center: Vec2::new(cell.loc.x as f32, cell.loc.y as f32),
                    base_height,
                });
            }
        }

        self.cells = cells;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn cells(&self) -> &[TerrainCell] {
        &self.cells
    }

    pub fn cell_at(&self, x: f32, z: f32) -> Option<&TerrainCell> {
        let p = Vec2::new(x, z);
        self.cells
            .iter()
            .min_by(|a, b| p.distance_squared(a.center).total_cmp(&p.distance_squared(b.center)))
    }

    /// Modify a single Voronoi cell's base height (supports negative values).
    pub fn set_cell_base_height(&mut self, cell_id: usize, base_height: f32) -> bool {
        let Some(cell) = self.cells.iter_mut().find(|c| c.id == cell_id) else {
            return false;
        };
        cell.base_height = base_height;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Base height contribution from the Voronoi cells at `(x, z)`.
    ///
    /// Returns the exact cell `base_height` in cell interiors and smoothly blends
    /// across `cell_blend_width` near cell boundaries (or hard-steps when `<= 0.0`).
    pub fn cell_base_height_at(&self, x: f32, z: f32) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let p = Vec2::new(x, z);

        let mut min_dist = f32::INFINITY;
        let mut nearest_height = 0.0;
        for cell in &self.cells {
            let d = p.distance(cell.center);
            if d < min_dist {
                min_dist = d;
                nearest_height = cell.base_height;
            }
        }

        if self.cell_blend_width <= 0.0 {
            return nearest_height;
        }

        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        for cell in &self.cells {
            let delta = p.distance(cell.center) - min_dist;
            if delta < self.cell_blend_width {
                let u = (delta / self.cell_blend_width).clamp(0.0, 1.0);
                let w = 1.0 - (3.0 * u * u - 2.0 * u * u * u);
                weighted_sum += cell.base_height * w;
                total_weight += w;
            }
        }

        if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            nearest_height
        }
    }

    pub fn max_height_extent(&self) -> f32 {
        (self.amplitude + self.cell_base_height_range).max(1.0)
    }

    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let broad = self.fbm.get([x as f64, z as f64]) as f32;
        let swell = ((x * 0.012).sin() + (z * 0.012).cos()) * 0.15;
        let noise_height = (broad + swell) * self.amplitude;
        noise_height + self.cell_base_height_at(x, z)
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn hash_unit_f32(seed: u64, index: u64, channel: u64) -> f32 {
    let h = splitmix64(
        seed ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ channel.wrapping_mul(0xbf58_476d_1ce4_e5b9),
    );
    ((h >> 40) as f32) / ((1u64 << 24) as f32)
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(&Mesh3d, &mut Collider), With<TerrainMesh>>,
) {
    if *last_generation == Some(terrain_map.generation) {
        return;
    }
    let first_run = last_generation.is_none();
    *last_generation = Some(terrain_map.generation);
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
    fn voronoi_cells_cover_map_with_positive_and_negative_base_heights() {
        let map = TerrainMap::default();
        let expected_count = (map.cell_grid_dim * map.cell_grid_dim) as usize;
        assert_eq!(map.cells().len(), expected_count);

        let half = map.size * 0.5;
        for cell in map.cells() {
            assert!(cell.center.x >= -half && cell.center.x <= half);
            assert!(cell.center.y >= -half && cell.center.y <= half);
            assert!(cell.base_height.abs() <= map.cell_base_height_range);
        }

        assert!(map.cells().iter().any(|c| c.base_height < 0.0), "must have negative cell heights");
        assert!(map.cells().iter().any(|c| c.base_height > 0.0), "must have positive cell heights");
    }

    #[test]
    fn cell_base_height_blends_smoothly_and_hard_steps_when_zero_blend() {
        let mut map = TerrainMap {
            size: 100.0,
            subdivisions: 10,
            amplitude: 0.0,
            cell_grid_dim: 2,
            cell_base_height_range: 10.0,
            cell_blend_width: 4.0,
            ..default()
        };
        map.cells = vec![
            TerrainCell {
                id: 0,
                center: Vec2::new(-20.0, 0.0),
                base_height: -8.0,
            },
            TerrainCell {
                id: 1,
                center: Vec2::new(20.0, 0.0),
                base_height: 12.0,
            },
        ];

        assert!((map.cell_base_height_at(-20.0, 0.0) - (-8.0)).abs() < 1e-5);
        assert!((map.cell_base_height_at(20.0, 0.0) - 12.0).abs() < 1e-5);
        assert!((map.cell_base_height_at(0.0, 0.0) - 2.0).abs() < 1e-5);

        map.cell_blend_width = 0.0;
        assert_eq!(map.cell_base_height_at(-0.5, 0.0), -8.0);
        assert_eq!(map.cell_base_height_at(0.5, 0.0), 12.0);
    }

    #[test]
    fn modifying_cell_base_height_updates_terrain_height() {
        let mut map = TerrainMap::default();
        let cell = map.cells()[0].clone();
        let before = map.height_at(cell.center.x, cell.center.y);
        let new_height = -18.5;

        assert!(map.set_cell_base_height(cell.id, new_height));
        let after = map.height_at(cell.center.x, cell.center.y);

        assert!((after - before - (new_height - cell.base_height)).abs() < 1e-4);
        assert!(!map.set_cell_base_height(usize::MAX, 0.0));
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