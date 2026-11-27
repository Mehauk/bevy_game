#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoamSettings {
    pub foam_fade_rate: f32,
    pub wave_foam_strength: f32,
    pub wave_foam_coverage: f32,
    pub shoreline_foam_max_depth: f32,
    pub shoreline_foam_strength: f32,
    pub dynwaves_foam_strength: f32,
}

impl Default for FoamSettings {
    fn default() -> Self {
        Self {
            foam_fade_rate: 0.8,
            wave_foam_strength: 1.0,
            wave_foam_coverage: 0.55,
            shoreline_foam_max_depth: 0.65,
            shoreline_foam_strength: 2.0,
            dynwaves_foam_strength: 0.0,
        }
    }
}

fn saturate(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub fn foam_source(
    jacobian: &[f32],
    sea_floor_depth: &[f32],
    dynwaves: &[f32],
    size: usize,
    settings: &FoamSettings,
) -> Vec<f32> {
    let count = size * size;
    let mut source = vec![0.0f32; count];
    for i in 0..count {
        let det = jacobian[i];
        let mut value =
            5.0 * settings.wave_foam_strength * saturate(settings.wave_foam_coverage - det);
        let depth = sea_floor_depth[i];
        value += settings.shoreline_foam_strength
            * saturate(1.0 - depth / settings.shoreline_foam_max_depth);
        value += settings.dynwaves_foam_strength * saturate(dynwaves[i]);
        source[i] = value;
    }
    source
}

pub fn foam_decay_step(
    prev: &[f32],
    source: &[f32],
    dt: f32,
    settings: &FoamSettings,
) -> Vec<f32> {
    let fade = (1.0 - settings.foam_fade_rate * dt).max(0.0);
    prev.iter()
        .zip(source.iter())
        .map(|(p, s)| saturate(p * fade + s * dt))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_with_zero_source_fades_to_zero() {
        let settings = FoamSettings::default();
        let prev = vec![1.0; 16];
        let source = vec![0.0; 16];
        let mut current = prev;
        for _ in 0..200 {
            current = foam_decay_step(&current, &source, 0.1, &settings);
        }
        assert!(current.iter().all(|v| *v < 1e-3));
    }

    #[test]
    fn decay_is_monotonic() {
        let settings = FoamSettings::default();
        let source = vec![0.0; 4];
        let mut current = vec![1.0; 4];
        let mut last = 1.0;
        for _ in 0..50 {
            current = foam_decay_step(&current, &source, 0.05, &settings);
            assert!(current[0] <= last + 1e-6);
            last = current[0];
        }
    }

    #[test]
    fn source_fires_where_jacobian_below_threshold() {
        let settings = FoamSettings::default();
        let size = 2;
        let jacobian = vec![0.0, 1.0, 1.0, 1.0];
        let depth = vec![100.0; 4];
        let dynwaves = vec![0.0; 4];
        let source = foam_source(&jacobian, &depth, &dynwaves, size, &settings);
        assert!(source[0] > 0.0);
        assert!(source[1..].iter().all(|v| *v == 0.0));
    }

    #[test]
    fn source_fires_near_shoreline() {
        let settings = FoamSettings::default();
        let size = 2;
        let jacobian = vec![1.0; 4];
        let depth = vec![0.2, 5.0, 5.0, 5.0];
        let dynwaves = vec![0.0; 4];
        let source = foam_source(&jacobian, &depth, &dynwaves, size, &settings);
        assert!(source[0] > 0.0);
        assert!(source[1..].iter().all(|v| *v == 0.0));
    }

    #[test]
    fn source_includes_dynwaves() {
        let settings = FoamSettings {
            dynwaves_foam_strength: 3.0,
            ..FoamSettings::default()
        };
        let size = 2;
        let source = foam_source(&[1.0; 4], &[50.0; 4], &[0.5, 0.0, 0.0, 0.0], size, &settings);
        assert!((source[0] - 3.0 * 0.5).abs() < 1e-6);
        assert_eq!(source[1], 0.0);
    }
}
