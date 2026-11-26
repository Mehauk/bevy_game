use bevy::prelude::*;

use super::grid::bilinear;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ErosionConfig {
    pub enabled: bool,
    pub seed: u32,
    pub thermal_iterations: u32,
    pub hydraulic_drops: u32,
    pub talus_angle: f32,
}

impl Default for ErosionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            seed: 0,
            thermal_iterations: 4,
            hydraulic_drops: 2000,
            talus_angle: 0.6,
        }
    }
}

const DROP_INERTIA: f32 = 0.05;
const DROP_EVAPORATION: f32 = 0.97;
const DROP_CAPACITY_GAIN: f32 = 4.0;
const DROP_MIN_CAPACITY: f32 = 0.01;
const DROP_DISSOLVE: f32 = 0.3;
const DROP_DEPOSIT: f32 = 0.3;
const DROP_MAX_STEPS: u32 = 128;
const DROP_MIN_WATER: f32 = 0.01;

pub(super) fn apply(
    config: &ErosionConfig,
    heights: &mut [f32],
    resolution: UVec2,
    bounds_min: Vec2,
    texel: Vec2,
) {
    if !config.enabled {
        return;
    }
    thermal(config, heights, resolution, texel);
    hydraulic(config, heights, resolution, bounds_min, texel);
}

fn thermal(
    config: &ErosionConfig,
    heights: &mut [f32],
    resolution: UVec2,
    texel: Vec2,
) {
    let talus = config.talus_angle.tan();
    for _ in 0..config.thermal_iterations {
        for iz in 0..resolution.y {
            for ix in 0..resolution.x {
                let here = (iz * resolution.x + ix) as usize;
                let height = heights[here];
                let mut best: Option<(f32, usize, f32)> = None;
                let mut consider = |field: &[f32], nx: u32, nz: u32, distance: f32| {
                    let neighbor = (nz * resolution.x + nx) as usize;
                    let value = field[neighbor];
                    if value < height && best.is_none_or(|(low, _, _)| value < low) {
                        best = Some((value, neighbor, distance));
                    }
                };
                if ix > 0 {
                    consider(heights, ix - 1, iz, texel.x);
                }
                if ix + 1 < resolution.x {
                    consider(heights, ix + 1, iz, texel.x);
                }
                if iz > 0 {
                    consider(heights, ix, iz - 1, texel.y);
                }
                if iz + 1 < resolution.y {
                    consider(heights, ix, iz + 1, texel.y);
                }
                if let Some((low, target, distance)) = best {
                    let drop = height - low;
                    if drop > talus * distance {
                        let transfer = (drop - talus * distance) * 0.5;
                        heights[here] = height - transfer;
                        heights[target] += transfer;
                    }
                }
            }
        }
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn next_f32(state: &mut u64) -> f32 {
    splitmix64(state) as f32 / u64::MAX as f32
}

fn splat(
    heights: &mut [f32],
    resolution: UVec2,
    bounds_min: Vec2,
    texel: Vec2,
    x: f32,
    z: f32,
    amount: f32,
) {
    let pos = (Vec2::new(x, z) - bounds_min) / texel - Vec2::splat(0.5);
    let origin = pos.floor();
    let last = resolution - UVec2::splat(1);
    let ix = origin.x.clamp(0.0, last.x as f32) as usize;
    let iz = origin.y.clamp(0.0, last.y as f32) as usize;
    let ix1 = (ix + 1).min(last.x as usize);
    let iz1 = (iz + 1).min(last.y as usize);
    let wx = (pos.x - origin.x).clamp(0.0, 1.0);
    let wz = (pos.y - origin.y).clamp(0.0, 1.0);
    let at = |cx: usize, cz: usize| cz * resolution.x as usize + cx;
    heights[at(ix, iz)] += amount * (1.0 - wx) * (1.0 - wz);
    heights[at(ix1, iz)] += amount * wx * (1.0 - wz);
    heights[at(ix, iz1)] += amount * (1.0 - wx) * wz;
    heights[at(ix1, iz1)] += amount * wx * wz;
}

fn hydraulic(
    config: &ErosionConfig,
    heights: &mut [f32],
    resolution: UVec2,
    bounds_min: Vec2,
    texel: Vec2,
) {
    let size = resolution.as_vec2() * texel;
    let bounds_max = bounds_min + size;
    let inv_size = Vec2::new(1.0 / size.x, 1.0 / size.y);
    let delta = texel.max_element() * 0.5;
    let sample = |field: &[f32], x: f32, z: f32| {
        bilinear(field, resolution, bounds_min, inv_size, x, z)
    };
    for drop in 0..config.hydraulic_drops {
        let mut state = (config.seed as u64)
            ^ ((drop as u64 + 0x9E37_79B9_7F4A_7C15).wrapping_mul(0xD6E8_FEB8_6659_FD93));
        let mut pos = bounds_min
            + Vec2::new(
                next_f32(&mut state) * (size.x - delta),
                next_f32(&mut state) * (size.y - delta),
            );
        let mut velocity = Vec2::ZERO;
        let mut water = 1.0f32;
        let mut sediment = 0.0f32;
        for _ in 0..DROP_MAX_STEPS {
            if water < DROP_MIN_WATER {
                break;
            }
            let gradient = Vec2::new(
                (sample(heights, pos.x + texel.x, pos.y)
                    - sample(heights, pos.x - texel.x, pos.y))
                    / (2.0 * texel.x),
                (sample(heights, pos.x, pos.y + texel.y)
                    - sample(heights, pos.x, pos.y - texel.y))
                    / (2.0 * texel.y),
            );
            velocity = velocity * DROP_INERTIA - gradient * (1.0 - DROP_INERTIA);
            let speed = velocity.length();
            if speed < 1.0e-6 {
                break;
            }
            velocity /= speed;
            let next = pos + velocity * delta;
            if next.x < bounds_min.x
                || next.y < bounds_min.y
                || next.x >= bounds_max.x
                || next.y >= bounds_max.y
            {
                break;
            }
            let height = sample(heights, pos.x, pos.y);
            let ahead = sample(heights, next.x, next.y);
            let descent = (-gradient.dot(velocity)).max(0.0);
            let capacity = descent * water * DROP_CAPACITY_GAIN + DROP_MIN_CAPACITY;
            if sediment > capacity {
                let deposit = (sediment - capacity) * DROP_DEPOSIT;
                sediment -= deposit;
                splat(heights, resolution, bounds_min, texel, pos.x, pos.y, deposit);
            } else {
                let erode = ((capacity - sediment) * DROP_DISSOLVE)
                    .min((height - ahead).max(0.0) * 0.5)
                    .min(0.5);
                sediment += erode;
                splat(heights, resolution, bounds_min, texel, pos.x, pos.y, -erode);
            }
            pos = next;
            water *= DROP_EVAPORATION;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spike_grid(resolution: u32) -> Vec<f32> {
        let mut heights = vec![0.0f32; (resolution * resolution) as usize];
        let center = (resolution / 2) as usize;
        for iz in center - 2..center + 3 {
            for ix in center - 2..center + 3 {
                heights[iz * resolution as usize + ix] = 20.0;
            }
        }
        heights
    }

    fn max_slope(heights: &[f32], resolution: u32, texel: f32) -> f32 {
        let mut steepest = 0.0f32;
        for iz in 0..resolution {
            for ix in 0..resolution {
                let here = (iz * resolution + ix) as usize;
                if ix + 1 < resolution {
                    let right = (iz * resolution + ix + 1) as usize;
                    steepest = steepest.max((heights[here] - heights[right]).abs() / texel);
                }
                if iz + 1 < resolution {
                    let below = ((iz + 1) * resolution + ix) as usize;
                    steepest = steepest.max((heights[here] - heights[below]).abs() / texel);
                }
            }
        }
        steepest
    }

    #[test]
    fn thermal_erosion_relaxes_steep_slopes_and_conserves_material() {
        let resolution = 32u32;
        let texel = Vec2::splat(1.0);
        let config = ErosionConfig {
            enabled: true,
            thermal_iterations: 8,
            hydraulic_drops: 0,
            talus_angle: 0.2,
            ..default()
        };
        let before = spike_grid(resolution);
        let mut after = before.clone();
        thermal(&config, &mut after, UVec2::splat(resolution), texel);
        assert!(
            max_slope(&after, resolution, 1.0) < max_slope(&before, resolution, 1.0),
            "thermal erosion must relax the spike"
        );
        assert!(after.iter().all(|height| height.is_finite()));
        let sum_before: f32 = before.iter().sum();
        let sum_after: f32 = after.iter().sum();
        assert!((sum_before - sum_after).abs() < 1.0e-3, "thermal erosion only moves material");
    }

    #[test]
    fn hydraulic_erosion_is_deterministic_and_stays_finite() {
        let resolution = 64u32;
        let texel = Vec2::splat(1.0);
        let bounds_min = -Vec2::splat(resolution as f32) * 0.5;
        let config = ErosionConfig {
            enabled: true,
            seed: 9,
            thermal_iterations: 0,
            hydraulic_drops: 300,
            talus_angle: 0.6,
        };
        let mut first = spike_grid(resolution);
        let mut second = spike_grid(resolution);
        hydraulic(&config, &mut first, UVec2::splat(resolution), bounds_min, texel);
        hydraulic(&config, &mut second, UVec2::splat(resolution), bounds_min, texel);
        assert_eq!(first, second, "the same seed must carve the same gullies");
        assert!(first.iter().all(|height| height.is_finite()));
        assert!(
            first.iter().all(|height| *height > -50.0 && *height < 50.0),
            "erosion must not excavate canyons"
        );
        let mut other_seed = spike_grid(resolution);
        hydraulic(
            &ErosionConfig {
                seed: 10,
                ..config
            },
            &mut other_seed,
            UVec2::splat(resolution),
            bounds_min,
            texel,
        );
        assert_ne!(first, other_seed, "a different seed must carve differently");
    }
}
