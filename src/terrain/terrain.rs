use avian3d::prelude::Collider;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, Mesh, PrimitiveTopology},
    prelude::*,
};

const CRATER_DEPTH: f32 = 30.0;
const CRATER_RIM_X: f32 = 30.0;
const CRATER_RIM_Z: f32 = 60.0;
const CRATER_INSET: f32 = 100.0;
const NORMAL_SAMPLE_STEP: f32 = 0.5;
const LOD_HYSTERESIS: f32 = 0.15;
const MIN_LOD_SUBDIVISIONS: u32 = 8;

#[derive(Resource, Clone, Copy)]
pub struct TerrainConfig {
    pub world: Vec2,
    pub chunk_size: f32,
    pub mesh_subdivisions: u32,
    pub collider_subdivisions: u32,
    pub collider_radius: f32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            world: Vec2::splat(1000.0),
            chunk_size: 60.0,
            mesh_subdivisions: 64,
            collider_subdivisions: 60,
            collider_radius: 120.0,
        }
    }
}

#[derive(Resource, Clone, Copy)]
pub struct Terrain(pub(crate) TerrainConfig);

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
        (0..count.y)
            .flat_map(move |iz| (0..count.x).map(move |ix| IVec2::new(ix as i32, iz as i32)))
    }

    pub fn sample_ground_height(&self, x: f32, z: f32) -> f32 {
        let over_x = (x.abs() - (self.0.world.x / 2.0 - CRATER_INSET)).max(0.0) / CRATER_RIM_X;
        let over_z = (z.abs() - (self.0.world.y / 2.0 - CRATER_INSET)).max(0.0) / CRATER_RIM_Z;
        let t = over_x.hypot(over_z).clamp(0.0, 1.0);
        -CRATER_DEPTH * t * t * (3.0 - 2.0 * t)
    }

    pub fn ground_normal(&self, x: f32, z: f32) -> Vec3 {
        let step = NORMAL_SAMPLE_STEP;
        let slope_x =
            self.sample_ground_height(x + step, z) - self.sample_ground_height(x - step, z);
        let slope_z =
            self.sample_ground_height(x, z + step) - self.sample_ground_height(x, z - step);
        Vec3::new(-slope_x, 2.0 * step, -slope_z).normalize()
    }

    pub fn max_lod(&self) -> u8 {
        self.config()
            .mesh_subdivisions
            .max(MIN_LOD_SUBDIVISIONS)
            .ilog2()
            .saturating_sub(MIN_LOD_SUBDIVISIONS.ilog2()) as u8
    }

    pub fn lod_subdivisions(&self, lod: u8) -> u32 {
        self.config().mesh_subdivisions >> lod.min(self.max_lod())
    }

    pub fn lod_ring(&self) -> f32 {
        self.chunk_extent().x.max(self.chunk_extent().y)
    }

    pub fn raw_lod(&self, distance: f32) -> u8 {
        ((distance / self.lod_ring()).floor().max(0.0) as u32).min(self.max_lod() as u32) as u8
    }

    pub fn lod_for_distance(&self, distance: f32, current: u8) -> u8 {
        let ring = self.lod_ring();
        let raw = self.raw_lod(distance);
        if raw == current {
            return current;
        }
        let margin = ring * LOD_HYSTERESIS;
        if raw < current {
            if distance < (raw as f32 + 1.0) * ring - margin {
                raw
            } else {
                current
            }
        } else if distance > raw as f32 * ring + margin {
            raw
        } else {
            current
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StitchFlags(u8);

impl StitchFlags {
    pub const NONE: Self = Self(0);
    const NORTH: u8 = 1;
    const SOUTH: u8 = 2;
    const EAST: u8 = 4;
    const WEST: u8 = 8;

    pub fn with_north(self) -> Self {
        Self(self.0 | Self::NORTH)
    }

    pub fn with_south(self) -> Self {
        Self(self.0 | Self::SOUTH)
    }

    pub fn with_east(self) -> Self {
        Self(self.0 | Self::EAST)
    }

    pub fn with_west(self) -> Self {
        Self(self.0 | Self::WEST)
    }

    pub fn has_north(self) -> bool {
        self.0 & Self::NORTH != 0
    }

    pub fn has_south(self) -> bool {
        self.0 & Self::SOUTH != 0
    }

    pub fn has_east(self) -> bool {
        self.0 & Self::EAST != 0
    }

    pub fn has_west(self) -> bool {
        self.0 & Self::WEST != 0
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

struct IndexGrid {
    lines: usize,
    last: usize,
    indices: Vec<u32>,
    claimed: Vec<bool>,
}

impl IndexGrid {
    fn new(lines: usize) -> Self {
        let last = lines - 1;
        Self {
            lines,
            last,
            indices: Vec::with_capacity(last * last * 6),
            claimed: vec![false; last * last],
        }
    }

    fn finish(self) -> Vec<u32> {
        self.indices
    }

    fn v(&self, ix: usize, iz: usize) -> u32 {
        (iz * self.lines + ix) as u32
    }

    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }

    fn claim(&mut self, ix: usize, iz: usize) {
        self.claimed[iz * self.last + ix] = true;
    }

    fn is_claimed(&self, ix: usize, iz: usize) -> bool {
        self.claimed[iz * self.last + ix]
    }

    fn quad(&mut self, ix: usize, iz: usize) {
        if self.is_claimed(ix, iz) {
            return;
        }
        self.claim(ix, iz);
        let near = self.v(ix, iz);
        let far = self.v(ix, iz + 1);
        let near_next = self.v(ix + 1, iz);
        let far_next = self.v(ix + 1, iz + 1);
        self.tri(near, far, near_next);
        self.tri(near_next, far, far_next);
    }

    fn east_strip(&mut self) {
        let last = self.last;
        for iz in (0..last).step_by(2) {
            if self.is_claimed(last - 1, iz) {
                continue;
            }
            let a = self.v(last - 1, iz);
            let b = self.v(last - 1, iz + 1);
            let c = self.v(last, iz);
            let e = self.v(last, iz + 2);
            let f = self.v(last - 1, iz + 2);
            self.tri(a, b, c);
            self.tri(b, f, c);
            self.tri(c, f, e);
            self.claim(last - 1, iz);
            self.claim(last - 1, iz + 1);
        }
    }

    fn west_strip(&mut self) {
        let last = self.last;
        for iz in (0..last).step_by(2) {
            if self.is_claimed(0, iz) {
                continue;
            }
            let a = self.v(1, iz);
            let b = self.v(1, iz + 1);
            let c = self.v(0, iz);
            let e = self.v(0, iz + 2);
            let f = self.v(1, iz + 2);
            self.tri(a, c, b);
            self.tri(b, c, f);
            self.tri(c, e, f);
            self.claim(0, iz);
            self.claim(0, iz + 1);
        }
    }

    fn north_strip(&mut self) {
        let last = self.last;
        for ix in (0..last).step_by(2) {
            if self.is_claimed(ix, last - 1) {
                continue;
            }
            let n = self.v(ix, last - 1);
            let far = self.v(ix, last);
            let b = self.v(ix + 1, last - 1);
            let f = self.v(ix + 2, last - 1);
            let e = self.v(ix + 2, last);
            self.tri(n, far, b);
            self.tri(b, far, f);
            self.tri(far, e, f);
            self.claim(ix, last - 1);
            self.claim(ix + 1, last - 1);
        }
    }

    fn south_strip(&mut self) {
        let last = self.last;
        for ix in (0..last).step_by(2) {
            if self.is_claimed(ix, 0) {
                continue;
            }
            let n = self.v(ix, 0);
            let far = self.v(ix, 1);
            let b = self.v(ix + 1, 1);
            let e = self.v(ix + 2, 0);
            let f = self.v(ix + 2, 1);
            self.tri(n, far, b);
            self.tri(n, b, e);
            self.tri(e, b, f);
            self.claim(ix, 0);
            self.claim(ix + 1, 0);
        }
    }

    fn north_east_corner(&mut self) {
        let last = self.last;
        self.claim(last - 2, last - 2);
        self.claim(last - 1, last - 2);
        self.claim(last - 2, last - 1);
        self.claim(last - 1, last - 1);
        self.tri(
            self.v(last - 2, last - 2),
            self.v(last - 2, last - 1),
            self.v(last - 1, last - 2),
        );
        self.tri(
            self.v(last - 1, last - 2),
            self.v(last - 2, last - 1),
            self.v(last - 1, last - 1),
        );
        self.tri(
            self.v(last - 2, last - 1),
            self.v(last - 2, last),
            self.v(last - 1, last - 1),
        );
        self.tri(
            self.v(last - 1, last - 2),
            self.v(last - 1, last - 1),
            self.v(last, last - 2),
        );
        self.tri(
            self.v(last - 1, last - 1),
            self.v(last - 2, last),
            self.v(last, last),
        );
        self.tri(
            self.v(last - 1, last - 1),
            self.v(last, last),
            self.v(last, last - 2),
        );
    }

    fn north_west_corner(&mut self) {
        let last = self.last;
        self.claim(0, last - 2);
        self.claim(1, last - 2);
        self.claim(0, last - 1);
        self.claim(1, last - 1);
        self.tri(
            self.v(1, last - 2),
            self.v(1, last - 1),
            self.v(2, last - 2),
        );
        self.tri(
            self.v(2, last - 2),
            self.v(1, last - 1),
            self.v(2, last - 1),
        );
        self.tri(
            self.v(0, last - 2),
            self.v(1, last - 1),
            self.v(1, last - 2),
        );
        self.tri(self.v(0, last - 2), self.v(0, last), self.v(1, last - 1));
        self.tri(self.v(1, last - 1), self.v(0, last), self.v(2, last));
        self.tri(self.v(2, last - 1), self.v(1, last - 1), self.v(2, last));
    }

    fn south_east_corner(&mut self) {
        let last = self.last;
        self.claim(last - 2, 0);
        self.claim(last - 1, 0);
        self.claim(last - 2, 1);
        self.claim(last - 1, 1);
        self.tri(
            self.v(last - 2, 1),
            self.v(last - 2, 2),
            self.v(last - 1, 1),
        );
        self.tri(
            self.v(last - 1, 1),
            self.v(last - 2, 2),
            self.v(last - 1, 2),
        );
        self.tri(
            self.v(last - 2, 0),
            self.v(last - 2, 1),
            self.v(last - 1, 1),
        );
        self.tri(self.v(last - 2, 0), self.v(last - 1, 1), self.v(last, 0));
        self.tri(self.v(last - 1, 1), self.v(last, 2), self.v(last, 0));
        self.tri(self.v(last - 2, 1), self.v(last, 2), self.v(last - 1, 1));
    }

    fn south_west_corner(&mut self) {
        self.claim(0, 0);
        self.claim(1, 0);
        self.claim(0, 1);
        self.claim(1, 1);
        self.tri(self.v(1, 1), self.v(1, 2), self.v(2, 1));
        self.tri(self.v(2, 1), self.v(1, 2), self.v(2, 2));
        self.tri(self.v(0, 0), self.v(1, 2), self.v(1, 1));
        self.tri(self.v(0, 0), self.v(0, 2), self.v(1, 2));
        self.tri(self.v(0, 0), self.v(1, 1), self.v(2, 0));
        self.tri(self.v(1, 1), self.v(2, 1), self.v(2, 0));
    }
}

pub fn chunk_indices(lines: usize, flags: StitchFlags) -> Vec<u32> {
    let mut grid = IndexGrid::new(lines);
    if flags.has_west() && flags.has_north() {
        grid.north_west_corner();
    }
    if flags.has_east() && flags.has_north() {
        grid.north_east_corner();
    }
    if flags.has_west() && flags.has_south() {
        grid.south_west_corner();
    }
    if flags.has_east() && flags.has_south() {
        grid.south_east_corner();
    }
    if flags.has_west() {
        grid.west_strip();
    }
    if flags.has_east() {
        grid.east_strip();
    }
    if flags.has_south() {
        grid.south_strip();
    }
    if flags.has_north() {
        grid.north_strip();
    }
    let last = grid.last;
    for iz in 0..last {
        for ix in 0..last {
            grid.quad(ix, iz);
        }
    }
    grid.finish()
}

pub fn build_chunk_mesh(terrain: &Terrain, coord: IVec2, lod: u8, flags: StitchFlags) -> Mesh {
    let grid = ChunkGrid::new(terrain, coord, terrain.lod_subdivisions(lod));
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

    let indices = chunk_indices(grid.lines, flags);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
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
        assert_eq!(terrain.chunk_count(), UVec2::new(50, 50));
        assert_eq!(terrain.chunk_extent(), Vec2::splat(60.0));
        assert_eq!(terrain.chunk_origin(IVec2::ZERO), Vec2::splat(-1500.0));
        assert_eq!(
            terrain.chunk_origin(IVec2::new(49, 49)) + terrain.chunk_extent(),
            Vec2::splat(1500.0)
        );
        assert_eq!(terrain.chunk_coords().count(), 2500);
    }

    #[test]
    fn the_plain_is_flat_and_the_rim_sinks() {
        let terrain = Terrain::default();
        let edge = terrain.config().world.y * 0.5;
        assert_eq!(terrain.sample_ground_height(0.0, 0.0), 0.0);
        assert_eq!(terrain.sample_ground_height(0.0, edge), -CRATER_DEPTH);
        assert_eq!(terrain.ground_normal(0.0, 0.0), Vec3::Y);
    }

    fn all_stitch_flags() -> Vec<StitchFlags> {
        (0..16u8)
            .map(|bits| {
                let mut flags = StitchFlags::NONE;
                if bits & 1 != 0 {
                    flags = flags.with_north();
                }
                if bits & 2 != 0 {
                    flags = flags.with_south();
                }
                if bits & 4 != 0 {
                    flags = flags.with_east();
                }
                if bits & 8 != 0 {
                    flags = flags.with_west();
                }
                flags
            })
            .collect()
    }

    fn grid_point(ix: usize, iz: usize) -> (f32, f32) {
        (ix as f32, iz as f32)
    }

    fn cross_y(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
        (b.1 - a.1) * (c.0 - a.0) - (b.0 - a.0) * (c.1 - a.1)
    }

    #[test]
    fn every_stitch_template_tiles_the_grid_exactly() {
        for lines in [9usize, 17, 65] {
            let last = lines - 1;
            for flags in all_stitch_flags() {
                let indices = chunk_indices(lines, flags);
                assert_eq!(indices.len() % 3, 0);
                let mut area = 0.0;
                for tri in indices.chunks(3) {
                    let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
                    let pa = grid_point(a % lines, a / lines);
                    let pb = grid_point(b % lines, b / lines);
                    let pc = grid_point(c % lines, c / lines);
                    let cross = cross_y(pa, pb, pc);
                    assert!(
                        cross > 0.0,
                        "{flags:?} at {lines} has a flipped or degenerate triangle"
                    );
                    area += cross * 0.5;
                }
                assert_eq!(
                    area,
                    (last * last) as f32,
                    "{flags:?} at {lines} must cover the grid with no gaps or overlaps"
                );
            }
        }
    }

    #[test]
    fn coarse_borders_never_reference_odd_vertices() {
        let lines = 17;
        let last = lines - 1;
        for flags in all_stitch_flags() {
            let indices = chunk_indices(lines, flags);
            for tri in indices.chunks(3) {
                for &index in tri {
                    let ix = index as usize % lines;
                    let iz = index as usize / lines;
                    let removed = (flags.has_east() && ix == last && iz % 2 == 1)
                        || (flags.has_west() && ix == 0 && iz % 2 == 1)
                        || (flags.has_north() && iz == last && ix % 2 == 1)
                        || (flags.has_south() && iz == 0 && ix % 2 == 1);
                    assert!(!removed, "{flags:?} references a vertex it should skip");
                }
            }
        }
    }

    #[test]
    fn a_stitched_fine_edge_meets_the_coarse_neighbour_exactly() {
        let terrain = Terrain::default();
        for (fine_lod, coarse_lod) in [(0u8, 1u8), (1, 2), (2, 3)] {
            let fine = build_chunk_mesh(
                &terrain,
                IVec2::new(0, 0),
                fine_lod,
                StitchFlags::NONE.with_east(),
            );
            let coarse =
                build_chunk_mesh(&terrain, IVec2::new(1, 0), coarse_lod, StitchFlags::NONE);
            let fine_lines = terrain.lod_subdivisions(fine_lod) as usize + 1;
            let coarse_lines = terrain.lod_subdivisions(coarse_lod) as usize + 1;
            let fine_positions = mesh_positions(&fine);
            let coarse_positions = mesh_positions(&coarse);

            let mut fine_edge = Vec::new();
            for tri in chunk_indices(fine_lines, StitchFlags::NONE.with_east()).chunks(3) {
                for &index in tri {
                    let ix = index as usize % fine_lines;
                    let iz = index as usize / fine_lines;
                    if ix == fine_lines - 1 {
                        fine_edge.push((iz, fine_positions[index as usize][1]));
                    }
                }
            }
            fine_edge.sort_by_key(|(iz, _)| *iz);
            fine_edge.dedup();
            let coarse_edge: Vec<(usize, f32)> = (0..coarse_lines)
                .map(|iz| (iz * 2, coarse_positions[iz * coarse_lines][1]))
                .collect();
            assert_eq!(
                fine_edge, coarse_edge,
                "lod {fine_lod} stitched to lod {coarse_lod} must share the coarse chord"
            );
        }
    }

    #[test]
    fn lod_subdivisions_halve_per_level() {
        let terrain = Terrain::default();
        assert_eq!(terrain.max_lod(), 3);
        assert_eq!(terrain.lod_subdivisions(0), 64);
        assert_eq!(terrain.lod_subdivisions(1), 32);
        assert_eq!(terrain.lod_subdivisions(2), 16);
        assert_eq!(terrain.lod_subdivisions(3), 8);
        assert_eq!(terrain.lod_subdivisions(9), 8);
    }

    #[test]
    fn lod_selection_follows_distance_with_hysteresis() {
        let terrain = Terrain::default();
        assert_eq!(terrain.lod_for_distance(0.0, 3), 0);
        assert_eq!(terrain.lod_for_distance(299.0, 0), 3);
        assert_eq!(
            terrain.lod_for_distance(61.0, 0),
            0,
            "hysteresis holds the finer lod"
        );
        assert_eq!(
            terrain.lod_for_distance(71.0, 0),
            1,
            "clearly outside coarsens"
        );
        assert_eq!(
            terrain.lod_for_distance(59.0, 1),
            1,
            "hysteresis holds the finer lod"
        );
        assert_eq!(
            terrain.lod_for_distance(50.0, 1),
            0,
            "clearly inside refines"
        );
        assert_eq!(
            terrain.lod_for_distance(119.0, 1),
            1,
            "hysteresis holds the finer lod"
        );
    }

    #[test]
    fn neighbouring_chunks_share_their_seam() {
        let terrain = Terrain::default();
        let left = build_chunk_mesh(&terrain, IVec2::new(0, 0), 0, StitchFlags::NONE);
        let right = build_chunk_mesh(&terrain, IVec2::new(1, 0), 0, StitchFlags::NONE);
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
            assert_eq!(
                left_normals[edge], right_normals[seam],
                "the seam shading line up"
            );
        }
    }

    #[test]
    fn the_mesh_grid_rests_on_the_sampled_ground() {
        let terrain = Terrain::default();
        let coord = IVec2::new(2, 3);
        let grid = ChunkGrid::new(&terrain, coord, terrain.lod_subdivisions(0));
        let heights = chunk_heights(&terrain, &grid);
        let mesh = build_chunk_mesh(&terrain, coord, 0, StitchFlags::NONE);
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
        let coord = terrain.chunk_count().as_ivec2() - IVec2::splat(2);
        let extent = terrain.chunk_extent();
        let aabb = build_chunk_collider(&terrain, coord)
            .shape()
            .compute_local_aabb();

        assert_eq!(aabb.mins.x, -extent.x * 0.5);
        assert_eq!(aabb.maxs.x, extent.x * 0.5);
        assert_eq!(aabb.mins.z, -extent.y * 0.5);
        assert_eq!(aabb.maxs.z, extent.y * 0.5);
        assert_eq!(aabb.maxs.y, 0.0, "the chunk starts inside the plain");
        assert_eq!(
            aabb.mins.y, -CRATER_DEPTH,
            "the chunk reaches the crater floor"
        );
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
