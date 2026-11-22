use std::collections::HashMap;
use std::sync::Arc;

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};

use crate::terrain::colour::{debug_colour, region_colour};
use crate::terrain::gpu::{self, TerrainRenderMaterial, TileArrays};
use crate::terrain::job::poll_task;
use crate::terrain::map::TerrainMap;
use crate::terrain::material::{self, DebugMaterialMode};
use crate::terrain::region::Region;
use crate::terrain::season::{Season, SeasonClock};
use crate::terrain::splat::{self, ControlHandles};
#[cfg(test)]
use crate::terrain::splat::{SplatHandles, SurfaceTiles};
use crate::terrain::stream::StreamLod;
use crate::terrain::topology::{wrapped_copies, WorldTopology};
use crate::terrain::{DebugView, TerrainDebugView};
use crate::utils::SpawnGroundHeight;

pub const DEFAULT_CHUNK_SIZE: f32 = 200.0;
#[allow(dead_code)]
pub const CHUNK_MESH_STEPS: usize = 48;
pub const CHUNK_UPLOAD_BUDGET: usize = 4;
pub const MAX_CHUNK_MESH_STEPS: usize = 512;

#[derive(Component)]
pub struct TerrainChunk;

#[derive(Component)]
pub struct ChunkKey(pub IVec2);

pub fn chunk_physics() -> (Friction, Restitution) {
    (Friction::new(1.0), Restitution::ZERO)
}

pub fn clamped_chunk_bounds(map: &TerrainMap, chunk: IVec2, chunk_size: f32) -> (Vec2, Vec2) {
    let (low, high) = map.chunk_bounds(chunk, chunk_size);
    let half = map.size() * 0.5;
    let clamp = |v: f32| v.clamp(-half, half);
    (
        Vec2::new(clamp(low.x), clamp(low.y)),
        Vec2::new(clamp(high.x), clamp(high.y)),
    )
}

pub fn chunk_mesh_steps(map: &TerrainMap, chunk: IVec2, chunk_size: f32) -> usize {
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let spacing = map.heights().spacing().max(f32::EPSILON);
    (((high.x - low.x) / spacing).round() as usize).clamp(1, MAX_CHUNK_MESH_STEPS)
}

fn sample_chunk(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    steps: usize,
) -> Vec<(Vec2, f32)> {
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let steps = steps.max(1);
    let step_x = (high.x - low.x) / steps as f32;
    let step_z = (high.y - low.y) / steps as f32;
    let mut samples = Vec::with_capacity((steps + 1) * (steps + 1));
    for iz in 0..=steps {
        for ix in 0..=steps {
            let point = Vec2::new(low.x + ix as f32 * step_x, low.y + iz as f32 * step_z);
            samples.push((point, map.height_at(point.x, point.y)));
        }
    }
    samples
}

fn flip_diagonal(ha: f32, hb: f32, hc: f32, hd: f32) -> bool {
    (ha - hd).abs() < (hb - hc).abs()
}

pub fn build_chunk_mesh(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    view: DebugView,
) -> Mesh {
    build_chunk_mesh_with_mode(map, chunk, chunk_size, view, DebugMaterialMode::Textured)
}

pub fn build_chunk_mesh_with_mode(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    view: DebugView,
    mode: DebugMaterialMode,
) -> Mesh {
    let steps = chunk_mesh_steps(map, chunk, chunk_size);
    let extent = map.max_height_extent();
    let samples = sample_chunk(map, chunk, chunk_size, steps);
    let stride = steps + 1;
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let step_x = (high.x - low.x) / steps as f32;
    let step_z = (high.y - low.y) / steps as f32;
    let flat_regions = mode == DebugMaterialMode::FlatColor && view == DebugView::None;

    let mut positions = Vec::with_capacity(samples.len());
    let mut colors = Vec::with_capacity(samples.len());
    let mut uvs = Vec::with_capacity(samples.len());
    for iz in 0..stride {
        for ix in 0..stride {
            let (point, height) = samples[iz * stride + ix];
            positions.push([point.x, height, point.y]);
            let colour = if flat_regions {
                map.region_at(point.x, point.y)
                    .map(region_colour)
                    .unwrap_or(LinearRgba::BLACK)
            } else {
                debug_colour(map, view, point, height, extent)
            };
            colors.push(colour.to_f32_array());
            uvs.push([ix as f32 / steps as f32, 1.0 - iz as f32 / steps as f32]);
        }
    }

    let mut indices: Vec<u32> = Vec::with_capacity(steps * steps * 6);
    for iz in 0..steps {
        for ix in 0..steps {
            let a = iz * stride + ix;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            let (a, b, c, d) = (a as u32, b as u32, c as u32, d as u32);
            if flip_diagonal(
                samples[a as usize].1,
                samples[b as usize].1,
                samples[c as usize].1,
                samples[d as usize].1,
            ) {
                indices.extend_from_slice(&[a, d, b, a, c, d]);
            } else {
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }

    let mut normals = Vec::with_capacity(samples.len());
    let mut tangents = Vec::with_capacity(samples.len());
    for iz in 0..stride {
        for ix in 0..stride {
            let index = iz * stride + ix;
            let (point, _) = samples[index];
            let dx = if ix > 0 && ix + 1 < stride {
                samples[index + 1].1 - samples[index - 1].1
            } else {
                map.height_at(point.x + step_x, point.y) - map.height_at(point.x - step_x, point.y)
            };
            let dz = if iz > 0 && iz + 1 < stride {
                samples[index + stride].1 - samples[index - stride].1
            } else {
                map.height_at(point.x, point.y + step_z) - map.height_at(point.x, point.y - step_z)
            };
            let slope_x = dx / (2.0 * step_x).max(f32::EPSILON);
            let slope_z = dz / (2.0 * step_z).max(f32::EPSILON);
            let normal = Vec3::new(-slope_x, 1.0, -slope_z)
                .try_normalize()
                .unwrap_or(Vec3::Y);
            normals.push([normal.x, normal.y, normal.z]);
            let tangent = Vec3::new(1.0, slope_x, 0.0)
                .try_normalize()
                .unwrap_or(Vec3::X);
            tangents.push([tangent.x, tangent.y, tangent.z, 1.0]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub fn chunk_material(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    mode: DebugMaterialMode,
    view: DebugView,
    clock: &SeasonClock,
    arrays: &TileArrays,
    control: Option<&ControlHandles>,
) -> TerrainRenderMaterial {
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let centre = (low + high) * 0.5;
    let profile = material::material_profile(
        map.region_at(centre.x, centre.y)
            .unwrap_or(Region::TemperateGrassland),
    );
    let mut params = gpu::terrain_uniform(map, chunk, chunk_size, profile, clock);
    params.debug_mode = if view != DebugView::None {
        gpu::DEBUG_MODE_VERTEX_COLOUR
    } else {
        match mode {
            DebugMaterialMode::UvGrid => gpu::DEBUG_MODE_UV_GRID,
            DebugMaterialMode::FlatColor => gpu::DEBUG_MODE_VERTEX_COLOUR,
            DebugMaterialMode::Textured
                if arrays.ready && control.is_some() =>
            {
                gpu::DEBUG_MODE_TEXTURED
            }
            DebugMaterialMode::Textured => gpu::DEBUG_MODE_FLAT,
        }
    };
    gpu::terrain_render_material(profile, arrays, control, params)
}

pub fn bake_controls(
    map: &TerrainMap,
    mode: DebugMaterialMode,
    view: DebugView,
    wanted: &[IVec2],
    images: &mut Assets<Image>,
) -> HashMap<IVec2, ControlHandles> {
    let mut baked = HashMap::new();
    if mode != DebugMaterialMode::Textured || view != DebugView::None {
        return baked;
    }
    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    for key in wanted {
        let maps = splat::bake_control_chunk(map, *key, chunk_size, splat::SPLAT_RESOLUTION);
        baked.insert(*key, splat::upload_control(&maps, images));
    }
    baked
}

#[cfg(test)]
pub fn bake_splats(
    map: &TerrainMap,
    tiles: &SurfaceTiles,
    mode: DebugMaterialMode,
    view: DebugView,
    wanted: &[IVec2],
    images: &mut Assets<Image>,
) -> HashMap<IVec2, SplatHandles> {
    let mut baked = HashMap::new();
    if mode != DebugMaterialMode::Textured || view != DebugView::None || !tiles.is_complete() {
        return baked;
    }
    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    for key in wanted {
        let maps = splat::bake_chunk(
            map,
            tiles,
            *key,
            chunk_size,
            splat::SPLAT_RESOLUTION,
        );
        if let Some(maps) = maps {
            baked.insert(*key, splat::upload(&maps, images));
        }
    }
    baked
}

pub fn heightfield_grid(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    steps: usize,
) -> Vec<Vec<f32>> {
    let samples = sample_chunk(map, chunk, chunk_size, steps);
    let stride = steps + 1;
    (0..stride)
        .map(|row| {
            (0..stride)
                .map(|column| samples[row * stride + column].1)
                .collect()
        })
        .collect()
}

pub fn chunk_heightfield(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    steps: usize,
) -> Collider {
    let stride = steps + 1;
    let grid = heightfield_grid(map, chunk, chunk_size, steps);
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let columns: Vec<Vec<f32>> = (0..stride)
        .map(|column| (0..stride).map(|row| grid[row][column]).collect())
        .collect();
    Collider::heightfield(
        columns,
        Vec3::new(high.x - low.x, 1.0, high.y - low.y),
    )
}

pub struct ChunkBuild {
    pub key: IVec2,
    pub mesh: Mesh,
    pub collider: Collider,
    pub control: Option<ControlHandles>,
}

pub fn build_chunk(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    view: DebugView,
    mode: DebugMaterialMode,
) -> ChunkBuild {
    let steps = chunk_mesh_steps(map, chunk, chunk_size);
    ChunkBuild {
        key: chunk,
        mesh: build_chunk_mesh_with_mode(map, chunk, chunk_size, view, mode),
        collider: chunk_heightfield(map, chunk, chunk_size, steps),
        control: None,
    }
}

pub fn spawn_chunk(
    commands: &mut Commands,
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    mode: DebugMaterialMode,
    view: DebugView,
    clock: &SeasonClock,
    arrays: &TileArrays,
    build: ChunkBuild,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TerrainRenderMaterial>,
) {
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let centre = (low + high) * 0.5;
    let material =
        chunk_material(map, chunk, chunk_size, mode, view, clock, arrays, build.control.as_ref());
    let mesh = meshes.add(build.mesh);
    let material = materials.add(material);
    let (friction, restitution) = chunk_physics();
    for offset in wrapped_copies(map.topology(), map.size()) {
        let entity = commands
            .spawn((
                Name::new(format!("Terrain Chunk {} {}", chunk.x, chunk.y)),
                TerrainChunk,
                ChunkKey(chunk),
                RigidBody::Static,
                friction,
                restitution,
                Transform::from_xyz(offset.x, 0.0, offset.y),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
            ))
            .id();
        commands.spawn((
            ChildOf(entity),
            Name::new(format!("Terrain Collider {} {}", chunk.x, chunk.y)),
            Transform::from_xyz(centre.x, 0.0, centre.y),
            build.collider.clone(),
        ));
    }
}

pub struct ChunkJob {
    pub fresh: bool,
    pub task: Task<ChunkBuild>,
}

#[derive(Resource, Default)]
pub struct ChunkJobQueue {
    pub jobs: Vec<ChunkJob>,
}

#[derive(Default)]
pub(crate) struct RebuildState {
    generation: Option<u64>,
    view: Option<DebugView>,
    material: Option<DebugMaterialMode>,
    season: Option<Season>,
}

pub fn rebuild_terrain(
    map: Res<TerrainMap>,
    view: Res<TerrainDebugView>,
    mode: Res<DebugMaterialMode>,
    arrays: Res<TileArrays>,
    clock: Res<SeasonClock>,
    mut state: Local<RebuildState>,
    mut queue: ResMut<ChunkJobQueue>,
    mut spawn_height: ResMut<SpawnGroundHeight>,
    mut materials: ResMut<Assets<TerrainRenderMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    existing: Query<
        (Entity, &ChunkKey, &MeshMaterial3d<TerrainRenderMaterial>),
        (With<TerrainChunk>, Without<StreamLod>),
    >,
) {
    let generation_changed = state.generation != Some(map.generation());
    let view_changed = state.view != Some(view.0);
    let material_changed = state.material != Some(*mode);
    let season_changed = state.season != Some(clock.season);
    if !generation_changed && !view_changed && !material_changed && !season_changed {
        return;
    }
    state.generation = Some(map.generation());
    state.view = Some(view.0);
    state.material = Some(*mode);
    state.season = Some(clock.season);
    spawn_height.0 = if map.topology() == WorldTopology::Infinite {
        crate::terrain::stream::stream_spawn_height(&map)
    } else {
        map.height_at(0.0, 0.0)
    };

    if map.topology() == WorldTopology::Infinite {
        for (entity, _, _) in &existing {
            commands.entity(entity).despawn();
        }
        queue.jobs.clear();
        return;
    }

    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    let wanted = map.chunk_keys(chunk_size);
    let controls = bake_controls(&map, *mode, view.0, &wanted, &mut images);
    let debug_view = view.0;
    let material_mode = *mode;

    if generation_changed {
        for (entity, _, _) in &existing {
            commands.entity(entity).despawn();
        }
        queue.jobs.clear();
        let shared = Arc::new(map.clone());
        for key in wanted {
            let map = shared.clone();
            let control = controls.get(&key).cloned();
            queue.jobs.push(ChunkJob {
                fresh: true,
                task: AsyncComputeTaskPool::get().spawn(async move {
                    let mut build = build_chunk(&map, key, chunk_size, debug_view, material_mode);
                    build.control = control;
                    build
                }),
            });
        }
        return;
    }

    let present: Vec<IVec2> = existing.iter().map(|(_, key, _)| key.0).collect();
    for (entity, key, _) in &existing {
        if !wanted.contains(&key.0) {
            commands.entity(entity).despawn();
        }
    }

    if view_changed || material_changed {
        queue.jobs.clear();
        let shared = Arc::new(map.clone());
        for &key in &wanted {
            let map = shared.clone();
            let fresh = !present.contains(&key);
            queue.jobs.push(ChunkJob {
                fresh,
                task: AsyncComputeTaskPool::get().spawn(async move {
                    build_chunk(&map, key, chunk_size, debug_view, material_mode)
                }),
            });
        }
    }

    if view_changed || material_changed || season_changed {
        for (_, key, material_handle) in &existing {
            if !wanted.contains(&key.0) {
                continue;
            }
            let built = chunk_material(
                &map,
                key.0,
                chunk_size,
                *mode,
                view.0,
                &clock,
                &arrays,
                controls.get(&key.0),
            );
            if let Some(mut existing_material) = materials.get_mut(material_handle) {
                *existing_material = built;
            }
        }
    }
}

pub fn drain_chunk_builds(
    map: Res<TerrainMap>,
    view: Res<TerrainDebugView>,
    mode: Res<DebugMaterialMode>,
    arrays: Res<TileArrays>,
    clock: Res<SeasonClock>,
    mut queue: ResMut<ChunkJobQueue>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainRenderMaterial>>,
    mut commands: Commands,
    chunks: Query<(Entity, &ChunkKey, &Mesh3d), (With<TerrainChunk>, Without<StreamLod>)>,
) {
    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    let mut applied = 0;
    let mut index = 0;
    while index < queue.jobs.len() && applied < CHUNK_UPLOAD_BUDGET {
        let fresh = queue.jobs[index].fresh;
        let completed = poll_task(&mut queue.jobs[index].task);
        match completed {
            Some(build) => {
                applied += 1;
                queue.jobs.remove(index);
                if fresh {
                    spawn_chunk(
                        &mut commands,
                        &map,
                        build.key,
                        chunk_size,
                        *mode,
                        view.0,
                        &clock,
                        &arrays,
                        build,
                        &mut meshes,
                        &mut materials,
                    );
                } else if let Some((_, _, mesh_handle)) =
                    chunks.iter().find(|(_, key, _)| key.0 == build.key)
                {
                    if let Some(mut existing_mesh) = meshes.get_mut(mesh_handle) {
                        *existing_mesh = build.mesh;
                    }
                }
            }
            None => index += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::map::TerrainConfig;
    use crate::terrain::material::DISK_ROOT;
    use crate::terrain::topology::WorldTopology;
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn cpu_bake_path_still_produces_splat_materials() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let tiles = splat::SurfaceTiles::load(DISK_ROOT);
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default()).init_asset::<Image>();
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let wanted = [IVec2::ZERO];
        let handles = bake_splats(
            &map,
            &tiles,
            DebugMaterialMode::Textured,
            DebugView::None,
            &wanted,
            &mut images,
        );
        let handles = handles.get(&IVec2::ZERO).expect("chunk zero must bake");
        let material = splat::splat_material(handles);
        assert_eq!(
            material.base_color_texture,
            Some(handles.albedo.clone()),
            "the CPU bake must still wire its albedo"
        );
        assert_eq!(material.normal_map_texture, Some(handles.normal.clone()));
        assert!(images.contains(&handles.cell_data));
    }

    #[test]
    fn chunk_material_selects_debug_modes() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let arrays = TileArrays::default();
        let clock = SeasonClock {
            season: Season::Summer,
            ..Default::default()
        };
        let flat = chunk_material(
            &map,
            IVec2::ZERO,
            120.0,
            DebugMaterialMode::FlatColor,
            DebugView::None,
            &clock,
            &arrays,
            None,
        );
        assert_eq!(
            flat.extension.params.debug_mode,
            gpu::DEBUG_MODE_VERTEX_COLOUR,
            "flat colour must blend per-vertex region colours, not one chunk-centre colour"
        );
        let grid = chunk_material(
            &map,
            IVec2::ZERO,
            120.0,
            DebugMaterialMode::UvGrid,
            DebugView::None,
            &clock,
            &arrays,
            None,
        );
        assert_eq!(grid.extension.params.debug_mode, gpu::DEBUG_MODE_UV_GRID);
        let viewed = chunk_material(
            &map,
            IVec2::ZERO,
            120.0,
            DebugMaterialMode::Textured,
            DebugView::Slope,
            &clock,
            &arrays,
            None,
        );
        assert_eq!(viewed.extension.params.debug_mode, gpu::DEBUG_MODE_VERTEX_COLOUR);
        let textured_without = chunk_material(
            &map,
            IVec2::ZERO,
            120.0,
            DebugMaterialMode::Textured,
            DebugView::None,
            &clock,
            &arrays,
            None,
        );
        assert_eq!(
            textured_without.extension.params.debug_mode,
            gpu::DEBUG_MODE_FLAT,
            "missing control maps must fall back to flat colour"
        );
    }

    fn normals_of(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(values)) => values.clone(),
            other => panic!("expected float32x3 normals, got {other:?}"),
        }
    }

    fn positions_of(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(values)) => values.clone(),
            other => panic!("expected float32x3 positions, got {other:?}"),
        }
    }

    fn whole_world_chunk(map: &TerrainMap) -> Mesh {
        build_chunk_mesh(map, IVec2::ZERO, DEFAULT_CHUNK_SIZE.min(map.size()), DebugView::None)
    }

    fn whole_world_steps(map: &TerrainMap) -> usize {
        chunk_mesh_steps(map, IVec2::ZERO, DEFAULT_CHUNK_SIZE.min(map.size()))
    }

    #[test]
    fn chunk_mesh_steps_match_the_heightfield_grid() {
        let finite = TerrainMap::new(TerrainConfig::compact());
        let finite_spacing = finite.heights().spacing();
        let finite_steps = whole_world_steps(&finite);
        assert_eq!(finite_steps, 31, "32 grid lines over a whole-world chunk");
        let drift = (finite_steps as f32 * finite_spacing - finite.size()).abs();
        assert!(drift < finite_spacing, "mesh steps must track the grid: {drift}");

        let torus = TerrainMap::new(
            TerrainConfig::compact().with_topology(WorldTopology::FiniteTorus),
        );
        let torus_spacing = torus.heights().spacing();
        let torus_steps = whole_world_steps(&torus);
        assert_eq!(torus_steps, 32);
        let drift = (torus_steps as f32 * torus_spacing - torus.size()).abs();
        assert!(drift < torus_spacing, "mesh steps must track the grid: {drift}");
    }

    #[test]
    fn chunk_meshes_carry_uv_and_unit_normals() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let mesh = whole_world_chunk(&map);
        let stride = whole_world_steps(&map) + 1;
        assert_eq!(positions_of(&mesh).len(), stride * stride);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        for normal in normals_of(&mesh) {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!((length - 1.0).abs() < 1e-3, "normals must be unit length");
        }
    }

    #[test]
    fn torus_normals_line_up_across_the_wrap() {
        let map = TerrainMap::new(
            TerrainConfig::compact().with_topology(WorldTopology::FiniteTorus),
        );
        let mesh = whole_world_chunk(&map);
        let normals = normals_of(&mesh);
        let stride = whole_world_steps(&map) + 1;
        for row in 0..stride {
            let left = normals[row * stride];
            let right = normals[row * stride + stride - 1];
            let delta: f32 = (0..3).map(|channel| (left[channel] - right[channel]).abs()).sum();
            assert!(delta < 0.05, "wrap seam normal differs by {delta} at row {row}");
        }
    }

    #[test]
    fn finite_normals_do_not_wrap() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let mesh = whole_world_chunk(&map);
        let normals = normals_of(&mesh);
        let stride = whole_world_steps(&map) + 1;
        let mut worst = 0.0f32;
        for row in 0..stride {
            let left = normals[row * stride];
            let right = normals[row * stride + stride - 1];
            worst = worst.max((0..3).map(|channel| (left[channel] - right[channel]).abs()).sum());
        }
        assert!(worst > 0.05, "a finite world must not fake a seamless border");
    }

    #[test]
    fn chunk_mesh_vertices_sit_on_the_sampled_surface() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let mesh = whole_world_chunk(&map);
        for point in positions_of(&mesh) {
            let sampled = map.height_at(point[0], point[2]);
            assert!(
                (sampled - point[1]).abs() < 1e-4,
                "mesh vertex {point:?} floats {sampled} in the air"
            );
        }
    }

    #[test]
    fn heightfield_collider_grid_matches_the_drawn_mesh_vertices() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let steps = whole_world_steps(&map);
        let grid = heightfield_grid(&map, IVec2::ZERO, chunk_size, steps);
        let mesh = whole_world_chunk(&map);
        let positions = positions_of(&mesh);
        let stride = steps + 1;
        assert_eq!(grid.len(), stride);
        for line in &grid {
            assert_eq!(line.len(), stride);
        }
        for (row, line) in grid.iter().enumerate() {
            for (column, height) in line.iter().enumerate() {
                let vertex = positions[row * stride + column];
                assert!(
                    (vertex[1] - height).abs() < 1e-6,
                    "grid [{row}][{column}] disagrees with the mesh vertex height"
                );
            }
        }
    }

    #[test]
    fn heightfield_grid_rows_run_along_z_and_columns_along_x() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let (low, _) = clamped_chunk_bounds(&map, IVec2::ZERO, chunk_size);
        let steps = whole_world_steps(&map);
        let grid = heightfield_grid(&map, IVec2::ZERO, chunk_size, steps);
        let step = chunk_size / steps as f32;
        for (row, line) in grid.iter().enumerate() {
            for (column, height) in line.iter().enumerate() {
                let x = low.x + column as f32 * step;
                let z = low.y + row as f32 * step;
                assert!(
                    (map.height_at(x, z) - height).abs() < 1e-5,
                    "row {row} (z) column {column} (x) must sample the heightfield in order"
                );
            }
        }
    }

    #[test]
    fn chunk_heightfield_builds_a_real_heightfield_shape() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let collider = chunk_heightfield(&map, IVec2::ZERO, chunk_size, whole_world_steps(&map));
        assert!(
            collider.shape().as_heightfield().is_some(),
            "the chunk collider must be a heightfield, not a trimesh"
        );
    }

    #[test]
    fn neighbouring_chunks_share_an_edge_height() {
        let map = TerrainMap::new(TerrainConfig {
            size: 400.0,
            ..TerrainConfig::compact()
        });
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let left = build_chunk_mesh(&map, IVec2::new(0, 0), chunk_size, DebugView::None);
        let right = build_chunk_mesh(&map, IVec2::new(1, 0), chunk_size, DebugView::None);
        let stride = chunk_mesh_steps(&map, IVec2::new(0, 0), chunk_size) + 1;
        assert_eq!(
            stride,
            chunk_mesh_steps(&map, IVec2::new(1, 0), chunk_size) + 1,
            "two full chunks must tessellate identically"
        );
        let column = |mesh: &Mesh, index: usize| -> Vec<[f32; 3]> {
            (0..stride)
                .map(|row| positions_of(mesh)[row * stride + index])
                .collect()
        };
        let shared = column(&left, stride - 1);
        let incoming = column(&right, 0);
        for (row, (a, b)) in shared.iter().zip(&incoming).enumerate() {
            let gap = Vec3::from(*a).distance(Vec3::from(*b));
            assert!(gap < 1e-4, "row {row}: chunks disagree across the border by {gap}");
        }
    }

    fn colours_of(mesh: &Mesh) -> Vec<[f32; 4]> {
        match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(values)) => values.clone(),
            other => panic!("expected float32x4 colours, got {other:?}"),
        }
    }

    #[test]
    fn flat_colour_mode_bakes_per_vertex_region_colours() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let size = DEFAULT_CHUNK_SIZE.min(map.size());
        let flat = build_chunk_mesh_with_mode(
            &map,
            IVec2::ZERO,
            size,
            DebugView::None,
            DebugMaterialMode::FlatColor,
        );
        let plain = build_chunk_mesh(&map, IVec2::ZERO, size, DebugView::None);
        let flat_colours = colours_of(&flat);
        assert_ne!(
            flat_colours,
            colours_of(&plain),
            "flat colour must not reuse the height-shaded colours"
        );
        let positions = positions_of(&flat);
        for (index, position) in positions.iter().enumerate().step_by(227) {
            let expected = map
                .region_at(position[0], position[2])
                .map(region_colour)
                .expect("sampled vertices sit inside the world");
            assert_eq!(
                flat_colours[index],
                expected.to_f32_array(),
                "vertex {index} must carry its own region colour"
            );
        }
    }

    #[test]
    fn mesh_carries_unit_tangents() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let mesh = whole_world_chunk(&map);
        let tangents = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
            Some(VertexAttributeValues::Float32x4(values)) => values.clone(),
            other => panic!("expected float32x4 tangents, got {other:?}"),
        };
        assert_eq!(tangents.len(), positions_of(&mesh).len());
        for tangent in &tangents {
            let length =
                (tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2]).sqrt();
            assert!((length - 1.0).abs() < 1e-3, "tangents must be unit length");
            assert_eq!(tangent[3], 1.0);
        }
    }

    #[test]
    fn every_triangle_faces_up() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let mesh = whole_world_chunk(&map);
        let positions = positions_of(&mesh);
        let indices: Vec<u32> = mesh
            .indices()
            .expect("indexed mesh")
            .iter()
            .map(|index| index as u32)
            .collect();
        assert_eq!(indices.len() % 3, 0);
        for triangle in indices.chunks(3) {
            let a = Vec3::from(positions[triangle[0] as usize]);
            let b = Vec3::from(positions[triangle[1] as usize]);
            let c = Vec3::from(positions[triangle[2] as usize]);
            let normal = (b - a).cross(c - a);
            assert!(normal.y > -1e-3, "triangle {triangle:?} faces down: {normal:?}");
        }
    }

    #[test]
    fn quad_diagonal_flips_toward_the_flatter_pair() {
        assert!(flip_diagonal(0.0, 0.0, 10.0, 1.0), "|ha-hd| < |hb-hc| must flip");
        assert!(!flip_diagonal(0.0, 0.0, 1.0, 10.0), "the flatter default diagonal must stay");
        assert!(!flip_diagonal(1.0, 1.0, 1.0, 1.0), "ties keep the default diagonal");
    }

    #[test]
    fn chunk_meshes_never_overhang_the_world_edge() {
        let map = TerrainMap::new(TerrainConfig {
            size: 300.0,
            ..TerrainConfig::compact()
        });
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        assert!(
            map.chunk_keys(chunk_size).contains(&IVec2::new(1, 1)),
            "a 300m world with 200m chunks must have a partial edge chunk"
        );
        let half = map.size() * 0.5;
        let (low, high) = clamped_chunk_bounds(&map, IVec2::new(1, 1), chunk_size);
        assert!((high.x - half).abs() < 1e-3, "clamped bounds must stop at the edge");
        assert!((high.y - half).abs() < 1e-3);
        let mesh = build_chunk_mesh(&map, IVec2::new(1, 1), chunk_size, DebugView::None);
        for point in positions_of(&mesh) {
            assert!(
                point[0] <= half + 1e-3 && point[2] <= half + 1e-3,
                "vertex {point:?} hangs past the world edge"
            );
            assert!(point[0] >= -half - 1e-3 && point[2] >= -half - 1e-3);
            assert!(point[0] >= low.x - 1e-3 && point[2] >= low.y - 1e-3);
        }
    }

    #[test]
    fn chunk_physics_keeps_grip_on_the_ground() {
        let (friction, restitution) = chunk_physics();
        assert!(
            friction.static_coefficient > 0.0 && friction.dynamic_coefficient > 0.0,
            "terrain must not be frictionless"
        );
        assert_ne!(
            friction.combine_rule,
            CoefficientCombine::Min,
            "terrain friction must not zero out every prop resting on it"
        );
        assert_eq!(restitution.coefficient, 0.0);
    }
}
