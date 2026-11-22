use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;
use crate::terrain::params::{CellParameters, ParamLevel, Wind};
use crate::terrain::voronoi::VertexBlendCache;

use super::geom::{deposit_bilinear, sample_values};

pub fn apply_wind_erosion(
    heights: &mut ScalarField,
    wind: Vec2,
    strength: f32,
    iterations: usize,
    erodibility: Option<&ScalarField>,
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
                let gate = erodibility
                    .map(|field| field.get_at(index).clamp(0.0, 1.0))
                    .unwrap_or(1.0);
                if gate <= 0.0 {
                    continue;
                }
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
                let carry = deficit * strength * gate;
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

const HUMID_ERODIBILITY_FACTOR: f32 = 0.3;
const WIND_FULL_EROSION_HEIGHT_METRES: f32 = 2.0;

pub fn wind_erodibility_field(
    cache: &VertexBlendCache,
    heights: &ScalarField,
    cell_parameters: &[CellParameters],
    sea_level: f32,
) -> ScalarField {
    let mut field = ScalarField::new(heights.dim, heights.size, heights.topology);
    let calmest_wind_index = Wind::COUNT.saturating_sub(1).max(1) as f32;
    for index in 0..heights.len() {
        let mut total = 0.0f32;
        let mut sum = 0.0f32;
        for &(cell_id, weight) in cache.weights_at(index) {
            let Some(parameters) = cell_parameters.get(cell_id) else {
                continue;
            };
            let gust = 1.0 - parameters.wind.index() as f32 / calmest_wind_index;
            let aridity = if parameters.is_arid() {
                1.0
            } else {
                HUMID_ERODIBILITY_FACTOR
            };
            sum += parameters.substrate.erodibility() * (0.2 + 0.8 * gust) * aridity;
            total += weight;
        }
        let dryness =
            ((heights.get_at(index) - sea_level) / WIND_FULL_EROSION_HEIGHT_METRES).clamp(0.0, 1.0);
        let gate = if total > 0.0 { sum / total } else { 0.0 };
        field.set_at(index, gate.max(0.0) * dryness);
    }
    field
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::hydrology::test_support::{cone_field, constant_field};
    use crate::terrain::topology::WorldTopology;

    #[test]
    fn wind_erosion_moves_material_downwind() {
        let mut field = ScalarField::new(20, 200.0, WorldTopology::Finite);
        for z in 0..20 {
            for x in 0..20 {
                field.set(x, z, (x as f32 * 0.5).sin() * 4.0);
            }
        }
        let before: f32 = field.values.iter().map(|v| v.abs()).sum();
        apply_wind_erosion(&mut field, Vec2::X, 0.25, 1, None);
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
        apply_wind_erosion(&mut field, Vec2::new(1.0, 0.3), 0.3, 30, None);
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
        apply_wind_erosion(&mut field, Vec2::new(0.7, -0.7), 0.25, 8, None);
        let after: f32 = field.values.iter().sum();
        assert!(
            (before - after).abs() < before.abs() * 1e-3 + 1e-2,
            "wind transport must conserve sediment: {before} -> {after}"
        );
    }

    #[test]
    fn wind_erodibility_gates_transport() {
        let mut field = ScalarField::new(20, 200.0, WorldTopology::Finite);
        for z in 0..20 {
            for x in 0..20 {
                field.set(x, z, (x as f32 * 0.5).sin() * 4.0);
            }
        }
        let mut sealed = field.clone();
        apply_wind_erosion(
            &mut sealed,
            Vec2::X,
            0.25,
            1,
            Some(&constant_field(20, 200.0, WorldTopology::Finite, 0.0)),
        );
        assert_eq!(
            sealed.values, field.values,
            "zero erodibility must freeze the surface entirely"
        );

        let mut unmasked = field.clone();
        let mut uniform = field.clone();
        apply_wind_erosion(&mut unmasked, Vec2::X, 0.25, 1, None);
        apply_wind_erosion(
            &mut uniform,
            Vec2::X,
            0.25,
            1,
            Some(&constant_field(20, 200.0, WorldTopology::Finite, 1.0)),
        );
        assert_eq!(
            unmasked.values, uniform.values,
            "uniform erodibility of one must match the unmasked pass"
        );
    }
}
