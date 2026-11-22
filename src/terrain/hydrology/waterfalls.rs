use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;

use super::flow::Hydrology;
use super::geom::{in_bounds, step_distance};
use super::rivers::river_cells;

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
const PLUNGE_POOL_DEPTH_FRACTION: f32 = 0.6;
const PLUNGE_POOL_MIN_DEPTH: f32 = 0.5;
const PLUNGE_POOL_MAX_DEPTH: f32 = 6.0;
const PLUNGE_POOL_RADIUS_CELLS: f32 = 2.0;
const OUTLET_POOL_COUNT: usize = 2;
const OUTLET_POOL_RADIUS_BASE: f32 = 2.0;
const OUTLET_POOL_DEPTH_FRACTION: f32 = 0.35;

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
            let step_drop = heights.get_at(cursor) - heights.get_at(after);
            if step_drop <= 0.0
                || step_drop / step_distance(heights, cursor, after, spacing) < WATERFALL_MIN_SLOPE
            {
                break;
            }
            claimed[cursor] = true;
            drop += step_drop;
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
        let depth =
            (fall.drop * PLUNGE_POOL_DEPTH_FRACTION).clamp(PLUNGE_POOL_MIN_DEPTH, PLUNGE_POOL_MAX_DEPTH);
        let radius = heights.spacing() * PLUNGE_POOL_RADIUS_CELLS;
        fall.pool_depth = depth;
        fall.pool_radius = radius;
        carve_bowl(heights, px, pz, radius, depth);

        let mut cursor = fall.next;
        for outlet in 0..OUTLET_POOL_COUNT {
            let Some(after) = hydrology.downstream[cursor] else {
                break;
            };
            let (ax, az) = heights.coords(after);
            carve_bowl(
                heights,
                ax,
                az,
                radius * (OUTLET_POOL_RADIUS_BASE + outlet as f32),
                depth * OUTLET_POOL_DEPTH_FRACTION,
            );
            cursor = after;
        }

        fall.drop = heights.get_at(fall.index) - heights.get_at(fall.next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::hydrology::test_support::{plain_field, plane};
    use crate::terrain::hydrology::{find_lakes, flow_accumulation, priority_flood};
    use crate::terrain::topology::WorldTopology;

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

    #[test]
    fn plunge_pools_carved_before_lake_extraction_hold_water() {
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

        carve_plunge_pools(&mut field, &hydro, &mut falls);

        let mut settled = priority_flood(&field, -1000.0);
        flow_accumulation(&mut settled, &rain);
        let lakes = find_lakes(&field, &settled, -1000.0, &vec![false; field.len()]);
        for fall in &falls {
            let holds_water = lakes
                .iter()
                .any(|lake| lake.cells.contains(&fall.next) && lake.depth > 0.0);
            assert!(
                holds_water,
                "the plunge pool at {} must be filled as a lake, not left a dry pit",
                fall.next
            );
        }
    }
}
