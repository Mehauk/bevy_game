use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;
use crate::terrain::seed::{WorldSeed, STAGE_CRATERS};

use super::geom::in_bounds;

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

const CRATER_PLACEMENT_INSET: f32 = 0.85;
const TERRACED_CRATER_MIN_RADIUS: f32 = 16.0;
const TERRACE_COUNT: usize = 4;
const CRATER_FIELD_RADIUS_MULTIPLIER: f32 = 3.0;
const CRATER_RIM_PEAK_RADII: f32 = 1.1;
const CRATER_RIM_HEIGHT_FRACTION: f32 = 0.45;
const CRATER_RIM_FALLOFF_RADII_SQ: f32 = 0.05;
const CRATER_RAY_HALF_WIDTH_RAD: f32 = 0.18;
const CRATER_RAY_REACH_RADII: f32 = 2.0;
const CRATER_RAY_EJECTA_FRACTION: f32 = 0.25;

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
                    seed.range(
                        STAGE_CRATERS,
                        id,
                        1,
                        -half * CRATER_PLACEMENT_INSET,
                        half * CRATER_PLACEMENT_INSET,
                    ),
                    seed.range(
                        STAGE_CRATERS,
                        id,
                        2,
                        -half * CRATER_PLACEMENT_INSET,
                        half * CRATER_PLACEMENT_INSET,
                    ),
                ),
                radius,
                depth,
                aspect: seed.range(STAGE_CRATERS, id, 7, 0.72, 1.0),
                rotation: seed.range(STAGE_CRATERS, id, 8, 0.0, std::f32::consts::TAU),
                degradation: config.degradation.clamp(0.0, 0.95),
                terraces: if radius > TERRACED_CRATER_MIN_RADIUS {
                    TERRACE_COUNT
                } else {
                    0
                },
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
        let reach =
            (crater.radius * CRATER_FIELD_RADIUS_MULTIPLIER / heights.spacing()).ceil() as isize + 2;
        let cos_r = crater.rotation.cos();
        let sin_r = crater.rotation.sin();
        let intactness = 1.0 - crater.degradation;

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
                if distance > crater.radius * CRATER_FIELD_RADIUS_MULTIPLIER {
                    continue;
                }
                let angle = rotated.y.atan2(rotated.x);
                let wobble = 1.0
                    + 0.12 * (2.0 * angle + crater.lobes[0]).sin()
                    + 0.08 * (3.0 * angle + crater.lobes[1]).sin()
                    + 0.05 * (5.0 * angle + crater.lobes[2]).sin();
                let major = crater.radius * wobble;
                let minor = major * crater.aspect;
                let angle_cos = angle.cos();
                let angle_sin = angle.sin();
                let local_radius = (major * minor)
                    / ((minor * minor * angle_cos * angle_cos
                        + major * major * angle_sin * angle_sin)
                        .sqrt());
                let distance_in_radii = distance / local_radius.max(1e-3);

                let mut offset = 0.0f32;
                if distance_in_radii < 1.0 {
                    let profile = if crater.terraces > 0 {
                        let steps = crater.terraces as f32;
                        (distance_in_radii * steps).floor() / steps
                    } else {
                        distance_in_radii
                    };
                    offset += -crater.depth * (1.0 - profile * profile) * intactness;
                }
                let rim_offset = distance_in_radii - CRATER_RIM_PEAK_RADII;
                offset += crater.depth * CRATER_RIM_HEIGHT_FRACTION
                    * intactness
                    * (-((rim_offset * rim_offset) / CRATER_RIM_FALLOFF_RADII_SQ)).exp();

                if distance_in_radii > 1.0 && distance_in_radii < CRATER_FIELD_RADIUS_MULTIPLIER {
                    for &ray in &crater.ray_angles {
                        let ray_angle_delta = angle_delta(angle - ray);
                        if ray_angle_delta.abs() >= CRATER_RAY_HALF_WIDTH_RAD {
                            continue;
                        }
                        let along =
                            (1.0 - (distance_in_radii - 1.0) / CRATER_RAY_REACH_RADII).max(0.0);
                        let across = 1.0 - ray_angle_delta.abs() / CRATER_RAY_HALF_WIDTH_RAD;
                        offset += crater.depth
                            * CRATER_RAY_EJECTA_FRACTION
                            * intactness
                            * along
                            * across;
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

    use crate::terrain::hydrology::test_support::plain_field;
    use crate::terrain::topology::WorldTopology;

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
}
