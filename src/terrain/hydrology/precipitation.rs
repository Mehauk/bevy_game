use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;
use crate::terrain::params::Precipitation;
use crate::terrain::voronoi::VertexBlendCache;

const DEFAULT_CELL_PRECIPITATION: f32 = 0.4;
const OROGRAPHIC_WIND_GAIN: f32 = 1.2;
const MAX_WINDWARD_RAIN_BONUS: f32 = 0.4;
const MAX_LEE_RAIN_PENALTY: f32 = 0.3;
const RAIN_SHADOW_SUPPRESSION: f32 = 0.7;
const MIN_PRECIPITATION: f32 = 0.005;
const MAX_PRECIPITATION: f32 = 1.5;

pub fn precipitation_intensity(precipitation: Precipitation) -> f32 {
    match precipitation {
        Precipitation::Torrential => 1.0,
        Precipitation::Heavy => 0.75,
        Precipitation::Moderate => 0.45,
        Precipitation::Low => 0.18,
        Precipitation::None => 0.03,
    }
}

pub struct PrecipitationInput<'a> {
    pub cell_of_vertex: &'a [usize],
    pub cell_precipitation: &'a [Precipitation],
    pub cell_rain_shadow: &'a [f32],
    pub wind: Vec2,
}

pub fn generate_precipitation_blended(
    field: &ScalarField,
    input: &PrecipitationInput,
    cache: Option<&VertexBlendCache>,
) -> ScalarField {
    let mut out = ScalarField::new(field.dim, field.size, field.topology);
    let wind = input.wind.normalize_or_zero();

    for z in 0..field.dim {
        for x in 0..field.dim {
            let index = field.index(x, z);
            let blended = cache
                .map(|cache| cache.weights_at(index))
                .filter(|weights| !weights.is_empty());
            let (base, shadow) = match blended {
                Some(weights) => {
                    let mut weighted_intensity = 0.0f32;
                    let mut weighted_shadow = 0.0f32;
                    let mut weight_total = 0.0f32;
                    for &(cell, weight) in weights {
                        weighted_intensity += cell_base(input, cell) * weight;
                        weighted_shadow += cell_shadow(input, cell) * weight;
                        weight_total += weight;
                    }
                    if weight_total > 0.0 {
                        (
                            weighted_intensity / weight_total,
                            weighted_shadow / weight_total,
                        )
                    } else if let Some(&cell) = input.cell_of_vertex.get(index) {
                        (cell_base(input, cell), cell_shadow(input, cell))
                    } else {
                        continue;
                    }
                }
                None => {
                    let Some(&cell) = input.cell_of_vertex.get(index) else {
                        continue;
                    };
                    (cell_base(input, cell), cell_shadow(input, cell))
                }
            };

            let slope = field.gradient(x, z);
            let windward = slope.dot(wind);
            let orographic = (windward * OROGRAPHIC_WIND_GAIN)
                .clamp(-MAX_LEE_RAIN_PENALTY, MAX_WINDWARD_RAIN_BONUS);
            let rainfall = (base * (1.0 - shadow * RAIN_SHADOW_SUPPRESSION) + orographic)
                .clamp(MIN_PRECIPITATION, MAX_PRECIPITATION);
            out.set(x, z, rainfall);
        }
    }
    out
}

fn cell_base(input: &PrecipitationInput, cell: usize) -> f32 {
    input
        .cell_precipitation
        .get(cell)
        .copied()
        .map(precipitation_intensity)
        .unwrap_or(DEFAULT_CELL_PRECIPITATION)
}

fn cell_shadow(input: &PrecipitationInput, cell: usize) -> f32 {
    input
        .cell_rain_shadow
        .get(cell)
        .copied()
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terrain::hydrology::test_support::plane;
    use crate::terrain::seed::WorldSeed;
    use crate::terrain::topology::WorldTopology;

    #[test]
    fn precipitation_inputs_stay_positive() {
        let field = plane(12, 120.0, WorldTopology::Finite, 0.5);
        let cells = vec![0usize; field.len()];
        let cell_precip = vec![Precipitation::Heavy; 1];
        let input = PrecipitationInput {
            cell_of_vertex: &cells,
            cell_precipitation: &cell_precip,
            cell_rain_shadow: &[0.0],
            wind: Vec2::X,
        };
        let rain = generate_precipitation_blended(&field, &input, None);
        assert!(rain.values.iter().all(|v| *v > 0.0));
        assert!(rain.values.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn rain_shadow_suppresses_precipitation() {
        let field = plane(12, 120.0, WorldTopology::Finite, 0.0);
        let cells = vec![0usize; field.len()];
        let cell_precip = vec![Precipitation::Heavy; 1];
        let build = |shadow: f32| {
            generate_precipitation_blended(
                &field,
                &PrecipitationInput {
                    cell_of_vertex: &cells,
                    cell_precipitation: &cell_precip,
                    cell_rain_shadow: &[shadow],
                    wind: Vec2::X,
                },
                None,
            )
            .mean()
        };
        assert!(build(0.9) < build(0.0), "rain shadow must reduce rainfall");
    }

    #[test]
    fn precipitation_blends_across_voronoi_cell_weights() {
        use crate::terrain::voronoi::{VoronoiConfig, VoronoiLayout};
        let topology = WorldTopology::Finite;
        let size = 200.0;
        let dim = 40usize;
        let layout = VoronoiLayout::generate(
            WorldSeed::new(3),
            VoronoiConfig {
                size,
                grid_dim: 6,
                topology,
                default_blend_width: 16.0,
                blend_min: 16.0,
                blend_max: 16.0,
                ..Default::default()
            },
        );
        let cache = layout.vertex_cache(dim, size, topology);
        let cell_of_vertex: Vec<usize> = (0..dim * dim)
            .map(|i| cache.cell_at(i).unwrap_or(0))
            .collect();
        let cell_count = layout.cells().len();
        let cell_precipitation: Vec<Precipitation> = (0..cell_count)
            .map(|i| {
                if i % 2 == 0 {
                    Precipitation::Torrential
                } else {
                    Precipitation::Low
                }
            })
            .collect();
        let cell_rain_shadow = vec![0.0f32; cell_count];
        let field = ScalarField::new(dim, size, topology);
        let input = PrecipitationInput {
            cell_of_vertex: &cell_of_vertex,
            cell_precipitation: &cell_precipitation,
            cell_rain_shadow: &cell_rain_shadow,
            wind: Vec2::ZERO,
        };
        let hard = generate_precipitation_blended(&field, &input, None);
        let blended = generate_precipitation_blended(&field, &input, Some(&cache));

        let max_step = |f: &ScalarField| -> f32 {
            let mut m = 0.0f32;
            for z in 0..dim {
                for x in 0..dim {
                    if x + 1 < dim {
                        m = m.max((f.get(x + 1, z) - f.get(x, z)).abs());
                    }
                    if z + 1 < dim {
                        m = m.max((f.get(x, z + 1) - f.get(x, z)).abs());
                    }
                }
            }
            m
        };
        let hard_step = max_step(&hard);
        let blended_step = max_step(&blended);
        assert!(
            hard_step > 0.5,
            "nearest-cell lookup must show sharp polygon steps: {hard_step}"
        );
        assert!(
            blended_step <= hard_step * 0.5,
            "blended precipitation must not step as hard as the nearest-cell field: {blended_step} vs {hard_step}"
        );

        let mut largest_cell = 0usize;
        let mut best = 0usize;
        for cell in 0..cell_count {
            let n = cell_of_vertex.iter().filter(|&&c| c == cell).count();
            if n > best {
                best = n;
                largest_cell = cell;
            }
        }
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for (i, &c) in cell_of_vertex.iter().enumerate() {
            if c == largest_cell {
                lo = lo.min(blended.get_at(i));
                hi = hi.max(blended.get_at(i));
            }
        }
        assert!(best > 4, "cell too small to test: {best} vertices");
        assert!(
            hi - lo > 1e-3,
            "blended precipitation is constant across cell {largest_cell}: {lo}..{hi}"
        );
    }
}
