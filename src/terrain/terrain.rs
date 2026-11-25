use avian3d::prelude::Collider;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, Mesh, PrimitiveTopology},
    prelude::*,
};

const CRATER_DEPTH: f32 = 30.0;
const CRATER_RIM: f32 = 30.0;
const CRATER_INSET: f32 = 100.0;
const NORMAL_SAMPLE_STEP: f32 = 0.5;

#[derive(Resource, Clone, Copy)]
pub struct TerrainConfig {
    pub world: Vec2,
    pub chunk_size: f32,
    pub mesh_subdivisions: u32,
    pub collider_subdivisions: u32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            world: Vec2::splat(300.0),
            chunk_size: 60.0,
            mesh_subdivisions: 60,
            collider_subdivisions: 60,
        }
    }
}

#[derive(Resource, Clone, Copy)]
pub struct Terrain(TerrainConfig);

impl Default for Terrain {
    fn default() -> Self {
        Self(TerrainConfig::default())
    }
}

impl Terrain {
    pub fn config(&self) -> TerrainConfig {
        self.0
    }

    pub fn chunk_count(&self) -> UVec2 {
        let chunks = (self.0.world / self.0.chunk_size.max(1.0)).ceil();
        UVec2::new(chunks.x.max(1.0) as u32, chunks.y.max(1.0) as u32)
    }

    pub fn chunk_extent(&self) -> Vec2 {
        self.0.world / self.chunk_count().as_vec2()
    }

    pub fn chunk_origin(&self, coord: IVec2) -> Vec2 {
        -self.0.world * 0.5 + coord.as_vec2() * self.chunk_extent()
    }

    pub fn chunk_center(&self, coord: IVec2) -> Vec2 {
        self.chunk_origin(coord) + self.chunk_extent() * 0.5
    }

    pub fn chunk_coords(&self) -> impl Iterator<Item = IVec2> {
        let count = self.chunk_count();
        (0..count.y).flat_map(move |iz| (0..count.x).map(move |ix| IVec2::new(ix as i32, iz as i32)))
    }

    pub fn sample_ground_height(&self, x: f32, z: f32) -> f32 {
        let over_x = (x.abs() - (self.0.world.x / 2.0 - CRATER_INSET)).max(0.0);
        let over_z = (z.abs() - (self.0.world.y / 2.0 - CRATER_INSET)).max(0.0);
        let distance = over_x.hypot(over_z);
        let t = (distance / CRATER_RIM).clamp(0.0, 1.0);
        -CRATER_DEPTH * t * t * (3.0 - 2.0 * t)
    }

    pub fn ground_normal(&self, x: f32, z: f32) -> Vec3 {
        let step = NORMAL_SAMPLE_STEP;
        let slope_x = self.sample_ground_height(x + step, z) - self.sample_ground_height(x - step, z);
        let slope_z = self.sample_ground_height(x, z + step) - self.sample_ground_height(x, z - step);
        Vec3::new(-slope_x, 2.0 * step, -slope_z).normalize()
    }
}

struct ChunkGrid {
    center: Vec2,
    cell: Vec2,
    half: Vec2,
    lines: usize,
}

impl ChunkGrid {
    fn new(terrain: &Terrain, coord: IVec2, subdivisions: u32) -> Self {
        let extent = terrain.chunk_extent();
        let lines = subdivisions.max(1) as usize + 1;
        let cell = extent / (lines - 1) as f32;
        Self {
            center: terrain.chunk_center(coord),
            cell,
            half: extent * 0.5,
            lines,
        }
    }

    fn local(&self, ix: usize, iz: usize) -> Vec2 {
        Vec2::new(ix as f32 * self.cell.x, iz as f32 * self.cell.y) - self.half
    }

    fn world(&self, ix: usize, iz: usize) -> Vec2 {
        self.center + self.local(ix, iz)
    }
}

fn chunk_heights(terrain: &Terrain, grid: &ChunkGrid) -> Vec<Vec<f32>> {
    (0..grid.lines)
        .map(|iz| {
            (0..grid.lines)
                .map(|ix| {
                    let world = grid.world(ix, iz);
                    terrain.sample_ground_height(world.x, world.y)
                })
                .collect()
        })
        .collect()
}

fn chunk_collider_heights(terrain: &Terrain, grid: &ChunkGrid) -> Vec<Vec<f32>> {
    (0..grid.lines)
        .map(|ix| {
            (0..grid.lines)
                .map(|iz| {
                    let world = grid.world(ix, iz);
                    terrain.sample_ground_height(world.x, world.y)
                })
                .collect()
        })
        .collect()
}

pub fn build_chunk_mesh(terrain: &Terrain, coord: IVec2) -> Mesh {
    let grid = ChunkGrid::new(terrain, coord, terrain.config().mesh_subdivisions);
    let heights = chunk_heights(terrain, &grid);

    let mut positions = Vec::with_capacity(grid.lines * grid.lines);
    let mut normals = Vec::with_capacity(grid.lines * grid.lines);
    for iz in 0..grid.lines {
        for ix in 0..grid.lines {
            let local = grid.local(ix, iz);
            let world = grid.world(ix, iz);
            positions.push([local.x, heights[iz][ix], local.y]);
            normals.push(terrain.ground_normal(world.x, world.y).to_array());
        }
    }

    let cells = grid.lines - 1;
    let mut indices = Vec::with_capacity(cells * cells * 6);
    for iz in 0..cells {
        for ix in 0..cells {
            let near = (iz * grid.lines + ix) as u32;
            let far = near + grid.lines as u32;
            indices.extend_from_slice(&[near, far, near + 1, near + 1, far, far + 1]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub fn build_chunk_collider(terrain: &Terrain, coord: IVec2) -> Collider {
    let config = terrain.config();
    let grid = ChunkGrid::new(terrain, coord, config.collider_subdivisions);
    let extent = terrain.chunk_extent();
    Collider::heightfield(
        chunk_collider_heights(terrain, &grid),
        Vec3::new(extent.x, 1.0, extent.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn mesh_positions(mesh: &Mesh) -> &Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(positions)) => positions,
            _ => panic!("chunk meshes carry float positions"),
        }
    }

    fn mesh_normals(mesh: &Mesh) -> &Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(normals)) => normals,
            _ => panic!("chunk meshes carry float normals"),
        }
    }

    fn coarse_collider_terrain() -> Terrain {
        Terrain(TerrainConfig {
            collider_subdivisions: 10,
            ..default()
        })
    }

    #[test]
    fn chunks_tile_the_world_exactly() {
        let terrain = Terrain::default();
        assert_eq!(terrain.chunk_count(), UVec2::new(5, 5));
        assert_eq!(terrain.chunk_extent(), Vec2::splat(60.0));
        assert_eq!(terrain.chunk_origin(IVec2::ZERO), Vec2::splat(-150.0));
        assert_eq!(
            terrain.chunk_origin(IVec2::new(4, 4)) + terrain.chunk_extent(),
            Vec2::splat(150.0)
        );
        assert_eq!(terrain.chunk_coords().count(), 25);
    }

    #[test]
    fn the_plain_is_flat_and_the_rim_sinks() {
        let terrain = Terrain::default();
        assert_eq!(terrain.sample_ground_height(0.0, 0.0), 0.0);
        assert_eq!(terrain.sample_ground_height(0.0, 150.0), -CRATER_DEPTH);
        assert_eq!(terrain.ground_normal(0.0, 0.0), Vec3::Y);
    }

    #[test]
    fn neighbouring_chunks_share_their_seam() {
        let terrain = Terrain::default();
        let left = build_chunk_mesh(&terrain, IVec2::new(0, 0));
        let right = build_chunk_mesh(&terrain, IVec2::new(1, 0));
        let lines = terrain.config().mesh_subdivisions as usize + 1;
        let left_positions = mesh_positions(&left);
        let right_positions = mesh_positions(&right);
        let left_normals = mesh_normals(&left);
        let right_normals = mesh_normals(&right);

        for iz in 0..lines {
            let edge = iz * lines + lines - 1;
            let seam = iz * lines;
            let [lx, ly, lz] = left_positions[edge];
            let [rx, ry, rz] = right_positions[seam];
            let left_world = terrain.chunk_center(IVec2::new(0, 0)) + Vec2::new(lx, lz);
            let right_world = terrain.chunk_center(IVec2::new(1, 0)) + Vec2::new(rx, rz);
            assert_eq!(left_world, right_world, "the seam positions line up");
            assert_eq!(ly, ry, "the seam heights line up");
            assert_eq!(left_normals[edge], right_normals[seam], "the seam shading line up");
        }
    }

    #[test]
    fn the_collider_grid_matches_the_mesh_grid() {
        let terrain = Terrain::default();
        let coord = IVec2::new(2, 3);
        let grid = ChunkGrid::new(&terrain, coord, terrain.config().collider_subdivisions);
        let heights = chunk_heights(&terrain, &grid);
        let mesh = build_chunk_mesh(&terrain, coord);
        let positions = mesh_positions(&mesh);

        assert_eq!(heights.len(), grid.lines);
        for (iz, row) in heights.iter().enumerate() {
            for (ix, height) in row.iter().enumerate() {
                let [_, y, _] = positions[iz * grid.lines + ix];
                assert_eq!(y, *height, "collision rests on exactly the drawn ground");
            }
        }
    }

    #[test]
    fn the_collider_fills_its_chunk_and_its_depth() {
        let terrain = coarse_collider_terrain();
        let coord = IVec2::new(2, 3);
        let extent = terrain.chunk_extent();
        let aabb = build_chunk_collider(&terrain, coord).shape().compute_local_aabb();

        assert_eq!(aabb.mins.x, -extent.x * 0.5);
        assert_eq!(aabb.maxs.x, extent.x * 0.5);
        assert_eq!(aabb.mins.z, -extent.y * 0.5);
        assert_eq!(aabb.maxs.z, extent.y * 0.5);
        assert_eq!(aabb.maxs.y, 0.0, "the chunk starts inside the plain");
        assert_eq!(aabb.mins.y, -CRATER_DEPTH, "the chunk reaches the crater floor");
    }

    #[test]
    fn collider_rows_run_along_z() {
        let terrain = Terrain::default();
        let coord = IVec2::new(2, 3);
        let center = terrain.chunk_center(coord);
        let field = build_chunk_collider(&terrain, coord)
            .shape()
            .as_heightfield()
            .expect("chunk colliders are heightfields")
            .clone();
        let heights = field.heights();
        let mut off_diagonal = 0;

        for i in 0..heights.nrows() {
            for j in 0..heights.ncols() {
                let x = center.x + field.x_at(j);
                let z = center.y + field.z_at(i);
                if x.round() != z.round() {
                    off_diagonal += 1;
                }
                let stored = heights[(i, j)] * field.scale().y;
                let expected = terrain.sample_ground_height(x, z);
                assert!(
                    (stored - expected).abs() < 1e-3,
                    "row {i} column {j} sits at ({x}, {z}) and stores {stored}, expected {expected}"
                );
            }
        }

        assert!(off_diagonal > 0, "the probe must leave the x == z diagonal");
    }
}
