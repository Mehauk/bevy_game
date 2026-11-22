use crate::terrain::field::ScalarField;
use crate::terrain::params::Substrate;

const THERMAL_TRANSFER_FRACTION: f32 = 0.25;

pub fn repose_tan(substrate: Substrate) -> f32 {
    match substrate {
        Substrate::Sand => 33.0f32.to_radians().tan(),
        Substrate::SiltClay => 35.0f32.to_radians().tan(),
        Substrate::LimestoneKarst => 40.0f32.to_radians().tan(),
        Substrate::RockCrystalline => 45.0f32.to_radians().tan(),
        Substrate::Volcanic => 42.0f32.to_radians().tan(),
    }
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
                        let transfer = (difference - limit) * THERMAL_TRANSFER_FRACTION;
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::topology::WorldTopology;

    #[test]
    fn thermal_relaxation_caps_steep_slopes() {
        let mut field = ScalarField::new(12, 120.0, WorldTopology::Finite);
        field.set(6, 6, 100.0);
        let mut repose = ScalarField::new(12, 120.0, WorldTopology::Finite);
        repose.fill(0.6);
        let before = field.max() - field.min();
        thermal_relax_repose(&mut field, &repose, 40);
        let after = field.max() - field.min();
        assert!(after < before, "a spike must relax: {before} -> {after}");

        let mut flat = ScalarField::new(8, 80.0, WorldTopology::Finite);
        flat.fill(5.0);
        let mut flat_repose = ScalarField::new(8, 80.0, WorldTopology::Finite);
        flat_repose.fill(0.5);
        thermal_relax_repose(&mut flat, &flat_repose, 10);
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
}
