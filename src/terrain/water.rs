use std::collections::{BTreeSet, HashSet, VecDeque};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::{Lake, Waterfall};
use crate::terrain::params::{Geothermal, Salinity, Temperature};
use crate::terrain::season::{self, Season};
use crate::terrain::topology::WorldTopology;

pub const RIBBON_LIFT: f32 = 0.06;
pub const BANK_TUCK: f32 = 0.15;
pub const WATER_NONE: f32 = f32::NEG_INFINITY;
pub const FOAM_DEPTH: f32 = 0.8;

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
    pub depth: f32,
    pub salinity: Salinity,
    pub frozen: bool,
    pub flow: Vec2,
    pub kind: WaterKind,
}

#[derive(Clone, Debug)]
pub struct WaterBody {
    pub center: Vec2,
    pub radius: f32,
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
                    let speed = (accumulation.get_at(vertex).max(0.0).sqrt() * 0.05)
                        .clamp(0.3, 2.5);
                    flow = delta * speed;
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
                heights.get_at(fall.index) + 0.15,
                WaterKind::Waterfall,
                Salinity::Freshwater,
                delta * 3.0,
                season,
                ctx,
            );
            field.paint(
                fall.next,
                heights.get_at(fall.next) + 0.08,
                WaterKind::Waterfall,
                Salinity::Freshwater,
                delta * 1.5,
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
        .filter(|(_, lake)| lake.cells.len() >= minimum_area && lake.depth > 0.05)
        .map(|(index, lake)| {
            let mut centre = Vec2::ZERO;
            for &cell in &lake.cells {
                centre += heights.world_of(heights.coords(cell).0, heights.coords(cell).1);
            }
            centre /= lake.cells.len().max(1) as f32;
            let radius = lake
                .cells
                .iter()
                .map(|&cell| {
                    let (x, z) = heights.coords(cell);
                    heights
                        .world_of(x, z)
                        .distance(centre)
                        .max(heights.spacing())
                })
                .fold(0.0f32, f32::max);
            WaterBody {
                center: centre,
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
    let mut near_sea_top = false;
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
            near_sea_top = near_sea_top || wet[index];
        }
    }
    if !near_sea_top {
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
        uvs.push([point.x / 8.0, point.y / 8.0]);
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
            if !corners.iter().all(|&(cx, cz)| wet[cz * stride + cx]) {
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
            if inside >= 3 {
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
            uvs.push([local.x / 8.0, local.y / 8.0]);
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

pub fn river_half_width(accumulation: f32, spacing: f32) -> f32 {
    spacing * (0.35 + 0.05 * accumulation.max(0.0).sqrt()).clamp(0.35, 3.0)
}

fn chaikin(points: &[Vec2]) -> Vec<Vec2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    for pair in points.windows(2) {
        out.push(pair[0] * 0.75 + pair[1] * 0.25);
        out.push(pair[0] * 0.25 + pair[1] * 0.75);
    }
    if let Some(last) = points.last() {
        out.push(*last);
    }
    out
}

pub fn river_ribbon_mesh(
    path: &[usize],
    heights: &ScalarField,
    accumulation: &ScalarField,
) -> Option<(Vec2, Mesh)> {
    if path.len() < 2 {
        return None;
    }
    let raw: Vec<Vec2> = path
        .iter()
        .map(|&cell| heights.world_of(heights.coords(cell).0, heights.coords(cell).1))
        .collect();
    let anchor = raw[0];
    let mut local = vec![Vec2::ZERO];
    let mut cursor = Vec2::ZERO;
    for pair in raw.windows(2) {
        cursor += heights.topology.delta_xz(pair[0], pair[1], heights.size);
        local.push(cursor);
    }

    let smoothed = chaikin(&chaikin(&local));
    let mut centre: Vec<Vec2> = Vec::with_capacity(smoothed.len());
    for point in smoothed {
        if centre
            .last()
            .is_some_and(|last| last.distance_squared(point) < 1e-6)
        {
            continue;
        }
        centre.push(point);
    }
    if centre.len() < 2 {
        return None;
    }

    let spacing = heights.spacing();
    let skirt = spacing * 0.3;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut travelled = 0.0f32;
    let mut previous: Option<Vec2> = None;

    for index in 0..centre.len() {
        let point = centre[index];
        let world = anchor + point;
        let ground = heights.sample_world(world);
        let accum = accumulation.sample_world(world);
        let half = river_half_width(accum, spacing);

        let before = centre
            .get(index.saturating_sub(1))
            .copied()
            .unwrap_or(point);
        let after = centre.get(index + 1).copied().unwrap_or(point);
        let tangent = after - before;
        let side = Vec2::new(-tangent.y, tangent.x).normalize_or_zero();
        let side = if side.length_squared() < 1e-9 {
            Vec2::Y
        } else {
            side
        };
        let flow_speed = (accum.max(0.0).sqrt() * 0.05).clamp(0.3, 2.5);
        let flow = tangent.normalize_or_zero() * flow_speed;

        if let Some(before) = previous {
            travelled += point.distance(before);
        }
        previous = Some(point);

        let surface_y = ground + RIBBON_LIFT;
        let skirt_y = ground - BANK_TUCK;
        let v = travelled / (spacing * 2.0).max(1e-3);
        let cross = [
            (point - side * (half + skirt), skirt_y, 0.0),
            (point - side * half, surface_y, 0.15),
            (point + side * half, surface_y, 0.85),
            (point + side * (half + skirt), skirt_y, 1.0),
        ];
        for (offset, y, u) in cross {
            positions.push([offset.x, y, offset.y]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([u, v]);
            colors.push([flow.x, flow.y, flow_speed, 1.0]);
        }
        if index > 0 {
            let a = (index as u32 - 1) * 4;
            let b = index as u32 * 4;
            for k in 0..3u32 {
                indices.extend_from_slice(&[a + k, a + k + 1, b + k, b + k, a + k + 1, b + k + 1]);
            }
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some((anchor, mesh))
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
    let probe = fall.lip + upstream * (spacing * 1.5);
    let reference = heights.sample_world(probe);
    let margin = (fall.drop * 0.1).clamp(0.5, 2.0);
    let step = spacing * 0.5;
    let max_walk = spacing * 8.0;
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
    width.clamp(spacing, spacing * 8.0)
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

    let top_y = lip_height + 0.15;
    let bottom_y = plunge_height + 0.05;
    let segments = 6usize;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for step in 0..=segments {
        let t = step as f32 / segments as f32;
        let centre = Vec3::new(fall.lip.x, 0.0, fall.lip.y) + flow_dir * t;
        let y = top_y - (top_y - bottom_y) * t * t;
        let half = width.max(heights.spacing()) * 0.5 * (1.0 + 0.5 * t);
        positions.push([(centre + side * half).x, y, (centre + side * half).z]);
        positions.push([(centre - side * half).x, y, (centre - side * half).z]);
        normals.push([side.x, 0.0, side.z]);
        normals.push([side.x, 0.0, side.z]);
        uvs.push([0.0, t]);
        uvs.push([1.0, t]);
        if step > 0 {
            let a = (step as u32 - 1) * 2;
            indices.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
    }

    let foam_y = plunge_height + 0.08;
    let foam_radius = fall.pool_radius.max(heights.spacing()) * 0.9;
    let foam_base = positions.len() as u32;
    positions.push([fall.plunge.x, foam_y, fall.plunge.y]);
    normals.push([0.0, 1.0, 0.0]);
    uvs.push([0.5, 0.5]);
    let ring = 16usize;
    for k in 0..=ring {
        let angle = k as f32 * std::f32::consts::TAU / ring as f32;
        let offset = Vec2::new(angle.cos(), angle.sin()) * foam_radius;
        positions.push([fall.plunge.x + offset.x, foam_y, fall.plunge.y + offset.y]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]);
    }
    for k in 0..ring as u32 {
        indices.extend_from_slice(&[foam_base, foam_base + 1 + k, foam_base + 2 + k]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
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
    use crate::terrain::hydrology::priority_flood;
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
            center: Vec2::ZERO,
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

    #[test]
    fn river_ribbon_needs_at_least_two_stations() {
        let (field, accumulation, path) = straight_field(10.0);
        assert!(river_ribbon_mesh(&[], &field, &accumulation).is_none());
        assert!(river_ribbon_mesh(&path[..1], &field, &accumulation).is_none());
        assert!(river_ribbon_mesh(&path, &field, &accumulation).is_some());
    }

    #[test]
    fn river_ribbon_is_a_four_vertex_per_station_strip() {
        let (field, accumulation, path) = straight_field(10.0);
        let (anchor, mesh) = river_ribbon_mesh(&path, &field, &accumulation).expect("ribbon");
        assert_eq!(anchor, field.world_of(2, 8));
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert!(positions.len() >= path.len() * 4 / 2);
        assert_eq!(positions.len() % 4, 0, "four vertices per station");
        assert!(mesh.indices().expect("indices").len() > 0);
    }

    #[test]
    fn river_ribbon_hugs_the_ground_and_widens_with_accumulation() {
        let (field, accumulation, path) = straight_field(16.0);
        let (_, mesh) = river_ribbon_mesh(&path, &field, &accumulation).expect("ribbon");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        for station in positions.chunks(4) {
            assert!(
                (station[1][1] - (10.0 + RIBBON_LIFT)).abs() < 1e-3,
                "the water surface rides just above the ground"
            );
            assert!(
                (station[2][1] - station[1][1]).abs() < 1e-6,
                "the crown is flat across the channel"
            );
            assert!(
                station[0][1] < station[1][1] - BANK_TUCK * 0.9,
                "the skirt must tuck below the bank"
            );
        }
        assert!(river_half_width(4.0, 4.0) < river_half_width(64.0, 4.0));
        assert!(river_half_width(10_000.0, 4.0) <= 4.0 * 3.0, "width stays bounded");
    }

    #[test]
    fn river_ribbon_encodes_flow_distance_in_uv() {
        let (field, accumulation, path) = straight_field(10.0);
        let (_, mesh) = river_ribbon_mesh(&path, &field, &accumulation).expect("ribbon");
        let VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("uvs")
        else {
            panic!("expected float uvs");
        };
        assert!(uvs.iter().any(|uv| uv[0] <= 0.01));
        assert!(uvs.iter().any(|uv| uv[0] >= 0.99));
        let first_v = uvs[0][1];
        let last_v = uvs[uvs.len() - 1][1];
        assert!(last_v > first_v, "UV.y must grow downstream");
    }

    #[test]
    fn river_ribbon_unwraps_across_the_torus_seam() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::FiniteTorus);
        for index in 0..field.len() {
            field.set_at(index, 10.0);
        }
        let accumulation = ScalarField::new(16, 160.0, WorldTopology::FiniteTorus);
        let path: Vec<usize> = (13..=18).map(|x| field.index(x, 8)).collect();
        let (anchor, mesh) = river_ribbon_mesh(&path, &field, &accumulation).expect("seam ribbon");
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("expected float positions");
        };
        let max_x = positions.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
        let min_x = positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
        assert!(
            max_x - min_x < field.size * 0.5,
            "the ribbon must not stretch across the whole map"
        );
        assert_eq!(anchor, field.world_of(13, 8));
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
    fn river_ribbon_encodes_flow_in_vertex_colors() {
        let (field, accumulation, path) = straight_field(100.0);
        let (_, mesh) = river_ribbon_mesh(&path, &field, &accumulation).expect("ribbon");
        let VertexAttributeValues::Float32x4(colors) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).expect("colors")
        else {
            panic!("expected float colors");
        };
        let expected_speed = (100.0f32.sqrt() * 0.05).clamp(0.3, 2.5);
        assert!(
            colors.iter().all(|c| (c[0] - expected_speed).abs() < 1e-4),
            "the flow must point downstream along +x"
        );
        assert!(colors.iter().all(|c| c[1].abs() < 1e-4));
        assert!(colors.iter().all(|c| (c[2] - expected_speed).abs() < 1e-4));
        assert!(colors.iter().all(|c| c[3] == 1.0));
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
