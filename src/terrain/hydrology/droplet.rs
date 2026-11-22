use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;
use crate::terrain::seed::{WorldSeed, splitmix64, STAGE_DROPLETS};

use super::geom::{deposit_bilinear, sample_values};

pub const UNCONSOLIDATED_HARDNESS: f32 = 0.1;

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
    pub step: f32,
    pub salt: u64,
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
            step: 1.0,
            salt: 0,
        }
    }
}

const MIN_DROPLET_STEP: f32 = 0.05;
const MIN_DROPLET_STEPS: f32 = 8.0;
const MIN_DROPLET_VELOCITY: f32 = 0.1;
const MIN_DROPLET_WATER: f32 = 0.01;
const LOOSE_FULL_EFFECT_DEPTH: f32 = 0.5;

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
    droplet_erosion_with_hardness(heights, precipitation, seed, config, None);
}

pub fn droplet_erosion_with_hardness(
    heights: &mut ScalarField,
    precipitation: &ScalarField,
    seed: WorldSeed,
    config: &ErosionConfig,
    hardness: Option<&ScalarField>,
) {
    if config.droplet_count == 0 || config.iterations == 0 {
        return;
    }
    let mut rng = DropletRng::new(
        seed.stage(STAGE_DROPLETS) ^ config.salt.wrapping_mul(0x9E37_79B9_7F4A_7C15),
    );
    let dim = heights.dim as f32;
    let step = config.step.max(MIN_DROPLET_STEP);
    let max_steps = (dim / step).max(MIN_DROPLET_STEPS) as usize;
    let wraps = heights.topology.wraps();
    let mut loose_sediment = vec![0.0f32; heights.len()];

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
                let gradient_x = sample_values(heights, &heights.values, pos.x + 0.5, pos.y)
                    - sample_values(heights, &heights.values, pos.x - 0.5, pos.y);
                let gradient_z = sample_values(heights, &heights.values, pos.x, pos.y + 0.5)
                    - sample_values(heights, &heights.values, pos.x, pos.y - 0.5);
                let slope = Vec2::new(-gradient_x, -gradient_z);
                if slope.length_squared() > 1e-9 {
                    dir = (dir * config.inertia + slope.normalize()).normalize_or_zero();
                    if dir.length_squared() < 1e-9 {
                        break;
                    }
                }
                let next = pos + dir * step;
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
                let height_change = new_height - height;
                velocity = (velocity * velocity + (-height_change).max(0.0) * config.gravity)
                    .sqrt()
                    .max(MIN_DROPLET_VELOCITY);
                let capacity = (-height_change).max(config.min_slope)
                    * velocity
                    * water
                    * config.capacity_k;

                if sediment > capacity {
                    let deposit = (sediment - capacity) * config.deposit_rate;
                    sediment -= deposit;
                    apply_brush(heights, pos.x, pos.y, config.deposit_radius, deposit);
                    deposit_bilinear(heights, &mut loose_sediment, pos.x, pos.y, deposit);
                } else {
                    let erodibility = match hardness {
                        None => 1.0,
                        Some(field) => {
                            let bedrock =
                                sample_values(field, &field.values, pos.x, pos.y).clamp(0.0, 1.0);
                            let unconsolidated = (sample_values(heights, &loose_sediment,
                                pos.x, pos.y)
                                / LOOSE_FULL_EFFECT_DEPTH)
                                .min(1.0);
                            let effective = bedrock * (1.0 - unconsolidated)
                                + UNCONSOLIDATED_HARDNESS * unconsolidated;
                            1.0 - effective
                        }
                    };
                    let erode = ((capacity - sediment) * config.erode_rate * erodibility)
                        .min((-height_change).max(0.0));
                    if erode > 0.0 {
                        sediment += erode;
                        apply_brush(heights, pos.x, pos.y, config.erode_radius, -erode);
                    }
                }

                water *= 1.0 - config.evaporation;
                if water < MIN_DROPLET_WATER {
                    break;
                }
            }

            if sediment > 0.0 {
                apply_brush(heights, pos.x, pos.y, config.deposit_radius, sediment);
                deposit_bilinear(heights, &mut loose_sediment, pos.x, pos.y, sediment);
            }
        }
    }
}

pub const EROSION_STEP_METRES: f32 = 2.0;
pub const EROSION_ERODE_RADIUS_METRES: f32 = 3.0;
pub const EROSION_DEPOSIT_RADIUS_METRES: f32 = 4.0;
pub const DROPLET_REFERENCE_CELLS: usize = 40_000;

pub fn erosion_geometry(spacing: f32) -> (f32, f32, f32) {
    let spacing = spacing.max(1e-3);
    (
        EROSION_STEP_METRES / spacing,
        EROSION_ERODE_RADIUS_METRES / spacing,
        EROSION_DEPOSIT_RADIUS_METRES / spacing,
    )
}

pub fn scaled_droplet_count(reference: usize, dim: usize) -> usize {
    let cells = (dim * dim).max(1) as f64;
    let scaled = reference as f64 * cells / DROPLET_REFERENCE_CELLS as f64;
    scaled.round().max(1.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::hydrology::test_support::{cone_field, constant_field, rain};
    use crate::terrain::topology::WorldTopology;

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

    fn total_change(field: &ScalarField, base: &ScalarField) -> f32 {
        field
            .values
            .iter()
            .zip(base.values.iter())
            .map(|(a, b)| (a - b).abs())
            .sum()
    }

    #[test]
    fn hard_bedrock_resists_droplet_erosion() {
        let config = ErosionConfig {
            droplet_count: 1_500,
            iterations: 2,
            ..Default::default()
        };
        let precipitation = rain(24, 240.0, WorldTopology::FiniteTorus);
        let base = cone_field(24, 240.0, WorldTopology::FiniteTorus);

        let mut soft = base.clone();
        droplet_erosion_with_hardness(
            &mut soft,
            &precipitation,
            WorldSeed::new(11),
            &config,
            Some(&constant_field(24, 240.0, WorldTopology::FiniteTorus, 0.05)),
        );
        let mut hard = base.clone();
        droplet_erosion_with_hardness(
            &mut hard,
            &precipitation,
            WorldSeed::new(11),
            &config,
            Some(&constant_field(24, 240.0, WorldTopology::FiniteTorus, 0.95)),
        );

        let soft_change = total_change(&soft, &base);
        let hard_change = total_change(&hard, &base);
        assert!(
            hard_change < soft_change * 0.5,
            "hard bedrock must erode far less than soft sediment: {hard_change} vs {soft_change}"
        );
    }

    #[test]
    fn droplet_erosion_without_hardness_matches_the_wrapper() {
        let config = ErosionConfig {
            droplet_count: 400,
            iterations: 1,
            ..Default::default()
        };
        let precipitation = rain(24, 240.0, WorldTopology::FiniteTorus);
        let mut wrapped = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        let mut explicit = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        droplet_erosion(&mut wrapped, &precipitation, WorldSeed::new(7), &config);
        droplet_erosion_with_hardness(
            &mut explicit,
            &precipitation,
            WorldSeed::new(7),
            &config,
            None,
        );
        assert_eq!(wrapped.values, explicit.values);
    }

    #[test]
    fn droplet_erosion_scales_with_step_and_salt() {
        let precipitation = rain(24, 240.0, WorldTopology::FiniteTorus);
        let base = cone_field(24, 240.0, WorldTopology::FiniteTorus);
        let mut first = base.clone();
        droplet_erosion(
            &mut first,
            &precipitation,
            WorldSeed::new(5),
            &ErosionConfig {
                droplet_count: 400,
                iterations: 1,
                salt: 1,
                ..Default::default()
            },
        );
        let mut second = base.clone();
        droplet_erosion(
            &mut second,
            &precipitation,
            WorldSeed::new(5),
            &ErosionConfig {
                droplet_count: 400,
                iterations: 1,
                salt: 2,
                ..Default::default()
            },
        );
        assert_ne!(
            first.values, second.values,
            "different salts must throw different droplet paths"
        );

        let mut halved = base.clone();
        droplet_erosion(
            &mut halved,
            &precipitation,
            WorldSeed::new(5),
            &ErosionConfig {
                droplet_count: 400,
                iterations: 1,
                step: 0.5,
                ..Default::default()
            },
        );
        let mut whole = base.clone();
        droplet_erosion(
            &mut whole,
            &precipitation,
            WorldSeed::new(5),
            &ErosionConfig {
                droplet_count: 400,
                iterations: 1,
                step: 1.0,
                ..Default::default()
            },
        );
        assert_ne!(
            halved.values, whole.values,
            "a shorter step must change the droplet reach"
        );
    }
    #[test]
    fn erosion_geometry_keeps_droplet_reach_in_world_metres() {
        let (step, erode, deposit) = erosion_geometry(2.0);
        assert_eq!((step, erode, deposit), (1.0, 1.5, 2.0));
        let (coarse_step, coarse_erode, coarse_deposit) = erosion_geometry(4.0);
        assert!(
            (coarse_step - step * 0.5).abs() < 1e-6
                && (coarse_erode - erode * 0.5).abs() < 1e-6
                && (coarse_deposit - deposit * 0.5).abs() < 1e-6
        );
        for spacing in [0.25, 1.0, 2.0, 8.0, 64.0] {
            let (step, erode, deposit) = erosion_geometry(spacing);
            assert!((step * spacing - EROSION_STEP_METRES).abs() < 1e-3);
            assert!((erode * spacing - EROSION_ERODE_RADIUS_METRES).abs() < 1e-3);
            assert!((deposit * spacing - EROSION_DEPOSIT_RADIUS_METRES).abs() < 1e-3);
            assert!(deposit > erode && erode > step);
        }
        let (fallback, ..) = erosion_geometry(0.0);
        assert!(fallback.is_finite() && fallback > 0.0);
    }

    #[test]
    fn droplet_count_scales_with_grid_area() {
        assert_eq!(scaled_droplet_count(8_000, 200), 8_000);
        assert_eq!(scaled_droplet_count(8_000, 400), 32_000);
        assert_eq!(scaled_droplet_count(8_000, 100), 2_000);
        assert_eq!(scaled_droplet_count(8_000, 32), 205);
        assert_eq!(scaled_droplet_count(0, 200), 1, "never drop every droplet");
        assert_eq!(scaled_droplet_count(8_000, 0), 1);
    }
}
