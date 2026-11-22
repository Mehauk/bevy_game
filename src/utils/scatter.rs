use std::collections::BTreeMap;

use bevy::prelude::*;

fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    x
}

pub fn hash_unit(seed: u64, x: i64, y: i64) -> f32 {
    let h = mix64(
        seed ^ (x as u64)
            .wrapping_mul(0x252F6E384F34BC70)
            ^ (y as u64).rotate_left(32),
    );
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub fn value_noise(seed: u64, x: f32, z: f32, scale: f32) -> f32 {
    let scale = scale.max(1e-3);
    let gx = x / scale;
    let gz = z / scale;
    let x0 = gx.floor() as i64;
    let z0 = gz.floor() as i64;
    let fx = smoothstep(gx - x0 as f32);
    let fz = smoothstep(gz - z0 as f32);
    let v00 = hash_unit(seed, x0, z0);
    let v10 = hash_unit(seed, x0 + 1, z0);
    let v01 = hash_unit(seed, x0, z0 + 1);
    let v11 = hash_unit(seed, x0 + 1, z0 + 1);
    let a = v00 + (v10 - v00) * fx;
    let b = v01 + (v11 - v01) * fx;
    a + (b - a) * fz
}

pub fn fbm(seed: u64, x: f32, z: f32, scale: f32, octaves: usize) -> f32 {
    let mut sum = 0.0;
    let mut weight = 1.0;
    let mut total = 0.0;
    let mut octave_scale = scale;
    for octave in 0..octaves.max(1) {
        sum += weight * value_noise(seed ^ mix64(octave as u64 + 1), x, z, octave_scale);
        total += weight;
        weight *= 0.5;
        octave_scale *= 0.5;
    }
    sum / total
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterClass {
    pub min_spacing: f32,
    pub max_count: usize,
}

pub fn poisson_scatter(
    seed: u64,
    origin: Vec2,
    extent: Vec2,
    classes: &[ScatterClass],
    density: &dyn Fn(usize, Vec2) -> f32,
) -> Vec<(usize, Vec2)> {
    if classes.is_empty() || extent.x <= 0.0 || extent.y <= 0.0 {
        return Vec::new();
    }
    let cell = classes
        .iter()
        .map(|class| class.min_spacing)
        .fold(f32::INFINITY, f32::min)
        .max(1e-3);
    let reach = classes
        .iter()
        .map(|class| (class.min_spacing / cell).ceil() as i32)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut placed: Vec<(usize, Vec2)> = Vec::new();
    let mut grid: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
    for (class_index, class) in classes.iter().enumerate() {
        let attempts = class.max_count.saturating_mul(32).max(1);
        let mut placed_for_class = 0usize;
        for attempt in 0..attempts {
            if placed_for_class >= class.max_count {
                break;
            }
            let hx = hash_unit(seed ^ 0x51, class_index as i64, attempt as i64);
            let hz = hash_unit(seed ^ 0xA7, class_index as i64, attempt as i64);
            let point = origin + Vec2::new(hx, hz) * extent;
            let chance = density(class_index, point);
            if chance <= 0.0 {
                continue;
            }
            if chance < 1.0
                && hash_unit(seed ^ 0x3D, class_index as i64, attempt as i64) > chance
            {
                continue;
            }
            let cell_coords = (
                ((point.x - origin.x) / cell).floor() as i32,
                ((point.y - origin.y) / cell).floor() as i32,
            );
            let mut clear = true;
            'neighbours: for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let Some(indices) = grid.get(&(cell_coords.0 + dx, cell_coords.1 + dz)) else {
                        continue;
                    };
                    for &index in indices {
                        let (other_class, other) = placed[index];
                        let min_dist =
                            class.min_spacing.max(classes[other_class].min_spacing);
                        if point.distance_squared(other) < min_dist * min_dist {
                            clear = false;
                            break 'neighbours;
                        }
                    }
                }
            }
            if !clear {
                continue;
            }
            grid.entry(cell_coords).or_default().push(placed.len());
            placed.push((class_index, point));
            placed_for_class += 1;
        }
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_unit_is_deterministic_and_in_range() {
        for x in -4..4 {
            for y in -4..4 {
                let value = hash_unit(7, x, y);
                assert!((0.0..1.0).contains(&value), "hash out of [0,1): {value}");
                assert_eq!(value, hash_unit(7, x, y));
            }
        }
        assert_ne!(hash_unit(7, 1, 2), hash_unit(8, 1, 2), "seed must matter");
    }

    #[test]
    fn value_noise_is_smooth_bounded_and_deterministic() {
        let scale = 16.0;
        let mut previous = value_noise(3, 0.0, 0.0, scale);
        let mut varied = false;
        for i in 0..64 {
            let x = i as f32 * 0.25;
            let value = value_noise(3, x, 1.0, scale);
            assert!((0.0..=1.0).contains(&value), "noise out of [0,1]: {value}");
            assert_eq!(value, value_noise(3, x, 1.0, scale));
            assert!(
                (value - previous).abs() < 0.2,
                "noise must vary smoothly, jumped {previous} -> {value}"
            );
            if (value - previous).abs() > 0.001 {
                varied = true;
            }
            previous = value;
        }
        assert!(varied, "noise must not be constant along a line");
    }

    #[test]
    fn fbm_is_bounded_and_deterministic() {
        for i in 0..32 {
            let x = i as f32 * 1.7;
            let value = fbm(11, x, -3.0, 12.0, 4);
            assert!((0.0..=1.0).contains(&value), "fbm out of [0,1]: {value}");
            assert_eq!(value, fbm(11, x, -3.0, 12.0, 4));
        }
    }

    #[test]
    fn poisson_scatter_is_deterministic() {
        let classes = [
            ScatterClass {
                min_spacing: 3.0,
                max_count: 20,
            },
            ScatterClass {
                min_spacing: 1.0,
                max_count: 40,
            },
        ];
        let open = |_class: usize, _point: Vec2| 1.0f32;
        let first = poisson_scatter(5, Vec2::ZERO, Vec2::splat(32.0), &classes, &open);
        let second = poisson_scatter(5, Vec2::ZERO, Vec2::splat(32.0), &classes, &open);
        assert_eq!(first, second);
        assert!(!first.is_empty(), "an open field must scatter something");
    }

    #[test]
    fn poisson_scatter_respects_per_class_spacing() {
        let classes = [
            ScatterClass {
                min_spacing: 4.0,
                max_count: 16,
            },
            ScatterClass {
                min_spacing: 1.5,
                max_count: 60,
            },
        ];
        let open = |_class: usize, _point: Vec2| 1.0f32;
        let points = poisson_scatter(9, Vec2::ZERO, Vec2::splat(40.0), &classes, &open);
        assert!(points.len() > 10, "expected a decent scatter, got {}", points.len());
        for (i, (class_a, a)) in points.iter().enumerate() {
            for (class_b, b) in points.iter().skip(i + 1) {
                let min = classes[*class_a].min_spacing.max(classes[*class_b].min_spacing);
                assert!(
                    a.distance(*b) >= min - 1e-3,
                    "class {class_a}@{a} and class {class_b}@{b} overlap (min {min})"
                );
            }
        }
    }

    #[test]
    fn poisson_scatter_honours_counts_and_closed_gates() {
        let classes = [ScatterClass {
            min_spacing: 1.0,
            max_count: 12,
        }];
        let closed = |_class: usize, _point: Vec2| 0.0f32;
        assert!(
            poisson_scatter(2, Vec2::ZERO, Vec2::splat(20.0), &classes, &closed).is_empty(),
            "a closed gate must scatter nothing"
        );
        let open = |_class: usize, _point: Vec2| 1.0f32;
        let points = poisson_scatter(2, Vec2::ZERO, Vec2::splat(20.0), &classes, &open);
        assert!(!points.is_empty() && points.len() <= 12);
    }

    #[test]
    fn poisson_scatter_clumps_to_the_open_half_of_a_gate() {
        let classes = [ScatterClass {
            min_spacing: 1.0,
            max_count: 30,
        }];
        let half_open = |_class: usize, point: Vec2| if point.x < 0.0 { 1.0 } else { 0.0 };
        let points = poisson_scatter(4, Vec2::splat(-16.0), Vec2::splat(32.0), &classes, &half_open);
        assert!(points.len() >= 5, "clump gate must still scatter, got {}", points.len());
        for (_class, point) in &points {
            assert!(point.x < 0.0, "point {point} landed in the closed half");
        }
    }
}
