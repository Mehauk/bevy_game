use std::collections::{BTreeSet, HashSet, VecDeque};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::{FlowField, Lake, Waterfall};
use crate::terrain::params::{Geothermal, Salinity, Temperature};
use crate::terrain::season::{self, Season};
use crate::terrain::topology::WorldTopology;

pub const RIBBON_LIFT: f32 = 0.06;
pub const BANK_TUCK: f32 = 0.15;
pub const WATER_NONE: f32 = f32::NEG_INFINITY;
pub const FOAM_DEPTH: f32 = 0.8;

const MIN_VISIBLE_LAKE_DEPTH: f32 = 0.05;
const MIN_LAKE_CORNERS_PER_QUAD: usize = 3;
const WATER_UV_TILE_METRES: f32 = 8.0;
const RIVER_FLOW_SPEED_GAIN: f32 = 0.05;
const RIVER_FLOW_SPEED_MIN: f32 = 0.3;
const RIVER_FLOW_SPEED_MAX: f32 = 2.5;
const RIVER_BANK_SKIRT_SPACING: f32 = 0.3;
const BANK_U_EXTENT: f32 = 0.15;
const RIVER_UV_V_TILE_SPACING: f32 = 2.0;
const WATERFALL_LIP_LIFT: f32 = 0.15;
const WATERFALL_PLUNGE_LIFT: f32 = 0.08;
const WATERFALL_LIP_FLOW_SPEED: f32 = 3.0;
const WATERFALL_PLUNGE_FLOW_SPEED: f32 = 1.5;
const LIP_PROBE_AHEAD_SPACING: f32 = 1.5;
const LIP_PROBE_STEP_SPACING: f32 = 0.5;
const LIP_MARGIN_DROP_FRACTION: f32 = 0.1;
const LIP_MARGIN_MIN_METRES: f32 = 0.5;
const LIP_MARGIN_MAX_METRES: f32 = 2.0;
const LIP_MAX_WALK_SPACING: f32 = 8.0;
const CASCADE_TOP_LIFT: f32 = 0.15;
const CASCADE_BOTTOM_LIFT: f32 = 0.05;
const CASCADE_SEGMENTS: usize = 6;
const CASCADE_SPEED_MIN: f32 = 0.5;
const CASCADE_SPEED_MAX: f32 = 6.0;
const CASCADE_FLARE_PER_LENGTH: f32 = 0.5;
const CASCADE_FOAM_LIFT: f32 = 0.08;
const CASCADE_FOAM_RADIUS_FRACTION: f32 = 0.9;
const CASCADE_FOAM_RING_SEGMENTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaterKind {
    Sea,
    Lake,
    River,
    Waterfall,
}

#[derive(Clone, Copy, Debug)]
pub struct WaterSample {
    pub surface_y: f32,
    #[allow(dead_code)]
    pub depth: f32,
    #[allow(dead_code)]
    pub salinity: Salinity,
    pub frozen: bool,
    pub flow: Vec2,
    #[allow(dead_code)]
    pub kind: WaterKind,
}

#[derive(Clone, Debug)]
pub struct WaterBody {
    #[allow(dead_code)]
    pub radius: f32,
    #[allow(dead_code)]
    pub surface: f32,
    pub salinity: Salinity,
    pub frozen: bool,
}

pub fn ocean_connectivity(heights: &ScalarField, sea_top: f32, seeds: &[usize]) -> Vec<bool> {
    let mut mask = vec![false; heights.len()];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for &seed in seeds {
        if seed < heights.len() && heights.get_at(seed) <= sea_top && !mask[seed] {
            mask[seed] = true;
            queue.push_back(seed);
        }
    }
    while let Some(index) = queue.pop_front() {
        let (x, z) = heights.coords(index);
        for (dx, dz) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            let neighbour = heights.index(heights.wrap(x as isize + dx), heights.wrap(z as isize + dz));
            if !mask[neighbour] && heights.get_at(neighbour) <= sea_top {
                mask[neighbour] = true;
                queue.push_back(neighbour);
            }
        }
    }
    mask
}

pub struct WaterCellContext<'a> {
    pub cell_of_vertex: &'a [usize],
    pub temperature: &'a [Temperature],
    pub geothermal: &'a [Geothermal],
}

#[derive(Clone, Debug)]
pub struct WaterField {
    pub surface: ScalarField,
    pub kind: Vec<WaterKind>,
    pub flow: Vec<Vec2>,
    pub salinity: Vec<Salinity>,
    pub frozen: Vec<bool>,
}

impl WaterField {
    pub fn empty(dim: usize, size: f32, topology: WorldTopology) -> Self {
        let mut surface = ScalarField::new(dim, size, topology);
        surface.fill(WATER_NONE);
        let len = surface.len();
        Self {
            surface,
            kind: vec![WaterKind::Sea; len],
            flow: vec![Vec2::ZERO; len],
            salinity: vec![Salinity::Freshwater; len],
            frozen: vec![false; len],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build(
        heights: &ScalarField,
        sea_level: f32,
        ocean: &[bool],
        lakes: &[Lake],
        lake_salinity: &[Salinity],
        river_paths: &[Vec<usize>],
        waterfalls: &[Waterfall],
        accumulation: &ScalarField,
        season: Season,
        ctx: &WaterCellContext,
    ) -> Self {
        let mut field = Self::empty(heights.dim, heights.size, heights.topology);
        for index in 0..heights.len() {
            if !ocean.get(index).copied().unwrap_or(false) || heights.get_at(index) > sea_level {
                continue;
            }
            field.paint(index, sea_level, WaterKind::Sea, Salinity::Marine, Vec2::ZERO, season, ctx);
        }

        for (lake_id, lake) in lakes.iter().enumerate() {
            let salinity = lake_salinity
                .get(lake_id)
                .copied()
                .unwrap_or(Salinity::Freshwater);
            for &vertex in &lake.cells {
                field.paint(
                    vertex,
                    lake.surface,
                    WaterKind::Lake,
                    salinity,
                    Vec2::ZERO,
                    season,
                    ctx,
                );
            }
        }

        for path in river_paths {
            for step in 0..path.len() {
                let vertex = path[step];
                let here = heights.world_of(heights.coords(vertex).0, heights.coords(vertex).1);
                let mut flow = Vec2::ZERO;
                if let Some(&next) = path.get(step + 1) {
                    let there = heights.world_of(heights.coords(next).0, heights.coords(next).1);
                    let delta = heights
                        .topology
                        .delta_xz(here, there, heights.size)
                        .normalize_or_zero();
                    let flow_speed = (accumulation.get_at(vertex).max(0.0).sqrt()
                        * RIVER_FLOW_SPEED_GAIN)
                        .clamp(RIVER_FLOW_SPEED_MIN, RIVER_FLOW_SPEED_MAX);
                    flow = delta * flow_speed;
                }
                field.paint(
                    vertex,
                    heights.get_at(vertex) + RIBBON_LIFT,
                    WaterKind::River,
                    Salinity::Freshwater,
                    flow,
                    season,
                    ctx,
                );
            }
        }

        for fall in waterfalls {
            let delta = heights
                .topology
                .delta_xz(fall.lip, fall.plunge, heights.size)
                .normalize_or_zero();
            field.paint(
                fall.index,
                heights.get_at(fall.index) + WATERFALL_LIP_LIFT,
                WaterKind::Waterfall,
                Salinity::Freshwater,
                delta * WATERFALL_LIP_FLOW_SPEED,
                season,
                ctx,
            );
            field.paint(
                fall.next,
                heights.get_at(fall.next) + WATERFALL_PLUNGE_LIFT,
                WaterKind::Waterfall,
                Salinity::Freshwater,
                delta * WATERFALL_PLUNGE_FLOW_SPEED,
                season,
                ctx,
            );
        }

        field
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        vertex: usize,
        surface: f32,
        kind: WaterKind,
        salinity: Salinity,
        flow: Vec2,
        season: Season,
        ctx: &WaterCellContext,
    ) {
        if vertex >= self.surface.len() {
            return;
        }
        let cell = ctx.cell_of_vertex.get(vertex).copied().unwrap_or(0);
        let temperature = ctx
            .temperature
            .get(cell)
            .copied()
            .unwrap_or(Temperature::Temperate);
        let geothermal = ctx
            .geothermal
            .get(cell)
            .copied()
            .unwrap_or(Geothermal::None);
        self.surface.set_at(vertex, surface);
        self.kind[vertex] = kind;
        self.flow[vertex] = flow;
        self.salinity[vertex] = salinity;
        self.frozen[vertex] = season::is_water_frozen(season, temperature, salinity, geothermal);
    }

    pub fn sample_at(&self, heights: &ScalarField, p: Vec2) -> Option<WaterSample> {
        let spacing = heights.spacing();
        let wrapped = heights.topology.wrap_xz(p, heights.size);
        let gx = (wrapped.x + heights.half()) / spacing;
        let gz = (wrapped.y + heights.half()) / spacing;
        let x0 = gx.floor();
        let z0 = gz.floor();
        let fx = gx - x0;
        let fz = gz - z0;

        let ix0 = heights.wrap(x0 as isize);
        let iz0 = heights.wrap(z0 as isize);
        let ix1 = heights.wrap(x0 as isize + 1);
        let iz1 = heights.wrap(z0 as isize + 1);
        let c00 = heights.index(ix0, iz0);
        let c10 = heights.index(ix1, iz0);
        let c01 = heights.index(ix0, iz1);
        let c11 = heights.index(ix1, iz1);

        let near = if fx < 0.5 {
            if fz < 0.5 {
                c00
            } else {
                c01
            }
        } else if fz < 0.5 {
            c10
        } else {
            c11
        };

        let s00 = self.surface.get_at(c00);
        let s10 = self.surface.get_at(c10);
        let s01 = self.surface.get_at(c01);
        let s11 = self.surface.get_at(c11);
        let surface = if s00.is_finite() && s10.is_finite() && s01.is_finite() && s11.is_finite() {
            let top = s00 + (s10 - s00) * fx;
            let bottom = s01 + (s11 - s01) * fx;
            top + (bottom - top) * fz
        } else {
            self.surface.get_at(near)
        };
        if !surface.is_finite() {
            return None;
        }

        Some(WaterSample {
            surface_y: surface,
            depth: (surface - heights.sample_world(p)).max(0.0),
            salinity: self.salinity[near],
            frozen: self.frozen[near],
            flow: self.flow[near],
            kind: self.kind[near],
        })
    }
}

pub fn lake_planes(
    lakes: &[Lake],
    heights: &ScalarField,
    minimum_area: usize,
    salinities: &[Salinity],
) -> Vec<WaterBody> {
    lakes
        .iter()
        .enumerate()
        .filter(|(_, lake)| lake.cells.len() >= minimum_area && lake.depth > MIN_VISIBLE_LAKE_DEPTH)
        .map(|(index, lake)| {
            let anchor = heights.world_of(
                heights.coords(lake.cells[0]).0,
                heights.coords(lake.cells[0]).1,
            );
            let unwrapped = |cell: usize| -> Vec2 {
                let raw = heights.world_of(heights.coords(cell).0, heights.coords(cell).1);
                anchor + heights.topology.delta_xz(anchor, raw, heights.size)
            };
            let mut centre = Vec2::ZERO;
            for &cell in &lake.cells {
                centre += unwrapped(cell);
            }
            centre /= lake.cells.len().max(1) as f32;
            let radius = lake
                .cells
                .iter()
                .map(|&cell| unwrapped(cell).distance(centre).max(heights.spacing()))
                .fold(0.0f32, f32::max);
            WaterBody {
                radius,
                surface: lake.surface,
                salinity: salinities
                    .get(index)
                    .copied()
                    .unwrap_or(Salinity::Freshwater),
                frozen: false,
            }
        })
        .collect()
}

pub const BRACKISH_SURFACE_MARGIN_METRES: f32 = 1.0;
pub const SALINITY_SAMPLE_STRIDE: usize = 4;

pub fn classify_lake_salinity(
    lakes: &[Lake],
    sea_level: f32,
    cell_of_vertex: &[usize],
    cell_arid: &[bool],
) -> Vec<Salinity> {
    lakes
        .iter()
        .map(|lake| {
            if lake.drains_to_sea {
                return Salinity::Freshwater;
            }
            if lake.surface <= sea_level + BRACKISH_SURFACE_MARGIN_METRES {
                return Salinity::Brackish;
            }
            let mut arid = 0usize;
            let mut sampled = 0usize;
            for &vertex in lake.cells.iter().step_by(SALINITY_SAMPLE_STRIDE) {
                let cell = cell_of_vertex.get(vertex).copied().unwrap_or(0);
                let Some(&arid_cell) = cell_arid.get(cell) else {
                    continue;
                };
                sampled += 1;
                if arid_cell {
                    arid += 1;
                }
            }
            let majority_arid = sampled > 0 && arid * 2 > sampled;
            if majority_arid {
                Salinity::Hypersaline
            } else {
                Salinity::Freshwater
            }
        })
        .collect()
}

pub fn sea_chunk_mesh(
    heights: &ScalarField,
    ocean: &[bool],
    frozen: &[bool],
    sea_level: f32,
    sea_top: f32,
    low: Vec2,
    high: Vec2,
    steps: usize,
    frozen_only: bool,
) -> Option<Mesh> {
    let steps = steps.max(1);
    let stride = steps + 1;
    let step = (high - low) / steps as f32;

    let mut ground = vec![0.0f32; stride * stride];
    let mut wet = vec![false; stride * stride];
    let mut any_wet_corner = false;
    for iz in 0..stride {
        for ix in 0..stride {
            let point = Vec2::new(
                (low.x + ix as f32 * step.x).min(high.x),
                (low.y + iz as f32 * step.y).min(high.y),
            );
            let height = heights.sample_world(point);
            let (gx, gz) = heights.grid_of(point);
            let vertex = heights.index(gx, gz);
            let connected = ocean.get(vertex).copied().unwrap_or(false);
            let index = iz * stride + ix;
            ground[index] = height;
            wet[index] = connected && height <= sea_top;
            any_wet_corner = any_wet_corner || wet[index];
        }
    }
    if !any_wet_corner {
        return None;
    }

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let push_vertex = |positions: &mut Vec<[f32; 3]>,
                           normals: &mut Vec<[f32; 3]>,
                           uvs: &mut Vec<[f32; 2]>,
                           colors: &mut Vec<[f32; 4]>,
                           ix: usize,
                           iz: usize|
     -> u32 {
        let grid = iz * stride + ix;
        let point = Vec2::new(
            (low.x + ix as f32 * step.x).min(high.x),
            (low.y + iz as f32 * step.y).min(high.y),
        );
        let depth = (sea_level - ground[grid]).max(0.0);
        let (gx, gz) = heights.grid_of(point);
        let vertex = heights.index(gx, gz);
        let is_frozen = frozen.get(vertex).copied().unwrap_or(false);
        let foam = (1.0 - depth / FOAM_DEPTH).clamp(0.0, 1.0);
        positions.push([point.x, 0.0, point.y]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([point.x / WATER_UV_TILE_METRES, point.y / WATER_UV_TILE_METRES]);
        colors.push([depth, if is_frozen { 1.0 } else { 0.0 }, foam, 1.0]);
        (positions.len() - 1) as u32
    };

    for iz in 0..steps {
        for ix in 0..steps {
            let corners = [
                (ix, iz),
                (ix + 1, iz),
                (ix, iz + 1),
                (ix + 1, iz + 1),
            ];
            if !corners.iter().any(|&(cx, cz)| wet[cz * stride + cx]) {
                continue;
            }
            if frozen_only && !corners.iter().all(|&(cx, cz)| {
                let point = Vec2::new(
                    (low.x + cx as f32 * step.x).min(high.x),
                    (low.y + cz as f32 * step.y).min(high.y),
                );
                let (gx, gz) = heights.grid_of(point);
                frozen.get(heights.index(gx, gz)).copied().unwrap_or(false)
            }) {
                continue;
            }
            let base = positions.len() as u32;
            for (k, &(cx, cz)) in corners.iter().enumerate() {
                let id = push_vertex(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut colors,
                    cx,
                    cz,
                );
                assert_eq!(id, base + k as u32);
            }
            indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
        }
    }

    if indices.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

pub fn lake_basin_mesh(lake: &Lake, heights: &ScalarField, frozen: bool) -> Option<(Vec2, Mesh)> {
    if lake.cells.is_empty() {
        return None;
    }
    let anchor = heights.world_of(
        heights.coords(lake.cells[0]).0,
        heights.coords(lake.cells[0]).1,
    );
    let in_lake: HashSet<usize> = lake.cells.iter().copied().collect();

    let mut quads: BTreeSet<(isize, isize)> = BTreeSet::new();
    for &cell in &lake.cells {
        let (x, z) = heights.coords(cell);
        for (dx, dz) in [(0isize, 0isize), (-1, 0), (0, -1), (-1, -1)] {
            let qx = x as isize + dx;
            let qz = z as isize + dz;
            let corners = [(qx, qz), (qx + 1, qz), (qx, qz + 1), (qx + 1, qz + 1)];
            if !heights.topology.wraps()
                && corners.iter().any(|&(cx, cz)| {
                    cx < 0 || cz < 0 || cx >= heights.dim as isize || cz >= heights.dim as isize
                })
            {
                continue;
            }
            let inside = corners.iter().filter(|&&(cx, cz)| {
                in_lake.contains(&heights.index(heights.wrap(cx), heights.wrap(cz)))
            }).count();
            if inside >= MIN_LAKE_CORNERS_PER_QUAD {
                quads.insert((qx, qz));
            }
        }
    }
    if quads.is_empty() {
        return None;
    }

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for &(qx, qz) in &quads {
        let base = positions.len() as u32;
        for (cx, cz) in [(qx, qz), (qx + 1, qz), (qx, qz + 1), (qx + 1, qz + 1)] {
            let vertex = heights.index(heights.wrap(cx), heights.wrap(cz));
            let raw = heights.world_of(heights.wrap(cx), heights.wrap(cz));
            let local = anchor
                + heights
                    .topology
                    .delta_xz(anchor, raw, heights.size);
            let ground = heights.get_at(vertex);
            let y = if in_lake.contains(&vertex) {
                lake.surface
            } else {
                lake.surface.min(ground)
            };
            positions.push([local.x, y, local.y]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([local.x / WATER_UV_TILE_METRES, local.y / WATER_UV_TILE_METRES]);
            colors.push([
                (lake.surface - ground).max(0.0),
                if frozen { 1.0 } else { 0.0 },
                0.0,
                1.0,
            ]);
        }
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some((anchor, mesh))
}

const RIVER_RUNOFF_SCALE: f32 = 0.02;
const RIVER_VELOCITY_COEFF: f32 = 1.8;
const RIVER_MIN_VELOCITY: f32 = 0.35;
const RIVER_MAX_VELOCITY: f32 = 4.5;
const RIVER_MIN_WIDTH_SPACING: f32 = 0.25;
const RIVER_MAX_WIDTH_SPACING: f32 = 4.5;
const RIVER_PROFILE_STEPS: usize = 24;
const RIVER_SPLINE_SUBDIV: usize = 4;
const RIVER_OVERFLOW_SPREAD: f32 = 1.6;
const RIVER_MAX_SHEET_DEPTH: f32 = 0.45;
const RIVER_SOLVER_STEPS: usize = 12;

fn seasonal_runoff_scale(season: Season) -> f32 {
    match season {
        Season::Spring => 1.35,
        Season::Summer => 0.85,
        Season::Fall => 1.05,
        Season::Winter => 0.70,
    }
}

fn unwrap_path(path: &[usize], heights: &ScalarField) -> Vec<Vec2> {
    let mut points = Vec::with_capacity(path.len());
    if path.is_empty() {
        return points;
    }
    let first = heights.world_of(heights.coords(path[0]).0, heights.coords(path[0]).1);
    points.push(first);
    let mut cursor = first;
    for pair in path.windows(2) {
        let a = heights.world_of(heights.coords(pair[0]).0, heights.coords(pair[0]).1);
        let b = heights.world_of(heights.coords(pair[1]).0, heights.coords(pair[1]).1);
        cursor += heights.topology.delta_xz(a, b, heights.size);
        if cursor.distance_squared(*points.last().unwrap()) > 1e-6 {
            points.push(cursor);
        }
    }
    points
}

fn centripetal_segment(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t_norm: f32) -> (Vec2, Vec2) {
    let chord = |a: Vec2, b: Vec2| a.distance(b).sqrt().max(1e-4);
    let t0 = 0.0f32;
    let t1 = t0 + chord(p0, p1);
    let t2 = t1 + chord(p1, p2);
    let t3 = t2 + chord(p2, p3);
    let t = t1 + (t2 - t1) * t_norm.clamp(0.0, 1.0);

    let lerp_pt = |a: Vec2, b: Vec2, ta: f32, tb: f32| -> (Vec2, Vec2) {
        let span = (tb - ta).max(1e-5);
        let u = (t - ta) / span;
        (a + (b - a) * u, (b - a) / span)
    };

    let (a1, da1) = lerp_pt(p0, p1, t0, t1);
    let (a2, da2) = lerp_pt(p1, p2, t1, t2);
    let (a3, da3) = lerp_pt(p2, p3, t2, t3);

    let (b1, db1) = {
        let span = (t2 - t0).max(1e-5);
        let u = (t - t0) / span;
        (
            a1 + (a2 - a1) * u,
            da1 * (1.0 - u) + da2 * u + (a2 - a1) / span,
        )
    };
    let (b2, db2) = {
        let span = (t3 - t1).max(1e-5);
        let u = (t - t1) / span;
        (
            a2 + (a3 - a2) * u,
            da2 * (1.0 - u) + da3 * u + (a3 - a2) / span,
        )
    };

    let span = (t2 - t1).max(1e-5);
    let u = (t - t1) / span;
    let pos = b1 + (b2 - b1) * u;
    let tan = (db1 * (1.0 - u) + db2 * u + (b2 - b1) / span).normalize_or_zero();
    (pos, tan)
}

fn sample_centripetal_spline(
    control: &[Vec2],
    subdiv: usize,
    start_hint: Option<Vec2>,
    end_hint: Option<Vec2>,
) -> Vec<(Vec2, Vec2)> {
    if control.len() < 2 {
        return Vec::new();
    }
    let subdiv = subdiv.max(1);
    let mut out = Vec::with_capacity((control.len() - 1) * subdiv + 1);
    for i in 0..control.len() - 1 {
        let p1 = control[i];
        let p2 = control[i + 1];
        let p0 = if i > 0 {
            control[i - 1]
        } else {
            start_hint.unwrap_or(p1 * 2.0 - p2)
        };
        let p3 = if i + 2 < control.len() {
            control[i + 2]
        } else {
            end_hint.unwrap_or(p2 * 2.0 - p1)
        };
        let count = if i + 2 == control.len() {
            subdiv + 1
        } else {
            subdiv
        };
        for s in 0..count {
            let t = s as f32 / subdiv as f32;
            let (pos, tan) = centripetal_segment(p0, p1, p2, p3, t);
            let tan = if tan.length_squared() > 1e-8 {
                tan
            } else {
                (p2 - p1).normalize_or_zero()
            };
            out.push((pos, tan));
        }
    }
    out
}

#[derive(Clone, Debug)]
struct RiverStation {
    pos: Vec2,
    tan: Vec2,
    side: Vec2,
    surface_y: f32,
    left_w: f32,
    right_w: f32,
    bed_y: f32,
    velocity: Vec2,
    frozen: bool,
    travelled: f32,
}

fn channel_cross_section(
    heights: &ScalarField,
    center: Vec2,
    side: Vec2,
    max_half: f32,
) -> ([f32; RIVER_PROFILE_STEPS * 2 + 1], f32, f32, f32, f32) {
    let mut profile = [0.0f32; RIVER_PROFILE_STEPS * 2 + 1];
    let step = max_half / RIVER_PROFILE_STEPS as f32;
    for k in 0..profile.len() {
        let offset = (k as isize - RIVER_PROFILE_STEPS as isize) as f32 * step;
        profile[k] = heights.sample_catmull_rom(center + side * offset);
    }
    let bed_y = profile[RIVER_PROFILE_STEPS - 1..=RIVER_PROFILE_STEPS + 1]
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let mut left_rim = bed_y;
    for k in (0..RIVER_PROFILE_STEPS).rev() {
        left_rim = left_rim.max(profile[k]);
    }
    let mut right_rim = bed_y;
    for k in (RIVER_PROFILE_STEPS + 1)..profile.len() {
        right_rim = right_rim.max(profile[k]);
    }
    (profile, step, bed_y, left_rim, right_rim)
}

fn wetted_area_and_widths(
    profile: &[f32; RIVER_PROFILE_STEPS * 2 + 1],
    step: f32,
    surface_y: f32,
    min_half: f32,
) -> (f32, f32, f32) {
    let mut area = 0.0f32;
    let mut left_w = min_half;
    for k in (0..RIVER_PROFILE_STEPS).rev() {
        let d0 = surface_y - profile[k + 1];
        let d1 = surface_y - profile[k];
        if d0 <= 0.0 && d1 <= 0.0 {
            break;
        }
        let dist = (RIVER_PROFILE_STEPS - k) as f32 * step;
        if d1 >= 0.0 {
            area += (d0.max(0.0) + d1) * 0.5 * step;
            left_w = left_w.max(dist);
        } else {
            let frac = (d0 / (d0 - d1).max(1e-5)).clamp(0.0, 1.0);
            area += d0 * 0.5 * step * frac;
            left_w = left_w.max(((RIVER_PROFILE_STEPS - k - 1) as f32 + frac) * step);
            break;
        }
    }
    let mut right_w = min_half;
    for k in (RIVER_PROFILE_STEPS + 1)..profile.len() {
        let d0 = surface_y - profile[k - 1];
        let d1 = surface_y - profile[k];
        if d0 <= 0.0 && d1 <= 0.0 {
            break;
        }
        let dist = (k - RIVER_PROFILE_STEPS) as f32 * step;
        if d1 >= 0.0 {
            area += (d0.max(0.0) + d1) * 0.5 * step;
            right_w = right_w.max(dist);
        } else {
            let frac = (d0 / (d0 - d1).max(1e-5)).clamp(0.0, 1.0);
            area += d0 * 0.5 * step * frac;
            right_w = right_w.max(((k - RIVER_PROFILE_STEPS - 1) as f32 + frac) * step);
            break;
        }
    }
    (area, left_w, right_w)
}

#[allow(clippy::too_many_arguments)]
fn solve_station_volume(
    pos: Vec2,
    tan: Vec2,
    heights: &ScalarField,
    accumulation: &ScalarField,
    flow: &FlowField,
    frozen_mask: &[bool],
    season: Season,
    travelled: f32,
) -> RiverStation {
    let spacing = heights.spacing();
    let min_half = spacing * RIVER_MIN_WIDTH_SPACING;
    let max_half = spacing * RIVER_MAX_WIDTH_SPACING;
    let side = if tan.length_squared() > 1e-8 {
        Vec2::new(-tan.y, tan.x).normalize()
    } else {
        Vec2::Y
    };

    let accum = accumulation.sample_world(pos).max(0.0);
    let discharge = accum * spacing * spacing * RIVER_RUNOFF_SCALE * seasonal_runoff_scale(season);
    let slope = flow.slope_at(pos).max(0.01);
    let speed = (RIVER_VELOCITY_COEFF * slope.sqrt()).clamp(RIVER_MIN_VELOCITY, RIVER_MAX_VELOCITY);
    let water_volume = (discharge / speed).max(0.05);

    let baked = flow.direction_at(pos);
    let flow_dir = if baked.length_squared() > 1e-6 {
        baked.normalize()
    } else {
        tan
    };
    let velocity = flow_dir * speed;

    let (gx, gz) = heights.grid_of(pos);
    let frozen = frozen_mask
        .get(heights.index(gx, gz))
        .copied()
        .unwrap_or(false);

    let (profile, step, bed_y, left_rim, right_rim) = channel_cross_section(heights, pos, side, max_half);
    let channel_rim = left_rim.min(right_rim);
    let min_surface = bed_y + RIBBON_LIFT;

    let (surface_y, mut left_w, mut right_w) = if channel_rim > min_surface {
        let (channel_volume, rim_lw, rim_rw) = wetted_area_and_widths(&profile, step, channel_rim, min_half);
        if water_volume <= channel_volume {
            let mut lo = min_surface;
            let mut hi = channel_rim;
            for _ in 0..RIVER_SOLVER_STEPS {
                let mid = 0.5 * (lo + hi);
                let (area, _, _) = wetted_area_and_widths(&profile, step, mid, min_half);
                if area < water_volume {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let y = 0.5 * (lo + hi);
            let (_, lw, rw) = wetted_area_and_widths(&profile, step, y, min_half);
            (y, lw, rw)
        } else {
            let excess = water_volume - channel_volume;
            let spread = (excess * RIVER_OVERFLOW_SPREAD).sqrt() * 0.5;
            let sheet = (excess / ((rim_lw + rim_rw + 2.0 * spread).max(1e-3))).min(RIVER_MAX_SHEET_DEPTH);
            (
                channel_rim + sheet,
                (rim_lw + spread).clamp(min_half, max_half),
                (rim_rw + spread).clamp(min_half, max_half),
            )
        }
    } else {
        let sheet = (water_volume.sqrt() * 0.25).clamp(RIBBON_LIFT, RIVER_MAX_SHEET_DEPTH);
        let half = (water_volume / (2.0 * sheet)).clamp(min_half, max_half);
        (bed_y + sheet, half, half)
    };

    left_w = left_w.clamp(min_half, max_half);
    right_w = right_w.clamp(min_half, max_half);

    RiverStation {
        pos,
        tan,
        side,
        surface_y,
        left_w,
        right_w,
        bed_y,
        velocity,
        frozen,
        travelled,
    }
}

fn smooth_stations(stations: &mut [RiverStation], outlet_y: Option<f32>) {
    if stations.len() < 3 {
        return;
    }
    let snapshot_w: Vec<(f32, f32, f32)> = stations
        .iter()
        .map(|s| (s.surface_y, s.left_w, s.right_w))
        .collect();
    for i in 1..stations.len() - 1 {
        stations[i].surface_y =
            snapshot_w[i - 1].0 * 0.25 + snapshot_w[i].0 * 0.5 + snapshot_w[i + 1].0 * 0.25;
        stations[i].left_w =
            snapshot_w[i - 1].1 * 0.25 + snapshot_w[i].1 * 0.5 + snapshot_w[i + 1].1 * 0.25;
        stations[i].right_w =
            snapshot_w[i - 1].2 * 0.25 + snapshot_w[i].2 * 0.5 + snapshot_w[i + 1].2 * 0.25;
    }
    if let Some(target_y) = outlet_y {
        if let Some(last) = stations.last_mut() {
            last.surface_y = last.surface_y.min(target_y);
        }
    }
    for i in 1..stations.len() {
        let min_y = stations[i].bed_y + RIBBON_LIFT;
        stations[i].surface_y = stations[i].surface_y.min(stations[i - 1].surface_y).max(min_y);
    }
}

struct Reach {
    stations: Vec<RiverStation>,
    parent: Option<(usize, usize)>,
}

#[derive(Clone)]
struct BankSplit {
    station_idx: usize,
    left_side: bool,
    u_center: f32,
    u_a: f32,
    u_b: f32,
    pos_a: Vec2,
    pos_center: Vec2,
    pos_b: Vec2,
    y_a: f32,
    y_center: f32,
    y_b: f32,
    skirt_a: Vec2,
    skirt_b: Vec2,
    vel: Vec2,
    depth_center: f32,
    frozen: bool,
    travelled: f32,
}

fn bank_point(station: &RiverStation, left: bool) -> Vec2 {
    if left {
        station.pos - station.side * station.left_w
    } else {
        station.pos + station.side * station.right_w
    }
}

fn skirt_point(station: &RiverStation, left: bool, skirt_dist: f32, heights: &ScalarField) -> Vec3 {
    let dir = if left { -station.side } else { station.side };
    let w = if left { station.left_w } else { station.right_w };
    let xz = station.pos + dir * (w + skirt_dist);
    let y = (heights.sample_world(xz) - BANK_TUCK).min(station.surface_y - BANK_TUCK);
    Vec3::new(xz.x, y, xz.y)
}

fn bank_vertex_y(station: &RiverStation, bank_xz: Vec2, heights: &ScalarField) -> f32 {
    let local_ground = heights.sample_world(bank_xz);
    station.surface_y.min(local_ground + RIBBON_LIFT)
}

fn seg_intersect(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> Option<(f32, f32, Vec2)> {
    let da = a1 - a0;
    let db = b1 - b0;
    let denom = da.x * db.y - da.y * db.x;
    if denom.abs() < 1e-6 {
        return None;
    }
    let diff = b0 - a0;
    let ua = (diff.x * db.y - diff.y * db.x) / denom;
    let ub = (diff.x * da.y - diff.y * da.x) / denom;
    if (-0.05..=1.05).contains(&ua) && (-0.05..=1.05).contains(&ub) {
        Some((ua.clamp(0.0, 1.0), ub.clamp(0.0, 1.0), a0 + da * ua.clamp(0.0, 1.0)))
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub fn river_network_mesh(
    paths: &[Vec<usize>],
    heights: &ScalarField,
    accumulation: &ScalarField,
    flow: &FlowField,
    lakes: &[Lake],
    ocean: &[bool],
    frozen_mask: &[bool],
    sea_level: f32,
    season: Season,
) -> Option<Mesh> {
    if paths.is_empty() {
        return None;
    }

    let mut ordered: Vec<(f32, usize)> = paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.len() >= 2)
        .map(|(i, p)| {
            let end_acc = p.last().map(|&v| accumulation.get_at(v)).unwrap_or(0.0);
            (end_acc, i)
        })
        .collect();
    if ordered.is_empty() {
        return None;
    }
    ordered.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut owner_of_vertex: std::collections::HashMap<usize, (usize, usize)> =
        std::collections::HashMap::new();
    let mut reaches: Vec<Reach> = Vec::with_capacity(ordered.len());
    let spacing = heights.spacing();
    let uv_v_scale = (spacing * RIVER_UV_V_TILE_SPACING).max(1e-3);

    for (_, path_idx) in ordered {
        let path = &paths[path_idx];
        let unwrapped = unwrap_path(path, heights);
        if unwrapped.len() < 2 {
            continue;
        }
        let end_vertex = *path.last().unwrap();
        let parent = owner_of_vertex.get(&end_vertex).copied();
        let outlet_y = if ocean.get(end_vertex).copied().unwrap_or(false) {
            Some(sea_level)
        } else {
            lakes
                .iter()
                .find(|l| l.cells.contains(&end_vertex))
                .map(|l| l.surface)
        };

        let samples = sample_centripetal_spline(&unwrapped, RIVER_SPLINE_SUBDIV, None, None);
        if samples.len() < 2 {
            continue;
        }

        let mut stations = Vec::with_capacity(samples.len());
        let mut travelled = 0.0f32;
        for (idx, &(pos, tan)) in samples.iter().enumerate() {
            if idx > 0 {
                travelled += pos.distance(samples[idx - 1].0);
            }
            stations.push(solve_station_volume(
                pos,
                tan,
                heights,
                accumulation,
                flow,
                frozen_mask,
                season,
                travelled,
            ));
        }
        smooth_stations(&mut stations, outlet_y);

        let reach_id = reaches.len();
        for (step_idx, &vertex) in path.iter().enumerate() {
            let station_idx = (step_idx * RIVER_SPLINE_SUBDIV).min(stations.len() - 1);
            owner_of_vertex.entry(vertex).or_insert((reach_id, station_idx));
        }
        reaches.push(Reach { stations, parent });
    }

    if reaches.is_empty() {
        return None;
    }

    let skirt_dist = spacing * RIVER_BANK_SKIRT_SPACING;
    let mut splits_by_reach: Vec<Vec<BankSplit>> = vec![Vec::new(); reaches.len()];
    let mut trimmed_end: Vec<Option<(usize, BankSplit)>> = vec![None; reaches.len()];

    for reach_id in 0..reaches.len() {
        let Some((parent_id, approx_p_idx)) = reaches[reach_id].parent else {
            continue;
        };
        let parent_len = reaches[parent_id].stations.len();
        if parent_len < 2 {
            continue;
        }
        let approx_p_idx = approx_p_idx.min(parent_len - 1);
        let parent_anchor = reaches[parent_id].stations[approx_p_idx].pos;
        let child_end = reaches[reach_id].stations.last().unwrap().pos;
        let shift = heights
            .topology
            .delta_xz(child_end, parent_anchor, heights.size)
            - (parent_anchor - child_end);
        if shift.length_squared() > 1e-4 {
            for s in &mut reaches[reach_id].stations {
                s.pos += shift;
            }
        }

        let p_station = &reaches[parent_id].stations[approx_p_idx];
        let c_last = reaches[reach_id].stations.last().unwrap();
        let entry_offset = c_last.pos - p_station.pos;
        let left_side = if entry_offset.length_squared() > 1e-4 {
            entry_offset.dot(-p_station.side) >= 0.0
        } else {
            c_last.tan.dot(p_station.side) > 0.0
        };

        let p_start = approx_p_idx.saturating_sub(RIVER_SPLINE_SUBDIV * 2);
        let p_end = (approx_p_idx + RIVER_SPLINE_SUBDIV * 2).min(parent_len - 1);
        let child_len = reaches[reach_id].stations.len();
        let c_start = child_len.saturating_sub(RIVER_SPLINE_SUBDIV * 3);

        let mut hit: Option<(usize, usize, f32, Vec2)> = None;
        'search: for ci in (c_start..child_len.saturating_sub(1)).rev() {
            let ca = reaches[reach_id].stations[ci].pos;
            let cb = reaches[reach_id].stations[ci + 1].pos;
            for pi in p_start..p_end {
                let pa = bank_point(&reaches[parent_id].stations[pi], left_side);
                let pb = bank_point(&reaches[parent_id].stations[pi + 1], left_side);
                if let Some((up, _, pt)) = seg_intersect(pa, pb, ca, cb) {
                    hit = Some((ci, pi, up, pt));
                    break 'search;
                }
            }
        }

        let (cut_ci, pi, u_center, hit_pt) = hit.unwrap_or((
            child_len.saturating_sub(2).max(1),
            approx_p_idx.min(parent_len.saturating_sub(2)),
            0.5,
            bank_point(
                &reaches[parent_id].stations[approx_p_idx.min(parent_len - 1)],
                left_side,
            ),
        ));
        let pi = pi.min(parent_len.saturating_sub(2));

        let pa_station = &reaches[parent_id].stations[pi];
        let pb_station = &reaches[parent_id].stations[pi + 1];
        let bank_a = bank_point(pa_station, left_side);
        let bank_b = bank_point(pb_station, left_side);
        let seg_len = bank_a.distance(bank_b).max(1e-3);

        let cut_ci = cut_ci.min(child_len - 1);
        let mouth_half = ((reaches[reach_id].stations[cut_ci].left_w
            + reaches[reach_id].stations[cut_ci].right_w)
            * 0.5)
            .min(seg_len * 0.45);
        let du = mouth_half / seg_len;
        let u_a = (u_center - du).clamp(0.02, 0.96);
        let u_b = (u_center + du).clamp((u_a + 0.02).min(0.98), 0.98);
        let pos_a = bank_a.lerp(bank_b, u_a);
        let pos_center = bank_a.lerp(bank_b, u_center);
        let pos_b = bank_a.lerp(bank_b, u_b);

        let parent_y = pa_station.surface_y + (pb_station.surface_y - pa_station.surface_y) * u_center;
        let y_a = pa_station.surface_y + (pb_station.surface_y - pa_station.surface_y) * u_a;
        let y_b = pa_station.surface_y + (pb_station.surface_y - pa_station.surface_y) * u_b;

        let skirt_dir = if left_side { -pa_station.side } else { pa_station.side };
        let skirt_a = pos_a + skirt_dir * skirt_dist;
        let skirt_b = pos_b + skirt_dir * skirt_dist;

        let mut split = BankSplit {
            station_idx: pi,
            left_side,
            u_center,
            u_a,
            u_b,
            pos_a,
            pos_center: hit_pt.lerp(pos_center, 0.5),
            pos_b,
            y_a,
            y_center: parent_y,
            y_b,
            skirt_a,
            skirt_b,
            vel: reaches[reach_id].stations[cut_ci].velocity,
            depth_center: (parent_y - heights.sample_world(pos_center)).max(0.05),
            frozen: reaches[reach_id].stations[cut_ci].frozen,
            travelled: reaches[reach_id].stations[cut_ci].travelled,
        };
        if split.u_a > split.u_b {
            std::mem::swap(&mut split.u_a, &mut split.u_b);
        }

        splits_by_reach[parent_id].push(split.clone());
        trimmed_end[reach_id] = Some((cut_ci, split));
    }

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for (reach_id, reach) in reaches.iter().enumerate() {
        let end_count = trimmed_end[reach_id]
            .as_ref()
            .map(|(cut, _)| (*cut + 1).max(1))
            .unwrap_or(reach.stations.len());
        let stations = &reach.stations[..end_count.min(reach.stations.len())];
        if stations.is_empty() {
            continue;
        }

        let push_station = |positions: &mut Vec<[f32; 3]>,
                            normals: &mut Vec<[f32; 3]>,
                            uvs: &mut Vec<[f32; 2]>,
                            colors: &mut Vec<[f32; 4]>,
                            s: &RiverStation,
                            shift: Vec2|
         -> u32 {
            let base = positions.len() as u32;
            let shifted = RiverStation {
                pos: s.pos + shift,
                ..*s
            };
            let lb_xz = bank_point(&shifted, true);
            let rb_xz = bank_point(&shifted, false);
            let ls = skirt_point(&shifted, true, skirt_dist, heights);
            let rs = skirt_point(&shifted, false, skirt_dist, heights);
            let lb_y = bank_vertex_y(&shifted, lb_xz, heights);
            let rb_y = bank_vertex_y(&shifted, rb_xz, heights);
            let c_depth = (shifted.surface_y - heights.sample_world(shifted.pos)).max(0.05);
            let f_flag = if shifted.frozen { 1.0 } else { 0.0 };
            let v = shifted.travelled / uv_v_scale;
            let col = [shifted.velocity.x, shifted.velocity.y, c_depth, f_flag];
            let edge_col = [shifted.velocity.x, shifted.velocity.y, 0.0, f_flag];

            positions.push(ls.to_array());
            positions.push([lb_xz.x, lb_y, lb_xz.y]);
            positions.push([shifted.pos.x, shifted.surface_y, shifted.pos.y]);
            positions.push([rb_xz.x, rb_y, rb_xz.y]);
            positions.push(rs.to_array());

            for _ in 0..5 {
                normals.push([0.0, 1.0, 0.0]);
            }
            uvs.push([0.0, v]);
            uvs.push([BANK_U_EXTENT, v]);
            uvs.push([0.5, v]);
            uvs.push([1.0 - BANK_U_EXTENT, v]);
            uvs.push([1.0, v]);

            colors.push(edge_col);
            colors.push(edge_col);
            colors.push(col);
            colors.push(edge_col);
            colors.push(edge_col);
            base
        };

        let wrap_shift = |pos: Vec2| -> Vec2 {
            if heights.topology.wraps() {
                heights.topology.wrap_xz(pos, heights.size) - pos
            } else {
                Vec2::ZERO
            }
        };

        let mut prev_shift = wrap_shift(stations[0].pos);
        let mut prev_base =
            push_station(&mut positions, &mut normals, &mut uvs, &mut colors, &stations[0], prev_shift);

        for i in 0..stations.len().saturating_sub(1) {
            let next_shift = wrap_shift(stations[i + 1].pos);
            let bridge_base = push_station(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut colors,
                &stations[i + 1],
                prev_shift,
            );

            let split = splits_by_reach[reach_id].iter().find(|sp| sp.station_idx == i);

            if let Some(sp) = split {
                let ps = prev_shift;
                let push_pt = |positions: &mut Vec<[f32; 3]>,
                               normals: &mut Vec<[f32; 3]>,
                               uvs: &mut Vec<[f32; 2]>,
                               colors: &mut Vec<[f32; 4]>,
                               xz: Vec2,
                               y: f32,
                               u: f32,
                               v_t: f32,
                               depth: f32|
                 -> u32 {
                    let id = positions.len() as u32;
                    let p = xz + ps;
                    positions.push([p.x, y, p.y]);
                    normals.push([0.0, 1.0, 0.0]);
                    uvs.push([u, v_t]);
                    colors.push([sp.vel.x, sp.vel.y, depth, if sp.frozen { 1.0 } else { 0.0 }]);
                    id
                };
                let v_a = (stations[i].travelled
                    + (stations[i + 1].travelled - stations[i].travelled) * sp.u_a)
                    / uv_v_scale;
                let v_c = (stations[i].travelled
                    + (stations[i + 1].travelled - stations[i].travelled) * sp.u_center)
                    / uv_v_scale;
                let v_b = (stations[i].travelled
                    + (stations[i + 1].travelled - stations[i].travelled) * sp.u_b)
                    / uv_v_scale;
                let u_bank = if sp.left_side { BANK_U_EXTENT } else { 1.0 - BANK_U_EXTENT };
                let u_skirt = if sp.left_side { 0.0 } else { 1.0 };

                let pa = push_pt(&mut positions, &mut normals, &mut uvs, &mut colors, sp.pos_a, sp.y_a, u_bank, v_a, 0.0);
                let pm = push_pt(&mut positions, &mut normals, &mut uvs, &mut colors, sp.pos_center, sp.y_center, 0.5, v_c, sp.depth_center);
                let pb = push_pt(&mut positions, &mut normals, &mut uvs, &mut colors, sp.pos_b, sp.y_b, u_bank, v_b, 0.0);
                let ska_y = (heights.sample_world(sp.skirt_a) - BANK_TUCK).min(sp.y_a - BANK_TUCK);
                let skb_y = (heights.sample_world(sp.skirt_b) - BANK_TUCK).min(sp.y_b - BANK_TUCK);
                let ska = push_pt(&mut positions, &mut normals, &mut uvs, &mut colors, sp.skirt_a, ska_y, u_skirt, v_a, 0.0);
                let skb = push_pt(&mut positions, &mut normals, &mut uvs, &mut colors, sp.skirt_b, skb_y, u_skirt, v_b, 0.0);

                if sp.left_side {
                    indices.extend_from_slice(&[prev_base, prev_base + 1, pa, prev_base, pa, ska]);
                    indices.extend_from_slice(&[pb, bridge_base + 1, bridge_base, pb, bridge_base, skb]);
                    indices.extend_from_slice(&[prev_base + 1, prev_base + 2, pa, pa, prev_base + 2, pm]);
                    indices.extend_from_slice(&[pm, prev_base + 2, bridge_base + 2, pm, bridge_base + 2, pb]);
                    indices.extend_from_slice(&[pb, bridge_base + 2, bridge_base + 1]);
                    for k in 2..4u32 {
                        indices.extend_from_slice(&[
                            prev_base + k,
                            prev_base + k + 1,
                            bridge_base + k,
                            bridge_base + k,
                            prev_base + k + 1,
                            bridge_base + k + 1,
                        ]);
                    }
                } else {
                    for k in 0..2u32 {
                        indices.extend_from_slice(&[
                            prev_base + k,
                            prev_base + k + 1,
                            bridge_base + k,
                            bridge_base + k,
                            prev_base + k + 1,
                            bridge_base + k + 1,
                        ]);
                    }
                    indices.extend_from_slice(&[prev_base + 2, prev_base + 3, pa, prev_base + 2, pa, pm]);
                    indices.extend_from_slice(&[prev_base + 2, pm, bridge_base + 2, pm, pb, bridge_base + 2]);
                    indices.extend_from_slice(&[pb, bridge_base + 3, bridge_base + 2]);
                    indices.extend_from_slice(&[prev_base + 3, prev_base + 4, ska, prev_base + 3, ska, pa]);
                    indices.extend_from_slice(&[pb, skb, bridge_base + 4, pb, bridge_base + 4, bridge_base + 3]);
                }
            } else {
                for k in 0..4u32 {
                    indices.extend_from_slice(&[
                        prev_base + k,
                        prev_base + k + 1,
                        bridge_base + k,
                        bridge_base + k,
                        prev_base + k + 1,
                        bridge_base + k + 1,
                    ]);
                }
            }

            if (next_shift - prev_shift).length_squared() > 1e-4 {
                prev_shift = next_shift;
                prev_base =
                    push_station(&mut positions, &mut normals, &mut uvs, &mut colors, &stations[i + 1], prev_shift);
            } else {
                prev_base = bridge_base;
            }
        }

        if let Some((_, sp)) = trimmed_end[reach_id].as_ref() {
            let ps = prev_shift;
            let push_end = |positions: &mut Vec<[f32; 3]>,
                            normals: &mut Vec<[f32; 3]>,
                            uvs: &mut Vec<[f32; 2]>,
                            colors: &mut Vec<[f32; 4]>,
                            xz: Vec2,
                            y: f32,
                            u: f32,
                            depth: f32|
             -> u32 {
                let id = positions.len() as u32;
                let p = xz + ps;
                positions.push([p.x, y, p.y]);
                normals.push([0.0, 1.0, 0.0]);
                uvs.push([u, sp.travelled / uv_v_scale]);
                colors.push([sp.vel.x, sp.vel.y, depth, if sp.frozen { 1.0 } else { 0.0 }]);
                id
            };
            let ska_y = (heights.sample_world(sp.skirt_a) - BANK_TUCK).min(sp.y_a - BANK_TUCK);
            let skb_y = (heights.sample_world(sp.skirt_b) - BANK_TUCK).min(sp.y_b - BANK_TUCK);
            let (l_pos, l_y, r_pos, r_y, ls_pos, ls_y, rs_pos, rs_y) = if sp.left_side {
                (sp.pos_b, sp.y_b, sp.pos_a, sp.y_a, sp.skirt_b, skb_y, sp.skirt_a, ska_y)
            } else {
                (sp.pos_a, sp.y_a, sp.pos_b, sp.y_b, sp.skirt_a, ska_y, sp.skirt_b, skb_y)
            };
            let e0 = push_end(&mut positions, &mut normals, &mut uvs, &mut colors, ls_pos, ls_y, 0.0, 0.0);
            let e1 = push_end(&mut positions, &mut normals, &mut uvs, &mut colors, l_pos, l_y, BANK_U_EXTENT, 0.0);
            let e2 = push_end(&mut positions, &mut normals, &mut uvs, &mut colors, sp.pos_center, sp.y_center, 0.5, sp.depth_center);
            let e3 = push_end(&mut positions, &mut normals, &mut uvs, &mut colors, r_pos, r_y, 1.0 - BANK_U_EXTENT, 0.0);
            let e4 = push_end(&mut positions, &mut normals, &mut uvs, &mut colors, rs_pos, rs_y, 1.0, 0.0);

            let end_ring = [e0, e1, e2, e3, e4];
            for k in 0..4usize {
                indices.extend_from_slice(&[
                    prev_base + k as u32,
                    prev_base + k as u32 + 1,
                    end_ring[k],
                    end_ring[k],
                    prev_base + k as u32 + 1,
                    end_ring[k + 1],
                ]);
            }
        }
    }

    if indices.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

pub fn cascade_lip_width(fall: &Waterfall, heights: &ScalarField, fallback: f32) -> f32 {
    let spacing = heights.spacing();
    let upstream = heights
        .topology
        .delta_xz(fall.plunge, fall.lip, heights.size)
        .normalize_or_zero();
    if upstream.length_squared() < 1e-9 {
        return fallback;
    }
    let side = Vec2::new(-upstream.y, upstream.x);
    let probe = fall.lip + upstream * (spacing * LIP_PROBE_AHEAD_SPACING);
    let reference = heights.sample_world(probe);
    let margin = (fall.drop * LIP_MARGIN_DROP_FRACTION)
        .clamp(LIP_MARGIN_MIN_METRES, LIP_MARGIN_MAX_METRES);
    let step = spacing * LIP_PROBE_STEP_SPACING;
    let max_walk = spacing * LIP_MAX_WALK_SPACING;
    let mut width = 0.0;
    for sign in [1.0f32, -1.0] {
        let mut walk = 0.0;
        while walk < max_walk {
            walk += step;
            if heights.sample_world(probe + side * (walk * sign)) > reference + margin {
                break;
            }
        }
        if walk >= max_walk {
            return fallback;
        }
        width += walk;
    }
    width.clamp(spacing, spacing * LIP_MAX_WALK_SPACING)
}

pub fn cascade_mesh(fall: &Waterfall, heights: &ScalarField, width: f32) -> Option<Mesh> {
    let lip_height = heights.get_at(fall.index);
    let plunge_height = heights.get_at(fall.next);
    if lip_height <= plunge_height {
        return None;
    }
    let flow = heights
        .topology
        .delta_xz(fall.lip, fall.plunge, heights.size);
    let side = Vec3::new(-flow.y, 0.0, flow.x).try_normalize()?;
    let flow_dir = Vec3::new(flow.x, 0.0, flow.y);
    if flow_dir.length_squared() < 1e-9 {
        return None;
    }
    let width = cascade_lip_width(fall, heights, width);

    let top_y = lip_height + CASCADE_TOP_LIFT;
    let bottom_y = plunge_height + CASCADE_BOTTOM_LIFT;
    let segments = CASCADE_SEGMENTS;
    let fall_speed = fall.drop.clamp(CASCADE_SPEED_MIN, CASCADE_SPEED_MAX);
    let fall_flow = Vec2::new(flow_dir.x, flow_dir.z).normalize_or_zero() * fall_speed;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for step in 0..=segments {
        let t = step as f32 / segments as f32;
        let centre = Vec3::new(fall.lip.x, 0.0, fall.lip.y) + flow_dir * t;
        let y = top_y - (top_y - bottom_y) * t * t;
        let half = width.max(heights.spacing()) * 0.5 * (1.0 + CASCADE_FLARE_PER_LENGTH * t);
        positions.push([(centre + side * half).x, y, (centre + side * half).z]);
        positions.push([(centre - side * half).x, y, (centre - side * half).z]);
        normals.push([side.x, 0.0, side.z]);
        normals.push([side.x, 0.0, side.z]);
        uvs.push([0.0, t]);
        uvs.push([1.0, t]);
        colors.push([fall_flow.x, fall_flow.y, fall_speed, 1.0]);
        colors.push([fall_flow.x, fall_flow.y, fall_speed, 1.0]);
        if step > 0 {
            let a = (step as u32 - 1) * 2;
            indices.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
    }

    let foam_y = plunge_height + CASCADE_FOAM_LIFT;
    let foam_radius = fall.pool_radius.max(heights.spacing()) * CASCADE_FOAM_RADIUS_FRACTION;
    let foam_base = positions.len() as u32;
    positions.push([fall.plunge.x, foam_y, fall.plunge.y]);
    normals.push([0.0, 1.0, 0.0]);
    uvs.push([0.5, 0.5]);
    colors.push([0.0, 0.0, 0.0, 1.0]);
    let ring = CASCADE_FOAM_RING_SEGMENTS;
    for k in 0..=ring {
        let angle = k as f32 * std::f32::consts::TAU / ring as f32;
        let offset = Vec2::new(angle.cos(), angle.sin()) * foam_radius;
        positions.push([fall.plunge.x + offset.x, foam_y, fall.plunge.y + offset.y]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]);
        colors.push([0.0, 0.0, 0.0, 1.0]);
    }
    for k in 0..ring as u32 {
        indices.extend_from_slice(&[foam_base, foam_base + 1 + k, foam_base + 2 + k]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

pub fn water_material(surface: &WaterBody) -> StandardMaterial {
    let base = match surface.salinity {
        Salinity::Freshwater => Color::srgba(0.16, 0.36, 0.42, 0.78),
        Salinity::Brackish => Color::srgba(0.18, 0.34, 0.36, 0.80),
        Salinity::Marine => Color::srgba(0.09, 0.28, 0.42, 0.76),
        Salinity::Hypersaline => Color::srgba(0.62, 0.66, 0.60, 0.72),
    };
    if surface.frozen {
        return StandardMaterial {
            base_color: Color::srgba(0.78, 0.88, 0.96, 0.92),
            perceptual_roughness: 0.25,
            reflectance: 0.6,
            ..default()
        };
    }
    StandardMaterial {
        base_color: base,
        perceptual_roughness: 0.08,
        reflectance: 0.8,
        alpha_mode: AlphaMode::Blend,
        ..default()
    }
}

pub fn river_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgba(0.20, 0.42, 0.52, 0.82),
        perceptual_roughness: 0.12,
        reflectance: 0.7,
        alpha_mode: AlphaMode::Blend,
        ..default()
    }
}

pub fn cascade_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgba(0.92, 0.97, 1.0, 0.85),
        perceptual_roughness: 0.05,
        reflectance: 0.9,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    }
}

#[cfg(test)]
pub fn depth_below_surface(ground: f32, surface: f32) -> f32 {
    (surface - ground).max(0.0)
}

#[cfg(test)]
pub fn is_submerged(ground: f32, surface: f32) -> bool {
    ground < surface
}

#[cfg(test)]
pub fn submerged_fraction(field: &ScalarField, surface: f32) -> f32 {
    if field.values.is_empty() {
        return 0.0;
    }
    let wet = field.values.iter().filter(|v| **v < surface).count();
    wet as f32 / field.values.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::hydrology::{bake_flow_vectors, priority_flood};
    use crate::terrain::topology::WorldTopology;
    use bevy::mesh::VertexAttributeValues;

    fn basin_field() -> (ScalarField, Vec<Lake>) {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, 8.0 + x as f32 * 0.2);
            }
        }
        for z in 5..11 {
            for x in 5..11 {
                field.set(x, z, -4.0);
            }
        }
        let hydro = priority_flood(&field, 0.0);
        let lakes = crate::terrain::hydrology::find_lakes(&field, &hydro, 0.0, &vec![false; field.len()]);
        (field, lakes)
    }

    #[test]
    fn depth_and_submersion_agree() {
        assert_eq!(depth_below_surface(-3.0, 0.0), 3.0);
        assert_eq!(depth_below_surface(5.0, 0.0), 0.0);
        assert!(is_submerged(-0.1, 0.0));
        assert!(!is_submerged(0.1, 0.0));
    }

    #[test]
    fn submerged_fraction_tracks_the_sea_level() {
        let mut field = ScalarField::new(10, 100.0, WorldTopology::Finite);
        for index in 0..field.len() {
            field.set_at(index, index as f32 * 0.1 - 2.0);
        }
        assert!(submerged_fraction(&field, f32::NEG_INFINITY) < 1e-6);
        assert!(submerged_fraction(&field, f32::INFINITY) > 1.0 - 1e-6);
        let mid = submerged_fraction(&field, 0.0);
        assert!(mid > 0.1 && mid < 0.9);
    }

    #[test]
    fn ocean_connectivity_skips_landlocked_depressions() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        field.fill(10.0);
        for z in 4..12 {
            for x in 0..4 {
                field.set(x, z, -5.0);
            }
        }
        for z in 4..12 {
            for x in 9..13 {
                field.set(x, z, -5.0);
            }
        }
        let sea_top = 0.0;
        let mut border = Vec::new();
        for z in 0..16 {
            for x in 0..16 {
                if x == 0 || z == 0 || x == 15 || z == 15 {
                    border.push(field.index(x, z));
                }
            }
        }
        let mask = ocean_connectivity(&field, sea_top, &border);
        let connected = field.index(1, 8);
        let landlocked = field.index(11, 8);
        assert!(mask[connected], "the border trench is ocean-connected");
        assert!(
            !mask[landlocked],
            "the inland trench is walled off by high ground and must stay dry"
        );
    }

    #[test]
    fn lake_planes_sit_at_the_filled_surface() {
        let (field, lakes) = basin_field();
        let salinities: Vec<Salinity> = lakes.iter().map(|_| Salinity::Freshwater).collect();
        let planes = lake_planes(&lakes, &field, 4, &salinities);
        assert!(!planes.is_empty(), "the pit must produce a lake plane");
        for plane in &planes {
            assert!(plane.radius > 0.0);
            assert!(plane.surface > 0.0, "the lake surface must sit above the pit floor");
        }
    }

    #[test]
    fn lake_salinity_follows_the_drainage() {
        let (field, lakes) = basin_field();
        let salinities: Vec<Salinity> = lakes
            .iter()
            .map(|lake| {
                if lake.drains_to_sea {
                    Salinity::Freshwater
                } else {
                    Salinity::Hypersaline
                }
            })
            .collect();
        let planes = lake_planes(&lakes, &field, 4, &salinities);
        assert!(!planes.is_empty());
        for (lake, plane) in lakes
            .iter()
            .filter(|l| l.cells.len() >= 4 && l.depth > 0.05)
            .zip(&planes)
        {
            let expected = if lake.drains_to_sea {
                Salinity::Freshwater
            } else {
                Salinity::Hypersaline
            };
            assert_eq!(plane.salinity, expected);
        }
    }

    #[test]
    fn frozen_water_gets_a_brighter_material() {
        let liquid = WaterBody {
            radius: 50.0,
            surface: 0.0,
            salinity: Salinity::Marine,
            frozen: false,
        };
        let liquid_material = water_material(&liquid);
        assert!(matches!(liquid_material.alpha_mode, AlphaMode::Blend));

        let mut frozen = WaterBody {
            frozen: true,
            ..liquid.clone()
        };
        frozen.radius = 50.0;
        let frozen_material = water_material(&frozen);
        assert!(
            frozen_material.perceptual_roughness > liquid_material.perceptual_roughness,
            "ice scatters more light than open water"
        );
        assert!(
            frozen_material.base_color.to_linear().green
                > liquid_material.base_color.to_linear().green
        );
    }

    fn sea_world() -> (ScalarField, Vec<bool>) {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, x as f32 * 0.8 - 8.0);
            }
        }
        let border: Vec<usize> = (0..16)
            .flat_map(|z| (0..16).map(move |x| (x, z)))
            .filter(|&(x, z)| x == 0 || z == 0 || x == 15 || z == 15)
            .map(|(x, z)| field.index(x, z))
            .collect();
        let mask = ocean_connectivity(&field, 0.0, &border);
        (field, mask)
    }

    #[test]
    fn sea_chunk_mesh_is_subdivided_and_skips_dry_chunks() {
        let (field, mask) = sea_world();
        let frozen = vec![false; field.len()];
        let wet = sea_chunk_mesh(
            &field,
            &mask,
            &frozen,
            0.0,
            0.0,
            Vec2::new(-80.0, -80.0),
            Vec2::new(0.0, 80.0),
            8,
            false,
        );
        assert!(wet.is_some(), "the west half is submerged and must mesh");
        let mesh = wet.expect("wet chunk mesh");
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert!(positions.len() > 4, "the sea mesh must be subdivided");
        assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());

        let dry = sea_chunk_mesh(
            &field,
            &mask,
            &frozen,
            0.0,
            0.0,
            Vec2::new(40.0, -80.0),
            Vec2::new(80.0, 80.0),
            8,
            false,
        );
        assert!(dry.is_none(), "a fully emerged chunk spawns no sea mesh");
    }

    #[test]
    fn sea_chunk_mesh_packs_depth_and_freeze_into_colors() {
        let (field, mask) = sea_world();
        let mut frozen = vec![false; field.len()];
        for index in 0..field.len() {
            frozen[index] = true;
        }
        let mesh = sea_chunk_mesh(
            &field,
            &mask,
            &frozen,
            0.0,
            0.0,
            Vec2::new(-80.0, -80.0),
            Vec2::new(0.0, 80.0),
            8,
            false,
        )
        .expect("wet chunk");
        let VertexAttributeValues::Float32x4(colors) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).expect("colors")
        else {
            panic!("expected float colors");
        };
        assert!(colors.iter().all(|c| c[0] >= 0.0), "depth must be non-negative");
        assert!(colors.iter().all(|c| c[1] > 0.5), "frozen flag must be set");
        assert!(colors.iter().all(|c| (0.0..=1.0).contains(&c[2])));
    }

    #[test]
    fn frozen_only_sea_mesh_drops_liquid_triangles() {
        let (field, mask) = sea_world();
        let liquid = vec![false; field.len()];
        let all = sea_chunk_mesh(
            &field, &mask, &liquid, 0.0, 0.0,
            Vec2::new(-80.0, -80.0), Vec2::new(0.0, 80.0), 8, false,
        )
        .expect("wet chunk");
        let none = sea_chunk_mesh(
            &field, &mask, &liquid, 0.0, 0.0,
            Vec2::new(-80.0, -80.0), Vec2::new(0.0, 80.0), 8, true,
        );
        assert!(none.is_none(), "nothing is frozen, so the frozen submesh is empty");
        assert!(all.indices().expect("indices").len() > 0);
    }

    #[test]
    fn lake_basin_mesh_follows_the_basin_not_a_square() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, 8.0 + x as f32 * 0.2);
            }
        }
        for z in 3..13 {
            for x in 3..6 {
                field.set(x, z, -4.0);
            }
        }
        for z in 3..6 {
            for x in 6..10 {
                field.set(x, z, -4.0);
            }
        }
        let hydro = priority_flood(&field, 0.0);
        let lakes = crate::terrain::hydrology::find_lakes(&field, &hydro, 0.0, &vec![false; field.len()]);
        let lake = lakes
            .iter()
            .max_by_key(|lake| lake.cells.len())
            .expect("the L-shaped pit is a lake");
        assert!(lake.cells.len() >= 30);

        let (anchor, mesh) = lake_basin_mesh(lake, &field, false).expect("basin mesh");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        assert!(!positions.is_empty());
        assert_eq!(positions.len() % 4, 0, "one quad per face");

        let quads = positions.len() / 4;
        let bounding_quads = 9 * 11;
        assert!(
            quads < bounding_quads,
            "the mesh must clip the L basin, not its bounding square ({quads} vs {bounding_quads})"
        );
        for point in positions {
            assert!(
                point[1] <= lake.surface + 1e-3,
                "no basin vertex may float above the lake surface"
            );
        }
        assert!(
            positions.iter().any(|p| (p[1] - lake.surface).abs() < 1e-3),
            "interior vertices sit exactly on the lake surface"
        );
        assert!(anchor.is_finite());
    }

    fn flow_field(field: &ScalarField) -> FlowField {
        let hydro = priority_flood(field, -1000.0);
        bake_flow_vectors(field, &hydro)
    }

    fn straight_field(accum: f32) -> (ScalarField, ScalarField, Vec<usize>) {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for index in 0..field.len() {
            field.set_at(index, 10.0);
        }
        let mut accumulation = ScalarField::new(16, 160.0, WorldTopology::Finite);
        accumulation.fill(accum);
        let path = (2..8).map(|x| field.index(x, 8)).collect();
        (field, accumulation, path)
    }

    fn network(field: &ScalarField, accumulation: &ScalarField, paths: Vec<Vec<usize>>) -> Option<Mesh> {
        let flow = flow_field(field);
        let frozen = vec![false; field.len()];
        river_network_mesh(&paths, field, accumulation, &flow, &[], &[], &frozen, 0.0, Season::Summer)
    }

    #[test]
    fn river_network_needs_at_least_two_stations() {
        let (field, accumulation, path) = straight_field(10.0);
        assert!(network(&field, &accumulation, vec![]).is_none());
        assert!(network(&field, &accumulation, vec![path[..1].to_vec()]).is_none());
        assert!(network(&field, &accumulation, vec![path]).is_some());
    }

    #[test]
    fn river_network_is_a_single_continuous_mesh() {
        let (field, accumulation, path) = straight_field(10.0);
        let mesh = network(&field, &accumulation, vec![path]).expect("network");
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        let indices = mesh.indices().expect("indices");
        assert!(positions.len() >= 5, "at least one 5-vertex station");
        assert!(!indices.is_empty(), "the ribbon must be triangulated");
        assert_eq!(indices.len() % 3, 0, "whole triangles");
    }

    #[test]
    fn river_network_center_encodes_positive_depth() {
        let (field, accumulation, path) = straight_field(16.0);
        let mesh = network(&field, &accumulation, vec![path]).expect("network");
        let VertexAttributeValues::Float32x4(colors) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).expect("colors")
        else {
            panic!("expected float colors");
        };
        assert!(
            colors.iter().any(|c| c[2] > 0.04),
            "the centre vertex must carry positive water depth for Beer-Lambert absorption"
        );
    }

    #[test]
    fn river_network_skirts_sit_below_the_bank() {
        let (field, accumulation, path) = straight_field(10.0);
        let mesh = network(&field, &accumulation, vec![path]).expect("network");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        let VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("uvs")
        else {
            panic!("expected float uvs");
        };
        for (pos, uv) in positions.iter().zip(uvs.iter()) {
            if uv[0] <= 0.001 || uv[0] >= 0.999 {
                let ground = field.sample_world(Vec2::new(pos[0], pos[2]));
                assert!(
                    pos[1] <= ground + 1e-3,
                    "the bank skirt must never float above the local ground"
                );
            }
        }
    }

    #[test]
    fn river_network_widens_with_discharge() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        field.fill(10.0);
        let flow = flow_field(&field);
        let frozen = vec![false; field.len()];
        let pos = field.world_of(5, 8);
        let mut low = ScalarField::new(16, 160.0, WorldTopology::Finite);
        low.fill(4.0);
        let mut high = ScalarField::new(16, 160.0, WorldTopology::Finite);
        high.fill(64.0);
        let narrow = solve_station_volume(pos, Vec2::X, &field, &low, &flow, &frozen, Season::Summer, 0.0);
        let wide = solve_station_volume(pos, Vec2::X, &field, &high, &flow, &frozen, Season::Summer, 0.0);
        assert!(
            wide.left_w + wide.right_w > narrow.left_w + narrow.right_w,
            "more discharge must widen the channel"
        );
    }

    #[test]
    fn river_network_narrow_canyon_is_narrower_than_a_wide_valley() {
        let mut gorge = ScalarField::new(16, 160.0, WorldTopology::Finite);
        gorge.fill(20.0);
        for x in 2..8 {
            gorge.set(x, 8, 4.0);
        }
        let mut valley = ScalarField::new(16, 160.0, WorldTopology::Finite);
        valley.fill(20.0);
        for z in 4..13 {
            for x in 2..8 {
                valley.set(x, z, 4.0);
            }
        }
        let mut accumulation = ScalarField::new(16, 160.0, WorldTopology::Finite);
        accumulation.fill(20.0);
        let pos = valley.world_of(5, 8);
        let frozen = vec![false; valley.len()];
        let g = solve_station_volume(pos, Vec2::X, &gorge, &accumulation, &flow_field(&gorge), &frozen, Season::Summer, 0.0);
        let v = solve_station_volume(pos, Vec2::X, &valley, &accumulation, &flow_field(&valley), &frozen, Season::Summer, 0.0);
        assert!(
            g.left_w + g.right_w < v.left_w + v.right_w,
            "the same discharge in a narrow gorge must stay narrower than in a wide valley"
        );
    }

    #[test]
    fn river_network_unwraps_across_the_torus_seam() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::FiniteTorus);
        for index in 0..field.len() {
            field.set_at(index, 10.0);
        }
        let accumulation = ScalarField::new(16, 160.0, WorldTopology::FiniteTorus);
        let path: Vec<usize> = (13..=18).map(|x| field.index(x, 8)).collect();
        let mesh = network(&field, &accumulation, vec![path]).expect("seam network");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        let idx: Vec<usize> = mesh.indices().expect("indices").iter().collect();
        let mut max_edge = 0.0f32;
        for tri in idx.chunks(3) {
            for k in 0..3 {
                let a = positions[tri[k]];
                let b = positions[tri[(k + 1) % 3]];
                max_edge = max_edge.max(Vec2::new(a[0] - b[0], a[2] - b[2]).length());
            }
        }
        assert!(
            max_edge < field.size * 0.3,
            "no triangle may stretch across the seam: max edge {max_edge}"
        );
    }

    #[test]
    fn river_network_stitches_a_confluence_into_one_mesh() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        field.fill(10.0);
        let mut accumulation = ScalarField::new(16, 160.0, WorldTopology::Finite);
        accumulation.fill(20.0);
        let trunk: Vec<usize> = (2..12).map(|x| field.index(x, 8)).collect();
        let tributary: Vec<usize> = (3..9).map(|z| field.index(6, z)).collect();
        let mesh = network(&field, &accumulation, vec![trunk, tributary]).expect("confluence mesh");
        assert!(!mesh.indices().expect("indices").is_empty());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions").len() >= 10);
    }

    fn scarp_field() -> (ScalarField, Waterfall) {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, if x < 8 { 30.0 } else { 12.0 });
            }
        }
        let lip = field.index(7, 8);
        let plunge = field.index(8, 8);
        let fall = Waterfall {
            index: lip,
            next: plunge,
            drop: 18.0,
            lip: field.world_of(7, 8),
            plunge: field.world_of(8, 8),
            pool_depth: 4.0,
            pool_radius: field.spacing() * 2.0,
        };
        (field, fall)
    }

    #[test]
    fn cascade_needs_a_real_drop() {
        let (field, fall) = scarp_field();
        assert!(cascade_mesh(&fall, &field, 2.0).is_some());
        let mut flat = field.clone();
        flat.fill(5.0);
        assert!(cascade_mesh(&fall, &flat, 2.0).is_none());
    }

    #[test]
    fn cascade_spans_the_drop_and_sits_at_the_plunge() {
        let (field, fall) = scarp_field();
        let mesh = cascade_mesh(&fall, &field, 2.0).expect("cascade");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        let top = positions.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        let sheet_bottom = positions[..14].iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        assert!(
            top - sheet_bottom >= fall.drop * 0.9,
            "the cascade sheet must span the drop: {top} -> {sheet_bottom}"
        );
        let foam_centre = Vec2::new(
            positions[14..].iter().map(|p| p[0]).sum::<f32>() / (positions.len() - 14) as f32,
            positions[14..].iter().map(|p| p[2]).sum::<f32>() / (positions.len() - 14) as f32,
        );
        assert!(
            foam_centre.distance(fall.plunge) < fall.pool_radius,
            "the foam apron must sit on the plunge pool"
        );
        assert!(mesh.indices().is_some());
    }

    #[test]
    fn cascade_orients_across_the_torus_seam() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::FiniteTorus);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, if x < 15 { 30.0 } else { 12.0 });
            }
        }
        let fall = Waterfall {
            index: field.index(14, 8),
            next: field.index(15, 8),
            drop: 18.0,
            lip: field.world_of(14, 8),
            plunge: field.world_of(15, 8),
            pool_depth: 4.0,
            pool_radius: field.spacing() * 2.0,
        };
        let mesh = cascade_mesh(&fall, &field, 2.0).expect("seam cascade");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        let min_x = positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
        let max_x = positions.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max_x - min_x < field.size * 0.5,
            "the cascade must follow the short way across the seam"
        );
    }

    #[test]
    fn cascade_bakes_world_space_flow_vectors() {
        let (field, fall) = scarp_field();
        let mesh = cascade_mesh(&fall, &field, 2.0).expect("cascade");
        let VertexAttributeValues::Float32x4(colors) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).expect("colors")
        else {
            panic!("expected float colors");
        };
        let expected_speed = fall.drop.clamp(0.5, 6.0);
        for sheet in colors.iter().take(14) {
            assert!(sheet[0] > 0.0, "the lip flow must point downstream (+x)");
            assert!(sheet[1].abs() < 1e-4);
            assert!((Vec2::new(sheet[0], sheet[1]).length() - expected_speed).abs() < 1e-4);
            assert_eq!(sheet[2], expected_speed);
        }
        for foam in colors.iter().skip(14) {
            assert_eq!(foam[0], 0.0, "the plunge pool carries no directional flow");
            assert_eq!(foam[1], 0.0);
        }
    }

    fn channel_scarp(channel_width: f32) -> (ScalarField, Waterfall) {
        let mut field = ScalarField::new(32, 128.0, WorldTopology::Finite);
        for z in 0..32 {
            for x in 0..32 {
                field.set(x, z, if x < 20 { 10.0 } else { 2.0 });
            }
        }
        for z in 0..32 {
            for x in 0..20 {
                if field.world_of(x, z).y.abs() <= channel_width * 0.5 {
                    field.set(x, z, 9.0);
                }
            }
        }
        let fall = Waterfall {
            index: field.index(19, 16),
            next: field.index(20, 16),
            drop: 8.0,
            lip: field.world_of(19, 16),
            plunge: field.world_of(20, 16),
            pool_depth: 4.0,
            pool_radius: field.spacing() * 2.0,
        };
        (field, fall)
    }

    #[test]
    fn cascade_lip_width_matches_the_feeding_channel() {
        let (field, fall) = channel_scarp(8.0);
        let narrow = cascade_lip_width(&fall, &field, 2.0);
        assert!(
            narrow > 6.0 && narrow < 14.0,
            "the lip must match the 8 m channel, got {narrow}"
        );
        let (wide_field, wide_fall) = channel_scarp(20.0);
        let wide = cascade_lip_width(&wide_fall, &wide_field, 2.0);
        assert!(
            wide > narrow * 1.5,
            "a wider river must feed a wider lip: {narrow} -> {wide}"
        );
        assert!(wide < 30.0, "the lip stays bounded: {wide}");
    }

    #[test]
    fn cascade_lip_width_falls_back_on_open_ground() {
        let (field, fall) = scarp_field();
        assert_eq!(cascade_lip_width(&fall, &field, 2.5), 2.5);
    }

    #[test]
    fn cascade_mesh_lip_tracks_the_feeding_channel() {
        let (field, fall) = channel_scarp(8.0);
        let mesh = cascade_mesh(&fall, &field, 2.0).expect("narrow cascade");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        let spread = Vec2::new(
            positions[0][0] - positions[1][0],
            positions[0][2] - positions[1][2],
        )
        .length();
        let derived = cascade_lip_width(&fall, &field, 2.0);
        assert!(
            (spread - derived).abs() < 1e-3,
            "the mesh lip {spread} must equal the derived width {derived}"
        );
        let (wide_field, wide_fall) = channel_scarp(20.0);
        let wide_mesh = cascade_mesh(&wide_fall, &wide_field, 2.0).expect("wide cascade");
        let VertexAttributeValues::Float32x3(wide_positions) =
            wide_mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        let wide_spread = Vec2::new(
            wide_positions[0][0] - wide_positions[1][0],
            wide_positions[0][2] - wide_positions[1][2],
        )
        .length();
        assert!(
            wide_spread > spread * 1.5,
            "the wide river's cascade lip must be wider: {spread} vs {wide_spread}"
        );
    }

    #[test]
    fn water_field_samples_sea_and_skips_dry_ground() {
        let (field, mask) = sea_world();
        let cells = vec![0usize; field.len()];
        let temps = vec![Temperature::Temperate; 1];
        let geos = vec![Geothermal::None; 1];
        let ctx = WaterCellContext {
            cell_of_vertex: &cells,
            temperature: &temps,
            geothermal: &geos,
        };
        let water = WaterField::build(
            &field,
            0.0,
            &mask,
            &[],
            &[],
            &[],
            &[],
            &ScalarField::new(16, 160.0, WorldTopology::Finite),
            Season::Summer,
            &ctx,
        );
        let wet_point = field.world_of(1, 8);
        let sample = water
            .sample_at(&field, wet_point)
            .expect("the submerged west must sample as water");
        assert_eq!(sample.kind, WaterKind::Sea);
        assert_eq!(sample.salinity, Salinity::Marine);
        assert!(!sample.frozen, "marine temperate summer water is liquid");
        assert!(sample.depth > 0.0);

        let dry_point = field.world_of(14, 8);
        assert!(
            water.sample_at(&field, dry_point).is_none(),
            "dry high ground has no water"
        );
    }

    #[test]
    fn water_field_wraps_on_the_torus() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::FiniteTorus);
        field.fill(-5.0);
        let mask = vec![true; field.len()];
        let cells = vec![0usize; field.len()];
        let temps = vec![Temperature::Temperate; 1];
        let geos = vec![Geothermal::None; 1];
        let ctx = WaterCellContext {
            cell_of_vertex: &cells,
            temperature: &temps,
            geothermal: &geos,
        };
        let water = WaterField::build(
            &field,
            0.0,
            &mask,
            &[],
            &[],
            &[],
            &[],
            &ScalarField::new(16, 160.0, WorldTopology::FiniteTorus),
            Season::Summer,
            &ctx,
        );
        let left = water
            .sample_at(&field, Vec2::new(-79.0, 0.0))
            .expect("west edge");
        let right = water
            .sample_at(&field, Vec2::new(79.0, 0.0))
            .expect("east edge");
        assert!((left.surface_y - right.surface_y).abs() < 1e-4);
        assert_eq!(left.kind, right.kind);
    }

    #[test]
    fn water_field_freezes_per_cell_not_globally() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        field.fill(-5.0);
        let mask = vec![true; field.len()];
        let cells = vec![0usize; field.len()];
        let hot_temps = vec![Temperature::Scorching; 1];
        let cold_temps = vec![Temperature::Freezing; 1];
        let geo = vec![Geothermal::None; 1];
        let hot = WaterCellContext {
            cell_of_vertex: &cells,
            temperature: &hot_temps,
            geothermal: &geo,
        };
        let cold = WaterCellContext {
            cell_of_vertex: &cells,
            temperature: &cold_temps,
            geothermal: &geo,
        };
        let empty = ScalarField::new(16, 160.0, WorldTopology::Finite);
        let summer = WaterField::build(
            &field, 0.0, &mask, &[], &[], &[], &[], &empty, Season::Summer, &hot,
        );
        assert!(
            !summer.frozen[field.index(8, 8)],
            "open sea in a hot summer is liquid"
        );
        let winter = WaterField::build(
            &field, 0.0, &mask, &[], &[], &[], &[], &empty, Season::Winter, &cold,
        );
        assert!(
            winter.frozen[field.index(8, 8)],
            "freezing marine water in winter is frozen"
        );
    }
}
