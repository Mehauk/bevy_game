use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;
use crate::terrain::params::{Precipitation, Substrate};
use crate::terrain::seed::{WorldSeed, splitmix64, STAGE_CRATERS, STAGE_DROPLETS};
use crate::terrain::voronoi::VertexBlendCache;

const TILT_EPSILON: f32 = 1e-3;
const DEPRESSION_EPSILON: f32 = 1e-4;

const SECTOR_DIRS: [(isize, isize); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

#[derive(Clone, Copy, PartialEq)]
struct HeightKey(f32);

impl Eq for HeightKey {}

impl PartialOrd for HeightKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeightKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

pub fn precipitation_intensity(precipitation: Precipitation) -> f32 {
    match precipitation {
        Precipitation::Torrential => 1.0,
        Precipitation::Heavy => 0.75,
        Precipitation::Moderate => 0.45,
        Precipitation::Low => 0.18,
        Precipitation::None => 0.03,
    }
}

pub struct PrecipitationInput<'a> {
    pub cell_of_vertex: &'a [usize],
    pub cell_precipitation: &'a [Precipitation],
    pub cell_rain_shadow: &'a [f32],
    pub wind: Vec2,
    pub relief: f32,
}

pub fn generate_precipitation(field: &ScalarField, input: &PrecipitationInput) -> ScalarField {
    generate_precipitation_blended(field, input, None)
}

pub fn generate_precipitation_blended(
    field: &ScalarField,
    input: &PrecipitationInput,
    cache: Option<&VertexBlendCache>,
) -> ScalarField {
    let mut out = ScalarField::new(field.dim, field.size, field.topology);
    let wind = input.wind.normalize_or_zero();

    for z in 0..field.dim {
        for x in 0..field.dim {
            let index = field.index(x, z);
            let blended = cache
                .map(|cache| cache.weights_at(index))
                .filter(|weights| !weights.is_empty());
            let (base, shadow) = match blended {
                Some(weights) => {
                    let mut base = 0.0f32;
                    let mut shadow = 0.0f32;
                    let mut total = 0.0f32;
                    for &(cell, weight) in weights {
                        let intensity = input
                            .cell_precipitation
                            .get(cell)
                            .copied()
                            .map(precipitation_intensity)
                            .unwrap_or(0.4);
                        let cell_shadow = input
                            .cell_rain_shadow
                            .get(cell)
                            .copied()
                            .unwrap_or(0.0);
                        base += intensity * weight;
                        shadow += cell_shadow * weight;
                        total += weight;
                    }
                    if total > 0.0 {
                        (base / total, shadow / total)
                    } else if let Some(&cell) = input.cell_of_vertex.get(index) {
                        (cell_base(input, cell), cell_shadow(input, cell))
                    } else {
                        continue;
                    }
                }
                None => {
                    let Some(&cell) = input.cell_of_vertex.get(index) else {
                        continue;
                    };
                    (cell_base(input, cell), cell_shadow(input, cell))
                }
            };

            let slope = field.gradient(x, z);
            let windward = slope.dot(wind);
            let orographic = (windward * 1.2).clamp(-0.3, 0.4);
            let value = (base * (1.0 - shadow * 0.7) + orographic).clamp(0.005, 1.5);
            out.set(x, z, value);
        }
    }
    out
}

fn cell_base(input: &PrecipitationInput, cell: usize) -> f32 {
    input
        .cell_precipitation
        .get(cell)
        .copied()
        .map(precipitation_intensity)
        .unwrap_or(0.4)
}

fn cell_shadow(input: &PrecipitationInput, cell: usize) -> f32 {
    input
        .cell_rain_shadow
        .get(cell)
        .copied()
        .unwrap_or(0.0)
}

pub struct Hydrology {
    pub filled: ScalarField,
    pub routed: ScalarField,
    pub downstream: Vec<Option<usize>>,
    pub secondary: Vec<Option<usize>>,
    pub primary_weight: Vec<f32>,
    pub accumulation: ScalarField,
    pub visit_order: Vec<usize>,
    pub depressions: Vec<bool>,
}

impl Hydrology {
    #[cfg(test)]
    pub fn total_precipitation_captured(&self) -> f32 {
        (0..self.accumulation.len())
            .filter(|&i| self.downstream[i].is_none())
            .map(|i| self.accumulation.get_at(i))
            .sum()
    }
}

pub fn priority_flood(heights: &ScalarField, sea_level: f32) -> Hydrology {
    let count = heights.len();
    let mut filled = heights.clone();
    let mut visited = vec![false; count];
    let mut heap: BinaryHeap<Reverse<(HeightKey, usize)>> = BinaryHeap::new();

    let mut seeds: Vec<usize> = Vec::new();
    if heights.topology.wraps() {
        for index in 0..count {
            if heights.get_at(index) <= sea_level {
                seeds.push(index);
            }
        }
        if seeds.is_empty() {
            let lowest = (0..count)
                .min_by(|a, b| heights.get_at(*a).total_cmp(&heights.get_at(*b)))
                .unwrap_or(0);
            seeds.push(lowest);
        }
    } else {
        for z in 0..heights.dim {
            for x in 0..heights.dim {
                if x == 0 || z == 0 || x == heights.dim - 1 || z == heights.dim - 1 {
                    seeds.push(heights.index(x, z));
                }
            }
        }
    }

    for &seed in &seeds {
        let level = heights.get_at(seed).max(sea_level);
        filled.set_at(seed, level);
        visited[seed] = true;
        heap.push(Reverse((HeightKey(level), seed)));
    }

    while let Some(Reverse((_, index))) = heap.pop() {
        let (x, z) = heights.coords(index);
        for neighbour in heights.neighbours8(x, z) {
            if visited[neighbour] || neighbour == index {
                continue;
            }
            visited[neighbour] = true;
            let level = heights.get_at(neighbour).max(filled.get_at(index));
            filled.set_at(neighbour, level);
            heap.push(Reverse((HeightKey(level), neighbour)));
        }
    }

    for index in 0..count {
        if !visited[index] {
            filled.set_at(index, heights.get_at(index).max(sea_level));
        }
    }

    let depressions: Vec<bool> = (0..count)
        .map(|index| filled.get_at(index) - heights.get_at(index) > DEPRESSION_EPSILON)
        .collect();

    let routed = tilt_toward_outlets(&filled);
    let (downstream, secondary, primary_weight) = flow_directions(&routed);

    let mut visit_order: Vec<usize> = (0..count).collect();
    visit_order.sort_by(|a, b| {
        routed
            .get_at(*b)
            .total_cmp(&routed.get_at(*a))
            .then(a.cmp(b))
    });

    Hydrology {
        filled,
        routed,
        downstream,
        secondary,
        primary_weight,
        accumulation: ScalarField::new(heights.dim, heights.size, heights.topology),
        visit_order,
        depressions,
    }
}

fn tilt_toward_outlets(filled: &ScalarField) -> ScalarField {
    let count = filled.len();
    let mut tilt = vec![0.0f32; count];
    let mut queued = vec![false; count];
    let mut queue: VecDeque<usize> = VecDeque::new();

    for index in 0..count {
        let (x, z) = filled.coords(index);
        let here = filled.get_at(index);
        if filled
            .neighbours8(x, z)
            .iter()
            .any(|&n| filled.get_at(n) < here)
        {
            queued[index] = true;
            queue.push_back(index);
        }
    }

    while let Some(index) = queue.pop_front() {
        let (x, z) = filled.coords(index);
        let level = filled.get_at(index);
        let next = tilt[index] + TILT_EPSILON;
        for neighbour in filled.neighbours8(x, z) {
            if !queued[neighbour] && filled.get_at(neighbour) == level {
                queued[neighbour] = true;
                tilt[neighbour] = next;
                queue.push_back(neighbour);
            }
        }
    }

    let scales = plateau_tilt_scales(filled, &tilt);
    let mut routed = filled.clone();
    for index in 0..count {
        routed.set_at(index, filled.get_at(index) + tilt[index] * scales[index]);
    }
    routed
}

fn plateau_tilt_scales(filled: &ScalarField, tilt: &[f32]) -> Vec<f32> {
    let count = filled.len();
    let mut scale = vec![1.0f32; count];
    let mut assigned = vec![usize::MAX; count];
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..count {
        if assigned[start] != usize::MAX {
            continue;
        }
        let level = filled.get_at(start);
        assigned[start] = start;
        stack.clear();
        stack.push(start);
        let mut cells: Vec<usize> = Vec::new();
        let mut max_tilt = 0.0f32;
        let mut gap = f32::INFINITY;
        while let Some(index) = stack.pop() {
            cells.push(index);
            max_tilt = max_tilt.max(tilt[index]);
            let (x, z) = filled.coords(index);
            for neighbour in filled.neighbours8(x, z) {
                let value = filled.get_at(neighbour);
                if value == level {
                    if assigned[neighbour] == usize::MAX {
                        assigned[neighbour] = start;
                        stack.push(neighbour);
                    }
                } else if value > level {
                    gap = gap.min(value - level);
                }
            }
        }
        let plateau_scale = if max_tilt > 0.0 && gap.is_finite() {
            ((gap * 0.5) / max_tilt).min(1.0)
        } else {
            1.0
        };
        for index in cells {
            scale[index] = plateau_scale;
        }
    }
    scale
}

fn sector_target(
    routed: &ScalarField,
    x: usize,
    z: usize,
    dir: (isize, isize),
    here: f32,
) -> Option<usize> {
    let nx = x as isize + dir.0;
    let nz = z as isize + dir.1;
    if !routed.topology.wraps()
        && (nx < 0 || nz < 0 || nx >= routed.dim as isize || nz >= routed.dim as isize)
    {
        return None;
    }
    let index = routed.index(routed.wrap(nx), routed.wrap(nz));
    if routed.get_at(index) < here {
        Some(index)
    } else {
        None
    }
}

fn flow_directions(
    routed: &ScalarField,
) -> (Vec<Option<usize>>, Vec<Option<usize>>, Vec<f32>) {
    let count = routed.len();
    let mut primary: Vec<Option<usize>> = vec![None; count];
    let mut secondary: Vec<Option<usize>> = vec![None; count];
    let mut weight: Vec<f32> = vec![1.0; count];
    let quarter = std::f32::consts::FRAC_PI_4;

    for index in 0..count {
        let (x, z) = routed.coords(index);
        let here = routed.get_at(index);
        let slope = routed.gradient(x, z);
        let descent = Vec2::new(-slope.x, -slope.y);

        let edge_outlet = !routed.topology.wraps()
            && (x == 0 || z == 0 || x + 1 >= routed.dim || z + 1 >= routed.dim);

        if edge_outlet {
            continue;
        }

        if descent.length_squared() > 1e-12 {
            let mut theta = descent.y.atan2(descent.x);
            if theta < 0.0 {
                theta += std::f32::consts::TAU;
            }
            let sector = ((theta / quarter) as usize).min(7);
            let split = ((theta - sector as f32 * quarter) / quarter).clamp(0.0, 1.0);
            let a = sector_target(routed, x, z, SECTOR_DIRS[sector], here);
            let b = sector_target(routed, x, z, SECTOR_DIRS[(sector + 1) % 8], here);
            match (a, b) {
                (Some(ta), Some(tb)) => {
                    primary[index] = Some(ta);
                    secondary[index] = Some(tb);
                    weight[index] = 1.0 - split;
                }
                (Some(ta), None) => {
                    primary[index] = Some(ta);
                }
                (None, Some(tb)) => {
                    primary[index] = Some(tb);
                }
                (None, None) => {}
            }
        }

        if primary[index].is_none() {
            let mut best: Option<(usize, f32)> = None;
            for neighbour in routed.neighbours8(x, z) {
                let drop = here - routed.get_at(neighbour);
                if drop > 0.0 && best.is_none_or(|(_, best_drop)| drop > best_drop) {
                    best = Some((neighbour, drop));
                }
            }
            if let Some((neighbour, _)) = best {
                primary[index] = Some(neighbour);
            }
        }
    }

    (primary, secondary, weight)
}

pub fn flow_accumulation(hydrology: &mut Hydrology, precipitation: &ScalarField) {
    hydrology.accumulation.fill(0.0);
    for index in 0..hydrology.accumulation.len() {
        hydrology
            .accumulation
            .set_at(index, precipitation.get_at(index));
    }
    for &index in &hydrology.visit_order {
        let carried = hydrology.accumulation.get_at(index);
        if carried <= 0.0 {
            continue;
        }
        if let Some(parent) = hydrology.downstream[index] {
            let share = hydrology.primary_weight[index];
            let current = hydrology.accumulation.get_at(parent);
            hydrology
                .accumulation
                .set_at(parent, current + carried * share);
        }
        if let Some(parent) = hydrology.secondary[index] {
            let share = 1.0 - hydrology.primary_weight[index];
            if share > 0.0 {
                let current = hydrology.accumulation.get_at(parent);
                hydrology
                    .accumulation
                    .set_at(parent, current + carried * share);
            }
        }
    }
}

pub fn river_cells(hydrology: &Hydrology, threshold: f32, ocean: &[bool]) -> Vec<bool> {
    (0..hydrology.accumulation.len())
        .map(|index| {
            !ocean[index] && hydrology.accumulation.get_at(index) >= threshold
        })
        .collect()
}

pub fn river_paths(hydrology: &Hydrology, threshold: f32, ocean: &[bool]) -> Vec<Vec<usize>> {
    let on_river = river_cells(hydrology, threshold, ocean);
    let mut paths: Vec<Vec<usize>> = Vec::new();
    let mut claimed = vec![false; on_river.len()];

    for &start in hydrology.visit_order.iter().rev() {
        if !on_river[start] || claimed[start] {
            continue;
        }
        let mut path = Vec::new();
        let mut cursor = Some(start);
        while let Some(index) = cursor {
            if claimed[index] {
                path.push(index);
                break;
            }
            claimed[index] = true;
            path.push(index);
            let next = hydrology.downstream[index];
            match next {
                Some(child) if on_river[child] => cursor = Some(child),
                Some(child) if ocean[child] || hydrology.depressions[child] => {
                    path.push(child);
                    cursor = None;
                }
                _ => cursor = None,
            }
        }
        if path.len() > 1 {
            paths.push(path);
        }
    }
    paths
}

#[derive(Clone, Debug)]
pub struct Waterfall {
    pub index: usize,
    pub next: usize,
    pub drop: f32,
    pub lip: Vec2,
    pub plunge: Vec2,
    pub pool_depth: f32,
    pub pool_radius: f32,
}

const WATERFALL_MIN_SLOPE: f32 = 1.0;
const WATERFALL_MAX_SCARP_STEPS: usize = 6;

fn step_distance(heights: &ScalarField, a: usize, b: usize, spacing: f32) -> f32 {
    let (ax, az) = heights.coords(a);
    let (bx, bz) = heights.coords(b);
    let dim = heights.dim as isize;
    let cell_delta = |a: usize, b: usize| -> f32 {
        let raw = (b as isize - a as isize).abs();
        if heights.topology.wraps() {
            raw.min(dim - raw) as f32
        } else {
            raw as f32
        }
    };
    let dx = cell_delta(ax, bx);
    let dz = cell_delta(az, bz);
    (dx * dx + dz * dz).sqrt().max(1.0) * spacing
}

pub fn find_waterfalls(
    heights: &ScalarField,
    hydrology: &Hydrology,
    threshold: f32,
    min_drop: f32,
    ocean: &[bool],
) -> Vec<Waterfall> {
    let on_river = river_cells(hydrology, threshold, ocean);
    let spacing = heights.spacing().max(1e-3);
    let mut claimed = vec![false; heights.len()];
    let mut falls = Vec::new();
    for &start in &hydrology.visit_order {
        if !on_river[start] || ocean[start] || claimed[start] {
            continue;
        }
        let Some(first) = hydrology.downstream[start] else {
            continue;
        };
        let first_drop = heights.get_at(start) - heights.get_at(first);
        if first_drop <= 0.0
            || first_drop / step_distance(heights, start, first, spacing) < WATERFALL_MIN_SLOPE
        {
            continue;
        }
        claimed[start] = true;
        let mut drop = first_drop;
        let mut cursor = first;
        let mut bottom = first;
        for _ in 1..WATERFALL_MAX_SCARP_STEPS {
            if !on_river[cursor] || ocean[cursor] || claimed[cursor] {
                break;
            }
            let Some(after) = hydrology.downstream[cursor] else {
                break;
            };
            let step = heights.get_at(cursor) - heights.get_at(after);
            if step <= 0.0
                || step / step_distance(heights, cursor, after, spacing) < WATERFALL_MIN_SLOPE
            {
                break;
            }
            claimed[cursor] = true;
            drop += step;
            bottom = after;
            cursor = after;
        }
        if drop >= min_drop {
            let (lx, lz) = heights.coords(start);
            let (px, pz) = heights.coords(bottom);
            falls.push(Waterfall {
                index: start,
                next: bottom,
                drop,
                lip: heights.world_of(lx, lz),
                plunge: heights.world_of(px, pz),
                pool_depth: 0.0,
                pool_radius: 0.0,
            });
        }
    }
    falls.sort_by(|a, b| b.drop.total_cmp(&a.drop));
    falls
}

fn in_bounds(field: &ScalarField, gx: isize, gz: isize) -> bool {
    if field.topology.wraps() {
        return true;
    }
    let dim = field.dim as isize;
    gx >= 0 && gz >= 0 && gx < dim && gz < dim
}

fn carve_bowl(heights: &mut ScalarField, gx: usize, gz: usize, radius: f32, depth: f32) {
    let spacing = heights.spacing().max(1e-3);
    let reach = (radius / spacing).ceil() as isize;
    for dz in -reach..=reach {
        for dx in -reach..=reach {
            let tx = gx as isize + dx;
            let tz = gz as isize + dz;
            if !in_bounds(heights, tx, tz) {
                continue;
            }
            let x = heights.wrap(tx);
            let z = heights.wrap(tz);
            let offset = ((dx * dx + dz * dz) as f32).sqrt() * spacing;
            if offset > radius {
                continue;
            }
            let falloff = (std::f32::consts::FRAC_PI_2 * (offset / radius)).cos();
            heights.add(x, z, -depth * falloff);
        }
    }
}

pub fn carve_plunge_pools(
    heights: &mut ScalarField,
    hydrology: &Hydrology,
    falls: &mut [Waterfall],
) {
    for fall in falls {
        let (px, pz) = heights.coords(fall.next);
        let depth = (fall.drop * 0.6).clamp(0.5, 6.0);
        let radius = heights.spacing() * 2.0;
        fall.pool_depth = depth;
        fall.pool_radius = radius;
        carve_bowl(heights, px, pz, radius, depth);

        let mut cursor = fall.next;
        for step in 0..2 {
            let Some(after) = hydrology.downstream[cursor] else {
                break;
            };
            let (ax, az) = heights.coords(after);
            carve_bowl(heights, ax, az, radius * (2.0 + step as f32), depth * 0.35);
            cursor = after;
        }

        fall.drop = heights.get_at(fall.index) - heights.get_at(fall.next);
    }
}

#[derive(Clone, Debug)]
pub struct Lake {
    pub cells: Vec<usize>,
    pub surface: f32,
    pub depth: f32,
    pub outlet: Option<usize>,
    pub drains_to_sea: bool,
}

fn spill_saddle(
    heights: &ScalarField,
    hydrology: &Hydrology,
    cells: &[usize],
    assigned: &[bool],
) -> Option<usize> {
    let mut saddle: Option<usize> = None;
    let mut saddle_level = f32::INFINITY;
    for &c in cells {
        let (x, z) = heights.coords(c);
        for neighbour in heights.neighbours8(x, z) {
            if assigned[neighbour] {
                continue;
            }
            let level = hydrology.filled.get_at(neighbour);
            if level < saddle_level {
                saddle_level = level;
                saddle = Some(neighbour);
            }
        }
    }
    saddle
}

fn drains_through_saddle(hydrology: &Hydrology, saddle: usize, sea_level: f32) -> bool {
    let mut cursor = Some(saddle);
    let mut steps = 0usize;
    let limit = hydrology.filled.len() + 1;
    while let Some(index) = cursor {
        if hydrology.filled.get_at(index) <= sea_level + DEPRESSION_EPSILON {
            return true;
        }
        cursor = hydrology.downstream[index];
        steps += 1;
        if steps > limit {
            break;
        }
    }
    false
}

pub fn find_lakes(
    heights: &ScalarField,
    hydrology: &Hydrology,
    sea_level: f32,
    ocean: &[bool],
) -> Vec<Lake> {
    let count = heights.len();
    let mut assigned = vec![false; count];
    let mut lakes: Vec<Lake> = Vec::new();

    for start in 0..count {
        if assigned[start] || ocean[start] || !hydrology.depressions[start] {
            continue;
        }
        let mut cells = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        assigned[start] = true;

        while let Some(index) = queue.pop_front() {
            cells.push(index);
            let (x, z) = heights.coords(index);
            for neighbour in heights.neighbours8(x, z) {
                if !assigned[neighbour] && !ocean[neighbour] && hydrology.depressions[neighbour] {
                    assigned[neighbour] = true;
                    queue.push_back(neighbour);
                }
            }
        }

        let surface = cells
            .iter()
            .map(|&c| hydrology.filled.get_at(c))
            .fold(f32::NEG_INFINITY, f32::max);
        let depth = cells
            .iter()
            .map(|&c| hydrology.filled.get_at(c) - heights.get_at(c))
            .fold(0.0f32, f32::max);
        let outlet = spill_saddle(heights, hydrology, &cells, &assigned);
        let drains_to_sea = outlet
            .map(|saddle| drains_through_saddle(hydrology, saddle, sea_level))
            .unwrap_or(false);

        lakes.push(Lake {
            cells,
            surface,
            depth,
            outlet,
            drains_to_sea,
        });
    }

    lakes.sort_by(|a, b| b.cells.len().cmp(&a.cells.len()));
    lakes
}

pub fn carve_rivers(
    heights: &mut ScalarField,
    hydrology: &Hydrology,
    threshold: f32,
    strength: f32,
) {
    let mut incisions = vec![0.0f32; heights.len()];
    let scale = (heights.max() - heights.min()).max(1.0);

    for &index in &hydrology.visit_order {
        let accumulation = hydrology.accumulation.get_at(index);
        if accumulation < threshold {
            continue;
        }
        let Some(parent) = hydrology.downstream[index] else {
            continue;
        };
        let slope = (hydrology.filled.get_at(index) - hydrology.filled.get_at(parent))
            .max(0.0)
            / scale;
        let capacity = strength * accumulation.sqrt() * slope.sqrt();
        let incision = capacity.min(6.0);
        incisions[index] = incision;
    }

    let mut carved = vec![false; heights.len()];
    for &index in &hydrology.visit_order {
        if incisions[index] <= 0.0 {
            continue;
        }
        carved[index] = true;
        let carried = incisions[index];
        heights.add(
            heights.coords(index).0,
            heights.coords(index).1,
            -carried,
        );
        if let Some(parent) = hydrology.downstream[index] {
            let (px, pz) = heights.coords(parent);
            heights.add(px, pz, carried * 0.35);
        }
    }

    for &index in hydrology.visit_order.iter().rev() {
        if incisions[index] <= 0.0 {
            continue;
        }
        if let Some(parent) = hydrology.downstream[index] {
            if heights.get_at(index) < heights.get_at(parent) {
                let level = heights.get_at(parent);
                let (x, z) = heights.coords(index);
                heights.set(x, z, level);
            }
        }
    }

    let mut mask = vec![false; heights.len()];
    for index in 0..heights.len() {
        if !carved[index] {
            continue;
        }
        mask[index] = true;
        let (x, z) = heights.coords(index);
        for neighbour in heights.neighbours8(x, z) {
            mask[neighbour] = true;
        }
    }
    smooth_once_masked(heights, &mask);
}

fn smooth_once_masked(field: &mut ScalarField, mask: &[bool]) {
    let snapshot = field.values.clone();
    let dim = field.dim;
    for z in 0..dim {
        for x in 0..dim {
            let index = field.index(x, z);
            if !mask[index] {
                continue;
            }
            let mut sum = snapshot[index] * 2.0;
            let mut weight = 2.0f32;
            for neighbour in field.neighbours8(x, z) {
                sum += snapshot[neighbour];
                weight += 1.0;
            }
            field.set_at(index, sum / weight);
        }
    }
}

pub fn smooth_once(field: &mut ScalarField) {
    let snapshot = field.values.clone();
    let dim = field.dim;
    for z in 0..dim {
        for x in 0..dim {
            let index = field.index(x, z);
            let mut sum = snapshot[index] * 2.0;
            let mut weight = 2.0f32;
            for neighbour in field.neighbours8(x, z) {
                sum += snapshot[neighbour];
                weight += 1.0;
            }
            field.set_at(index, sum / weight);
        }
    }
}

pub fn repose_tan(substrate: Substrate) -> f32 {
    match substrate {
        Substrate::Sand => 33.0f32.to_radians().tan(),
        Substrate::SiltClay => 35.0f32.to_radians().tan(),
        Substrate::LimestoneKarst => 40.0f32.to_radians().tan(),
        Substrate::RockCrystalline => 45.0f32.to_radians().tan(),
        Substrate::Volcanic => 42.0f32.to_radians().tan(),
    }
}

pub fn thermal_relax(heights: &mut ScalarField, max_slope: f32, iterations: usize) {
    let mut repose = ScalarField::new(heights.dim, heights.size, heights.topology);
    repose.fill(max_slope);
    thermal_relax_repose(heights, &repose, iterations);
}

pub fn thermal_relax_repose(heights: &mut ScalarField, repose: &ScalarField, iterations: usize) {
    let spacing = heights.spacing().max(1e-3);

    for _ in 0..iterations.max(1) {
        let mut moved = 0.0f32;
        let dim = heights.dim;
        for z in 0..dim {
            for x in 0..dim {
                let index = heights.index(x, z);
                let here = heights.get(x, z);
                for neighbour in heights.neighbours8(x, z) {
                    let there = heights.get_at(neighbour);
                    let difference = here - there;
                    let limit =
                        repose.get_at(index).max(repose.get_at(neighbour)) * spacing;
                    if difference > limit {
                        let transfer = (difference - limit) * 0.25;
                        heights.set_at(index, heights.get(x, z) - transfer);
                        heights.set_at(neighbour, there + transfer);
                        moved += transfer;
                    }
                }
            }
        }
        if moved <= 1e-5 {
            break;
        }
    }
}

#[inline(always)]
fn sample_values(field: &ScalarField, values: &[f32], fx: f32, fz: f32) -> f32 {
    let x0 = fx.floor();
    let z0 = fz.floor();
    let tx = fx - x0;
    let tz = fz - z0;
    let x0i = field.wrap(x0 as isize);
    let x1i = field.wrap(x0 as isize + 1);
    let z0i = field.wrap(z0 as isize);
    let z1i = field.wrap(z0 as isize + 1);
    let v00 = values[field.index(x0i, z0i)];
    let v10 = values[field.index(x1i, z0i)];
    let v01 = values[field.index(x0i, z1i)];
    let v11 = values[field.index(x1i, z1i)];
    let top = v00 + (v10 - v00) * tx;
    let bottom = v01 + (v11 - v01) * tx;
    top + (bottom - top) * tz
}

fn deposit_bilinear(
    field: &ScalarField,
    values: &mut [f32],
    fx: f32,
    fz: f32,
    amount: f32,
) {
    let x0 = fx.floor();
    let z0 = fz.floor();
    let tx = fx - x0;
    let tz = fz - z0;
    let x0i = field.wrap(x0 as isize);
    let x1i = field.wrap(x0 as isize + 1);
    let z0i = field.wrap(z0 as isize);
    let z1i = field.wrap(z0 as isize + 1);
    values[field.index(x0i, z0i)] += amount * (1.0 - tx) * (1.0 - tz);
    values[field.index(x1i, z0i)] += amount * tx * (1.0 - tz);
    values[field.index(x0i, z1i)] += amount * (1.0 - tx) * tz;
    values[field.index(x1i, z1i)] += amount * tx * tz;
}

pub fn apply_wind_erosion(
    heights: &mut ScalarField,
    wind: Vec2,
    strength: f32,
    iterations: usize,
) {
    let direction = wind.normalize_or_zero();
    if direction.length_squared() < 1e-6 || strength <= 0.0 {
        return;
    }
    let dim = heights.dim;

    for _ in 0..iterations.max(1) {
        let snapshot = heights.values.clone();
        let mut output = snapshot.clone();
        for z in 0..dim {
            for x in 0..dim {
                let index = heights.index(x, z);
                let here = snapshot[index];
                let upwind = sample_values(
                    heights,
                    &snapshot,
                    x as f32 - direction.x,
                    z as f32 - direction.y,
                );
                let deficit = upwind - here;
                if deficit <= 0.0 {
                    continue;
                }
                let carry = deficit * strength;
                output[index] -= carry;
                deposit_bilinear(
                    heights,
                    &mut output,
                    x as f32 + direction.x,
                    z as f32 + direction.y,
                    carry,
                );
            }
        }
        heights.values = output;
    }
}

struct DropletRng(u64);

impl DropletRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn unit(&mut self) -> f32 {
        self.0 = splitmix64(self.0);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ErosionConfig {
    pub droplet_count: usize,
    pub iterations: usize,
    pub inertia: f32,
    pub min_slope: f32,
    pub erode_rate: f32,
    pub deposit_rate: f32,
    pub evaporation: f32,
    pub max_volume: f32,
    pub gravity: f32,
    pub capacity_k: f32,
    pub erode_radius: f32,
    pub deposit_radius: f32,
}

impl Default for ErosionConfig {
    fn default() -> Self {
        Self {
            droplet_count: 8_000,
            iterations: 2,
            inertia: 0.05,
            min_slope: 0.01,
            erode_rate: 0.4,
            deposit_rate: 0.35,
            evaporation: 0.015,
            max_volume: 0.3,
            gravity: 4.0,
            capacity_k: 0.02,
            erode_radius: 1.5,
            deposit_radius: 2.0,
        }
    }
}

#[inline(always)]
fn brush_weight(nx: isize, nz: isize, fx: f32, fz: f32, radius: f32) -> f32 {
    let ox = nx as f32 - fx;
    let oz = nz as f32 - fz;
    (1.0 - (ox * ox + oz * oz).sqrt() / radius).max(0.0)
}

fn apply_brush(field: &mut ScalarField, fx: f32, fz: f32, radius: f32, delta: f32) {
    let reach = radius.ceil() as isize;
    let cx = fx.round() as isize;
    let cz = fz.round() as isize;
    let wraps = field.topology.wraps();
    let dim = field.dim as isize;
    let mut total = 0.0f32;
    for dz in -reach..=reach {
        for dx in -reach..=reach {
            let nx = cx + dx;
            let nz = cz + dz;
            if !wraps && (nx < 0 || nz < 0 || nx >= dim || nz >= dim) {
                continue;
            }
            total += brush_weight(nx, nz, fx, fz, radius);
        }
    }
    if total <= 0.0 {
        return;
    }
    let scale = delta / total;
    for dz in -reach..=reach {
        for dx in -reach..=reach {
            let nx = cx + dx;
            let nz = cz + dz;
            if !wraps && (nx < 0 || nz < 0 || nx >= dim || nz >= dim) {
                continue;
            }
            let weight = brush_weight(nx, nz, fx, fz, radius);
            if weight > 0.0 {
                let index = field.index(field.wrap(nx), field.wrap(nz));
                field.set_at(index, field.get_at(index) + weight * scale);
            }
        }
    }
}

pub fn droplet_erosion(
    heights: &mut ScalarField,
    precipitation: &ScalarField,
    seed: WorldSeed,
    config: &ErosionConfig,
) {
    if config.droplet_count == 0 || config.iterations == 0 {
        return;
    }
    let mut rng = DropletRng::new(seed.stage(STAGE_DROPLETS));
    let dim = heights.dim as f32;
    let max_steps = heights.dim.max(8);
    let wraps = heights.topology.wraps();

    for _ in 0..config.iterations {
        for _ in 0..config.droplet_count {
            let mut pos = Vec2::new(rng.unit() * dim, rng.unit() * dim);
            let angle = rng.unit() * std::f32::consts::TAU;
            let mut dir = Vec2::new(angle.cos(), angle.sin());
            let mut velocity = 1.0f32;
            let local_rain =
                sample_values(heights, &precipitation.values, pos.x, pos.y).clamp(0.0, 1.5);
            let mut water = config.max_volume * (0.25 + 0.75 * rng.unit())
                * (0.4 + 0.6 * local_rain.min(1.0));
            let mut sediment = 0.0f32;

            for _ in 0..max_steps {
                let height = sample_values(heights, &heights.values, pos.x, pos.y);
                let gx = sample_values(heights, &heights.values, pos.x + 0.5, pos.y)
                    - sample_values(heights, &heights.values, pos.x - 0.5, pos.y);
                let gz = sample_values(heights, &heights.values, pos.x, pos.y + 0.5)
                    - sample_values(heights, &heights.values, pos.x, pos.y - 0.5);
                let slope = Vec2::new(-gx, -gz);
                if slope.length_squared() > 1e-9 {
                    dir = (dir * config.inertia + slope.normalize()).normalize_or_zero();
                    if dir.length_squared() < 1e-9 {
                        break;
                    }
                }
                let next = pos + dir;
                if !wraps && (next.x < 0.0 || next.y < 0.0 || next.x > dim - 1.0 || next.y > dim - 1.0)
                {
                    break;
                }
                pos = if wraps {
                    Vec2::new(next.x.rem_euclid(dim), next.y.rem_euclid(dim))
                } else {
                    next
                };
                let new_height = sample_values(heights, &heights.values, pos.x, pos.y);
                let dh = new_height - height;
                velocity = (velocity * velocity + (-dh).max(0.0) * config.gravity)
                    .sqrt()
                    .max(0.1);
                let capacity =
                    (-dh).max(config.min_slope) * velocity * water * config.capacity_k;

                if sediment > capacity {
                    let deposit = (sediment - capacity) * config.deposit_rate;
                    sediment -= deposit;
                    apply_brush(heights, pos.x, pos.y, config.deposit_radius, deposit);
                } else {
                    let erode = ((capacity - sediment) * config.erode_rate).min((-dh).max(0.0));
                    if erode > 0.0 {
                        sediment += erode;
                        apply_brush(heights, pos.x, pos.y, config.erode_radius, -erode);
                    }
                }

                water *= 1.0 - config.evaporation;
                if water < 0.01 {
                    break;
                }
            }

            if sediment > 0.0 {
                apply_brush(heights, pos.x, pos.y, config.deposit_radius, sediment);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CraterConfig {
    pub count: usize,
    pub min_radius: f32,
    pub max_radius: f32,
    pub depth_ratio: f32,
    pub degradation: f32,
    pub ray_count: usize,
}

impl Default for CraterConfig {
    fn default() -> Self {
        Self {
            count: 6,
            min_radius: 8.0,
            max_radius: 26.0,
            depth_ratio: 0.35,
            degradation: 0.25,
            ray_count: 5,
        }
    }
}

pub struct Crater {
    pub center: Vec2,
    pub radius: f32,
    pub depth: f32,
    pub aspect: f32,
    pub rotation: f32,
    pub degradation: f32,
    pub terraces: usize,
    pub lobes: [f32; 3],
    pub ray_angles: Vec<f32>,
}

pub fn place_craters(seed: WorldSeed, config: &CraterConfig, size: f32) -> Vec<Crater> {
    let half = size * 0.5;
    (0..config.count)
        .map(|index| {
            let id = index as u64;
            let radius = seed.range(STAGE_CRATERS, id, 0, config.min_radius, config.max_radius);
            let depth = radius * config.depth_ratio;
            let lobes = [
                seed.range(STAGE_CRATERS, id, 3, 0.0, std::f32::consts::TAU),
                seed.range(STAGE_CRATERS, id, 4, 0.0, std::f32::consts::TAU),
                seed.range(STAGE_CRATERS, id, 5, 0.0, std::f32::consts::TAU),
            ];
            let ray_angles = (0..config.ray_count)
                .map(|ray| seed.range(STAGE_CRATERS, id, 6 + ray as u64, 0.0, std::f32::consts::TAU))
                .collect();
            Crater {
                center: Vec2::new(
                    seed.range(STAGE_CRATERS, id, 1, -half * 0.85, half * 0.85),
                    seed.range(STAGE_CRATERS, id, 2, -half * 0.85, half * 0.85),
                ),
                radius,
                depth,
                aspect: seed.range(STAGE_CRATERS, id, 7, 0.72, 1.0),
                rotation: seed.range(STAGE_CRATERS, id, 8, 0.0, std::f32::consts::TAU),
                degradation: config.degradation.clamp(0.0, 0.95),
                terraces: if radius > 16.0 { 4 } else { 0 },
                lobes,
                ray_angles,
            }
        })
        .collect()
}

fn angle_delta(mut delta: f32) -> f32 {
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta
}

pub fn apply_craters(
    heights: &mut ScalarField,
    craters: &[Crater],
    allowed: &dyn Fn(Vec2) -> bool,
) {
    for crater in craters {
        if !allowed(crater.center) {
            continue;
        }
        let (gx, gz) = heights.grid_of(crater.center);
        let reach = (crater.radius * 3.0 / heights.spacing()).ceil() as isize + 2;
        let cos_r = crater.rotation.cos();
        let sin_r = crater.rotation.sin();
        let degrade = 1.0 - crater.degradation;

        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let tx = gx as isize + dx;
                let tz = gz as isize + dz;
                if !in_bounds(heights, tx, tz) {
                    continue;
                }
                let x = heights.wrap(tx);
                let z = heights.wrap(tz);
                let world = heights.world_of(x, z);
                let delta = heights.topology.delta_xz(crater.center, world, heights.size);
                let rotated = Vec2::new(
                    delta.x * cos_r + delta.y * sin_r,
                    -delta.x * sin_r + delta.y * cos_r,
                );
                let distance = rotated.length();
                if distance > crater.radius * 3.0 {
                    continue;
                }
                let angle = rotated.y.atan2(rotated.x);
                let wobble = 1.0
                    + 0.12 * (2.0 * angle + crater.lobes[0]).sin()
                    + 0.08 * (3.0 * angle + crater.lobes[1]).sin()
                    + 0.05 * (5.0 * angle + crater.lobes[2]).sin();
                let major = crater.radius * wobble;
                let minor = major * crater.aspect;
                let ca = angle.cos();
                let sa = angle.sin();
                let local_radius = (major * minor) / ((minor * minor * ca * ca + major * major * sa * sa).sqrt());
                let u = distance / local_radius.max(1e-3);

                let mut offset = 0.0f32;
                if u < 1.0 {
                    let profile = if crater.terraces > 0 {
                        let steps = crater.terraces as f32;
                        (u * steps).floor() / steps
                    } else {
                        u
                    };
                    offset += -crater.depth * (1.0 - profile * profile) * degrade;
                }
                let rim_offset = u - 1.1;
                offset += crater.depth * 0.45 * degrade * (-((rim_offset * rim_offset) / 0.05)).exp();

                if u > 1.0 && u < 3.0 {
                    for &ray in &crater.ray_angles {
                        let dphi = angle_delta(angle - ray);
                        if dphi.abs() >= 0.18 {
                            continue;
                        }
                        let along = (1.0 - (u - 1.0) / 2.0).max(0.0);
                        let across = 1.0 - dphi.abs() / 0.18;
                        offset += crater.depth * 0.25 * degrade * along * across;
                    }
                }

                heights.add(x, z, offset);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::topology::WorldTopology;

    fn plane(dim: usize, size: f32, topology: WorldTopology, slope_x: f32) -> ScalarField {
        let mut field = ScalarField::new(dim, size, topology);
        for z in 0..dim {
            for x in 0..dim {
                field.set(x, z, x as f32 * slope_x);
            }
        }
        field
    }

    fn rain(dim: usize, size: f32, topology: WorldTopology) -> ScalarField {
        let mut field = ScalarField::new(dim, size, topology);
        field.fill(1.0);
        field
    }

    #[test]
    fn priority_flood_never_lowers_the_surface() {
        let mut field = ScalarField::new(10, 100.0, WorldTopology::Finite);
        for z in 0..10 {
            for x in 0..10 {
                field.set(x, z, ((x as f32 - 5.0).powi(2) + (z as f32 - 5.0).powi(2)) * 0.3);
            }
        }
        let original = field.values.clone();
        let hydro = priority_flood(&field, 0.0);
        for index in 0..field.len() {
            assert!(
                hydro.filled.get_at(index) >= original[index] - 1e-5,
                "priority flood lowered cell {index}"
            );
        }
    }

    #[test]
    fn priority_flood_removes_interior_depressions() {
        let mut field = plane(12, 120.0, WorldTopology::Finite, 1.0);
        field.set(6, 6, -50.0);
        field.set(6, 7, -40.0);
        let hydro = priority_flood(&field, 0.0);
        assert!(hydro.depressions[hydro.filled.index(6, 6)]);
        assert!(hydro.filled.get(6, 6) > 0.0, "the pit must be filled to its spill level");
    }

    #[test]
    fn flow_directions_never_cycle() {
        let mut field = ScalarField::new(12, 120.0, WorldTopology::Finite);
        for z in 0..12 {
            for x in 0..12 {
                field.set(x, z, ((x as f32 - 3.0).powi(2) + (z as f32 - 8.0).powi(2)) * -0.2);
            }
        }
        let hydro = priority_flood(&field, -1000.0);
        for start in 0..field.len() {
            let mut seen = std::collections::HashSet::new();
            let mut cursor = Some(start);
            let mut steps = 0;
            while let Some(index) = cursor {
                assert!(seen.insert(index), "cycle detected in the drainage tree");
                cursor = hydro.downstream[index];
                steps += 1;
                if steps > field.len() {
                    break;
                }
            }
        }
    }

    #[test]
    fn flow_accumulation_conserves_precipitation_on_a_plane() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let field = plane(16, 160.0, topology, 0.6);
            let mut rain = ScalarField::new(16, 160.0, topology);
            rain.fill(1.0);
            let mut hydro = priority_flood(&field, -1000.0);
            flow_accumulation(&mut hydro, &rain);

            let total_in: f32 = rain.values.iter().sum();
            let total_out = hydro.total_precipitation_captured();
            assert!(
                (total_in - total_out).abs() < 1e-2,
                "{topology:?} lost water: in {total_in}, out {total_out}"
            );
        }
    }

    #[test]
    fn accumulation_increases_downstream() {
        let field = plane(16, 160.0, WorldTopology::Finite, 1.0);
        let mut rain = ScalarField::new(16, 160.0, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);

        for index in 0..field.len() {
            if let Some(parent) = hydro.downstream[index] {
                assert!(
                    hydro.accumulation.get_at(parent) >= hydro.accumulation.get_at(index) - 1e-4,
                    "accumulation fell downstream at {index}"
                );
            }
        }
    }

    #[test]
    fn accumulation_is_conserved_across_split_targets() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, x as f32 * 2.0 + z as f32);
            }
        }
        let precipitation = rain(16, 160.0, WorldTopology::Finite);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &precipitation);

        let total_in: f32 = precipitation.values.iter().sum();
        let total_out = hydro.total_precipitation_captured();
        assert!(
            (total_in - total_out).abs() < 1e-2,
            "split flow leaked water: in {total_in}, out {total_out}"
        );
    }

    #[test]
    fn flow_directions_are_not_grid_aligned() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, x as f32 * 2.0 + z as f32);
            }
        }
        let hydro = priority_flood(&field, -1000.0);
        let split = (0..field.len()).any(|index| {
            hydro.secondary[index].is_some()
                && hydro.primary_weight[index] > 0.05
                && hydro.primary_weight[index] < 0.95
        });
        assert!(
            split,
            "D-infinity flow must split between two sectors on an oblique slope"
        );
    }

    #[test]
    fn tilted_plane_drains_straight() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, x as f32 * 2.0 + z as f32);
            }
        }
        let hydro = priority_flood(&field, -1000.0);
        let mut cursor = Some(field.index(12, 12));
        let mut path = Vec::new();
        while let Some(index) = cursor {
            path.push(index);
            cursor = hydro.downstream[index];
        }
        assert!(path.len() > 4, "the plane must drain for a good distance");
        let first = field.world_of(field.coords(path[0]).0, field.coords(path[0]).1);
        let last = field.world_of(field.coords(*path.last().unwrap()).0, field.coords(*path.last().unwrap()).1);
        let line = last - first;
        for &index in &path {
            let point = field.world_of(field.coords(index).0, field.coords(index).1);
            let offset = point - first;
            let cross = line.x * offset.y - line.y * offset.x;
            assert!(
                cross.abs() < 1e-3 * line.length(),
                "downstream path left the straight line at {index}"
            );
        }
    }

    #[test]
    fn flat_plateau_drains_toward_the_outlet() {
        let mut field = ScalarField::new(12, 120.0, WorldTopology::Finite);
        field.fill(10.0);
        for z in 0..12 {
            field.set(0, z, 4.0);
        }
        let hydro = priority_flood(&field, 0.0);
        for z in 1..11 {
            for x in 1..11 {
                let mut cursor = Some(field.index(x, z));
                let mut steps = 0;
                let mut reached_outlet = false;
                while let Some(index) = cursor {
                    if field.coords(index).0 == 0 {
                        reached_outlet = true;
                        break;
                    }
                    cursor = hydro.downstream[index];
                    steps += 1;
                    if steps > field.len() {
                        break;
                    }
                }
                assert!(
                    reached_outlet,
                    "plateau cell ({x},{z}) stalled instead of draining to the outlet"
                );
            }
        }
    }

    #[test]
    fn torus_flow_crosses_the_seam() {
        let mut field = ScalarField::new(12, 120.0, WorldTopology::FiniteTorus);
        for z in 0..12 {
            for x in 0..12 {
                field.set(x, z, (x as f32 - 11.5).abs() * -1.0 + (z as f32 - 6.0).abs() * 0.05);
            }
        }
        let mut rain = ScalarField::new(12, 120.0, WorldTopology::FiniteTorus);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);
        let total: f32 = rain.values.iter().sum();
        assert!(
            (hydro.total_precipitation_captured() - total).abs() < 1e-2,
            "toroidal drainage must not leak across the seam"
        );
    }

    #[test]
    fn rivers_follow_high_accumulation_paths() {
        let field = plane(20, 200.0, WorldTopology::Finite, 1.0);
        let mut rain = ScalarField::new(20, 200.0, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);

        let paths = river_paths(&hydro, 12.0, &vec![false; field.len()]);
        assert!(!paths.is_empty(), "a draining plane must produce rivers");
        for path in &paths {
            for pair in path.windows(2) {
                assert!(
                    hydro.downstream[pair[0]] == Some(pair[1]),
                    "river path must follow the drainage tree"
                );
            }
        }
    }

    #[test]
    fn carving_incises_channels_without_inverting_drainage() {
        let field = plane(20, 200.0, WorldTopology::Finite, 1.0);
        let mut rain = ScalarField::new(20, 200.0, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);

        let mut carved = field.clone();
        carve_rivers(&mut carved, &hydro, 12.0, 2.0);
        assert!(carved.mean() < field.mean() + 0.5, "carving must not raise the terrain overall");
        let incised = carved
            .values
            .iter()
            .zip(field.values.iter())
            .filter(|(carved, original)| **carved < **original - 1e-3)
            .count();
        assert!(incised > 0, "channels should be cut below the source surface");
    }

    fn cone_field(dim: usize, size: f32, topology: WorldTopology) -> ScalarField {
        let mut field = ScalarField::new(dim, size, topology);
        let centre = dim as f32 * 0.5;
        for z in 0..dim {
            for x in 0..dim {
                let dx = x as f32 - centre;
                let dz = z as f32 - centre;
                let radius = (dx * dx + dz * dz).sqrt();
                field.set(x, z, (12.0 - radius * 0.8).max(0.0));
            }
        }
        field
    }

    #[test]
    fn droplet_erosion_is_deterministic() {
        let config = ErosionConfig {
            droplet_count: 400,
            iterations: 1,
            ..Default::default()
        };
        let precipitation = rain(24, 240.0, WorldTopology::FiniteTorus);
        let mut first = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        let mut second = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        let mut other = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        droplet_erosion(&mut first, &precipitation, WorldSeed::new(7), &config);
        droplet_erosion(&mut second, &precipitation, WorldSeed::new(7), &config);
        droplet_erosion(&mut other, &precipitation, WorldSeed::new(8), &config);
        assert_eq!(first.values, second.values, "same seed must erode identically");
        assert_ne!(first.values, other.values, "different seeds must erode differently");
    }

    #[test]
    fn droplet_erosion_conserves_sediment() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let config = ErosionConfig {
                droplet_count: 500,
                iterations: 1,
                ..Default::default()
            };
            let precipitation = rain(24, 240.0, topology);
            let mut field = cone_field(24, 240.0, topology);
            let before: f32 = field.values.iter().sum();
            droplet_erosion(&mut field, &precipitation, WorldSeed::new(3), &config);
            let after: f32 = field.values.iter().sum();
            assert!(
                (before - after).abs() < before.abs() * 1e-3 + 1e-2,
                "{topology:?} sediment not conserved: {before} -> {after}"
            );
        }
    }

    #[test]
    fn droplet_erosion_flattens_a_cone() {
        let config = ErosionConfig {
            droplet_count: 1_500,
            iterations: 2,
            ..Default::default()
        };
        let precipitation = rain(24, 240.0, WorldTopology::FiniteTorus);
        let mut field = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        let before_peak = field.max();
        let before_span = field.max() - field.min();
        droplet_erosion(&mut field, &precipitation, WorldSeed::new(11), &config);
        assert!(field.max() < before_peak, "the cone peak must be worn down");
        assert!(
            field.max() - field.min() < before_span,
            "erosion plus deposition must reduce relief"
        );
    }

    #[test]
    fn droplet_erosion_recomputes_flow_between_iterations() {
        let precipitation = rain(24, 240.0, WorldTopology::FiniteTorus);
        let base = cone_field(24, 240.0, WorldTopology::FiniteTorus);

        let mut once = base.clone();
        droplet_erosion(
            &mut once,
            &precipitation,
            WorldSeed::new(5),
            &ErosionConfig {
                droplet_count: 600,
                iterations: 1,
                ..Default::default()
            },
        );
        let mut many = base.clone();
        droplet_erosion(
            &mut many,
            &precipitation,
            WorldSeed::new(5),
            &ErosionConfig {
                droplet_count: 600,
                iterations: 4,
                ..Default::default()
            },
        );

        let change_once: f32 = once
            .values
            .iter()
            .zip(base.values.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        let change_many: f32 = many
            .values
            .iter()
            .zip(base.values.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(
            change_many > change_once * 1.2,
            "later iterations must keep carving on the changed terrain: {change_once} vs {change_many}"
        );
    }

    #[test]
    fn thermal_relaxation_caps_steep_slopes() {
        let mut field = ScalarField::new(12, 120.0, WorldTopology::Finite);
        field.set(6, 6, 100.0);
        let before = field.max() - field.min();
        thermal_relax(&mut field, 0.6, 40);
        let after = field.max() - field.min();
        assert!(after < before, "a spike must relax: {before} -> {after}");

        let mut flat = ScalarField::new(8, 80.0, WorldTopology::Finite);
        flat.fill(5.0);
        thermal_relax(&mut flat, 0.5, 10);
        assert!((flat.max() - flat.min()).abs() < 1e-5, "flat ground must not move");
    }

    fn slope_field(angle_deg: f32) -> ScalarField {
        let mut field = ScalarField::new(8, 70.0, WorldTopology::Finite);
        let tan = angle_deg.to_radians().tan();
        for z in 0..8 {
            for x in 0..8 {
                field.set(x, z, x as f32 * tan * field.spacing());
            }
        }
        field
    }

    #[test]
    fn repose_slump_depends_on_substrate() {
        let sand_tan = repose_tan(Substrate::Sand);
        let rock_tan = repose_tan(Substrate::RockCrystalline);
        let sand_angle = sand_tan.atan().to_degrees();
        let rock_angle = rock_tan.atan().to_degrees();
        assert!(
            sand_angle > 30.0 && sand_angle < 35.0,
            "sand repose must sit in 30-35 degrees: {sand_angle}"
        );
        assert!(
            rock_angle > 40.0 && rock_angle < 50.0,
            "rock repose must sit in 40-50 degrees: {rock_angle}"
        );

        let angle = 37.0f32;
        let tan = angle.to_radians().tan();
        assert!(tan > sand_tan && tan < rock_tan);

        let span_before = tan * 10.0 * 7.0;
        let mut sand = slope_field(angle);
        let mut rock = slope_field(angle);
        let before: f32 = sand.values.iter().sum();
        let mut sand_repose = ScalarField::new(8, 70.0, WorldTopology::Finite);
        sand_repose.fill(sand_tan);
        let mut rock_repose = ScalarField::new(8, 70.0, WorldTopology::Finite);
        rock_repose.fill(rock_tan);
        thermal_relax_repose(&mut sand, &sand_repose, 10);
        thermal_relax_repose(&mut rock, &rock_repose, 10);

        assert!(
            sand.max() - sand.min() < span_before - 1e-3,
            "a 37 degree slope must slump on sand"
        );
        assert!(
            (rock.max() - rock.min() - span_before).abs() < 1e-3,
            "a 37 degree slope must stand on rock"
        );
        let after: f32 = sand.values.iter().sum();
        assert!((before - after).abs() < 1e-3, "slumping must conserve sediment");
    }

    #[test]
    fn lakes_are_flat_and_deepen_toward_their_floor() {
        let mut field = ScalarField::new(14, 140.0, WorldTopology::Finite);
        for z in 0..14 {
            for x in 0..14 {
                field.set(x, z, 10.0 + (x as f32) * 0.2);
            }
        }
        for z in 5..9 {
            for x in 5..9 {
                field.set(x, z, -5.0);
            }
        }
        let hydro = priority_flood(&field, 0.0);
        let lakes = find_lakes(&field, &hydro, 0.0, &vec![false; field.len()]);
        assert!(!lakes.is_empty(), "the pit must become a lake");
        for lake in &lakes {
            assert!(lake.depth > 0.0);
            for &cell in &lake.cells {
                assert!(
                    hydro.filled.get_at(cell) >= field.get_at(cell) - 1e-4,
                    "lake surface must sit at or above the ground"
                );
            }
        }
    }

    #[test]
    fn lake_outlet_is_the_spill_saddle_and_drains_to_sea() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, x as f32 * 0.5 - 4.0);
            }
        }
        for z in 6..10 {
            for x in 10..14 {
                field.set(x, z, -10.0);
            }
        }
        let hydro = priority_flood(&field, 0.0);
        let lakes = find_lakes(&field, &hydro, 0.0, &vec![false; field.len()]);
        let pit = field.index(11, 7);
        let lake = lakes
            .iter()
            .find(|lake| lake.cells.contains(&pit))
            .expect("the pit above sea level must become a lake");

        let outlet = lake.outlet.expect("a basin that spills over its rim has a saddle outlet");
        assert!(!lake.cells.contains(&outlet), "the saddle is a rim cell, not a lake cell");
        assert!(
            (hydro.filled.get_at(outlet) - lake.surface).abs() < 0.05,
            "the saddle must sit at the lake surface: {} vs {}",
            hydro.filled.get_at(outlet),
            lake.surface
        );
        assert!(
            lake.surface > 0.0,
            "this lake sits above sea level, so the old sea-level gate would have lost its outlet"
        );
        assert!(
            lake.drains_to_sea,
            "water spilling over the saddle flows downhill into the sea"
        );
    }

    #[test]
    fn closed_basin_lake_keeps_its_saddle_but_does_not_drain() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, x as f32 * 0.5 + 5.0);
            }
        }
        for z in 6..10 {
            for x in 6..10 {
                field.set(x, z, -2.0);
            }
        }
        let hydro = priority_flood(&field, 0.0);
        let lakes = find_lakes(&field, &hydro, 0.0, &vec![false; field.len()]);
        let pit = field.index(7, 7);
        let lake = lakes
            .iter()
            .find(|lake| lake.cells.contains(&pit))
            .expect("the pit must become a lake");

        assert!(lake.outlet.is_some(), "the basin still spills over its lowest rim");
        assert!(
            !lake.drains_to_sea,
            "no cell on the downhill chain reaches sea level, so the basin is closed"
        );
    }

    fn seaward_ramp(scarp: bool) -> (ScalarField, Vec<bool>) {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                let mut height = x as f32 * 0.5 - 6.0;
                if scarp && x >= 13 {
                    height += 25.0;
                }
                field.set(x, z, height);
            }
        }
        let ocean: Vec<bool> = (0..field.len())
            .map(|index| field.get_at(index) <= 0.0)
            .collect();
        (field, ocean)
    }

    fn flooded_hydro(field: &ScalarField) -> Hydrology {
        let mut rain = ScalarField::new(16, 160.0, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(field, 0.0);
        flow_accumulation(&mut hydro, &rain);
        hydro
    }

    #[test]
    fn ocean_connected_cells_spawn_no_lakes_rivers_or_falls() {
        let (field, ocean) = seaward_ramp(true);
        let hydro = flooded_hydro(&field);

        let lakes = find_lakes(&field, &hydro, 0.0, &ocean);
        for lake in &lakes {
            assert!(
                lake.cells.iter().all(|&cell| !ocean[cell]),
                "a lake claimed seafloor cells"
            );
        }

        let paths = river_paths(&hydro, 2.0, &ocean);
        assert!(!paths.is_empty(), "the ramp must still drain a river");
        for path in &paths {
            assert!(
                path.iter().take(path.len() - 1).all(|&v| !ocean[v]),
                "a river ribbon ran along the seafloor: {path:?}"
            );
        }

        let falls = find_waterfalls(&field, &hydro, 2.0, 5.0, &ocean);
        assert!(!falls.is_empty(), "the scarp into the sea is a real waterfall");
        assert!(
            falls.iter().all(|fall| !ocean[fall.index]),
            "no waterfall lip may sit below sea level"
        );
    }

    #[test]
    fn river_paths_reach_one_step_past_the_shoreline() {
        let (field, ocean) = seaward_ramp(false);
        let hydro = flooded_hydro(&field);

        let paths = river_paths(&hydro, 2.0, &ocean);
        assert!(!paths.is_empty());
        for path in &paths {
            let last = *path.last().unwrap();
            let before = path[path.len() - 2];
            assert!(
                ocean[last],
                "the river must terminate inside the sea, not on the bank: {path:?}"
            );
            assert!(!ocean[before], "only one step past the shoreline may be added");
        }
    }

    #[test]
    fn river_paths_include_the_confluence_vertex() {
        let mut field = ScalarField::new(5, 50.0, WorldTopology::Finite);
        field.fill(0.0);
        let mut hydro = Hydrology {
            filled: field.clone(),
            routed: field.clone(),
            downstream: vec![Some(1), Some(2), Some(4), Some(2), None],
            secondary: vec![None; 5],
            primary_weight: vec![1.0; 5],
            accumulation: ScalarField::new(5, 50.0, WorldTopology::Finite),
            visit_order: vec![4, 2, 1, 0, 3],
            depressions: vec![false; 25],
        };
        hydro.accumulation.fill(10.0);

        let paths = river_paths(&hydro, 5.0, &vec![false; 25]);
        assert_eq!(paths.len(), 2, "two sources feed one mouth: {paths:?}");
        let junction_shared = paths.iter().any(|path| {
            let last = *path.last().expect("non-empty path");
            paths.iter().any(|other| other != path && other.contains(&last))
        });
        assert!(
            junction_shared,
            "a tributary must end on the confluence vertex it joins: {paths:?}"
        );
        for path in &paths {
            for pair in path.windows(2) {
                assert_eq!(hydro.downstream[pair[0]], Some(pair[1]));
            }
        }
    }

    #[test]
    fn waterfalls_mark_steep_river_drops() {
        let mut field = plane(16, 160.0, WorldTopology::Finite, 1.0);
        for z in 0..16 {
            for x in 8..16 {
                field.set(x, z, field.get(x, z) + 25.0);
            }
        }
        let mut rain = ScalarField::new(16, 160.0, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);
        let falls = find_waterfalls(&field, &hydro, 4.0, 5.0, &vec![false; field.len()]);
        assert!(!falls.is_empty(), "a scarp across a river is a waterfall");
        assert!(falls.iter().all(|f| f.drop >= 5.0));
    }

    #[test]
    fn plunge_pool_is_deeper_than_the_pre_carve_ground() {
        let mut field = plane(16, 160.0, WorldTopology::Finite, 1.0);
        for z in 0..16 {
            for x in 8..16 {
                field.set(x, z, field.get(x, z) + 25.0);
            }
        }
        let mut rain = ScalarField::new(16, 160.0, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);
        let mut falls = find_waterfalls(&field, &hydro, 4.0, 5.0, &vec![false; field.len()]);
        assert!(!falls.is_empty());

        let fall = &falls[0];
        let pre_pool = field.get_at(fall.next);
        assert_eq!(
            hydro.downstream[fall.index],
            Some(fall.next),
            "the cascade descriptor must point at the downstream cell"
        );
        let pre_drop = fall.drop;

        carve_plunge_pools(&mut field, &hydro, &mut falls);

        let fall = &falls[0];
        assert!(
            field.get_at(fall.next) < pre_pool,
            "the plunge pool floor must sit below the pre-carve ground"
        );
        assert!(fall.pool_depth > 0.0);
        assert!(fall.pool_radius > 0.0);
        assert!(fall.drop >= pre_drop, "carving the pool must deepen the step");
    }

    #[test]
    fn craters_carve_a_bowl_with_a_raised_rim() {
        let mut field = ScalarField::new(40, 200.0, WorldTopology::Finite);
        field.fill(10.0);
        let craters = vec![Crater {
            center: Vec2::ZERO,
            radius: 20.0,
            depth: 6.0,
            aspect: 1.0,
            rotation: 0.0,
            degradation: 0.0,
            terraces: 0,
            lobes: [0.0; 3],
            ray_angles: vec![],
        }];
        let before = field.get_at(field.index(20, 20));
        apply_craters(&mut field, &craters, &|_| true);
        let centre = field.get_at(field.index(20, 20));
        assert!(centre < before, "the crater floor must drop below the plain");

        let rim_x = 20 + (24.0 / field.spacing()).round() as usize;
        let rim = field.get_at(field.index(rim_x.min(39), 20));
        assert!(rim > before - 1e-3, "the rim must not sit below the plain: {rim}");
    }

    fn plain_field(dim: usize, size: f32) -> ScalarField {
        let mut field = ScalarField::new(dim, size, WorldTopology::Finite);
        field.fill(10.0);
        field
    }

    fn test_crater(radius: f32) -> Crater {
        Crater {
            center: Vec2::ZERO,
            radius,
            depth: radius * 0.35,
            aspect: 1.0,
            rotation: 0.0,
            degradation: 0.0,
            terraces: 0,
            lobes: [0.0; 3],
            ray_angles: vec![],
        }
    }

    fn rim_peak(field: &ScalarField, centre: usize, radius: f32, spacing: f32) -> f32 {
        let (cx, cz) = field.coords(centre);
        let mut peak = f32::NEG_INFINITY;
        for dx in -(radius * 1.6 / spacing) as isize..=(radius * 1.6 / spacing) as isize {
            let x = cx as isize + dx;
            if x < 0 || x >= field.dim as isize {
                continue;
            }
            let value = field.get(x as usize, cz);
            let distance = (dx as f32 * spacing).abs();
            if distance > radius * 0.9 && distance < radius * 1.5 {
                peak = peak.max(value);
            }
        }
        peak
    }

    #[test]
    fn crater_rim_height_scales_with_radius() {
        let small = test_crater(10.0);
        let large = test_crater(30.0);
        let mut small_field = plain_field(60, 300.0);
        let mut large_field = plain_field(60, 300.0);
        apply_craters(&mut small_field, &[small], &|_| true);
        apply_craters(&mut large_field, &[large], &|_| true);
        let centre = small_field.index(30, 30);
        let small_rim = rim_peak(&small_field, centre, 10.0, small_field.spacing()) - 10.0;
        let large_rim = rim_peak(&large_field, centre, 30.0, large_field.spacing()) - 10.0;
        assert!(small_rim > 0.0 && large_rim > 0.0);
        assert!(
            large_rim > small_rim * 2.0,
            "rim height must scale with radius: {small_rim} vs {large_rim}"
        );
    }

    #[test]
    fn crater_degradation_monotonically_flattens_the_rim() {
        let mut previous = f32::INFINITY;
        for degradation in [0.0, 0.3, 0.6, 0.9] {
            let mut crater = test_crater(20.0);
            crater.degradation = degradation;
            let mut field = plain_field(60, 300.0);
            apply_craters(&mut field, &[crater], &|_| true);
            let rim = rim_peak(&field, field.index(30, 30), 20.0, field.spacing()) - 10.0;
            assert!(
                rim < previous,
                "rim must degrade monotonically: {degradation} gave {rim} >= {previous}"
            );
            previous = rim;
        }
    }

    #[test]
    fn crater_rays_are_seeded_deterministic() {
        let config = CraterConfig {
            count: 1,
            min_radius: 20.0,
            max_radius: 20.0,
            degradation: 0.0,
            ..Default::default()
        };
        let mut field = plain_field(80, 400.0);
        let craters = place_craters(WorldSeed::new(4), &config, 400.0);
        assert!(!craters[0].ray_angles.is_empty());
        apply_craters(&mut field, &craters, &|_| true);

        let mut again = plain_field(80, 400.0);
        let repeated = place_craters(WorldSeed::new(4), &config, 400.0);
        apply_craters(&mut again, &repeated, &|_| true);
        assert_eq!(again.values, field.values, "rays must be seeded-deterministic");

        let mut other = plain_field(80, 400.0);
        let different = place_craters(WorldSeed::new(5), &config, 400.0);
        apply_craters(&mut other, &different, &|_| true);
        assert_ne!(other.values, field.values, "a new seed must throw new rays");
    }

    #[test]
    fn crater_rim_is_elliptical() {
        let mut crater = test_crater(20.0);
        crater.aspect = 0.6;
        let mut field = plain_field(60, 300.0);
        apply_craters(&mut field, &[crater], &|_| true);
        let spacing = field.spacing();

        let mut major_distance = 0.0f32;
        let mut minor_distance = 0.0f32;
        for step in 1..40 {
            let d = step as f32 * spacing;
            if field.get(30 + step, 30) > 10.5 {
                major_distance = d;
            }
            if field.get(30, 30 + step) > 10.5 {
                minor_distance = d;
            }
        }
        assert!(
            major_distance > minor_distance * 1.2,
            "the rim must reach farther along the major axis: {major_distance} vs {minor_distance}"
        );
    }

    #[test]
    fn crater_rays_are_directional() {
        let mut crater = test_crater(20.0);
        crater.ray_angles = vec![0.0];
        let distance = crater.radius * 1.8;
        let mut field = plain_field(60, 300.0);
        apply_craters(&mut field, &[crater], &|_| true);
        let on_ray = field.sample_world(Vec2::new(distance, 0.0));
        let off_ray = field.sample_world(Vec2::new(0.0, distance));
        assert!(
            on_ray > 10.5,
            "the ray must throw ejecta beyond the rim: {on_ray}"
        );
        assert!(
            off_ray < 10.1,
            "ground between rays must stay near the plain: {off_ray}"
        );
    }

    #[test]
    fn large_craters_have_terraced_inner_walls() {
        let mut terraced = test_crater(24.0);
        terraced.terraces = 4;
        let mut smooth = test_crater(24.0);
        smooth.terraces = 0;

        let count_flats = |field: &ScalarField| -> usize {
            let spacing = field.spacing();
            let (cx, cz) = field.grid_of(Vec2::ZERO);
            let mut flats = 0;
            let mut previous: Option<f32> = None;
            for step in 1..=((24.0 * 0.55) / spacing) as usize {
                let value = field.get(cx + step, cz);
                if let Some(before) = previous {
                    if (value - before).abs() < 0.02 {
                        flats += 1;
                    }
                }
                previous = Some(value);
            }
            flats
        };

        let mut terraced_field = plain_field(60, 120.0);
        let mut smooth_field = plain_field(60, 120.0);
        apply_craters(&mut terraced_field, &[terraced], &|_| true);
        apply_craters(&mut smooth_field, &[smooth], &|_| true);
        let terraced_flats = count_flats(&terraced_field);
        let smooth_flats = count_flats(&smooth_field);
        assert!(
            terraced_flats >= 2,
            "a terraced bowl must show flat steps: {terraced_flats}"
        );
        assert!(
            smooth_flats < terraced_flats,
            "a smooth bowl must show fewer flat steps: {smooth_flats} vs {terraced_flats}"
        );
    }

    #[test]
    fn crater_placement_is_deterministic_and_bounded() {
        let config = CraterConfig::default();
        let a = place_craters(WorldSeed::new(5), &config, 400.0);
        let b = place_craters(WorldSeed::new(5), &config, 400.0);
        assert_eq!(a.len(), config.count);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.center, y.center);
            assert!((x.radius..=26.0).contains(&x.radius));
            assert!(x.center.x.abs() < 200.0 && x.center.y.abs() < 200.0);
        }
        assert_ne!(
            place_craters(WorldSeed::new(6), &config, 400.0)[0].center,
            a[0].center
        );
    }

    #[test]
    fn wind_erosion_moves_material_downwind() {
        let mut field = ScalarField::new(20, 200.0, WorldTopology::Finite);
        for z in 0..20 {
            for x in 0..20 {
                field.set(x, z, (x as f32 * 0.5).sin() * 4.0);
            }
        }
        let before: f32 = field.values.iter().map(|v| v.abs()).sum();
        apply_wind_erosion(&mut field, Vec2::X, 0.25, 1);
        let after: f32 = field.values.iter().map(|v| v.abs()).sum();
        assert!(after < before, "wind must smooth the profile: {before} -> {after}");
    }

    #[test]
    fn wind_migrates_a_dune_downwind() {
        let mut field = ScalarField::new(32, 320.0, WorldTopology::Finite);
        field.fill(0.0);
        for z in 0..32 {
            for x in 0..32 {
                let dx = x as f32 - 12.0;
                let dz = z as f32 - 16.0;
                field.add(x, z, 6.0 * (-((dx * dx + dz * dz) / 8.0)).exp());
            }
        }
        let centroid = |field: &ScalarField| -> f32 {
            let mut mass = 0.0f32;
            let mut moment = 0.0f32;
            for z in 0..32 {
                for x in 0..32 {
                    let value = field.get(x, z).max(0.0);
                    mass += value;
                    moment += value * x as f32;
                }
            }
            moment / mass
        };
        let before = centroid(&field);
        apply_wind_erosion(&mut field, Vec2::new(1.0, 0.3), 0.3, 30);
        let after = centroid(&field);
        assert!(
            after > before + 0.3,
            "the dune must migrate downwind: {before} -> {after}"
        );
    }

    #[test]
    fn wind_conserves_sediment() {
        let mut field = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        let before: f32 = field.values.iter().sum();
        apply_wind_erosion(&mut field, Vec2::new(0.7, -0.7), 0.25, 8);
        let after: f32 = field.values.iter().sum();
        assert!(
            (before - after).abs() < before.abs() * 1e-3 + 1e-2,
            "wind transport must conserve sediment: {before} -> {after}"
        );
    }

    #[test]
    fn precipitation_inputs_stay_positive() {
        let field = plane(12, 120.0, WorldTopology::Finite, 0.5);
        let cells = vec![0usize; field.len()];
        let cell_precip = vec![Precipitation::Heavy; 1];
        let input = PrecipitationInput {
            cell_of_vertex: &cells,
            cell_precipitation: &cell_precip,
            cell_rain_shadow: &[0.0],
            wind: Vec2::X,
            relief: 60.0,
        };
        let rain = generate_precipitation(&field, &input);
        assert!(rain.values.iter().all(|v| *v > 0.0));
        assert!(rain.values.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn rain_shadow_suppresses_precipitation() {
        let field = plane(12, 120.0, WorldTopology::Finite, 0.0);
        let cells = vec![0usize; field.len()];
        let cell_precip = vec![Precipitation::Heavy; 1];
        let build = |shadow: f32| {
            generate_precipitation(
                &field,
                &PrecipitationInput {
                    cell_of_vertex: &cells,
                    cell_precipitation: &cell_precip,
                    cell_rain_shadow: &[shadow],
                    wind: Vec2::X,
                    relief: 60.0,
                },
            )
            .mean()
        };
        assert!(build(0.9) < build(0.0), "rain shadow must reduce rainfall");
    }

    #[test]
    fn precipitation_blends_across_voronoi_cell_weights() {
        use crate::terrain::voronoi::{VoronoiConfig, VoronoiLayout};
        let topology = WorldTopology::Finite;
        let size = 200.0;
        let dim = 40usize;
        let layout = VoronoiLayout::generate(
            WorldSeed::new(3),
            VoronoiConfig {
                size,
                grid_dim: 6,
                topology,
                default_blend_width: 16.0,
                blend_min: 16.0,
                blend_max: 16.0,
                ..Default::default()
            },
        );
        let cache = layout.vertex_cache(dim, size, topology);
        let cell_of_vertex: Vec<usize> = (0..dim * dim)
            .map(|i| cache.cell_at(i).unwrap_or(0))
            .collect();
        let cell_count = layout.cells().len();
        let cell_precipitation: Vec<Precipitation> = (0..cell_count)
            .map(|i| {
                if i % 2 == 0 {
                    Precipitation::Torrential
                } else {
                    Precipitation::Low
                }
            })
            .collect();
        let cell_rain_shadow = vec![0.0f32; cell_count];
        let field = ScalarField::new(dim, size, topology);
        let input = PrecipitationInput {
            cell_of_vertex: &cell_of_vertex,
            cell_precipitation: &cell_precipitation,
            cell_rain_shadow: &cell_rain_shadow,
            wind: Vec2::ZERO,
            relief: 60.0,
        };
        let hard = generate_precipitation(&field, &input);
        let blended = generate_precipitation_blended(&field, &input, Some(&cache));

        let max_step = |f: &ScalarField| -> f32 {
            let mut m = 0.0f32;
            for z in 0..dim {
                for x in 0..dim {
                    if x + 1 < dim {
                        m = m.max((f.get(x + 1, z) - f.get(x, z)).abs());
                    }
                    if z + 1 < dim {
                        m = m.max((f.get(x, z + 1) - f.get(x, z)).abs());
                    }
                }
            }
            m
        };
        let hard_step = max_step(&hard);
        let blended_step = max_step(&blended);
        assert!(
            hard_step > 0.5,
            "nearest-cell lookup must show sharp polygon steps: {hard_step}"
        );
        assert!(
            blended_step <= hard_step * 0.5,
            "blended precipitation must not step as hard as the nearest-cell field: {blended_step} vs {hard_step}"
        );

        let mut largest_cell = 0usize;
        let mut best = 0usize;
        for cell in 0..cell_count {
            let n = cell_of_vertex.iter().filter(|&&c| c == cell).count();
            if n > best {
                best = n;
                largest_cell = cell;
            }
        }
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for (i, &c) in cell_of_vertex.iter().enumerate() {
            if c == largest_cell {
                lo = lo.min(blended.get_at(i));
                hi = hi.max(blended.get_at(i));
            }
        }
        assert!(best > 4, "cell too small to test: {best} vertices");
        assert!(
            hi - lo > 1e-3,
            "blended precipitation is constant across cell {largest_cell}: {lo}..{hi}"
        );
    }

    #[test]
    fn crater_across_a_finite_edge_does_not_stack_on_the_border() {
        let mut edge = plain_field(60, 300.0);
        let mut inland = plain_field(60, 300.0);
        let mut edge_crater = test_crater(20.0);
        edge_crater.center = edge.world_of(4, 30);
        let mut inland_crater = test_crater(20.0);
        inland_crater.center = inland.world_of(30, 30);
        apply_craters(&mut edge, &[edge_crater], &|_| true);
        apply_craters(&mut inland, &[inland_crater], &|_| true);

        let border_delta = edge.get(0, 30) - 10.0;
        let inland_delta = inland.get(26, 30) - 10.0;
        assert!(
            inland_delta.abs() > 1e-3,
            "the comparison point must sit inside the crater profile: {inland_delta}"
        );
        assert!(
            (border_delta - inland_delta).abs() < 1e-4,
            "border column absorbed stacked crater contributions: {border_delta} vs {inland_delta}"
        );
    }

    #[test]
    fn plunge_pool_bowl_does_not_stack_on_a_finite_border() {
        let mut edge = plain_field(40, 200.0);
        let mut inland = plain_field(40, 200.0);
        carve_bowl(&mut edge, 1, 20, 12.0, 5.0);
        carve_bowl(&mut inland, 20, 20, 12.0, 5.0);
        let border_delta = edge.get(0, 20) - 10.0;
        let inland_delta = inland.get(19, 20) - 10.0;
        assert!(
            inland_delta.abs() > 1e-3,
            "the comparison point must sit inside the bowl: {inland_delta}"
        );
        assert!(
            (border_delta - inland_delta).abs() < 1e-5,
            "border cell absorbed stacked bowl contributions: {border_delta} vs {inland_delta}"
        );
    }

    #[test]
    fn marginal_rim_basin_drains_to_outlet_without_spilling() {
        let dim = 24usize;
        let mut field = ScalarField::new(dim, 240.0, WorldTopology::Finite);
        field.fill(10.0);
        for z in 0..dim {
            for x in 0..dim {
                if x == 0 || z == 0 || x == dim - 1 || z == dim - 1 {
                    field.set(x, z, 10.001);
                }
            }
        }
        field.set(0, dim / 2, 5.0);
        let hydro = priority_flood(&field, 0.0);
        let outlet = field.index(0, dim / 2);
        for z in 1..dim - 1 {
            for x in 1..dim - 1 {
                let index = field.index(x, z);
                assert!(
                    hydro.routed.get_at(index) < 10.001,
                    "routing tilt inflated floor cell ({x},{z}) past the rim: {}",
                    hydro.routed.get_at(index)
                );
                if let Some(parent) = hydro.downstream[index] {
                    assert!(
                        hydro.filled.get_at(parent) < 10.0005,
                        "floor cell ({x},{z}) spilled onto the rim"
                    );
                }
                let mut cursor = Some(index);
                let mut reached = false;
                let mut steps = 0;
                while let Some(current) = cursor {
                    if current == outlet {
                        reached = true;
                        break;
                    }
                    cursor = hydro.downstream[current];
                    steps += 1;
                    if steps > field.len() {
                        break;
                    }
                }
                assert!(reached, "floor cell ({x},{z}) never reached the outlet");
            }
        }
    }

    fn single_channel(dim: usize) -> (ScalarField, Hydrology) {
        let mut field = ScalarField::new(dim, dim as f32 * 10.0, WorldTopology::Finite);
        for z in 0..dim {
            for x in 0..dim {
                field.set(x, z, x as f32);
            }
        }
        field.set(dim - 4, dim - 4, 30.0);
        let mut downstream = vec![None; field.len()];
        let mut accumulation = ScalarField::new(dim, field.size, WorldTopology::Finite);
        for x in 1..dim {
            downstream[field.index(x, dim / 2)] = Some(field.index(x - 1, dim / 2));
            accumulation.set(x, dim / 2, 100.0);
        }
        let mut visit_order: Vec<usize> = (0..field.len()).collect();
        visit_order.sort_by(|a, b| {
            field
                .get_at(*b)
                .total_cmp(&field.get_at(*a))
                .then(a.cmp(b))
        });
        let hydro = Hydrology {
            filled: field.clone(),
            routed: field.clone(),
            downstream,
            secondary: vec![None; field.len()],
            primary_weight: vec![1.0; field.len()],
            accumulation,
            visit_order,
            depressions: vec![false; field.len()],
        };
        (field, hydro)
    }

    #[test]
    fn carve_rivers_never_undercuts_its_downstream_neighbour() {
        let (field, hydro) = single_channel(16);
        let mut carved = field.clone();
        carve_rivers(&mut carved, &hydro, 10.0, 50.0);
        let mut incised = 0;
        for x in 1..16 {
            let child = carved.get(x, 8);
            let parent = carved.get(x - 1, 8);
            assert!(
                child >= parent - 1e-6,
                "carved cell {x} sits below its downstream neighbour: {child} < {parent}"
            );
            if child < field.get(x, 8) - 1e-3 {
                incised += 1;
            }
        }
        assert!(incised > 0, "the channel must still be incised");
    }

    #[test]
    fn carve_rivers_leaves_distant_peaks_untouched() {
        let (field, hydro) = single_channel(16);
        let mut carved = field.clone();
        carve_rivers(&mut carved, &hydro, 10.0, 50.0);
        assert_eq!(
            carved.get(12, 12),
            field.get(12, 12),
            "a peak far from the channel must not be blurred"
        );
    }

    fn scarp_waterfall_count(dim: usize) -> usize {
        let size = 160.0;
        let mut field = ScalarField::new(dim, size, WorldTopology::Finite);
        for z in 0..dim {
            for x in 0..dim {
                let w = field.world_of(x, z);
                let mut h = -0.3 * w.x + 0.5 * w.y.abs();
                h += 60.0 * (1.0 - ((w.x + 12.5) / 25.0).clamp(0.0, 1.0));
                field.set(x, z, h);
            }
        }
        let mut rain = ScalarField::new(dim, size, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);
        let falls = find_waterfalls(
            &field,
            &hydro,
            (dim * dim) as f32 * 0.07,
            5.0,
            &vec![false; field.len()],
        );
        falls.len()
    }

    #[test]
    fn waterfall_count_is_stable_across_subdivisions() {
        let coarse = scarp_waterfall_count(17);
        let fine = scarp_waterfall_count(33);
        assert!(coarse >= 1, "the scarp must produce a waterfall: {coarse}");
        assert!(
            (coarse as isize - fine as isize).abs() <= 1,
            "waterfall count must be stable under a resolution change: {coarse} vs {fine}"
        );
    }
}
