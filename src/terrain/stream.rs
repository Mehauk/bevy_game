use std::sync::Arc;

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};

use crate::terrain::chunk::{ChunkKey, DEFAULT_CHUNK_SIZE, TerrainChunk, chunk_material};
use crate::terrain::colour::debug_colour;
use crate::terrain::gpu::{TerrainRenderMaterial, TileArrays};
use crate::terrain::job::poll_task;
use crate::terrain::map::TerrainMap;
use crate::terrain::material::DebugMaterialMode;
use crate::terrain::season::{Season, SeasonClock};
use crate::terrain::seed::{splitmix64, WorldSeed};
use crate::terrain::topology::{chunk_key, chunk_origin, WorldTopology};
use crate::terrain::{DebugView, TerrainDebugView};

pub const STREAM_BASE_STEPS: usize = 48;
pub const STREAM_MAX_LOD: usize = 2;
pub const STREAM_RING_RADIUS: i32 = 3;
pub const STREAM_SKIRT_DROP: f32 = 8.0;
pub const STREAM_UPLOAD_BUDGET: usize = 4;
pub const STREAM_MAX_JOBS: usize = 8;
pub const STREAM_STAGE: u64 = 21;

const RIVER_CELL: f32 = 8.0;
const RIVER_HALO: i32 = 12;
const RIVER_THRESHOLD: f32 = 20.0;
const RIVER_DEPTH: f32 = 5.0;
const VORONOI_CELL: f32 = 70.0;
const TEXTURE_METRES: f32 = 24.0;

fn lattice_hash(base: u64, x: i64, z: i64) -> u64 {
    splitmix64(
        base ^ splitmix64(
            (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ (z as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f),
        ),
    )
}

fn unit_at(base: u64, x: i64, z: i64) -> f32 {
    ((lattice_hash(base, x, z) >> 40) as f32) / ((1u64 << 24) as f32)
}

fn smooth_unit(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(base: u64, x: f32, z: f32, scale: f32) -> f32 {
    let gx = x / scale;
    let gz = z / scale;
    let x0 = gx.floor() as i64;
    let z0 = gz.floor() as i64;
    let fx = smooth_unit(gx - x0 as f32);
    let fz = smooth_unit(gz - z0 as f32);
    let v00 = unit_at(base, x0, z0);
    let v10 = unit_at(base, x0 + 1, z0);
    let v01 = unit_at(base, x0, z0 + 1);
    let v11 = unit_at(base, x0 + 1, z0 + 1);
    let top = v00 + (v10 - v00) * fx;
    let bottom = v01 + (v11 - v01) * fx;
    top + (bottom - top) * fz
}

fn fbm(base: u64, x: f32, z: f32, scale: f32, octaves: usize, gain: f32) -> f32 {
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    let mut amp = 1.0f32;
    let mut step = scale;
    for octave in 0..octaves {
        let band = value_noise(base ^ splitmix64(octave as u64 + 1), x, z, step) * 2.0 - 1.0;
        sum += amp * band;
        norm += amp;
        amp *= gain;
        step *= 0.5;
    }
    sum / norm.max(f32::EPSILON)
}

#[derive(Clone, Copy, Debug)]
pub struct StreamCell {
    pub id: u64,
    pub base: f32,
    pub edge: f32,
}

fn voronoi_at(base: u64, x: f32, z: f32, cell: f32) -> StreamCell {
    let cx = (x / cell).floor() as i64;
    let cz = (z / cell).floor() as i64;
    let mut best = f32::MAX;
    let mut second = f32::MAX;
    let mut best_id = 0u64;
    let mut best_base = 0.0f32;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let ix = cx + dx;
            let iz = cz + dz;
            let jitter = Vec2::new(
                0.2 + 0.6 * unit_at(base ^ 0x2a, ix, iz),
                0.2 + 0.6 * unit_at(base ^ 0x5c, ix, iz),
            );
            let centre = Vec2::new(ix as f32 * cell, iz as f32 * cell) + jitter * cell;
            let distance = Vec2::new(x, z).distance(centre);
            if distance < best {
                second = best;
                best = distance;
                best_id = lattice_hash(base ^ 0x77, ix, iz);
                best_base = unit_at(base ^ 0x91, ix, iz) * 2.0 - 1.0;
            } else if distance < second {
                second = distance;
            }
        }
    }
    StreamCell {
        id: best_id,
        base: best_base,
        edge: ((second - best) / cell).clamp(0.0, 1.0),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InfiniteSampler {
    base: u64,
    seed: WorldSeed,
}

impl InfiniteSampler {
    pub fn new(seed: WorldSeed) -> Self {
        Self {
            base: seed.stage(STREAM_STAGE),
            seed,
        }
    }

    pub fn seed(&self) -> WorldSeed {
        self.seed
    }

    pub fn macro_elevation(&self, x: f32, z: f32) -> f32 {
        fbm(self.base, x, z, 900.0, 3, 0.5) * 55.0
            + fbm(self.base ^ 0x1f, x, z, 2600.0, 2, 0.5) * 30.0
    }

    pub fn cell_at(&self, x: f32, z: f32) -> StreamCell {
        voronoi_at(self.base ^ 0x33, x, z, VORONOI_CELL)
    }

    pub fn base_height(&self, x: f32, z: f32) -> f32 {
        let cell = self.cell_at(x, z);
        let detail = fbm(self.base ^ 0x11, x, z, 42.0, 4, 0.5) * 9.0;
        let ridge = (1.0 - fbm(self.base ^ 0x2b, x, z, 130.0, 3, 0.5).abs()) * 6.0;
        self.macro_elevation(x, z)
            + cell.base * 9.0 * (0.35 + 0.65 * cell.edge)
            + detail
            + ridge
            - 4.0
    }
}

struct CoarseField {
    origin: Vec2,
    cell: f32,
    dim_x: usize,
    dim_z: usize,
    accumulation: Vec<f32>,
}

impl CoarseField {
    fn sample(&self, x: f32, z: f32) -> f32 {
        let gx = ((x - self.origin.x) / self.cell - 0.5)
            .clamp(0.0, self.dim_x as f32 - 1.001);
        let gz = ((z - self.origin.y) / self.cell - 0.5)
            .clamp(0.0, self.dim_z as f32 - 1.001);
        let x0 = gx.floor() as usize;
        let z0 = gz.floor() as usize;
        let fx = gx - x0 as f32;
        let fz = gz - z0 as f32;
        let at = |ix: usize, iz: usize| self.accumulation[iz * self.dim_x + ix];
        let top = at(x0, z0) + (at(x0 + 1, z0) - at(x0, z0)) * fx;
        let bottom = at(x0, z0 + 1) + (at(x0 + 1, z0 + 1) - at(x0, z0 + 1)) * fx;
        top + (bottom - top) * fz
    }
}

fn coarse_rivers(sampler: &InfiniteSampler, low: Vec2, high: Vec2) -> CoarseField {
    let margin = RIVER_HALO as f32 * RIVER_CELL;
    let x0 = ((low.x - margin) / RIVER_CELL).floor();
    let z0 = ((low.y - margin) / RIVER_CELL).floor();
    let x1 = ((high.x + margin) / RIVER_CELL).ceil();
    let z1 = ((high.y + margin) / RIVER_CELL).ceil();
    let dim_x = (x1 - x0) as usize + 1;
    let dim_z = (z1 - z0) as usize + 1;
    let origin = Vec2::new(x0 * RIVER_CELL, z0 * RIVER_CELL);
    let heights: Vec<f32> = (0..dim_z)
        .flat_map(|iz| {
            (0..dim_x).map(move |ix| {
                sampler.base_height(
                    origin.x + ix as f32 * RIVER_CELL + RIVER_CELL * 0.5,
                    origin.y + iz as f32 * RIVER_CELL + RIVER_CELL * 0.5,
                )
            })
        })
        .collect();
    let mut accumulation = vec![1.0f32; heights.len()];
    let mut order: Vec<usize> = (0..heights.len()).collect();
    order.sort_by(|a, b| heights[*b].total_cmp(&heights[*a]));
    for &index in &order {
        let ix = index % dim_x;
        let iz = index / dim_x;
        let mut lowest = index;
        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nx = ix as i64 + dx;
                let nz = iz as i64 + dz;
                if nx < 0 || nz < 0 || nx >= dim_x as i64 || nz >= dim_z as i64 {
                    continue;
                }
                let neighbour = nz as usize * dim_x + nx as usize;
                if heights[neighbour] < heights[lowest] {
                    lowest = neighbour;
                }
            }
        }
        if lowest != index {
            accumulation[lowest] += accumulation[index];
        }
    }
    CoarseField {
        origin,
        cell: RIVER_CELL,
        dim_x,
        dim_z,
        accumulation,
    }
}

fn river_cut(rivers: &CoarseField, x: f32, z: f32) -> f32 {
    let accum = rivers.sample(x, z);
    if accum <= RIVER_THRESHOLD {
        return 0.0;
    }
    let t = ((accum - RIVER_THRESHOLD) / (RIVER_THRESHOLD * 6.0)).clamp(0.0, 1.0);
    smooth_unit(t) * RIVER_DEPTH
}

#[derive(Clone, Debug)]
pub struct StreamField {
    pub key: IVec2,
    pub lod: usize,
    pub steps: usize,
    pub origin: Vec2,
    pub step: f32,
    pub heights: Vec<f32>,
}

impl StreamField {
    pub fn height(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz * (self.steps + 1) + ix]
    }

    pub fn point(&self, ix: usize, iz: usize) -> Vec2 {
        self.origin + Vec2::splat(self.step) * Vec2::new(ix as f32, iz as f32)
    }
}

pub fn stream_steps(lod: usize) -> usize {
    (STREAM_BASE_STEPS >> lod.min(STREAM_MAX_LOD)).max(4)
}

pub fn build_stream_field(
    sampler: &InfiniteSampler,
    key: IVec2,
    lod: usize,
    chunk_size: f32,
) -> StreamField {
    let steps = stream_steps(lod);
    let origin = chunk_origin(key, chunk_size);
    let step = chunk_size / steps as f32;
    let rivers = coarse_rivers(sampler, origin, origin + Vec2::splat(chunk_size));
    let mut heights = Vec::with_capacity((steps + 1) * (steps + 1));
    for iz in 0..=steps {
        for ix in 0..=steps {
            let point = origin + Vec2::splat(step) * Vec2::new(ix as f32, iz as f32);
            heights.push(
                sampler.base_height(point.x, point.y) - river_cut(&rivers, point.x, point.y),
            );
        }
    }
    StreamField {
        key,
        lod,
        steps,
        origin,
        step,
        heights,
    }
}

pub fn build_stream_mesh(map: &TerrainMap, field: &StreamField, view: DebugView) -> Mesh {
    let steps = field.steps;
    let stride = steps + 1;
    let extent = map.max_height_extent();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    for iz in 0..stride {
        for ix in 0..stride {
            let point = field.point(ix, iz);
            let height = field.height(ix, iz);
            positions.push([point.x, height, point.y]);
            colors.push(debug_colour(map, view, point, height, extent).to_f32_array());
            uvs.push([point.x / TEXTURE_METRES, point.y / TEXTURE_METRES]);
            let left = field.height(ix.saturating_sub(1), iz);
            let right = field.height((ix + 1).min(steps), iz);
            let down = field.height(ix, iz.saturating_sub(1));
            let up = field.height(ix, (iz + 1).min(steps));
            let normal = Vec3::new(
                -(right - left),
                2.0 * field.step,
                -(up - down),
            )
            .try_normalize()
            .unwrap_or(Vec3::Y);
            normals.push([normal.x, normal.y, normal.z]);
        }
    }
    let mut indices: Vec<u32> = Vec::with_capacity(steps * steps * 6);
    for iz in 0..steps {
        for ix in 0..steps {
            let a = (iz * stride + ix) as u32;
            let b = a + 1;
            let c = a + stride as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let edges: [Vec<(usize, usize)>; 4] = [
        (0..stride).map(|ix| (ix, 0)).collect(),
        (0..stride).map(|ix| (ix, steps)).collect(),
        (0..stride).map(|iz| (0, iz)).collect(),
        (0..stride).map(|iz| (steps, iz)).collect(),
    ];
    let outward = [Vec3::NEG_Z, Vec3::Z, Vec3::NEG_X, Vec3::X];
    for (edge, normal) in edges.iter().zip(outward) {
        let start = positions.len() as u32;
        for &(ix, iz) in edge {
            let point = field.point(ix, iz);
            let height = field.height(ix, iz) - STREAM_SKIRT_DROP;
            positions.push([point.x, height, point.y]);
            colors.push(debug_colour(map, view, point, height, extent).to_f32_array());
            uvs.push([point.x / TEXTURE_METRES, point.y / TEXTURE_METRES]);
            normals.push(normal.to_array());
        }
        for i in 0..edge.len() - 1 {
            let b0 = (edge[i].1 * stride + edge[i].0) as u32;
            let b1 = (edge[i + 1].1 * stride + edge[i + 1].0) as u32;
            let s0 = start + i as u32;
            let s1 = start + i as u32 + 1;
            indices.extend_from_slice(&[b0, s0, b1, b1, s0, s1]);
            indices.extend_from_slice(&[b0, b1, s0, b1, s1, s0]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub fn stream_collider(field: &StreamField, chunk_size: f32) -> Collider {
    let stride = field.steps + 1;
    let grid: Vec<Vec<f32>> = (0..stride)
        .map(|column| {
            (0..stride)
                .map(|row| field.height(column, row))
                .collect()
        })
        .collect();
    Collider::heightfield(grid, Vec3::new(chunk_size, 1.0, chunk_size))
}

#[derive(Component)]
pub struct StreamLod(pub usize);

#[derive(Resource, Default)]
pub struct ChunkFocus(pub Vec3);

struct StreamBuild {
    key: IVec2,
    lod: usize,
    mesh: Mesh,
    collider: Option<Collider>,
}

struct StreamJob {
    key: IVec2,
    lod: usize,
    task: Task<StreamBuild>,
}

#[derive(Resource, Default)]
pub struct StreamQueue {
    jobs: Vec<StreamJob>,
}

pub fn stream_lod(ring: i32) -> usize {
    (ring - 1).max(0).min(STREAM_MAX_LOD as i32) as usize
}

pub fn wanted_stream_chunks(focus: Vec3, chunk_size: f32) -> Vec<(IVec2, usize)> {
    let centre = chunk_key(focus.x, focus.z, chunk_size);
    let mut wanted = Vec::new();
    for dz in -STREAM_RING_RADIUS..=STREAM_RING_RADIUS {
        for dx in -STREAM_RING_RADIUS..=STREAM_RING_RADIUS {
            let ring = dx.abs().max(dz.abs());
            wanted.push((centre + IVec2::new(dx, dz), stream_lod(ring)));
        }
    }
    wanted
}

#[derive(Default)]
pub(crate) struct StreamState {
    generation: Option<u64>,
    view: Option<DebugView>,
    material: Option<DebugMaterialMode>,
    season: Option<Season>,
}

pub fn stream_infinite_chunks(
    map: Res<TerrainMap>,
    focus: Res<ChunkFocus>,
    view: Res<TerrainDebugView>,
    mode: Res<DebugMaterialMode>,
    clock: Res<SeasonClock>,
    mut state: Local<StreamState>,
    mut queue: ResMut<StreamQueue>,
    mut commands: Commands,
    existing: Query<(Entity, &ChunkKey, &StreamLod)>,
) {
    if map.topology() != WorldTopology::Infinite {
        if !queue.jobs.is_empty() {
            queue.jobs.clear();
        }
        for (entity, _, _) in &existing {
            commands.entity(entity).despawn();
        }
        *state = StreamState::default();
        return;
    }
    let key = (
        map.generation(),
        view.0,
        *mode,
        clock.season,
    );
    let restyle = state.generation != Some(key.0)
        || state.view != Some(key.1)
        || state.material != Some(key.2)
        || state.season != Some(key.3);
    if restyle {
        state.generation = Some(key.0);
        state.view = Some(key.1);
        state.material = Some(key.2);
        state.season = Some(key.3);
        queue.jobs.clear();
        for (entity, _, _) in &existing {
            commands.entity(entity).despawn();
        }
    }
    let chunk_size = DEFAULT_CHUNK_SIZE;
    let wanted = wanted_stream_chunks(focus.0, chunk_size);
    for (entity, chunk, lod) in &existing {
        if !wanted.iter().any(|(k, l)| *k == chunk.0 && *l == lod.0) {
            commands.entity(entity).despawn();
        }
    }
    queue.jobs.retain(|job| wanted.iter().any(|(k, l)| *k == job.key && *l == job.lod));
    if queue.jobs.len() >= STREAM_MAX_JOBS {
        return;
    }
    let shared = Arc::new(map.clone());
    let debug_view = view.0;
    for (key, lod) in wanted {
        if queue.jobs.iter().any(|job| job.key == key && job.lod == lod) {
            continue;
        }
        if existing.iter().any(|(_, chunk, l)| chunk.0 == key && l.0 == lod) {
            continue;
        }
        let map = shared.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let sampler = InfiniteSampler::new(map.seed());
            let field = build_stream_field(&sampler, key, lod, DEFAULT_CHUNK_SIZE);
            let mesh = build_stream_mesh(&map, &field, debug_view);
            let collider =
                (lod == 0).then(|| stream_collider(&field, DEFAULT_CHUNK_SIZE));
            StreamBuild {
                key,
                lod,
                mesh,
                collider,
            }
        });
        queue.jobs.push(StreamJob { key, lod, task });
        if queue.jobs.len() >= STREAM_MAX_JOBS {
            break;
        }
    }
}

pub fn drain_stream_builds(
    map: Res<TerrainMap>,
    view: Res<TerrainDebugView>,
    mode: Res<DebugMaterialMode>,
    arrays: Res<TileArrays>,
    clock: Res<SeasonClock>,
    mut queue: ResMut<StreamQueue>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainRenderMaterial>>,
    mut commands: Commands,
) {
    let mut applied = 0;
    let mut index = 0;
    while index < queue.jobs.len() && applied < STREAM_UPLOAD_BUDGET {
        let completed = poll_task(&mut queue.jobs[index].task);
        match completed {
            Some(build) => {
                applied += 1;
                queue.jobs.remove(index);
                let centre =
                    chunk_origin(build.key, DEFAULT_CHUNK_SIZE) + Vec2::splat(DEFAULT_CHUNK_SIZE * 0.5);
                let material = chunk_material(
                    &map,
                    build.key,
                    DEFAULT_CHUNK_SIZE,
                    *mode,
                    view.0,
                    &clock,
                    &arrays,
                    None,
                );
                let entity = commands
                    .spawn((
                        Name::new(format!(
                            "Stream Chunk {} {} L{}",
                            build.key.x, build.key.y, build.lod
                        )),
                        TerrainChunk,
                        ChunkKey(build.key),
                        StreamLod(build.lod),
                        RigidBody::Static,
                        Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
                        Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
                        Transform::default(),
                        Mesh3d(meshes.add(build.mesh)),
                        MeshMaterial3d(materials.add(material)),
                    ))
                    .id();
                if let Some(collider) = build.collider {
                    commands.spawn((
                        Name::new(format!(
                            "Stream Collider {} {}",
                            build.key.x, build.key.y
                        )),
                        ChildOf(entity),
                        Transform::from_xyz(centre.x, 0.0, centre.y),
                        collider,
                    ));
                }
            }
            None => index += 1,
        }
    }
}

pub fn stream_spawn_height(map: &TerrainMap) -> f32 {
    InfiniteSampler::new(map.seed()).base_height(0.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::map::TerrainConfig;
    use crate::terrain::topology::WorldTopology;
    use bevy::mesh::VertexAttributeValues;

    fn sampler() -> InfiniteSampler {
        InfiniteSampler::new(WorldSeed::new(20_261_120))
    }

    #[test]
    fn the_same_key_and_lod_always_rebuild_the_same_field() {
        let sampler = sampler();
        for lod in 0..=STREAM_MAX_LOD {
            let first = build_stream_field(&sampler, IVec2::new(3, -2), lod, DEFAULT_CHUNK_SIZE);
            let second = build_stream_field(&sampler, IVec2::new(3, -2), lod, DEFAULT_CHUNK_SIZE);
            assert_eq!(first.heights, second.heights, "lod {lod} drifted");
            assert_eq!(first.steps, second.steps);
        }
    }

    #[test]
    fn the_mesh_of_a_key_and_lod_is_bit_identical_across_rebuilds() {
        let map = TerrainMap::new(
            TerrainConfig::compact().with_topology(WorldTopology::Infinite),
        );
        let sampler = sampler();
        for lod in 0..=STREAM_MAX_LOD {
            let a = build_stream_mesh(&map, &build_stream_field(&sampler, IVec2::new(-1, 4), lod, DEFAULT_CHUNK_SIZE), DebugView::None);
            let b = build_stream_mesh(&map, &build_stream_field(&sampler, IVec2::new(-1, 4), lod, DEFAULT_CHUNK_SIZE), DebugView::None);
            assert_eq!(
                a.attribute(Mesh::ATTRIBUTE_POSITION),
                b.attribute(Mesh::ATTRIBUTE_POSITION),
                "lod {lod} mesh drifted"
            );
        }
    }

    #[test]
    fn different_keys_produce_different_geometry() {
        let sampler = sampler();
        let a = build_stream_field(&sampler, IVec2::ZERO, 0, DEFAULT_CHUNK_SIZE);
        let b = build_stream_field(&sampler, IVec2::new(1, 0), 0, DEFAULT_CHUNK_SIZE);
        assert_ne!(a.heights, b.heights);
    }

    #[test]
    fn different_seeds_produce_different_geometry() {
        let a = build_stream_field(
            &InfiniteSampler::new(WorldSeed::new(11)),
            IVec2::ZERO,
            0,
            DEFAULT_CHUNK_SIZE,
        );
        let b = build_stream_field(
            &InfiniteSampler::new(WorldSeed::new(12)),
            IVec2::ZERO,
            0,
            DEFAULT_CHUNK_SIZE,
        );
        assert_ne!(a.heights, b.heights);
    }

    #[test]
    fn neighbouring_chunks_share_their_border_heights() {
        let sampler = sampler();
        let left = build_stream_field(&sampler, IVec2::new(0, 0), 0, DEFAULT_CHUNK_SIZE);
        let right = build_stream_field(&sampler, IVec2::new(1, 0), 0, DEFAULT_CHUNK_SIZE);
        for row in 0..=left.steps {
            let a = left.height(left.steps, row);
            let b = right.height(0, row);
            assert!(
                (a - b).abs() < 1e-4,
                "row {row}: chunks disagree across the border by {}",
                a - b
            );
        }
    }

    #[test]
    fn lod_decimates_the_grid_by_the_expected_factor() {
        assert_eq!(stream_steps(0), STREAM_BASE_STEPS);
        assert_eq!(stream_steps(1), STREAM_BASE_STEPS / 2);
        assert_eq!(stream_steps(2), STREAM_BASE_STEPS / 4);
        assert_eq!(stream_steps(9), stream_steps(STREAM_MAX_LOD));
    }

    #[test]
    fn the_ring_lod_map_never_exceeds_the_max() {
        assert_eq!(stream_lod(0), 0);
        assert_eq!(stream_lod(1), 0);
        assert_eq!(stream_lod(2), 1);
        assert_eq!(stream_lod(3), 2);
        assert_eq!(stream_lod(99), STREAM_MAX_LOD);
    }

    #[test]
    fn the_ring_covers_a_square_of_chunks_around_the_focus() {
        let wanted = wanted_stream_chunks(Vec3::new(250.0, 0.0, -50.0), DEFAULT_CHUNK_SIZE);
        let side = 2 * STREAM_RING_RADIUS + 1;
        assert_eq!(wanted.len(), (side * side) as usize);
        let centre = chunk_key(250.0, -50.0, DEFAULT_CHUNK_SIZE);
        assert!(wanted.iter().any(|(k, l)| *k == centre && *l == 0));
        for (key, lod) in &wanted {
            let ring = (key.x - centre.x).abs().max((key.y - centre.y).abs());
            assert!(*lod == stream_lod(ring));
        }
    }

    #[test]
    fn the_mesh_carries_skirt_vertices_below_its_border() {
        let map = TerrainMap::new(
            TerrainConfig::compact().with_topology(WorldTopology::Infinite),
        );
        let sampler = sampler();
        let field = build_stream_field(&sampler, IVec2::ZERO, 0, DEFAULT_CHUNK_SIZE);
        let mesh = build_stream_mesh(&map, &field, DebugView::None);
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("stream mesh must carry positions");
        };
        let stride = field.steps + 1;
        let grid = stride * stride;
        let expected = grid + 4 * stride;
        assert_eq!(positions.len(), expected, "skirt vertices must be appended");
        let border_height = field.height(0, 0);
        let skirt = positions[grid];
        assert!(
            (skirt[1] - (border_height - STREAM_SKIRT_DROP)).abs() < 1e-4,
            "first skirt vertex must hang exactly one skirt drop below its border"
        );
    }

    #[test]
    fn the_stream_collider_is_a_real_heightfield() {
        let sampler = sampler();
        let field = build_stream_field(&sampler, IVec2::ZERO, 0, DEFAULT_CHUNK_SIZE);
        let collider = stream_collider(&field, DEFAULT_CHUNK_SIZE);
        assert!(collider.shape().as_heightfield().is_some());
    }

    #[test]
    fn the_sampler_is_a_pure_function_of_world_position() {
        let sampler = sampler();
        for (x, z) in [(13.0, -77.0), (0.0, 0.0), (1234.5, 987.25)] {
            assert_eq!(sampler.base_height(x, z), sampler.base_height(x, z));
            assert!(sampler.base_height(x, z).is_finite());
        }
    }

    #[test]
    fn rivers_cut_the_channel_below_the_surrounding_base() {
        let sampler = sampler();
        let field = build_stream_field(&sampler, IVec2::ZERO, 0, DEFAULT_CHUNK_SIZE);
        let mut deepest = 0.0f32;
        for iz in 0..=field.steps {
            for ix in 0..=field.steps {
                let point = field.point(ix, iz);
                let raw = sampler.base_height(point.x, point.y);
                deepest = deepest.max(raw - field.height(ix, iz));
            }
        }
        assert!(
            deepest > 0.5,
            "the hydrology pass must carve at least one channel, deepest cut {deepest}"
        );
    }

    #[test]
    fn the_streaming_system_is_inert_outside_the_infinite_topology() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TerrainMap::compact())
            .init_resource::<ChunkFocus>()
            .init_resource::<StreamQueue>()
            .init_resource::<TerrainDebugView>()
            .insert_resource(DebugMaterialMode::FlatColor)
            .init_resource::<SeasonClock>()
            .add_systems(Update, stream_infinite_chunks);
        app.update();
        app.update();
        assert!(app.world().resource::<StreamQueue>().jobs.is_empty());
    }
}
