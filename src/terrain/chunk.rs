use std::collections::HashMap;
use std::sync::Arc;

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};

use crate::terrain::colour::debug_colour;
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
pub const CHUNK_MESH_STEPS: usize = 48;
pub const CHUNK_UPLOAD_BUDGET: usize = 4;

#[derive(Component)]
pub struct TerrainChunk;

#[derive(Component)]
pub struct ChunkKey(pub IVec2);

pub fn build_chunk_mesh(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    view: DebugView,
) -> Mesh {
    let steps = CHUNK_MESH_STEPS;
    let extent = map.max_height_extent();
    let samples = map.height_in_chunk(chunk, chunk_size, steps);
    let stride = steps + 1;

    let mut positions = Vec::with_capacity(samples.len());
    let mut colors = Vec::with_capacity(samples.len());
    let mut uvs = Vec::with_capacity(samples.len());
    for iz in 0..stride {
        for ix in 0..stride {
            let (point, height) = samples[iz * stride + ix];
            positions.push([point.x, height, point.y]);
            colors.push(debug_colour(map, view, point, height, extent).to_f32_array());
            uvs.push([ix as f32 / steps as f32, 1.0 - iz as f32 / steps as f32]);
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

    let step = chunk_size / steps as f32;
    let mut normals = Vec::with_capacity(samples.len());
    for (point, _) in &samples {
        let dx = map.height_at(point.x + step, point.y) - map.height_at(point.x - step, point.y);
        let dz = map.height_at(point.x, point.y + step) - map.height_at(point.x, point.y - step);
        let normal = Vec3::new(-dx, 2.0 * step, -dz)
            .try_normalize()
            .unwrap_or(Vec3::Y);
        normals.push([normal.x, normal.y, normal.z]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
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
    let (low, high) = map.chunk_bounds(chunk, chunk_size);
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
            DebugMaterialMode::FlatColor => gpu::DEBUG_MODE_FLAT,
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
    season: Season,
    wanted: &[IVec2],
    images: &mut Assets<Image>,
) -> HashMap<IVec2, ControlHandles> {
    let mut baked = HashMap::new();
    if mode != DebugMaterialMode::Textured || view != DebugView::None {
        return baked;
    }
    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    for key in wanted {
        let maps = splat::bake_control_chunk(
            map,
            *key,
            chunk_size,
            season,
            splat::SPLAT_RESOLUTION,
        );
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
    season: Season,
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
            season,
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
    let samples = map.height_in_chunk(chunk, chunk_size, steps);
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
    let samples = map.height_in_chunk(chunk, chunk_size, steps);
    let stride = steps + 1;
    let columns: Vec<Vec<f32>> = (0..stride)
        .map(|column| {
            (0..stride)
                .map(|row| samples[row * stride + column].1)
                .collect()
        })
        .collect();
    Collider::heightfield(columns, Vec3::new(chunk_size, 1.0, chunk_size))
}

pub struct ChunkBuild {
    pub key: IVec2,
    pub mesh: Mesh,
    pub collider: Collider,
    pub control: Option<ControlHandles>,
}

pub fn build_chunk(map: &TerrainMap, chunk: IVec2, chunk_size: f32, view: DebugView) -> ChunkBuild {
    ChunkBuild {
        key: chunk,
        mesh: build_chunk_mesh(map, chunk, chunk_size, view),
        collider: chunk_heightfield(map, chunk, chunk_size, CHUNK_MESH_STEPS),
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
    let (low, high) = map.chunk_bounds(chunk, chunk_size);
    let centre = (low + high) * 0.5;
    let material =
        chunk_material(map, chunk, chunk_size, mode, view, clock, arrays, build.control.as_ref());
    let mesh = meshes.add(build.mesh);
    let material = materials.add(material);
    for offset in wrapped_copies(map.topology(), map.size()) {
        let entity = commands
            .spawn((
                Name::new(format!("Terrain Chunk {} {}", chunk.x, chunk.y)),
                TerrainChunk,
                ChunkKey(chunk),
                RigidBody::Static,
                Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
                Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
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
    let controls = bake_controls(&map, *mode, view.0, clock.season, &wanted, &mut images);
    let debug_view = view.0;

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
                    let mut build = build_chunk(&map, key, chunk_size, debug_view);
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

    if view_changed {
        queue.jobs.clear();
        let shared = Arc::new(map.clone());
        for &key in &wanted {
            let map = shared.clone();
            let fresh = !present.contains(&key);
            queue.jobs.push(ChunkJob {
                fresh,
                task: AsyncComputeTaskPool::get().spawn(async move {
                    build_chunk(&map, key, chunk_size, debug_view)
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
        let map = TerrainMap::compact();
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
            Season::Summer,
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
        let map = TerrainMap::compact();
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
        assert_eq!(flat.extension.params.debug_mode, gpu::DEBUG_MODE_FLAT);
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

    #[test]
    fn chunk_meshes_carry_uv_and_unit_normals() {
        let map = TerrainMap::compact();
        let mesh = whole_world_chunk(&map);
        let stride = CHUNK_MESH_STEPS + 1;
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
        let stride = CHUNK_MESH_STEPS + 1;
        for row in 0..stride {
            let left = normals[row * stride];
            let right = normals[row * stride + stride - 1];
            let delta: f32 = (0..3).map(|channel| (left[channel] - right[channel]).abs()).sum();
            assert!(delta < 0.05, "wrap seam normal differs by {delta} at row {row}");
        }
    }

    #[test]
    fn finite_normals_do_not_wrap() {
        let map = TerrainMap::compact();
        let mesh = whole_world_chunk(&map);
        let normals = normals_of(&mesh);
        let stride = CHUNK_MESH_STEPS + 1;
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
        let map = TerrainMap::compact();
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
        let map = TerrainMap::compact();
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let grid = heightfield_grid(&map, IVec2::ZERO, chunk_size, CHUNK_MESH_STEPS);
        let mesh = whole_world_chunk(&map);
        let positions = positions_of(&mesh);
        let stride = CHUNK_MESH_STEPS + 1;
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
        let map = TerrainMap::compact();
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let (low, _) = map.chunk_bounds(IVec2::ZERO, chunk_size);
        let grid = heightfield_grid(&map, IVec2::ZERO, chunk_size, CHUNK_MESH_STEPS);
        let step = chunk_size / CHUNK_MESH_STEPS as f32;
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
        let map = TerrainMap::compact();
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let collider = chunk_heightfield(&map, IVec2::ZERO, chunk_size, CHUNK_MESH_STEPS);
        assert!(
            collider.shape().as_heightfield().is_some(),
            "the chunk collider must be a heightfield, not a trimesh"
        );
    }

    #[test]
    fn neighbouring_chunks_share_an_edge_height() {
        let map = TerrainMap::compact();
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let left = build_chunk_mesh(&map, IVec2::new(0, 0), chunk_size, DebugView::None);
        let right = build_chunk_mesh(&map, IVec2::new(1, 0), chunk_size, DebugView::None);
        let stride = CHUNK_MESH_STEPS + 1;
        let column = |mesh: &Mesh, index: usize| -> Vec<[f32; 3]> {
            (0..stride)
                .map(|row| positions_of(mesh)[row * stride + index])
                .collect()
        };
        let shared = column(&left, CHUNK_MESH_STEPS);
        let incoming = column(&right, 0);
        for (row, (a, b)) in shared.iter().zip(&incoming).enumerate() {
            let gap = Vec3::from(*a).distance(Vec3::from(*b));
            assert!(gap < 1e-4, "row {row}: chunks disagree across the border by {gap}");
        }
    }
}
