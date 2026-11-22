use crate::terrain::field::ScalarField;

use super::flow::Hydrology;

const MAX_RIVER_INCISION: f32 = 6.0;
const RIVER_DEPOSIT_FRACTION: f32 = 0.35;

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
    let mut has_upstream = vec![false; on_river.len()];
    for (index, child) in hydrology.downstream.iter().enumerate() {
        if !on_river.get(index).copied().unwrap_or(false) {
            continue;
        }
        if let Some(child) = child {
            if on_river.get(*child).copied().unwrap_or(false) {
                has_upstream[*child] = true;
            }
        }
    }

    for &start in hydrology.visit_order.iter().rev() {
        if !on_river[start] || claimed[start] || has_upstream[start] {
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

pub fn carve_rivers(
    heights: &mut ScalarField,
    hydrology: &Hydrology,
    threshold: f32,
    strength: f32,
    hardness: Option<&ScalarField>,
) {
    let mut incisions = vec![0.0f32; heights.len()];
    let relief = (heights.max() - heights.min()).max(1.0);

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
            / relief;
        let erodibility = hardness
            .map(|field| 1.0 - field.get_at(index).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        let capacity = strength * accumulation.sqrt() * slope.sqrt() * erodibility;
        let incision = capacity.min(MAX_RIVER_INCISION);
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
            heights.add(px, pz, carried * RIVER_DEPOSIT_FRACTION);
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::hydrology::test_support::{constant_field, plane};
    use crate::terrain::hydrology::{Hydrology, find_lakes, find_waterfalls, flow_accumulation, priority_flood};
    use crate::terrain::topology::WorldTopology;

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
        carve_rivers(&mut carved, &hydro, 12.0, 2.0, None);
        assert!(carved.mean() < field.mean() + 0.5, "carving must not raise the terrain overall");
        let incised = carved
            .values
            .iter()
            .zip(field.values.iter())
            .filter(|(carved, original)| **carved < **original - 1e-3)
            .count();
        assert!(incised > 0, "channels should be cut below the source surface");
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
        carve_rivers(&mut carved, &hydro, 10.0, 50.0, None);
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
        carve_rivers(&mut carved, &hydro, 10.0, 50.0, None);
        assert_eq!(
            carved.get(12, 12),
            field.get(12, 12),
            "a peak far from the channel must not be blurred"
        );
    }

    #[test]
    fn hard_bedrock_resists_river_incision() {
        let (field, hydro) = single_channel(16);
        let mut soft = field.clone();
        carve_rivers(&mut soft, &hydro, 10.0, 50.0, None);
        let mut hard = field.clone();
        carve_rivers(
            &mut hard,
            &hydro,
            10.0,
            50.0,
            Some(&constant_field(16, 160.0, WorldTopology::Finite, 0.95)),
        );
        let soft_cut: f32 = soft
            .values
            .iter()
            .zip(field.values.iter())
            .map(|(c, o)| (o - c).max(0.0))
            .sum();
        let hard_cut: f32 = hard
            .values
            .iter()
            .zip(field.values.iter())
            .map(|(c, o)| (o - c).max(0.0))
            .sum();
        let deeper: f32 = soft
            .values
            .iter()
            .zip(hard.values.iter())
            .map(|(soft, hard)| (hard - soft).max(0.0))
            .sum();
        assert!(soft_cut > 0.0, "the soft channel must be incised");
        assert!(
            hard_cut < soft_cut,
            "hard bedrock must resist incision: {hard_cut} vs {soft_cut}"
        );
        assert!(
            deeper > 1.0,
            "the soft bed must finish measurably below the hard bed: {deeper}"
        );
    }
}
