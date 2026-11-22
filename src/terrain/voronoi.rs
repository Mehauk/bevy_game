use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bevy::prelude::Vec2;
use glam27::DVec3 as VoroDVec3;
use meshless_voronoi::{Dimensionality, Voronoi};
use noise::{NoiseFn, Perlin};

use crate::terrain::seed::{WorldSeed, STAGE_BOUNDARY_BLEND, STAGE_VORONOI};
use crate::terrain::topology::WorldTopology;

#[derive(Clone, Debug)]
pub struct VoronoiCellData {
    pub id: usize,
    pub center: Vec2,
    pub neighbours: Vec<usize>,
    pub polygon: Vec<Vec2>,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundaryEdge {
    pub cell_a: usize,
    pub cell_b: usize,
    pub p0: Vec2,
    pub p1: Vec2,
    pub midpoint: Vec2,
    pub blend_width: f32,
    #[allow(dead_code)]
    pub wraps: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct VoronoiConfig {
    pub size: f32,
    pub grid_dim: u32,
    pub topology: WorldTopology,
    pub default_blend_width: f32,
    pub blend_min: f32,
    pub blend_max: f32,
    pub jitter_min: f32,
    pub jitter_max: f32,
    pub lloyd_passes: u32,
    pub warp_amplitude: f32,
}

impl Default for VoronoiConfig {
    fn default() -> Self {
        Self {
            size: 400.0,
            grid_dim: 8,
            topology: WorldTopology::Finite,
            default_blend_width: 8.0,
            blend_min: 4.0,
            blend_max: 16.0,
            jitter_min: 0.15,
            jitter_max: 0.85,
            lloyd_passes: 2,
            warp_amplitude: 3.0,
        }
    }
}

#[derive(Clone, Debug)]
struct SpatialIndex {
    per_axis: usize,
    bucket_size: f32,
    half: f32,
    size: f32,
    topology: WorldTopology,
    buckets: Vec<Vec<usize>>,
}

impl SpatialIndex {
    fn build(cells: &[VoronoiCellData], config: &VoronoiConfig) -> Self {
        let per_axis = (config.grid_dim.max(1) as usize).max(1);
        let bucket_size = (config.size / per_axis as f32).max(f32::EPSILON);
        let mut buckets = vec![Vec::new(); per_axis * per_axis];
        for cell in cells {
            let bx = Self::axis_index(cell.center.x, config.size, bucket_size);
            let bz = Self::axis_index(cell.center.y, config.size, bucket_size);
            if let (Some(bx), Some(bz)) = (bx, bz) {
                buckets[bz * per_axis + bx].push(cell.id);
            }
        }
        Self {
            per_axis,
            bucket_size,
            half: config.size * 0.5,
            size: config.size,
            topology: config.topology,
            buckets,
        }
    }

    fn axis_index(value: f32, size: f32, bucket_size: f32) -> Option<usize> {
        let index = ((value + size * 0.5) / bucket_size).floor();
        if index.is_finite() && index >= 0.0 && index < (size / bucket_size) {
            Some(index as usize)
        } else {
            None
        }
    }

    fn raw_axis(value: f32, bucket_size: f32, half: f32, per_axis: usize, wraps: bool) -> i64 {
        let raw = ((value + half) / bucket_size).floor() as i64;
        if wraps {
            raw
        } else {
            raw.clamp(0, per_axis as i64 - 1)
        }
    }

    fn wrapped_axis(raw: i64, per_axis: usize, wraps: bool) -> Option<usize> {
        if wraps {
            Some(raw.rem_euclid(per_axis as i64) as usize)
        } else if raw >= 0 && raw < per_axis as i64 {
            Some(raw as usize)
        } else {
            None
        }
    }

    fn nearest(&self, cells: &[VoronoiCellData], p: Vec2) -> Option<(f32, usize)> {
        if cells.is_empty() {
            return None;
        }
        let wraps = self.topology.wraps();
        let raw_x = Self::raw_axis(p.x, self.bucket_size, self.half, self.per_axis, wraps);
        let raw_z = Self::raw_axis(p.y, self.bucket_size, self.half, self.per_axis, wraps);
        let mut best: Option<(f32, usize)> = None;
        let max_ring = self.per_axis as i64 + 1;
        for ring in 1..=max_ring {
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    let bx = Self::wrapped_axis(raw_x + dx, self.per_axis, wraps);
                    let bz = Self::wrapped_axis(raw_z + dz, self.per_axis, wraps);
                    let (Some(bx), Some(bz)) = (bx, bz) else {
                        continue;
                    };
                    for &id in &self.buckets[bz * self.per_axis + bx] {
                        let distance = self.topology.distance_xz(p, cells[id].center, self.size);
                        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                            best = Some((distance, id));
                        }
                    }
                }
            }
            if let Some((distance, _)) = best {
                if distance <= ring as f32 * self.bucket_size {
                    break;
                }
            }
        }
        best
    }

    fn candidates_within(&self, cells: &[VoronoiCellData], p: Vec2, cutoff: f32, out: &mut CandidateSet) {
        let wraps = self.topology.wraps();
        let accept = |id: usize, out: &mut CandidateSet| {
            if self.topology.distance_xz(p, cells[id].center, self.size) <= cutoff {
                out.push(id);
            }
        };
        let ring = ((cutoff / self.bucket_size).ceil() as i64).max(1);
        if ring > self.per_axis as i64 {
            for bucket in &self.buckets {
                for &id in bucket {
                    accept(id, out);
                }
            }
            return;
        }
        let raw_x = Self::raw_axis(p.x, self.bucket_size, self.half, self.per_axis, wraps);
        let raw_z = Self::raw_axis(p.y, self.bucket_size, self.half, self.per_axis, wraps);
        for dz in -ring..=ring {
            for dx in -ring..=ring {
                let bx = Self::wrapped_axis(raw_x + dx, self.per_axis, wraps);
                let bz = Self::wrapped_axis(raw_z + dz, self.per_axis, wraps);
                let (Some(bx), Some(bz)) = (bx, bz) else {
                    continue;
                };
                for &id in &self.buckets[bz * self.per_axis + bx] {
                    accept(id, out);
                }
            }
        }
    }
}

const WARP_FIELD_FREQUENCY: f64 = 0.02;
const TORUS_WARP_FIELD_FREQUENCY: f64 = 0.45;
const TORUS_WARP_DECORRELATION: f64 = 3.7;
const WARP_FIELD_DECORRELATION_X: f64 = 31.7;
const WARP_FIELD_DECORRELATION_Z: f64 = 19.3;

const MIN_CENTRE_DISTANCE: f32 = 1e-6;
const HARD_BOUNDARY_WIDTH: f32 = 1e-5;
const MIN_BLEND_WEIGHT: f32 = 1e-6;
const WRAP_REACH_FRACTION: f32 = 0.75;

#[derive(Clone, Debug)]
struct WarpField {
    x: Perlin,
    z: Perlin,
    amplitude: f32,
    topology: WorldTopology,
    size: f32,
}

impl WarpField {
    fn new(seed: WorldSeed, config: &VoronoiConfig) -> Self {
        let base = seed.stage(STAGE_VORONOI);
        Self {
            x: Perlin::new((base as u32) ^ 0x51f3_24ae),
            z: Perlin::new(((base >> 32) as u32) ^ 0x3f5a_1e77),
            amplitude: config.warp_amplitude.max(0.0),
            topology: config.topology,
            size: config.size,
        }
    }

    fn offset(&self, p: Vec2) -> Vec2 {
        if self.amplitude <= 1e-6 {
            return Vec2::ZERO;
        }
        let (wx, wz) = if self.topology.wraps() && self.size > 0.0 {
            let tau = std::f64::consts::TAU;
            let u = (p.x / self.size) as f64 * tau;
            let v = (p.y / self.size) as f64 * tau;
            let q = [
                u.cos() * TORUS_WARP_FIELD_FREQUENCY,
                u.sin() * TORUS_WARP_FIELD_FREQUENCY,
                v.cos() * TORUS_WARP_FIELD_FREQUENCY,
                v.sin() * TORUS_WARP_FIELD_FREQUENCY,
            ];
            (
                self.x.get(q),
                self.z.get([
                    q[0] + TORUS_WARP_DECORRELATION,
                    q[1] - TORUS_WARP_DECORRELATION * 0.7,
                    q[2] + TORUS_WARP_DECORRELATION * 0.5,
                    q[3] + TORUS_WARP_DECORRELATION * 1.3,
                ]),
            )
        } else {
            let q = [
                p.x as f64 * WARP_FIELD_FREQUENCY,
                p.y as f64 * WARP_FIELD_FREQUENCY,
            ];
            (
                self.x.get([q[0], q[1], 0.0, 0.0]),
                self.z.get([
                    q[0] + WARP_FIELD_DECORRELATION_X,
                    q[1] - WARP_FIELD_DECORRELATION_Z,
                    0.0,
                    0.0,
                ]),
            )
        };
        Vec2::new(wx as f32 * self.amplitude, wz as f32 * self.amplitude)
    }
}

#[derive(Clone, Debug)]
pub struct VertexBlendCache {
    cell_ids: Vec<Option<usize>>,
    offsets: Vec<usize>,
    weights: Vec<(usize, f32)>,
}

impl VertexBlendCache {
    pub fn cell_at(&self, index: usize) -> Option<usize> {
        self.cell_ids.get(index).copied().flatten()
    }

    pub fn weights_at(&self, index: usize) -> &[(usize, f32)] {
        let (Some(start), Some(end)) = (self.offsets.get(index), self.offsets.get(index + 1)) else {
            return &[];
        };
        &self.weights[*start..*end]
    }
}

#[derive(Clone, Debug)]
pub struct VoronoiLayout {
    cells: Vec<VoronoiCellData>,
    boundaries: Vec<BoundaryEdge>,
    blends: HashMap<(usize, usize), f32>,
    cell_weights: Vec<f32>,
    seed: WorldSeed,
    config: VoronoiConfig,
    index: SpatialIndex,
    warp: WarpField,
    vertex_caches: Arc<Mutex<HashMap<(usize, u32, u8), Arc<VertexBlendCache>>>>,
}

const INLINE_CANDIDATES: usize = 64;

struct CandidateSet {
    inline: [(usize, f32); INLINE_CANDIDATES],
    spill: Vec<(usize, f32)>,
    len: usize,
}

impl CandidateSet {
    fn new() -> Self {
        Self {
            inline: [(0, 0.0); INLINE_CANDIDATES],
            spill: Vec::new(),
            len: 0,
        }
    }

    fn push(&mut self, id: usize) {
        if self.len < INLINE_CANDIDATES {
            self.inline[self.len] = (id, 1.0);
        } else {
            self.spill.push((id, 1.0));
        }
        self.len += 1;
    }

    fn len(&self) -> usize {
        self.len
    }

    fn get(&self, index: usize) -> (usize, f32) {
        if index < INLINE_CANDIDATES {
            self.inline[index]
        } else {
            self.spill[index - INLINE_CANDIDATES]
        }
    }

    fn set_weight(&mut self, index: usize, weight: f32) {
        if index < INLINE_CANDIDATES {
            self.inline[index].1 = weight;
        } else {
            self.spill[index - INLINE_CANDIDATES].1 = weight;
        }
    }

    fn multiply_weight(&mut self, index: usize, factor: f32) {
        if index < INLINE_CANDIDATES {
            self.inline[index].1 *= factor;
        } else {
            self.spill[index - INLINE_CANDIDATES].1 *= factor;
        }
    }
}

fn boundary_seed_key(a: usize, b: usize) -> u64 {
    ((a as u64) << 32) | b as u64
}

fn pair_key(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn smooth_response(relative: f32) -> f32 {
    let u = ((relative + 1.0) * 0.5).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

fn clip_half_plane(polygon: &[Vec2], m: Vec2, normal: Vec2) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(polygon.len() + 1);
    if polygon.is_empty() {
        return out;
    }
    let signed = |p: Vec2| (p - m).dot(normal);
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let side_a = signed(a);
        let side_b = signed(b);
        if side_a <= 0.0 {
            out.push(a);
        }
        if (side_a < 0.0 && side_b > 0.0) || (side_a > 0.0 && side_b < 0.0) {
            out.push(a.lerp(b, side_a / (side_a - side_b)));
        }
    }
    out
}

impl VoronoiLayout {
    pub fn generate(seed: WorldSeed, config: VoronoiConfig) -> Self {
        Self::generate_with_weights(seed, config, &[])
    }

    pub fn generate_with_weights(
        seed: WorldSeed,
        config: VoronoiConfig,
        weights: &[f32],
    ) -> Self {
        let dim = config.grid_dim.max(1);
        let half = config.size * 0.5;
        let span = config.size / dim as f32;

        let mut generators = Vec::with_capacity((dim * dim) as usize);
        for gz in 0..dim {
            for gx in 0..dim {
                let index = (gz * dim + gx) as u64;
                let jitter_x = config.jitter_min
                    + (config.jitter_max - config.jitter_min)
                        * seed.unit(STAGE_VORONOI, index, 0);
                let jitter_z = config.jitter_min
                    + (config.jitter_max - config.jitter_min)
                        * seed.unit(STAGE_VORONOI, index, 1);
                generators.push(VoroDVec3::new(
                    (-half + (gx as f32 + jitter_x) * span) as f64,
                    (-half + (gz as f32 + jitter_z) * span) as f64,
                    0.0,
                ));
            }
        }

        for _ in 0..config.lloyd_passes {
            let diagram = Self::build_voronoi(&generators, &config);
            generators = Self::relax_generators(&diagram, &config);
        }

        let centers: Vec<Vec2> = generators
            .iter()
            .map(|generator| Vec2::new(generator.x as f32, generator.y as f32))
            .collect();
        Self::assemble(seed, config, &centers, weights)
    }

    pub fn with_cell_weights(&self, weights: &[f32]) -> Self {
        let centers: Vec<Vec2> = self.cells.iter().map(|cell| cell.center).collect();
        Self::assemble(self.seed, self.config, &centers, weights)
    }

    fn assemble(
        seed: WorldSeed,
        config: VoronoiConfig,
        centers: &[Vec2],
        weights: &[f32],
    ) -> Self {
        let generators: Vec<VoroDVec3> = centers
            .iter()
            .map(|center| VoroDVec3::new(center.x as f64, center.y as f64, 0.0))
            .collect();
        let voronoi = Self::build_voronoi(&generators, &config);

        let mut cells: Vec<VoronoiCellData> = centers
            .iter()
            .enumerate()
            .map(|(index, center)| VoronoiCellData {
                id: index,
                center: *center,
                neighbours: Vec::new(),
                polygon: Vec::new(),
            })
            .collect();

        let cell_weights: Vec<f32> = (0..cells.len())
            .map(|index| {
                weights
                    .get(index)
                    .copied()
                    .filter(|weight| weight.is_finite())
                    .unwrap_or(0.0)
                    .max(0.0)
            })
            .collect();

        let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut seen: HashSet<(usize, usize)> = HashSet::new();

        for face in voronoi.faces() {
            let Some(right) = face.right() else {
                continue;
            };
            let left = face.left();
            if left == right {
                continue;
            }
            let key = pair_key(left, right);
            if seen.insert(key) {
                pairs.push(key);
            }
            adjacency.entry(left).or_default().push(right);
            adjacency.entry(right).or_default().push(left);
        }

        for cell in cells.iter_mut() {
            if let Some(list) = adjacency.get_mut(&cell.id) {
                list.sort_unstable();
                list.dedup();
                cell.neighbours = list.clone();
            }
        }

        let polygons: Vec<Vec<Vec2>> = cells
            .iter()
            .map(|cell| Self::cell_polygon(&cells, &cell_weights, cell, &config))
            .collect();
        for (cell, polygon) in cells.iter_mut().zip(polygons) {
            cell.polygon = polygon;
        }

        let mut blends: HashMap<(usize, usize), f32> = HashMap::new();
        for (a, b) in pairs {
            let key = (a, b);
            if blends.contains_key(&key) {
                continue;
            }
            let blend_width = if config.blend_max > 0.0 {
                seed.range(
                    STAGE_BOUNDARY_BLEND,
                    boundary_seed_key(key.0, key.1),
                    0,
                    config.blend_min,
                    config.blend_max,
                )
            } else {
                0.0
            };
            blends.insert(key, blend_width);
        }

        let mut boundaries = Self::boundary_edges(&cells, &cell_weights, &config);
        for boundary in boundaries.iter_mut() {
            boundary.blend_width = *blends
                .get(&pair_key(boundary.cell_a, boundary.cell_b))
                .unwrap_or(&config.default_blend_width);
        }
        boundaries.sort_by_key(|b| (b.cell_a, b.cell_b));

        let index = SpatialIndex::build(&cells, &config);
        let warp = WarpField::new(seed, &config);

        Self {
            cells,
            boundaries,
            blends,
            cell_weights,
            seed,
            config,
            index,
            warp,
            vertex_caches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn build_voronoi(generators: &[VoroDVec3], config: &VoronoiConfig) -> Voronoi {
        let half = config.size * 0.5;
        let anchor = VoroDVec3::new(-half as f64, -half as f64, -0.5);
        let width = VoroDVec3::new(config.size as f64, config.size as f64, 1.0);
        Voronoi::build(
            generators,
            anchor,
            width,
            Dimensionality::TwoD,
            config.topology.wraps(),
        )
    }

    fn relax_generators(voronoi: &Voronoi, config: &VoronoiConfig) -> Vec<VoroDVec3> {
        let half = config.size * 0.5;
        voronoi
            .cells()
            .iter()
            .map(|cell| {
                let centroid = cell.centroid();
                if !centroid.x.is_finite() || !centroid.y.is_finite() {
                    return cell.loc();
                }
                let mut point = Vec2::new(centroid.x as f32, centroid.y as f32);
                if config.topology.wraps() {
                    point = config.topology.wrap_xz(point, config.size);
                } else {
                    point = Vec2::new(
                        point.x.clamp(-half, half),
                        point.y.clamp(-half, half),
                    );
                }
                VoroDVec3::new(point.x as f64, point.y as f64, 0.0)
            })
            .collect()
    }

    fn radical_axis(
        weights: &[f32],
        cell: &VoronoiCellData,
        other: &VoronoiCellData,
        config: &VoronoiConfig,
    ) -> Option<(Vec2, Vec2)> {
        let delta = config
            .topology
            .delta_xz(cell.center, other.center, config.size);
        let length = delta.length();
        if length <= MIN_CENTRE_DISTANCE {
            return None;
        }
        let shift =
            (weights[cell.id] - weights[other.id]) / (2.0 * length * length);
        Some((cell.center + delta * (0.5 + shift), delta / length))
    }

    fn cell_polygon(
        cells: &[VoronoiCellData],
        weights: &[f32],
        cell: &VoronoiCellData,
        config: &VoronoiConfig,
    ) -> Vec<Vec2> {
        let half = config.size * 0.5;
        let mut polygon = if config.topology.wraps() {
            let reach = config.size * WRAP_REACH_FRACTION;
            vec![
                cell.center + Vec2::new(-reach, -reach),
                cell.center + Vec2::new(reach, -reach),
                cell.center + Vec2::new(reach, reach),
                cell.center + Vec2::new(-reach, reach),
            ]
        } else {
            vec![
                Vec2::new(-half, -half),
                Vec2::new(half, -half),
                Vec2::new(half, half),
                Vec2::new(-half, half),
            ]
        };
        for other in cells {
            if other.id == cell.id {
                continue;
            }
            let Some((bisector, normal)) =
                Self::radical_axis(weights, cell, other, config)
            else {
                continue;
            };
            polygon = clip_half_plane(&polygon, bisector, normal);
            if polygon.is_empty() {
                break;
            }
        }
        polygon
    }

    fn boundary_edges(
        cells: &[VoronoiCellData],
        weights: &[f32],
        config: &VoronoiConfig,
    ) -> Vec<BoundaryEdge> {
        let half = config.size * 0.5;
        let bisector_tolerance = 1e-2;
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        let mut boundaries: Vec<BoundaryEdge> = Vec::new();
        for cell in cells {
            let polygon = &cell.polygon;
            if polygon.len() < 2 {
                continue;
            }
            for index in 0..polygon.len() {
                let p0 = polygon[index];
                let p1 = polygon[(index + 1) % polygon.len()];
                if p0 == p1 {
                    continue;
                }
                let mut best: Option<(f32, usize)> = None;
                for other in cells {
                    if other.id == cell.id {
                        continue;
                    }
                    let Some((bisector, normal)) =
                        Self::radical_axis(weights, cell, other, config)
                    else {
                        continue;
                    };
                    let p0_offset = (p0 - bisector).dot(normal).abs();
                    let p1_offset = (p1 - bisector).dot(normal).abs();
                    if p0_offset <= bisector_tolerance && p1_offset <= bisector_tolerance {
                        let total_offset = p0_offset + p1_offset;
                        if best.is_none_or(|(best_offset, _)| total_offset < best_offset) {
                            best = Some((total_offset, other.id));
                        }
                    }
                }
                let Some((_, owner)) = best else {
                    continue;
                };
                if !seen.insert(pair_key(cell.id, owner)) {
                    continue;
                }
                let wraps = config.topology.wraps()
                    && [p0, p1]
                        .iter()
                        .any(|p| p.x.abs() > half + 1e-3 || p.y.abs() > half + 1e-3);
                boundaries.push(BoundaryEdge {
                    cell_a: cell.id,
                    cell_b: owner,
                    p0,
                    p1,
                    midpoint: (p0 + p1) * 0.5,
                    blend_width: 0.0,
                    wraps,
                });
            }
        }
        boundaries
    }

    pub fn cells(&self) -> &[VoronoiCellData] {
        &self.cells
    }

    pub fn boundaries(&self) -> &[BoundaryEdge] {
        &self.boundaries
    }

    pub fn cell(&self, id: usize) -> Option<&VoronoiCellData> {
        self.cells.get(id)
    }

    pub fn max_blend_width(&self) -> f32 {
        self.boundaries
            .iter()
            .map(|b| b.blend_width)
            .fold(self.config.default_blend_width, f32::max)
    }

    pub fn blend_width(&self, a: usize, b: usize) -> f32 {
        if a == b {
            return 0.0;
        }
        self.blends
            .get(&pair_key(a, b))
            .copied()
            .unwrap_or(self.config.default_blend_width)
    }

    pub fn warped_point(&self, p: Vec2) -> Vec2 {
        p + self.warp.offset(p)
    }

    pub fn cell_weight(&self, id: usize) -> f32 {
        self.cell_weights.get(id).copied().unwrap_or(0.0)
    }

    pub fn max_cell_weight(&self) -> f32 {
        self.cell_weights
            .iter()
            .cloned()
            .fold(0.0f32, f32::max)
    }

    pub fn power_distance(&self, p: Vec2, id: usize) -> f32 {
        let distance = self.distance(p, self.cells[id].center);
        distance * distance - self.cell_weight(id)
    }

    pub fn cell_at(&self, p: Vec2) -> Option<usize> {
        self.power_nearest(self.warped_point(p))
    }

    fn power_nearest(&self, q: Vec2) -> Option<usize> {
        if self.cells.is_empty() {
            return None;
        }
        let index = &self.index;
        let wraps = index.topology.wraps();
        let raw_x =
            SpatialIndex::raw_axis(q.x, index.bucket_size, index.half, index.per_axis, wraps);
        let raw_z =
            SpatialIndex::raw_axis(q.y, index.bucket_size, index.half, index.per_axis, wraps);
        let weight_ceiling = self.max_cell_weight();
        let mut best: Option<(f32, usize)> = None;
        let max_ring = index.per_axis as i64 + 1;
        for ring in 1..=max_ring {
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    let bx = SpatialIndex::wrapped_axis(raw_x + dx, index.per_axis, wraps);
                    let bz = SpatialIndex::wrapped_axis(raw_z + dz, index.per_axis, wraps);
                    let (Some(bx), Some(bz)) = (bx, bz) else {
                        continue;
                    };
                    for &id in &index.buckets[bz * index.per_axis + bx] {
                        let power = self.power_distance(q, id);
                        if best.is_none_or(|(best_power, _)| power < best_power) {
                            best = Some((power, id));
                        }
                    }
                }
            }
            if let Some((power, _)) = best {
                let reach = ring as f32 * index.bucket_size;
                if power <= reach * reach - weight_ceiling {
                    break;
                }
            }
        }
        best.map(|(_, id)| id)
    }

    fn nearest_cell(&self, p: Vec2) -> Option<(f32, usize)> {
        self.index.nearest(&self.cells, p)
    }

    pub fn distance(&self, a: Vec2, b: Vec2) -> f32 {
        self.config.topology.distance_xz(a, b, self.config.size)
    }

    pub fn delta(&self, from: Vec2, to: Vec2) -> Vec2 {
        self.config.topology.delta_xz(from, to, self.config.size)
    }

    pub fn cell_weights_at(&self, p: Vec2) -> Vec<(usize, f32)> {
        let mut out = Vec::new();
        self.cell_weights_into(p, &mut out);
        out
    }

    fn cell_weights_into(&self, p: Vec2, out: &mut Vec<(usize, f32)>) {
        out.clear();
        let q = self.warped_point(p);
        let Some((nearest_distance, nearest_id)) = self.nearest_cell(q) else {
            return;
        };
        let width = self.max_blend_width().max(0.0);
        let weight_ceiling = self.max_cell_weight();
        let nearest_weight = self.cell_weight(nearest_id);
        let zero_span = width + (width * width + weight_ceiling).sqrt();
        let cutoff = nearest_distance + 2.0 * zero_span;
        let mut candidates = CandidateSet::new();
        self.index
            .candidates_within(&self.cells, q, cutoff, &mut candidates);
        let count = candidates.len();
        if count == 0 {
            out.push((nearest_id, 1.0));
            return;
        }
        out.reserve(count);

        for slot_i in 0..count {
            let i = candidates.get(slot_i).0;
            let core_span =
                width + (width * width + (self.cell_weight(i) - nearest_weight).max(0.0)).sqrt();
            if self.distance(q, self.cells[i].center) > nearest_distance + core_span {
                candidates.set_weight(slot_i, 0.0);
                continue;
            }
            for slot_j in 0..count {
                if slot_i == slot_j {
                    continue;
                }
                let j = candidates.get(slot_j).0;
                let pair_width = self.blend_width(i, j);
                let delta = self.delta(self.cells[i].center, self.cells[j].center);
                let length = delta.length();
                if length <= MIN_CENTRE_DISTANCE {
                    continue;
                }
                let normal = delta / length;
                let shift =
                    (self.cell_weight(i) - self.cell_weight(j)) / (2.0 * length * length);
                let bisector = self.cells[i].center + delta * (0.5 + shift);
                let side = self.delta(bisector, q).dot(normal);
                let response = if pair_width <= HARD_BOUNDARY_WIDTH {
                    if side < 0.0 {
                        1.0
                    } else if side > 0.0 {
                        0.0
                    } else {
                        0.5
                    }
                } else {
                    smooth_response(-side / pair_width)
                };
                candidates.multiply_weight(slot_i, response);
            }
        }

        let mut total = 0.0f32;
        for slot in 0..count {
            total += candidates.get(slot).1;
        }
        if total <= f32::EPSILON {
            let mut best_slot = 0usize;
            let mut best_distance = f32::INFINITY;
            for slot in 0..count {
                let id = candidates.get(slot).0;
                let distance = self.distance(q, self.cells[id].center);
                if distance < best_distance {
                    best_distance = distance;
                    best_slot = slot;
                }
            }
            out.push((candidates.get(best_slot).0, 1.0));
            return;
        }

        for slot in 0..count {
            let (id, weight) = candidates.get(slot);
            if weight > MIN_BLEND_WEIGHT {
                out.push((id, weight / total));
            }
        }
    }

    pub fn vertex_cache(&self, dim: usize, size: f32, topology: WorldTopology) -> Arc<VertexBlendCache> {
        let key = Self::cache_key(dim, size, topology);
        if let Some(cache) = self.vertex_caches.lock().unwrap().get(&key).cloned() {
            return cache;
        }
        let cache = Arc::new(self.build_vertex_cache(dim, size, topology));
        self.vertex_caches.lock().unwrap().insert(key, cache.clone());
        cache
    }

    fn cache_key(dim: usize, size: f32, topology: WorldTopology) -> (usize, u32, u8) {
        let topology_bits = match topology {
            WorldTopology::Finite => 0u8,
            WorldTopology::FiniteTorus => 1u8,
            WorldTopology::Infinite => 2u8,
        };
        (dim, size.to_bits(), topology_bits)
    }

    fn build_vertex_cache(
        &self,
        dim: usize,
        size: f32,
        topology: WorldTopology,
    ) -> VertexBlendCache {
        let dim = dim.max(2);
        let spacing = if topology.wraps() {
            size / dim as f32
        } else {
            size / (dim - 1).max(1) as f32
        };
        let half = size * 0.5;
        let mut cell_ids = Vec::with_capacity(dim * dim);
        let mut offsets = Vec::with_capacity(dim * dim + 1);
        let mut weights = Vec::new();
        let mut scratch: Vec<(usize, f32)> = Vec::new();
        for z in 0..dim {
            for x in 0..dim {
                let point = Vec2::new(-half + x as f32 * spacing, -half + z as f32 * spacing);
                cell_ids.push(self.cell_at(point));
                offsets.push(weights.len());
                self.cell_weights_into(point, &mut scratch);
                weights.extend(scratch.iter().copied());
            }
        }
        offsets.push(weights.len());
        VertexBlendCache {
            cell_ids,
            offsets,
            weights,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(topology: WorldTopology, dim: u32) -> VoronoiConfig {
        VoronoiConfig {
            size: 200.0,
            grid_dim: dim,
            topology,
            default_blend_width: 8.0,
            blend_min: 4.0,
            blend_max: 12.0,
            jitter_min: 0.15,
            jitter_max: 0.85,
            lloyd_passes: 2,
            warp_amplitude: 3.0,
        }
    }

    fn signed_area(polygon: &[Vec2]) -> f32 {
        let mut area = 0.0f32;
        for index in 0..polygon.len() {
            let a = polygon[index];
            let b = polygon[(index + 1) % polygon.len()];
            area += a.x * b.y - b.x * a.y;
        }
        area * 0.5
    }

    fn sample_grid(size: f32, count: usize) -> Vec<Vec2> {
        let mut points = Vec::with_capacity(count * count);
        for gx in 0..count {
            for gz in 0..count {
                points.push(Vec2::new(
                    -size * 0.5 + (gx as f32 + 0.5) * size / count as f32,
                    -size * 0.5 + (gz as f32 + 0.5) * size / count as f32,
                ));
            }
        }
        points
    }

    fn brute_cell_at(layout: &VoronoiLayout, p: Vec2) -> Option<usize> {
        let q = layout.warped_point(p);
        let mut best: Option<(f32, usize)> = None;
        for cell in layout.cells() {
            let power = layout.power_distance(q, cell.id);
            if best.is_none_or(|(bp, _)| power < bp) {
                best = Some((power, cell.id));
            }
        }
        best.map(|(_, id)| id)
    }

    fn brute_cell_weights(layout: &VoronoiLayout, p: Vec2) -> Vec<(usize, f32)> {
        let cells = layout.cells();
        if cells.is_empty() {
            return Vec::new();
        }
        let q = layout.warped_point(p);
        let mut weights = vec![1.0f32; cells.len()];
        for i in 0..cells.len() {
            for j in 0..cells.len() {
                if i == j {
                    continue;
                }
                let width = layout.blend_width(i, j);
                let delta = layout.delta(cells[i].center, cells[j].center);
                let length = delta.length();
                if length <= 1e-6 {
                    continue;
                }
                let normal = delta / length;
                let shift =
                    (layout.cell_weight(i) - layout.cell_weight(j)) / (2.0 * length * length);
                let bisector = cells[i].center + delta * (0.5 + shift);
                let side = layout.delta(bisector, q).dot(normal);
                let response = if width <= 1e-5 {
                    if side < 0.0 {
                        1.0
                    } else if side > 0.0 {
                        0.0
                    } else {
                        0.5
                    }
                } else {
                    smooth_response(-side / width)
                };
                weights[i] *= response;
            }
        }
        let total: f32 = weights.iter().sum();
        if total <= f32::EPSILON {
            let mut best = (0usize, f32::INFINITY);
            for cell in cells {
                let d = layout.distance(q, cell.center);
                if d < best.1 {
                    best = (cell.id, d);
                }
            }
            return vec![(best.0, 1.0)];
        }
        let mut out: Vec<(usize, f32)> = cells
            .iter()
            .zip(weights.iter())
            .filter(|(_, w)| **w > 1e-6)
            .map(|(cell, w)| (cell.id, w / total))
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    fn sorted_weights(weights: &[(usize, f32)]) -> Vec<(usize, f32)> {
        let mut out = weights.to_vec();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    fn min_generator_spacing(layout: &VoronoiLayout) -> f32 {
        let cells = layout.cells();
        let mut min = f32::INFINITY;
        for i in 0..cells.len() {
            for j in i + 1..cells.len() {
                min = min.min(layout.distance(cells[i].center, cells[j].center));
            }
        }
        min
    }

    fn edge_owner(
        layout: &VoronoiLayout,
        cell: &VoronoiCellData,
        from: Vec2,
        to: Vec2,
    ) -> Option<usize> {
        let eps = 1e-2;
        for &n in &cell.neighbours {
            let other = &layout.cells()[n];
            let delta = layout.delta(cell.center, other.center);
            let length = delta.length();
            if length <= 1e-6 {
                continue;
            }
            let normal = delta / length;
            let bisector = cell.center + delta * 0.5;
            if (from - bisector).dot(normal).abs() <= eps
                && (to - bisector).dot(normal).abs() <= eps
            {
                return Some(n);
            }
        }
        None
    }

    #[test]
    fn layout_produces_one_cell_per_generator() {
        let layout = VoronoiLayout::generate(WorldSeed::new(7), config(WorldTopology::Finite, 6));
        assert_eq!(layout.cells().len(), 36);
        for cell in layout.cells() {
            let half = 100.0;
            assert!(cell.center.x >= -half && cell.center.x <= half);
            assert!(cell.center.y >= -half && cell.center.y <= half);
        }
    }

    #[test]
    fn interior_cells_have_several_neighbours() {
        let layout = VoronoiLayout::generate(WorldSeed::new(11), config(WorldTopology::Finite, 8));
        let counts: Vec<usize> = layout.cells().iter().map(|c| c.neighbours.len()).collect();
        assert!(
            counts.iter().any(|c| *c >= 5),
            "voronoi cells should average around six neighbours: {counts:?}"
        );
        assert!(counts.iter().all(|c| *c >= 2), "no cell may be isolated: {counts:?}");
    }

    #[test]
    fn adjacency_is_symmetric() {
        let layout = VoronoiLayout::generate(WorldSeed::new(21), config(WorldTopology::Finite, 7));
        for cell in layout.cells() {
            for &n in &cell.neighbours {
                let other = layout.cell(n).expect("neighbour id must exist");
                assert!(
                    other.neighbours.contains(&cell.id),
                    "cell {} lists {} but not the reverse",
                    cell.id,
                    n
                );
            }
        }
    }

    #[test]
    fn torus_cells_wrap_to_a_full_neighbour_ring() {
        let layout = VoronoiLayout::generate(WorldSeed::new(5), config(WorldTopology::FiniteTorus, 6));
        assert_eq!(layout.cells().len(), 36);
        for cell in layout.cells() {
            assert!(
                cell.neighbours.len() >= 5,
                "on a torus every cell has a full ring, cell {} has {:?}",
                cell.id,
                cell.neighbours.len()
            );
        }
        assert!(
            layout.boundaries().iter().any(|b| b.wraps),
            "a periodic layout must report wrapped boundaries"
        );
    }

    #[test]
    fn boundary_blend_widths_are_symmetric_and_in_range() {
        let layout = VoronoiLayout::generate(WorldSeed::new(3), config(WorldTopology::Finite, 6));
        assert!(!layout.boundaries().is_empty());
        for boundary in layout.boundaries() {
            let forward = layout.blend_width(boundary.cell_a, boundary.cell_b);
            let backward = layout.blend_width(boundary.cell_b, boundary.cell_a);
            assert_eq!(forward, backward, "blend width must be symmetric");
            assert!((4.0..=12.0).contains(&forward), "blend out of rolled range: {forward}");
        }
    }

    #[test]
    fn weights_are_normalised_and_peak_at_a_cell_centre() {
        let layout = VoronoiLayout::generate(WorldSeed::new(9), config(WorldTopology::Finite, 6));
        let cell = layout.cells()[4].clone();
        let weights = layout.cell_weights_at(cell.center);
        let total: f32 = weights.iter().map(|(_, w)| w).sum();
        assert!((total - 1.0).abs() < 1e-4, "weights must sum to 1, got {total}");
        let dominant = weights.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        assert_eq!(dominant.0, cell.id, "a cell centre must be dominated by its own cell");
        assert!(dominant.1 > 0.9, "centre weight should be near one, got {}", dominant.1);
    }

    #[test]
    fn weights_blend_smoothly_across_a_boundary() {
        let layout = VoronoiLayout::generate(WorldSeed::new(13), config(WorldTopology::Finite, 4));
        let boundary = *layout
            .boundaries()
            .iter()
            .filter(|b| b.blend_width > 2.0)
            .next()
            .expect("a layout must have blended boundaries");

        let on_a = layout.cell_weights_at(layout.cells()[boundary.cell_a].center);
        let on_b = layout.cell_weights_at(layout.cells()[boundary.cell_b].center);
        assert!(on_a.iter().any(|(id, _)| *id == boundary.cell_a));
        assert!(on_b.iter().any(|(id, _)| *id == boundary.cell_b));

        let mut previous: Option<f32> = None;
        for step in 0..=10 {
            let t = step as f32 / 10.0;
            let a = layout.cells()[boundary.cell_a].center;
            let b = layout.cells()[boundary.cell_b].center;
            let point = a.lerp(b, t);
            let weight_a = layout
                .cell_weights_at(point)
                .iter()
                .find(|(id, _)| *id == boundary.cell_a)
                .map(|(_, w)| *w)
                .unwrap_or(0.0);
            if let Some(prev) = previous {
                assert!(
                    weight_a <= prev + 1e-5,
                    "weight of cell a must fall monotonically toward b: {prev} -> {weight_a}"
                );
            }
            previous = Some(weight_a);
        }
    }

    #[test]
    fn zero_blend_width_gives_hard_nearest_cell_selection() {
        let mut cfg = config(WorldTopology::Finite, 5);
        cfg.default_blend_width = 0.0;
        cfg.blend_min = 0.0;
        cfg.blend_max = 0.0;
        let layout = VoronoiLayout::generate(WorldSeed::new(17), cfg);
        for sample in [Vec2::new(-33.0, 12.0), Vec2::new(5.0, -77.0), Vec2::new(61.0, 40.0)] {
            let weights = layout.cell_weights_at(sample);
            assert_eq!(weights.len(), 1, "hard selection must yield exactly one cell");
            assert_eq!(weights[0].0, layout.cell_at(sample).unwrap());
            assert!((weights[0].1 - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn weights_are_torus_continuous_across_the_seam() {
        let layout = VoronoiLayout::generate(WorldSeed::new(23), config(WorldTopology::FiniteTorus, 6));
        let seam = Vec2::new(99.9, 0.0);
        let across = Vec2::new(-99.9, 0.0);
        let left = layout.cell_weights_at(seam);
        let right = layout.cell_weights_at(across);
        let shared: Vec<usize> = left
            .iter()
            .filter(|(_, w)| *w > 0.05)
            .filter_map(|(id, _)| right.iter().any(|(rid, _)| rid == id).then_some(*id))
            .collect();
        assert!(
            !shared.is_empty(),
            "weights must not reset at the seam: {left:?} vs {right:?}"
        );
    }

    #[test]
    fn generation_is_deterministic() {
        let a = VoronoiLayout::generate(WorldSeed::new(77), config(WorldTopology::FiniteTorus, 5));
        let b = VoronoiLayout::generate(WorldSeed::new(77), config(WorldTopology::FiniteTorus, 5));
        assert_eq!(a.cells().len(), b.cells().len());
        for (x, y) in a.cells().iter().zip(b.cells()) {
            assert_eq!(x.center, y.center);
            assert_eq!(x.neighbours, y.neighbours);
            assert_eq!(x.polygon, y.polygon);
        }
        assert_eq!(a.boundaries().len(), b.boundaries().len());
    }

    #[test]
    fn cell_at_matches_the_nearest_generator_under_wrapping() {
        let layout = VoronoiLayout::generate(WorldSeed::new(31), config(WorldTopology::FiniteTorus, 6));
        for cell in layout.cells() {
            assert_eq!(layout.cell_at(cell.center), Some(cell.id));
        }
    }

    #[test]
    fn accelerated_cell_at_matches_a_brute_force_scan() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let layout = VoronoiLayout::generate(WorldSeed::new(61), config(topology, 6));
            for p in sample_grid(200.0, 24) {
                let fast = layout.cell_at(p);
                let slow = brute_cell_at(&layout, p);
                if fast != slow {
                    let df = layout.distance(p, layout.cells()[fast.unwrap()].center);
                    let ds = layout.distance(p, layout.cells()[slow.unwrap()].center);
                    assert!(
                        (df - ds).abs() < 1e-3,
                        "accelerated query disagreed at {p:?}: {fast:?} ({df}) vs {slow:?} ({ds})"
                    );
                }
            }
        }
    }

    #[test]
    fn accelerated_weights_match_a_brute_force_scan() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            for cells_per_side in [6u32, 11] {
                let layout =
                    VoronoiLayout::generate(WorldSeed::new(67), config(topology, cells_per_side));
                for p in sample_grid(200.0, 24) {
                    let fast = sorted_weights(&layout.cell_weights_at(p));
                    let slow = brute_cell_weights(&layout, p);
                    let mut ids: Vec<usize> = fast.iter().map(|(id, _)| *id).collect();
                    ids.extend(slow.iter().map(|(id, _)| *id));
                    ids.sort_unstable();
                    ids.dedup();
                    for id in ids {
                        let a = fast
                            .iter()
                            .find(|(fid, _)| *fid == id)
                            .map(|(_, w)| *w)
                            .unwrap_or(0.0);
                        let b = slow
                            .iter()
                            .find(|(sid, _)| *sid == id)
                            .map(|(_, w)| *w)
                            .unwrap_or(0.0);
                        assert!(
                            (a - b).abs() < 1e-6,
                            "{topology:?} dim {cells_per_side} weight mismatch at {p:?}: \
                             cell {id} {a} vs {b}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cached_vertex_weights_match_direct_queries() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let layout = VoronoiLayout::generate(WorldSeed::new(71), config(topology, 6));
            let cache = layout.vertex_cache(24, 200.0, topology);
            assert_eq!(cache.cell_ids.len(), 24 * 24);
            let spacing = if topology.wraps() {
                200.0 / 24.0
            } else {
                200.0 / 23.0
            };
            for index in 0..cache.cell_ids.len() {
                let point = Vec2::new(
                    -100.0 + (index % 24) as f32 * spacing,
                    -100.0 + (index / 24) as f32 * spacing,
                );
                assert_eq!(cache.cell_at(index), layout.cell_at(point), "cell at {point:?}");
                let direct = layout.cell_weights_at(point);
                let cached = cache.weights_at(index);
                assert_eq!(direct.len(), cached.len(), "weight count at {point:?}");
                for ((di, dw), (ci, cw)) in direct.iter().zip(cached.iter()) {
                    assert_eq!(di, ci);
                    assert!((dw - cw).abs() < 1e-6, "weight at {point:?}");
                }
            }
        }
    }

    #[test]
    fn lloyd_relaxation_improves_the_smallest_generator_distance() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let mut jittered = config(topology, 6);
            jittered.lloyd_passes = 0;
            let mut relaxed = config(topology, 6);
            relaxed.lloyd_passes = 2;
            let before = VoronoiLayout::generate(WorldSeed::new(83), jittered);
            let after = VoronoiLayout::generate(WorldSeed::new(83), relaxed);
            assert!(
                min_generator_spacing(&after) > min_generator_spacing(&before),
                "{topology:?}: relaxation must widen the closest pair: {} -> {}",
                min_generator_spacing(&before),
                min_generator_spacing(&after)
            );
            let half = 100.0;
            for cell in after.cells() {
                assert!(
                    cell.center.x >= -half && cell.center.x <= half
                        && cell.center.y >= -half && cell.center.y <= half,
                    "relaxed generator {} escaped the world: {:?}",
                    cell.id,
                    cell.center
                );
            }
            if topology == WorldTopology::FiniteTorus {
                assert!(
                    after.boundaries().iter().any(|b| b.wraps),
                    "a relaxed torus layout must stay periodic"
                );
                for cell in after.cells() {
                    assert!(
                        cell.neighbours.len() >= 5,
                        "relaxed torus cell {} lost its ring",
                        cell.id
                    );
                }
            }
        }
    }

    #[test]
    fn polygons_are_closed_and_consistently_wound() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let layout = VoronoiLayout::generate(WorldSeed::new(91), config(topology, 6));
            for cell in layout.cells() {
                assert!(cell.polygon.len() >= 3, "cell {} has a degenerate polygon", cell.id);
                let area = signed_area(&cell.polygon);
                assert!(
                    area > 1e-3,
                    "cell {} polygon must be closed and CCW, area {area}",
                    cell.id
                );
            }
        }
    }

    #[test]
    fn polygons_tile_the_world_area() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let layout = VoronoiLayout::generate(WorldSeed::new(97), config(topology, 6));
            let total: f32 = layout
                .cells()
                .iter()
                .map(|cell| signed_area(&cell.polygon).abs())
                .sum();
            let world = 200.0 * 200.0;
            assert!(
                (total - world).abs() < world * 1e-3,
                "{topology:?} polygons must tile the world: {total} vs {world}"
            );
        }
    }

    #[test]
    fn shared_edges_appear_once_per_neighbour_pair() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let layout = VoronoiLayout::generate(WorldSeed::new(101), config(topology, 6));
            let mut pair_count: HashMap<(usize, usize), usize> = HashMap::new();
            for boundary in layout.boundaries() {
                *pair_count.entry(pair_key(boundary.cell_a, boundary.cell_b)).or_default() += 1;
                assert!(
                    boundary.p0 != boundary.p1,
                    "boundary {}|{} has a degenerate edge",
                    boundary.cell_a,
                    boundary.cell_b
                );
                let delta = layout.delta(
                    layout.cells()[boundary.cell_a].center,
                    layout.cells()[boundary.cell_b].center,
                );
                let normal = delta / delta.length();
                let bisector = layout.cells()[boundary.cell_a].center + delta * 0.5;
                for p in [boundary.p0, boundary.p1] {
                    assert!(
                        (p - bisector).dot(normal).abs() < 1e-2,
                        "endpoint {p:?} of {}|{} is off the bisector",
                        boundary.cell_a,
                        boundary.cell_b
                    );
                    assert!(
                        layout.cells()[boundary.cell_a]
                            .polygon
                            .iter()
                            .any(|v| (p - *v).length() < 1e-3),
                        "endpoint {p:?} of {}|{} is not a vertex of cell {}",
                        boundary.cell_a,
                        boundary.cell_b,
                        boundary.cell_a
                    );
                }
            }
            for cell in layout.cells() {
                for &n in &cell.neighbours {
                    assert!(
                        pair_count.contains_key(&pair_key(cell.id, n)),
                        "neighbour pair {}|{} has no boundary edge",
                        cell.id,
                        n
                    );
                }
            }
            assert_eq!(pair_count.len(), layout.boundaries().len());
        }
    }

    #[test]
    fn torus_faces_wrap_into_canonical_space() {
        let layout = VoronoiLayout::generate(WorldSeed::new(103), config(WorldTopology::FiniteTorus, 6));
        let half = 100.0;
        for cell in layout.cells() {
            assert!(
                cell.center.x >= -half && cell.center.x < half
                    && cell.center.y >= -half && cell.center.y < half,
                "generator {} is not canonical: {:?}",
                cell.id,
                cell.center
            );
            for vertex in &cell.polygon {
                assert!(
                    layout.distance(cell.center, *vertex) <= layout.config.size,
                    "vertex {vertex:?} of cell {} escapes one period",
                    cell.id
                );
            }
        }
        assert!(layout.boundaries().iter().any(|b| b.wraps));
        for boundary in layout.boundaries() {
            let span = (boundary.p1 - boundary.p0).length();
            assert!(span <= 100.0, "shared edge spans {span}: not a minimal lift");
            if boundary.wraps {
                assert!(
                    boundary.midpoint.x.abs() <= 150.0 && boundary.midpoint.y.abs() <= 150.0,
                    "wrapping midpoint {:?} is more than one period out",
                    boundary.midpoint
                );
                assert!(
                    [boundary.p0, boundary.p1]
                        .iter()
                        .any(|p| p.x.abs() > half || p.y.abs() > half),
                    "wrapping edge {}|{} never crosses the seam",
                    boundary.cell_a,
                    boundary.cell_b
                );
            } else {
                for p in [boundary.p0, boundary.p1] {
                    assert!(
                        p.x >= -half - 1e-2 && p.x <= half + 1e-2
                            && p.y >= -half - 1e-2 && p.y <= half + 1e-2,
                        "non-wrapping edge {}|{} leaves the canonical box at {p:?}",
                        boundary.cell_a,
                        boundary.cell_b
                    );
                }
            }
        }
    }

    #[test]
    fn weights_form_a_partition_of_unity_everywhere() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let layout = VoronoiLayout::generate(WorldSeed::new(107), config(topology, 6));
            for p in sample_grid(200.0, 20) {
                let total: f32 = layout.cell_weights_at(p).iter().map(|(_, w)| w).sum();
                assert!(
                    (total - 1.0).abs() < 1e-4,
                    "weights must sum to 1 at {p:?}, got {total}"
                );
            }
        }
        let torus = VoronoiLayout::generate(WorldSeed::new(109), config(WorldTopology::FiniteTorus, 6));
        for gz in 0..20 {
            let y = -95.0 + gz as f32 * 9.5;
            for x in [99.9, -99.9, 99.99, -99.99] {
                let total: f32 = torus
                    .cell_weights_at(Vec2::new(x, y))
                    .iter()
                    .map(|(_, w)| w)
                    .sum();
                assert!(
                    (total - 1.0).abs() < 1e-4,
                    "seam weights must sum to 1 at ({x}, {y}), got {total}"
                );
            }
        }
    }

    #[test]
    fn weights_are_c1_across_a_triple_junction_with_mismatched_widths() {
        let mut cfg = config(WorldTopology::Finite, 6);
        cfg.blend_min = 4.0;
        cfg.blend_max = 16.0;
        let layout = VoronoiLayout::generate(WorldSeed::new(113), cfg);
        let half = 100.0;
        let mut junction: Option<(usize, Vec2)> = None;
        'cells: for cell in layout.cells() {
            let count = cell.polygon.len();
            for k in 0..count {
                let vertex = cell.polygon[k];
                if vertex.x.abs() > half - 2.0 || vertex.y.abs() > half - 2.0 {
                    continue;
                }
                let prev = cell.polygon[(k + count - 1) % count];
                let next = cell.polygon[(k + 1) % count];
                let (Some(a), Some(b)) = (
                    edge_owner(&layout, cell, prev, vertex),
                    edge_owner(&layout, cell, vertex, next),
                ) else {
                    continue;
                };
                let widths = [
                    layout.blend_width(cell.id, a),
                    layout.blend_width(cell.id, b),
                    layout.blend_width(a, b),
                ];
                let widest = widths.iter().fold(0.0f32, |acc, w| acc.max(*w));
                let narrowest = widths.iter().fold(f32::MAX, |acc, w| acc.min(*w));
                if widest - narrowest > 4.0 {
                    junction = Some((cell.id, vertex));
                    break 'cells;
                }
            }
        }
        let (cell_id, point) =
            junction.expect("layout must contain a triple junction with mismatched blend widths");
        let direction = (point - layout.cells()[cell_id].center).normalize_or(Vec2::X);
        let samples = 961usize;
        let extent = 36.0f32;
        let step = 2.0 * extent / (samples - 1) as f32;
        let mut series: Vec<f32> = Vec::with_capacity(samples);
        for s in 0..samples {
            let t = -extent + s as f32 * step;
            let p = point + direction * t;
            let weights = layout.cell_weights_at(p);
            let total: f32 = weights.iter().map(|(_, w)| w).sum();
            assert!((total - 1.0).abs() < 1e-4, "partition of unity broken at t={t}: {total}");
            series.push(
                weights
                    .iter()
                    .find(|(id, _)| *id == cell_id)
                    .map(|(_, w)| *w)
                    .unwrap_or(0.0),
            );
        }
        let derivatives: Vec<f32> = (1..samples - 1)
            .map(|s| (series[s + 1] - series[s - 1]) / (2.0 * step))
            .collect();
        let mut worst = 0.0f32;
        for pair in derivatives.windows(2) {
            worst = worst.max((pair[1] - pair[0]).abs());
        }
        assert!(
            worst < 0.05,
            "weight derivative jumps by {worst} across the junction: blend is not C1"
        );
    }

    #[test]
    fn cell_at_resolves_points_far_outside_a_finite_world() {
        let layout = VoronoiLayout::generate(WorldSeed::new(131), config(WorldTopology::Finite, 6));
        for point in [
            Vec2::new(1000.0, 0.0),
            Vec2::new(-1000.0, 0.0),
            Vec2::new(0.0, 1000.0),
            Vec2::new(0.0, -1000.0),
            Vec2::new(1000.0, 1000.0),
        ] {
            assert!(layout.cell_at(point).is_some(), "no cell found for {point:?}");
            assert!(
                !layout.cell_weights_at(point).is_empty(),
                "no weights found for {point:?}"
            );
        }
    }

    #[test]
    fn cell_at_resolves_points_far_outside_a_torus() {
        let layout = VoronoiLayout::generate(WorldSeed::new(137), config(WorldTopology::FiniteTorus, 6));
        for point in [
            Vec2::new(1000.0, 0.0),
            Vec2::new(-1000.0, 0.0),
            Vec2::new(0.0, 1000.0),
            Vec2::new(0.0, -1000.0),
        ] {
            assert!(layout.cell_at(point).is_some(), "no cell found for {point:?}");
        }
    }

    #[test]
    fn boundary_seed_keys_survive_large_cell_grids() {
        assert_ne!(boundary_seed_key(1, 0), boundary_seed_key(0, 4096));
        assert_ne!(boundary_seed_key(0, 4096), boundary_seed_key(0, 4097));
        let mut seen = std::collections::HashSet::new();
        for a in [0usize, 1, 4095, 4096, 65535] {
            for b in [0usize, 1, 4095, 4096, 65535] {
                assert!(
                    seen.insert(boundary_seed_key(a, b)),
                    "keys ({a}, {b}) collide with an earlier pair"
                );
            }
        }
    }

    fn candidate_count_at(layout: &VoronoiLayout, p: Vec2) -> usize {
        let q = layout.warped_point(p);
        let Some((nearest, _)) = layout.nearest_cell(q) else {
            return 0;
        };
        let width = layout.max_blend_width().max(0.0);
        let zero_span = width + (width * width + layout.max_cell_weight()).sqrt();
        let mut set = CandidateSet::new();
        layout
            .index
            .candidates_within(layout.cells(), q, nearest + 2.0 * zero_span, &mut set);
        set.len()
    }

    #[test]
    fn measured_max_candidate_count_fits_inline_capacity() {
        let mut worst = 0usize;
        let mut worst_at = (WorldTopology::Finite, 0u32, 0.0f32, Vec2::ZERO);
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let cases: [(f32, u32, f32); 5] = [
                (200.0, 6, 12.0),
                (200.0, 11, 12.0),
                (400.0, 10, 16.0),
                (120.0, 4, 16.0),
                (200.0, 6, 16.0),
            ];
            for (size, dim, blend_max) in cases {
                let cfg = VoronoiConfig {
                    size,
                    grid_dim: dim,
                    topology,
                    blend_max,
                    ..config(topology, dim)
                };
                let layout = VoronoiLayout::generate(WorldSeed::new(173), cfg);
                for p in sample_grid(size, 48) {
                    let count = candidate_count_at(&layout, p);
                    if count > worst {
                        worst = count;
                        worst_at = (topology, dim, size, p);
                    }
                }
                if topology == WorldTopology::Finite {
                    let half = size * 0.5;
                    for offset in [0.0, 40.0, 800.0] {
                        for sx in [-1.0, 1.0] {
                            for sz in [-1.0, 1.0] {
                                let p = Vec2::new(sx * (half + offset), sz * (half + offset));
                                let count = candidate_count_at(&layout, p);
                                if count > worst {
                                    worst = count;
                                    worst_at = (topology, dim, size, p);
                                }
                            }
                        }
                    }
                }
            }
            for (size, dim, blend_max) in [(200.0f32, 6u32, 12.0f32), (400.0, 10, 16.0)] {
                let cfg = VoronoiConfig {
                    size,
                    grid_dim: dim,
                    topology,
                    blend_max,
                    ..config(topology, dim)
                };
                let base = VoronoiLayout::generate(WorldSeed::new(173), cfg);
                let spacing = size / dim as f32;
                let weights: Vec<f32> = (0..base.cells().len())
                    .map(|index| {
                        let t = ((index as f64 + 1.0) * 0.6180339887).fract();
                        (t * 0.5) as f32 * spacing * spacing
                    })
                    .collect();
                let layout = base.with_cell_weights(&weights);
                for p in sample_grid(size, 48) {
                    let count = candidate_count_at(&layout, p);
                    if count > worst {
                        worst = count;
                        worst_at = (topology, dim, size, p);
                    }
                }
            }
        }
        println!("measured max candidate count {worst} at {worst_at:?}");
        assert!(
            worst <= INLINE_CANDIDATES,
            "measured max candidate count {worst} exceeds inline capacity {INLINE_CANDIDATES}"
        );
    }

    #[test]
    fn dense_sweep_weights_match_brute_force_within_shipped_tolerance() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let layout = VoronoiLayout::generate(WorldSeed::new(179), config(topology, 6));
            for p in sample_grid(200.0, 40) {
                let fast = sorted_weights(&layout.cell_weights_at(p));
                let slow = brute_cell_weights(&layout, p);
                let mut ids: Vec<usize> = fast.iter().map(|(id, _)| *id).collect();
                ids.extend(slow.iter().map(|(id, _)| *id));
                ids.sort_unstable();
                ids.dedup();
                for id in ids {
                    let a = fast
                        .iter()
                        .find(|(fid, _)| *fid == id)
                        .map(|(_, w)| *w)
                        .unwrap_or(0.0);
                    let b = slow
                        .iter()
                        .find(|(sid, _)| *sid == id)
                        .map(|(_, w)| *w)
                        .unwrap_or(0.0);
                    assert!(
                        (a - b).abs() < 1.2e-7,
                        "{topology:?} dense sweep mismatch at {p:?}: cell {id} {a} vs {b}"
                    );
                }
            }
        }
    }

    #[test]
    fn vertex_cache_returns_the_same_entry_for_a_repeated_key() {
        let layout = VoronoiLayout::generate(WorldSeed::new(181), config(WorldTopology::Finite, 6));
        let first = layout.vertex_cache(24, 200.0, WorldTopology::Finite);
        let second = layout.vertex_cache(24, 200.0, WorldTopology::Finite);
        assert!(
            Arc::ptr_eq(&first, &second),
            "a repeated key must reuse the cached entry"
        );
    }

    #[test]
    fn vertex_cache_keeps_a_separate_entry_per_key() {
        let layout = VoronoiLayout::generate(WorldSeed::new(191), config(WorldTopology::Finite, 6));
        let a = layout.vertex_cache(16, 200.0, WorldTopology::Finite);
        let b = layout.vertex_cache(24, 200.0, WorldTopology::Finite);
        let c = layout.vertex_cache(16, 200.0, WorldTopology::FiniteTorus);
        assert!(!Arc::ptr_eq(&a, &b), "distinct dims must not share an entry");
        assert!(!Arc::ptr_eq(&a, &c), "distinct topologies must not share an entry");
        assert!(!Arc::ptr_eq(&b, &c), "distinct keys must not share an entry");
        let a_again = layout.vertex_cache(16, 200.0, WorldTopology::Finite);
        let b_again = layout.vertex_cache(24, 200.0, WorldTopology::Finite);
        let c_again = layout.vertex_cache(16, 200.0, WorldTopology::FiniteTorus);
        assert!(Arc::ptr_eq(&a, &a_again), "key a must stay cached after other keys");
        assert!(Arc::ptr_eq(&b, &b_again), "key b must stay cached after other keys");
        assert!(Arc::ptr_eq(&c, &c_again), "key c must stay cached after other keys");
    }

    #[test]
    fn uniform_weights_reproduce_the_unweighted_partition() {
        let plain = VoronoiLayout::generate(WorldSeed::new(211), config(WorldTopology::Finite, 6));
        let uniform = plain.with_cell_weights(&vec![37.0; plain.cells().len()]);
        assert_eq!(uniform.max_cell_weight(), 37.0);
        for cell in plain.cells() {
            let before = &cell.polygon;
            let after = &uniform.cells()[cell.id].polygon;
            assert_eq!(before.len(), after.len(), "cell {} shape drifted", cell.id);
            for (x, y) in before.iter().zip(after) {
                assert!((x - y).length() < 1e-3, "cell {} vertex drifted", cell.id);
            }
        }
        for p in sample_grid(200.0, 24) {
            assert_eq!(plain.cell_at(p), uniform.cell_at(p));
        }
    }

    #[test]
    fn heavy_cells_grow_their_power_cells() {
        let plain = VoronoiLayout::generate(WorldSeed::new(223), config(WorldTopology::Finite, 6));
        let mut weights = vec![0.0f32; plain.cells().len()];
        weights[0] = 900.0;
        let weighted = plain.with_cell_weights(&weights);
        let before = signed_area(&plain.cells()[0].polygon).abs();
        let after = signed_area(&weighted.cells()[0].polygon).abs();
        assert!(
            after > before * 1.15,
            "a heavy cell must claim more area: {before} -> {after}"
        );
        let mut flipped = 0;
        for p in sample_grid(200.0, 40) {
            if plain.cell_at(p) != weighted.cell_at(p) {
                flipped += 1;
            }
        }
        assert!(flipped > 0, "weighting must move the partition");
    }

    #[test]
    fn weighted_dense_sweep_matches_brute_force_within_shipped_tolerance() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let base = VoronoiLayout::generate(WorldSeed::new(227), config(topology, 6));
            let weights: Vec<f32> = (0..base.cells().len())
                .map(|index| {
                    let t = ((index as f64 + 1.0) * 0.6180339887).fract();
                    (t * 400.0) as f32
                })
                .collect();
            let layout = base.with_cell_weights(&weights);
            for p in sample_grid(200.0, 40) {
                let fast = sorted_weights(&layout.cell_weights_at(p));
                let slow = brute_cell_weights(&layout, p);
                let mut ids: Vec<usize> = fast.iter().map(|(id, _)| *id).collect();
                ids.extend(slow.iter().map(|(id, _)| *id));
                ids.sort_unstable();
                ids.dedup();
                for id in ids {
                    let a = fast
                        .iter()
                        .find(|(fid, _)| *fid == id)
                        .map(|(_, w)| *w)
                        .unwrap_or(0.0);
                    let b = slow
                        .iter()
                        .find(|(sid, _)| *sid == id)
                        .map(|(_, w)| *w)
                        .unwrap_or(0.0);
                    assert!(
                        (a - b).abs() < 1.2e-7,
                        "{topology:?} weighted dense sweep mismatch at {p:?}: cell {id} {a} vs {b}"
                    );
                }
            }
        }
    }

    #[test]
    fn warp_offset_is_periodic_on_the_torus() {
        let layout =
            VoronoiLayout::generate(WorldSeed::new(229), config(WorldTopology::FiniteTorus, 6));
        let shift = Vec2::new(200.0, -200.0);
        for i in 0..16 {
            let p = Vec2::new(i as f32 * 11.0 - 80.0, i as f32 * 7.0 - 50.0);
            let here = layout.warped_point(p);
            let shifted = layout.warped_point(p + shift);
            assert!(
                (shifted - here - shift).length() < 1e-4,
                "the warp field must be periodic: {here:?} vs {shifted:?}"
            );
        }
    }

    #[test]
    fn warp_curves_the_partition_and_keeps_a_partition_of_unity() {
        let mut straight = config(WorldTopology::Finite, 6);
        straight.warp_amplitude = 0.0;
        let straight = VoronoiLayout::generate(WorldSeed::new(233), straight);
        let curved = VoronoiLayout::generate(WorldSeed::new(233), config(WorldTopology::Finite, 6));
        let mut different = 0;
        for p in sample_grid(200.0, 40) {
            if straight.cell_at(p) != curved.cell_at(p) {
                different += 1;
            }
        }
        assert!(different > 0, "the warp must bend cell borders");
        for p in sample_grid(200.0, 24) {
            let total: f32 = curved.cell_weights_at(p).iter().map(|(_, w)| w).sum();
            assert!(
                (total - 1.0).abs() < 1e-4,
                "warped weights must sum to 1 at {p:?}, got {total}"
            );
        }
    }
}
