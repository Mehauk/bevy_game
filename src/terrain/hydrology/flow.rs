use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;

use super::geom::step_distance;

const TILT_EPSILON: f32 = 1e-3;
pub(crate) const DEPRESSION_EPSILON: f32 = 1e-4;
const SECTOR_WIDTH_RAD: f32 = std::f32::consts::FRAC_PI_4;
const PLATEAU_TILT_GAP_FRACTION: f32 = 0.5;
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

pub struct Hydrology {
    pub filled: ScalarField,
    #[allow(dead_code)]
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
        let next_tilt = tilt[index] + TILT_EPSILON;
        for neighbour in filled.neighbours8(x, z) {
            if !queued[neighbour] && filled.get_at(neighbour) == level {
                queued[neighbour] = true;
                tilt[neighbour] = next_tilt;
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
        let mut min_rim_gap = f32::INFINITY;
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
                    min_rim_gap = min_rim_gap.min(value - level);
                }
            }
        }
        let plateau_scale = if max_tilt > 0.0 && min_rim_gap.is_finite() {
            ((min_rim_gap * PLATEAU_TILT_GAP_FRACTION) / max_tilt).min(1.0)
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
            let sector = ((theta / SECTOR_WIDTH_RAD) as usize).min(7);
            let split = ((theta - sector as f32 * SECTOR_WIDTH_RAD) / SECTOR_WIDTH_RAD)
                .clamp(0.0, 1.0);
            let primary_candidate = sector_target(routed, x, z, SECTOR_DIRS[sector], here);
            let secondary_candidate =
                sector_target(routed, x, z, SECTOR_DIRS[(sector + 1) % 8], here);
            match (primary_candidate, secondary_candidate) {
                (Some(main), Some(alt)) => {
                    primary[index] = Some(main);
                    secondary[index] = Some(alt);
                    weight[index] = 1.0 - split;
                }
                (Some(main), None) => {
                    primary[index] = Some(main);
                }
                (None, Some(alt)) => {
                    primary[index] = Some(alt);
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

#[derive(Clone, Debug)]
pub struct FlowField {
    pub direction_x: ScalarField,
    pub direction_y: ScalarField,
    pub slope: ScalarField,
}

impl FlowField {
    pub fn direction_at(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            self.direction_x.sample_world(p),
            self.direction_y.sample_world(p),
        )
    }

    pub fn slope_at(&self, p: Vec2) -> f32 {
        self.slope.sample_world(p)
    }
}

pub fn bake_flow_vectors(heights: &ScalarField, hydrology: &Hydrology) -> FlowField {
    let spacing = heights.spacing().max(1e-3);
    let mut direction_x = ScalarField::new(heights.dim, heights.size, heights.topology);
    let mut direction_y = ScalarField::new(heights.dim, heights.size, heights.topology);
    let mut slope = ScalarField::new(heights.dim, heights.size, heights.topology);

    for index in 0..heights.len() {
        let (x, z) = heights.coords(index);
        let here = heights.world_of(x, z);
        let mut vector = Vec2::ZERO;
        let mut slope_acc = 0.0f32;
        let mut weight_sum = 0.0f32;

        let mut contribute = |target: Option<usize>, share: f32| {
            let Some(target) = target else {
                return;
            };
            if share <= 0.0 {
                return;
            }
            let (tx, tz) = heights.coords(target);
            let there = heights.world_of(tx, tz);
            let delta = heights.topology.delta_xz(here, there, heights.size);
            let distance = step_distance(heights, index, target, spacing);
            let drop = (hydrology.filled.get_at(index) - hydrology.filled.get_at(target)).max(0.0);
            vector += delta.normalize_or_zero() * share;
            slope_acc += drop / distance.max(1e-3) * share;
            weight_sum += share;
        };
        contribute(hydrology.downstream[index], hydrology.primary_weight[index]);
        contribute(
            hydrology.secondary[index],
            1.0 - hydrology.primary_weight[index],
        );

        if weight_sum > 0.0 {
            direction_x.set_at(index, vector.x / weight_sum);
            direction_y.set_at(index, vector.y / weight_sum);
            slope.set_at(index, slope_acc / weight_sum);
        }
    }

    FlowField {
        direction_x,
        direction_y,
        slope,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::hydrology::test_support::{plane, rain};
    use crate::terrain::topology::WorldTopology;

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

    #[test]
    fn flow_vectors_point_downstream_with_slope() {
        let field = plane(16, 160.0, WorldTopology::Finite, 1.0);
        let mut rain = ScalarField::new(16, 160.0, WorldTopology::Finite);
        rain.fill(1.0);
        let mut hydro = priority_flood(&field, -1000.0);
        flow_accumulation(&mut hydro, &rain);
        let flow = bake_flow_vectors(&field, &hydro);

        let probe = field.index(12, 8);
        let (px, pz) = field.coords(probe);
        let dir = flow.direction_at(field.world_of(px, pz));
        assert!(dir.x < -0.5, "flow must point downhill along -x: {dir:?}");
        assert!(dir.length() <= 1.0 + 1e-3, "blended direction must stay unit-ish");
        assert!(
            flow.slope_at(field.world_of(px, pz)) > 0.0,
            "sloped terrain must report a positive downstream slope"
        );

        let outlet = field.index(0, 8);
        let (ox, oz) = field.coords(outlet);
        assert!(
            flow.direction_at(field.world_of(ox, oz)).length() < 1e-3,
            "an edge outlet with no downstream must bake a zero vector"
        );
    }

    #[test]
    fn flow_vectors_cross_the_torus_seam() {
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
        let flow = bake_flow_vectors(&field, &hydro);

        let seam = field.index(11, 6);
        let (sx, sz) = field.coords(seam);
        let dir = flow.direction_at(field.world_of(sx, sz));
        assert!(
            dir.x > 0.5,
            "flow at the high seam edge must wrap across the boundary toward the low side: {dir:?}"
        );
        assert!((dir.length() - 1.0).abs() < 0.2, "seam vector must stay unit-ish: {dir:?}");
    }
}
