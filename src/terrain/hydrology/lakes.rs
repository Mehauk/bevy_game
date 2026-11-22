use std::collections::VecDeque;

use crate::terrain::field::ScalarField;

use super::flow::{DEPRESSION_EPSILON, Hydrology};

#[derive(Clone, Debug)]
pub struct Lake {
    pub cells: Vec<usize>,
    pub surface: f32,
    pub depth: f32,
    #[allow(dead_code)]
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
    for &cell in cells {
        let (x, z) = heights.coords(cell);
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::hydrology::priority_flood;
    use crate::terrain::topology::WorldTopology;

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
}
