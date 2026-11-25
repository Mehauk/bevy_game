use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use bevy_shader_kit::{
    DebugCurtainExt, DebugCurtainMaterial, MAX_SURFACE_DEBUG_CELLS, SurfaceDebug, WetSandMaterial,
    region_color,
};

use crate::terrain::{Terrain, TerrainChunk, TerrainSurfaceMaterial};
use crate::utils::registry::debug_views::{DebugViewLevel, DebugViewRegistry};

const CHUNKS_LAYER: &str = "terrain>chunks";
const LOD_LAYER: &str = "terrain>lod";
const CURTAIN_HEIGHT: f32 = 5.0;
const CURTAIN_STEPS: u32 = 16;
const GRID_CURTAIN_COLOR: [f32; 3] = [0.9, 0.95, 1.0];

#[derive(Clone, Debug, PartialEq)]
struct CurtainSegment {
    a: Vec2,
    b: Vec2,
    color: [f32; 3],
}

#[derive(Resource, Default)]
struct DebugCurtains {
    entity: Option<Entity>,
    mesh: Option<Handle<Mesh>>,
    material: Option<Handle<DebugCurtainMaterial>>,
    segments: Vec<CurtainSegment>,
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<DebugCurtains>()
        .add_systems(Update, (sync_surface_debug, sync_debug_curtains).chain());
}

fn sync_surface_debug(
    registry: Res<DebugViewRegistry>,
    terrain: Res<Terrain>,
    chunks: Query<&TerrainChunk>,
    surface: Option<Res<TerrainSurfaceMaterial>>,
    mut materials: ResMut<Assets<WetSandMaterial>>,
    mut applied: Local<Option<SurfaceDebug>>,
) {
    let desired = desired_surface_debug(&registry, &terrain, &chunks);
    if applied.as_ref() == Some(&desired) {
        return;
    }
    let Some(surface) = surface else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    material.extension.debug = desired;
    *applied = Some(desired);
}

fn desired_surface_debug(
    registry: &DebugViewRegistry,
    terrain: &Terrain,
    chunks: &Query<&TerrainChunk>,
) -> SurfaceDebug {
    let (level, use_values) = match (registry.level(LOD_LAYER), registry.level(CHUNKS_LAYER)) {
        (DebugViewLevel::Off, DebugViewLevel::Off) => return SurfaceDebug::default(),
        (DebugViewLevel::Off, level) => (level, false),
        (level, _) => (level, true),
    };
    let count = terrain.chunk_count();
    if count.x as usize * count.y as usize > MAX_SURFACE_DEBUG_CELLS {
        warn!("the surface debug overlay holds at most {MAX_SURFACE_DEBUG_CELLS} cells, not {count:?}");
        return SurfaceDebug::default();
    }
    let mut debug = SurfaceDebug {
        origin: terrain.chunk_origin(IVec2::ZERO),
        cell: terrain.chunk_extent(),
        count: count.as_vec2(),
        mode: level.index(),
        use_values: use_values as u32,
        ..default()
    };
    if use_values {
        for chunk in chunks.iter() {
            let index = chunk.coord.y.max(0) as usize * count.x as usize + chunk.coord.x.max(0) as usize;
            debug.set_cell(index, chunk.built_lod().map_or(0, |lod| lod as u32 + 1));
        }
    }
    debug
}

fn sync_debug_curtains(
    registry: Res<DebugViewRegistry>,
    terrain: Res<Terrain>,
    chunks: Query<&TerrainChunk>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<DebugCurtainMaterial>>,
    mut state: ResMut<DebugCurtains>,
) {
    let segments = desired_curtain_segments(&registry, &terrain, &chunks);
    if segments.is_empty() {
        if let Some(entity) = state.entity.take() {
            commands.entity(entity).despawn();
        }
        if let Some(mesh) = state.mesh.take() {
            meshes.remove(&mesh);
        }
        state.segments.clear();
        return;
    }
    if state.entity.is_some() && state.segments == segments {
        return;
    }

    let mesh = meshes.add(curtain_mesh(&terrain, &segments));
    if let Some(old) = state.mesh.replace(mesh.clone()) {
        meshes.remove(&old);
    }
    let material = match &state.material {
        Some(handle) => handle.clone(),
        None => {
            let handle = materials.add(DebugCurtainMaterial {
                base: StandardMaterial {
                    base_color: Color::srgba(1.0, 1.0, 1.0, 1.0),
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    ..default()
                },
                extension: DebugCurtainExt::default(),
            });
            state.material = Some(handle.clone());
            handle
        }
    };
    state.segments = segments;
    match state.entity {
        Some(entity) => {
            commands.entity(entity).insert((Mesh3d(mesh), MeshMaterial3d(material)));
        }
        None => {
            state.entity = Some(
                commands
                    .spawn((Name::new("Debug Curtains"), Mesh3d(mesh), MeshMaterial3d(material)))
                    .id(),
            );
        }
    }
}

fn desired_curtain_segments(
    registry: &DebugViewRegistry,
    terrain: &Terrain,
    chunks: &Query<&TerrainChunk>,
) -> Vec<CurtainSegment> {
    match (registry.level(LOD_LAYER), registry.level(CHUNKS_LAYER)) {
        (DebugViewLevel::Translucent, _) => lod_segments(terrain, &built_lods(chunks.iter())),
        (DebugViewLevel::Off, DebugViewLevel::Translucent) => chunk_grid_segments(terrain),
        _ => Vec::new(),
    }
}

fn built_lods<'a>(chunks: impl Iterator<Item = &'a TerrainChunk>) -> HashMap<IVec2, u8> {
    chunks
        .filter_map(|chunk| chunk.built_lod().map(|lod| (chunk.coord, lod)))
        .collect()
}

fn chunk_grid_segments(terrain: &Terrain) -> Vec<CurtainSegment> {
    let origin = terrain.chunk_origin(IVec2::ZERO);
    let cell = terrain.chunk_extent();
    let count = terrain.chunk_count();
    let far = origin + terrain.config().world;
    let mut segments = Vec::new();
    for ix in 0..=count.x {
        let x = origin.x + ix as f32 * cell.x;
        segments.push(CurtainSegment {
            a: Vec2::new(x, origin.y),
            b: Vec2::new(x, far.y),
            color: GRID_CURTAIN_COLOR,
        });
    }
    for iz in 0..=count.y {
        let z = origin.y + iz as f32 * cell.y;
        segments.push(CurtainSegment {
            a: Vec2::new(origin.x, z),
            b: Vec2::new(far.x, z),
            color: GRID_CURTAIN_COLOR,
        });
    }
    segments
}

fn lod_segments(terrain: &Terrain, built: &HashMap<IVec2, u8>) -> Vec<CurtainSegment> {
    let origin = terrain.chunk_origin(IVec2::ZERO);
    let cell = terrain.chunk_extent();
    let mut segments = Vec::new();
    for coord in terrain.chunk_coords() {
        let Some(&lod) = built.get(&coord) else {
            continue;
        };
        let corner = origin + coord.as_vec2() * cell;
        if let Some(&east) = built.get(&(coord + IVec2::X)) {
            if east != lod {
                let x = corner.x + cell.x;
                segments.push(CurtainSegment {
                    a: Vec2::new(x, corner.y),
                    b: Vec2::new(x, corner.y + cell.y),
                    color: blended_region_color(lod, east),
                });
            }
        }
        if let Some(&north) = built.get(&(coord + IVec2::Y)) {
            if north != lod {
                let z = corner.y + cell.y;
                segments.push(CurtainSegment {
                    a: Vec2::new(corner.x, z),
                    b: Vec2::new(corner.x + cell.x, z),
                    color: blended_region_color(lod, north),
                });
            }
        }
    }
    segments
}

fn blended_region_color(a: u8, b: u8) -> [f32; 3] {
    let first = region_color(a as u32 + 1);
    let second = region_color(b as u32 + 1);
    [
        (first[0] + second[0]) * 0.5,
        (first[1] + second[1]) * 0.5,
        (first[2] + second[2]) * 0.5,
    ]
}

fn curtain_mesh(terrain: &Terrain, segments: &[CurtainSegment]) -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for segment in segments {
        let base = positions.len() as u32;
        for step in 0..=CURTAIN_STEPS {
            let t = step as f32 / CURTAIN_STEPS as f32;
            let point = segment.a.lerp(segment.b, t);
            let ground = terrain.sample_ground_height(point.x, point.y);
            positions.push([point.x, ground, point.y]);
            uvs.push([t, 0.0]);
            colors.push([segment.color[0], segment.color[1], segment.color[2], 1.0]);
            positions.push([point.x, ground + CURTAIN_HEIGHT, point.y]);
            uvs.push([t, 1.0]);
            colors.push([segment.color[0], segment.color[1], segment.color[2], 1.0]);
        }
        for step in 0..CURTAIN_STEPS {
            let a = base + step * 2;
            indices.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attribute_positions(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) => positions.clone(),
            _ => panic!("curtain meshes carry float positions"),
        }
    }

    #[test]
    fn the_chunk_grid_curtain_draws_every_grid_line() {
        let terrain = Terrain::default();
        let segments = chunk_grid_segments(&terrain);
        assert_eq!(segments.len(), 6 + 6, "five chunks make six lines per axis");
        let origin = terrain.chunk_origin(IVec2::ZERO);
        let far = origin + terrain.config().world;
        for segment in &segments {
            assert!(segment.a.x >= origin.x - 0.01 && segment.a.x <= far.x + 0.01);
            assert!(segment.a.y >= origin.y - 0.01 && segment.a.y <= far.y + 0.01);
        }
    }

    #[test]
    fn lod_curtains_rise_only_where_neighbours_differ() {
        let terrain = Terrain::default();
        let mut built = HashMap::new();
        built.insert(IVec2::ZERO, 0u8);
        built.insert(IVec2::X, 0);
        built.insert(IVec2::Y, 1);
        assert_eq!(lod_segments(&terrain, &built).len(), 1);

        built.insert(IVec2::X, 2);
        assert_eq!(lod_segments(&terrain, &built).len(), 2);
    }

    #[test]
    fn unbuilt_chunks_draw_no_curtain() {
        let terrain = Terrain::default();
        let built: HashMap<IVec2, u8> = HashMap::new();
        assert!(lod_segments(&terrain, &built).is_empty());
    }

    #[test]
    fn a_curtain_ribbon_stands_on_the_sampled_ground() {
        let terrain = Terrain::default();
        let segments = vec![CurtainSegment {
            a: Vec2::new(0.0, -10.0),
            b: Vec2::new(0.0, 10.0),
            color: [1.0, 0.0, 0.0],
        }];
        let mesh = curtain_mesh(&terrain, &segments);
        let positions = attribute_positions(&mesh);
        assert_eq!(positions.len(), 2 * (CURTAIN_STEPS as usize + 1));
        for pair in positions.chunks(2) {
            let ground = terrain.sample_ground_height(pair[0][0], pair[0][2]);
            assert_eq!(pair[0][1], ground);
            assert_eq!(pair[1][1], ground + CURTAIN_HEIGHT);
        }
        assert_eq!(
            mesh.indices().map(|indices| indices.len()),
            Some(CURTAIN_STEPS as usize * 6)
        );
    }
}
