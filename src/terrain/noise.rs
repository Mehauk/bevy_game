use bevy::prelude::Vec2;
use noise::{NoiseFn, Perlin};

use crate::terrain::params::{
    BaseHeightBand, CellParameters, Geothermal, Hydrology, ParamLevel, SlopeMagnitude, Substrate,
    Wind,
};
use crate::terrain::region::Region;
use crate::terrain::topology::WorldTopology;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoiseAlgorithm {
    Hilly,
    Ridged,
    Dune,
    Terraced,
    Billow,
    Cellular,
    Plateau,
}

impl NoiseAlgorithm {
    #[allow(dead_code)]
    pub fn all() -> [NoiseAlgorithm; 7] {
        [
            NoiseAlgorithm::Hilly,
            NoiseAlgorithm::Ridged,
            NoiseAlgorithm::Dune,
            NoiseAlgorithm::Terraced,
            NoiseAlgorithm::Billow,
            NoiseAlgorithm::Cellular,
            NoiseAlgorithm::Plateau,
        ]
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            NoiseAlgorithm::Hilly => "hilly",
            NoiseAlgorithm::Ridged => "ridged",
            NoiseAlgorithm::Dune => "dune",
            NoiseAlgorithm::Terraced => "terraced",
            NoiseAlgorithm::Billow => "billow",
            NoiseAlgorithm::Cellular => "cellular",
            NoiseAlgorithm::Plateau => "plateau",
        }
    }

    #[allow(dead_code)]
    pub fn parse(label: &str) -> Option<NoiseAlgorithm> {
        NoiseAlgorithm::all().into_iter().find(|a| a.label() == label)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NoiseContext {
    pub topology: WorldTopology,
    pub size: f32,
    pub wind: Vec2,
}

impl Default for NoiseContext {
    fn default() -> Self {
        Self {
            topology: WorldTopology::Finite,
            size: 400.0,
            wind: Vec2::X,
        }
    }
}

pub fn select_algorithm(params: &CellParameters, sub_variant: &str) -> NoiseAlgorithm {
    if sub_variant.contains("salt") {
        return NoiseAlgorithm::Cellular;
    }
    if params.geothermal == Geothermal::ActiveVolcanic {
        return NoiseAlgorithm::Cellular;
    }
    if params.base_height.index() >= BaseHeightBand::Alpine.index() {
        return NoiseAlgorithm::Ridged;
    }
    if params.substrate == Substrate::Volcanic {
        return NoiseAlgorithm::Cellular;
    }
    if params.substrate == Substrate::RockCrystalline {
        return NoiseAlgorithm::Plateau;
    }
    if params.substrate == Substrate::LimestoneKarst {
        return NoiseAlgorithm::Terraced;
    }
    if params.substrate == Substrate::Sand
        && params.is_arid()
        && params.base_gradient.slope.index() <= SlopeMagnitude::Gentle.index()
    {
        return NoiseAlgorithm::Dune;
    }
    if params.base_height.index() >= BaseHeightBand::Highland.index() && params.is_arid() {
        return NoiseAlgorithm::Terraced;
    }
    if matches!(
        params.hydrology,
        Hydrology::PoorlyDrained | Hydrology::FloodedBasin
    ) && params.base_height.index() <= BaseHeightBand::Lowland.index()
    {
        return NoiseAlgorithm::Billow;
    }
    NoiseAlgorithm::Hilly
}

#[derive(Clone, Copy, Debug)]
pub enum SamplePoint {
    Plane([f64; 2]),
    Torus([f64; 4]),
}

trait FieldSource {
    fn eval(&self, point: SamplePoint) -> f64;
}

impl FieldSource for Perlin {
    fn eval(&self, point: SamplePoint) -> f64 {
        match point {
            SamplePoint::Plane(point) => self.get(point),
            SamplePoint::Torus(point) => self.get(point),
        }
    }
}

fn scale_point(point: SamplePoint, factor: f64) -> SamplePoint {
    match point {
        SamplePoint::Plane([x, z]) => SamplePoint::Plane([x * factor, z * factor]),
        SamplePoint::Torus([a, b, c, d]) => {
            SamplePoint::Torus([a * factor, b * factor, c * factor, d * factor])
        }
    }
}

const TERRACE_STEPS: f64 = 7.0;
const TERRACE_RISER: f64 = 0.28;
const DUNE_ELONGATION: f64 = 5.0;
const DUNE_WAVE_FREQUENCY: f64 = 0.03;
const DUNE_SKEW: f64 = 0.35;
const LACUNARITY: f64 = 2.0;
const RIDGE_ATTENUATION: f64 = 1.0;
const CRACK_WIDTH: f64 = 0.1;
const MAX_OCTAVES: usize = 6;

const HILLY_FREQUENCY: f64 = 0.009;
const SWELL_FREQUENCY: f64 = 0.0032;
const RIDGED_FREQUENCY: f64 = 0.0075;
const BILOW_FREQUENCY: f64 = 0.011;
const TERRACE_FREQUENCY: f64 = 0.006;
const DUNE_FREQUENCY: f64 = 0.05;
const CELLULAR_FREQUENCY: f64 = 0.02;
const PLATEAU_FREQUENCY: f64 = 0.012;
const PLATEAU_STEPS: f64 = 5.0;
const PLATEAU_RISER: f64 = 0.03;
const PLATEAU_EXTRA_WARP: f64 = 9.0;
const PLATEAU_MICRO_FREQUENCY: f64 = 0.4;
const WARP_FREQUENCY: f64 = 0.004;
const TORUS_WARP_FIELD_FREQUENCY: f64 = 0.45;
const TORUS_WARP_DECORRELATION: f64 = 3.7;
const WARP_FIELD_DECORRELATION_X: f64 = 11.3;
const WARP_FIELD_DECORRELATION_Z: f64 = 7.7;

const MIN_OCTAVES: usize = 3;
const MAX_CRACK_OCTAVES: f64 = 3.0;
const CRACK_PERSISTENCE: f64 = 0.3;
const CRACK_LACUNARITY: f64 = 2.3;
const HILLY_SWELL_MIX: f64 = 0.35;
const TERRACE_STRATA_FREQUENCY_RATIO: f64 = 0.45;
const TERRACE_STRATA_MIX: f64 = 0.6;
const TERRACE_MICRO_FREQUENCY: f64 = 0.09;
const TERRACE_MICRO_MIX: f64 = 0.07;
const PLATEAU_MICRO_MIX: f64 = 0.12;
const DUNE_WIGGLE_CROSS_FREQUENCY: f64 = 0.02;
const DUNE_WIGGLE_ALONG_FREQUENCY: f64 = 0.007;
const DUNE_WIGGLE_PHASE_GAIN: f64 = 0.8;
const DUNE_RIDGE_MIX: f64 = 0.6;
const DUNE_ASYMMETRY_MIX: f64 = 0.4;
const DUNE_HARMONIC_RADIUS: i32 = 3;
const DUNE_ALIGNMENT_EXPONENT: u32 = 8;
const SLOPE_AMPLITUDE_GAIN_PER_STEP: f32 = 0.12;
const WIND_WARP_GAIN_PER_STEP: f32 = 0.8;
const GLACIER_AMPLITUDE_MULTIPLIER: f32 = 1.25;

#[derive(Clone, Copy, Debug)]
pub struct NoiseShape {
    pub amplitude: f32,
    pub frequency: f32,
    pub octaves: usize,
    pub octave_budget: f32,
    pub warp: f32,
}

impl Default for NoiseShape {
    fn default() -> Self {
        Self {
            amplitude: 8.0,
            frequency: 1.0,
            octaves: 4,
            octave_budget: 4.0,
            warp: 6.0,
        }
    }
}

pub fn shape_for(params: &CellParameters, region: Region, sub_variant: &str) -> NoiseShape {
    let mut shape = match region.definition().tier {
        crate::terrain::region::RegionTier::Submerged => NoiseShape {
            amplitude: 1.2,
            frequency: 0.7,
            octaves: 3,
            octave_budget: 3.0,
            warp: 2.0,
        },
        crate::terrain::region::RegionTier::Coastal => NoiseShape {
            amplitude: 1.5,
            frequency: 1.7,
            octaves: 3,
            octave_budget: 3.0,
            warp: 3.0,
        },
        crate::terrain::region::RegionTier::Lowland => NoiseShape {
            amplitude: 6.0,
            frequency: 1.0,
            octaves: 4,
            octave_budget: 4.0,
            warp: 6.0,
        },
        crate::terrain::region::RegionTier::Highland => NoiseShape {
            amplitude: 14.0,
            frequency: 1.0,
            octaves: 5,
            octave_budget: 5.0,
            warp: 7.0,
        },
        crate::terrain::region::RegionTier::Alpine => NoiseShape {
            amplitude: 35.0,
            frequency: 1.3,
            octaves: 6,
            octave_budget: 6.0,
            warp: 9.0,
        },
    };
    if params.base_height.index() <= BaseHeightBand::DeepOcean.index() {
        shape.amplitude = 0.8;
        shape.frequency = 0.6;
        shape.octaves = 3;
        shape.warp = 1.5;
    }
    if matches!(
        region,
        Region::TidalFlat
            | Region::SandyBeachDunes
            | Region::MangroveSwamp
            | Region::RiverDelta
            | Region::SeagrassMeadow
    ) {
        shape.amplitude = shape.amplitude.min(1.5);
        shape.frequency = shape.frequency.max(1.8);
    }
    shape.amplitude *=
        1.0 + SLOPE_AMPLITUDE_GAIN_PER_STEP * params.base_gradient.slope.index() as f32;
    shape.warp += WIND_WARP_GAIN_PER_STEP * (Wind::COUNT - 1 - params.wind.index()) as f32;
    if sub_variant.contains("dune") {
        shape.frequency = shape.frequency.max(2.2);
        shape.warp = shape.warp.min(3.0);
    }
    if sub_variant.contains("salt") {
        shape.amplitude = shape.amplitude.min(1.0);
    }
    if sub_variant.contains("glacier") || sub_variant.contains("ice") {
        shape.amplitude *= GLACIER_AMPLITUDE_MULTIPLIER;
        shape.octaves = shape.octaves.max(5);
    }
    if sub_variant.contains("reef") {
        shape.amplitude = shape.amplitude.min(1.2);
        shape.frequency = shape.frequency.max(2.4);
    }
    shape.octave_budget = shape.octaves as f32;
    shape
}

pub fn blend_shapes(items: &[(NoiseShape, f32)]) -> NoiseShape {
    let mut amplitude = 0.0f32;
    let mut frequency = 0.0f32;
    let mut octaves = 0.0f32;
    let mut warp = 0.0f32;
    let mut total = 0.0f32;
    for (shape, weight) in items {
        amplitude += shape.amplitude * weight;
        frequency += shape.frequency * weight;
        octaves += shape.octaves as f32 * weight;
        warp += shape.warp * weight;
        total += weight;
    }
    if total <= f32::EPSILON {
        return NoiseShape::default();
    }
    let budget = (octaves / total).clamp(MIN_OCTAVES as f32, MAX_OCTAVES as f32);
    NoiseShape {
        amplitude: amplitude / total,
        frequency: frequency / total,
        octaves: (budget.floor() as usize).clamp(MIN_OCTAVES, MAX_OCTAVES),
        octave_budget: budget,
        warp: warp / total,
    }
}

fn octave_gain(frac: f64) -> f64 {
    let t = frac.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn octave_sum(
    budget: f64,
    max_octaves: usize,
    persistence: f64,
    lacunarity: f64,
    frequency: f64,
    mut octave: impl FnMut(usize, f64) -> (f64, f64),
) -> (f64, f64) {
    let budget = budget.clamp(1.0, max_octaves as f64);
    let full = budget.floor() as usize;
    let frac = budget - budget.floor();
    let count = if frac > 0.0 { full + 1 } else { full };
    let mut sum = 0.0;
    let mut norm = 0.0;
    let mut amplitude = 1.0;
    let mut scale = frequency;
    for index in 0..count {
        let gain = if frac > 0.0 && index + 1 == count {
            octave_gain(frac)
        } else {
            1.0
        };
        let (signal, weight) = octave(index, scale);
        sum += signal * amplitude * weight * gain;
        norm += amplitude * weight * gain;
        amplitude *= persistence;
        scale *= lacunarity;
    }
    (sum, norm)
}

pub struct NoiseEngine {
    seed: u32,
    hilly: Vec<Perlin>,
    swell: Vec<Perlin>,
    ridged: Vec<Perlin>,
    billow: Vec<Perlin>,
    terrace: Vec<Perlin>,
    strata: Vec<Perlin>,
    cliff: Vec<Perlin>,
    dune: Vec<Perlin>,
    warp: Vec<Perlin>,
}

fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    x
}

fn sources(seed: u32, count: usize) -> Vec<Perlin> {
    (0..count)
        .map(|index| Perlin::new(seed ^ mix64(index as u64 + 1) as u32))
        .collect()
}

impl NoiseEngine {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            hilly: sources(seed, MAX_OCTAVES),
            swell: sources(seed ^ 0x5bf0_3635, 2),
            ridged: sources(seed ^ 0x9e37_79b9, MAX_OCTAVES),
            billow: sources(seed ^ 0x2545_f491, MAX_OCTAVES),
            terrace: sources(seed ^ 0x68e3_1da4, 4),
            strata: sources(seed ^ 0x2722_0a95, 2),
            cliff: sources(seed ^ 0x7a57_e471, 2),
            dune: sources(seed ^ 0x1b87_3593, 4),
            warp: sources(seed ^ 0x1656_67b1, 2),
        }
    }

    fn project(&self, x: f32, z: f32, ctx: &NoiseContext) -> SamplePoint {
        if ctx.topology.wraps() && ctx.size > 0.0 {
            let tau = std::f64::consts::TAU;
            let radius = ctx.size as f64 / tau;
            let u = (x / ctx.size) as f64 * tau;
            let v = (z / ctx.size) as f64 * tau;
            SamplePoint::Torus([
                u.cos() * radius,
                u.sin() * radius,
                v.cos() * radius,
                v.sin() * radius,
            ])
        } else {
            SamplePoint::Plane([x as f64, z as f64])
        }
    }

    fn warped(&self, point: SamplePoint, amount: f64) -> SamplePoint {
        match point {
            SamplePoint::Plane([x, z]) => {
                let q = [x * WARP_FREQUENCY, z * WARP_FREQUENCY];
                let wx = self.warp[0].eval(SamplePoint::Plane(q));
                let wz = self.warp[1].eval(SamplePoint::Plane([
                    q[0] + WARP_FIELD_DECORRELATION_X * WARP_FREQUENCY,
                    q[1] - WARP_FIELD_DECORRELATION_Z * WARP_FREQUENCY,
                ]));
                SamplePoint::Plane([x + wx * amount, z + wz * amount])
            }
            SamplePoint::Torus([a, b, c, d]) => {
                let radius = a.hypot(b).max(1e-6);
                let u = b.atan2(a);
                let v = d.atan2(c);
                let q = [
                    u.cos() * TORUS_WARP_FIELD_FREQUENCY,
                    u.sin() * TORUS_WARP_FIELD_FREQUENCY,
                    v.cos() * TORUS_WARP_FIELD_FREQUENCY,
                    v.sin() * TORUS_WARP_FIELD_FREQUENCY,
                ];
                let wx = self.warp[0].eval(SamplePoint::Torus(q));
                let wz = self.warp[1].eval(SamplePoint::Torus([
                    q[0] + TORUS_WARP_DECORRELATION,
                    q[1] - TORUS_WARP_DECORRELATION * 0.7,
                    q[2] + TORUS_WARP_DECORRELATION * 0.5,
                    q[3] + TORUS_WARP_DECORRELATION * 1.3,
                ]));
                let up = u + wx * amount / radius;
                let vp = v + wz * amount / radius;
                SamplePoint::Torus([
                    up.cos() * radius,
                    up.sin() * radius,
                    vp.cos() * radius,
                    vp.sin() * radius,
                ])
            }
        }
    }

    fn fbm(&self, sources: &[Perlin], point: SamplePoint, frequency: f64, budget: f64, persistence: f64) -> f64 {
        let (sum, norm) = octave_sum(
            budget,
            sources.len(),
            persistence,
            LACUNARITY,
            frequency,
            |index, scale| (sources[index].eval(scale_point(point, scale)), 1.0),
        );
        sum / norm
    }

    fn billow_field(&self, sources: &[Perlin], point: SamplePoint, frequency: f64, budget: f64, persistence: f64) -> f64 {
        let (sum, norm) = octave_sum(
            budget,
            sources.len(),
            persistence,
            LACUNARITY,
            frequency,
            |index, scale| {
                (
                    2.0 * sources[index].eval(scale_point(point, scale)).abs() - 1.0,
                    1.0,
                )
            },
        );
        (sum / norm).clamp(-1.0, 1.0)
    }

    fn ridged_field(&self, point: SamplePoint, frequency: f64, budget: f64) -> f64 {
        let mut slope_weight = 1.0;
        let (sum, norm) = octave_sum(
            budget,
            self.ridged.len(),
            0.5,
            LACUNARITY,
            frequency,
            |index, scale| {
                let raw = self.ridged[index].eval(scale_point(point, scale));
                let ridge = 1.0 - raw.abs();
                let signal = ridge * ridge;
                let weight = slope_weight;
                slope_weight = (signal / RIDGE_ATTENUATION).clamp(0.0, 1.0);
                (signal, weight)
            },
        );
        let normalized = sum / norm.max(1e-9);
        (normalized * 2.0 - 1.0).clamp(-1.0, 1.0)
    }

    fn worley_f1_f2<const N: usize>(
        &self,
        point: &[f64; N],
        frequency: f64,
    ) -> (f64, f64, [i64; N], [i64; N]) {
        let p = point.map(|c| c * frequency);
        let cell = p.map(|c| c.floor() as i64);
        let mut f1 = f64::MAX;
        let mut f2 = f64::MAX;
        let mut first = [0i64; N];
        let mut second = [0i64; N];
        let corners = 3usize.pow(N as u32);
        for corner in 0..corners {
            let mut neighbor = [0i64; N];
            let mut rest = corner;
            for slot in neighbor.iter_mut() {
                *slot = rest as i64 % 3 - 1;
                rest /= 3;
            }
            for (slot, base) in neighbor.iter_mut().zip(cell.iter()) {
                *slot += *base;
            }
            let hash = neighbor.iter().fold(self.seed as u64, |acc, &c| {
                mix64(acc ^ (c as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
            });
            let mut distance_sq = 0.0;
            for (axis, (&center, &coord)) in neighbor.iter().zip(p.iter()).enumerate() {
                let jitter = mix64(hash ^ (axis as u64).wrapping_mul(0xff51_afd7_ed55_8ccd)) as f64
                    / u64::MAX as f64;
                let delta = (center as f64 + jitter) - coord;
                distance_sq += delta * delta;
            }
            if distance_sq < f1 {
                f2 = f1;
                second = first;
                f1 = distance_sq;
                first = neighbor;
            } else if distance_sq < f2 {
                f2 = distance_sq;
                second = neighbor;
            }
        }
        (f1.sqrt(), f2.sqrt(), first, second)
    }

    fn cell_level<const N: usize>(&self, cell: &[i64; N]) -> f64 {
        let hash = cell.iter().fold(self.seed as u64 ^ 0x5bf0_3c1d, |acc, &c| {
            mix64(acc ^ (c as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        });
        (hash % (PLATEAU_STEPS as u64 + 1)) as f64 / PLATEAU_STEPS
    }

    fn f2_minus_f1<const N: usize>(&self, point: &[f64; N], frequency: f64) -> f64 {
        let (f1, f2, _, _) = self.worley_f1_f2(point, frequency);
        f2 - f1
    }

    fn plateau_field(&self, point: SamplePoint, frequency: f64) -> f64 {
        let warped = self.warped(point, PLATEAU_EXTRA_WARP);
        let (f1, f2, top, neighbour) = match warped {
            SamplePoint::Plane(point) => {
                let (f1, f2, first, second) = self.worley_f1_f2(&point, frequency);
                (f1, f2, self.cell_level(&first), self.cell_level(&second))
            }
            SamplePoint::Torus(point) => {
                let (f1, f2, first, second) = self.worley_f1_f2(&point, frequency);
                (f1, f2, self.cell_level(&first), self.cell_level(&second))
            }
        };
        let u = ((f2 - f1) / PLATEAU_RISER).clamp(0.0, 1.0);
        let mesa = neighbour + (top - neighbour) * (u * u * (3.0 - 2.0 * u));
        let micro = self.fbm(&self.cliff, warped, PLATEAU_MICRO_FREQUENCY, 2.0, 0.5);
        ((mesa * 2.0 - 1.0) + micro * PLATEAU_MICRO_MIX * (1.0 - u)).clamp(-1.0, 1.0)
    }

    fn cracks(&self, point: SamplePoint, frequency: f64, budget: f64) -> f64 {
        let (sum, norm) = octave_sum(
            budget,
            MAX_CRACK_OCTAVES as usize,
            CRACK_PERSISTENCE,
            CRACK_LACUNARITY,
            frequency,
            |_index, scale| {
                let gap = match point {
                    SamplePoint::Plane(p) => self.f2_minus_f1(&p, scale),
                    SamplePoint::Torus(p) => self.f2_minus_f1(&p, scale),
                };
                (2.0 * (gap / CRACK_WIDTH).clamp(0.0, 1.0) - 1.0, 1.0)
            },
        );
        (sum / norm).clamp(-1.0, 1.0)
    }

    fn dune_plane(&self, x: f32, z: f32, ctx: &NoiseContext, freq: f64) -> f32 {
        let wind = ctx.wind.normalize_or_zero();
        let (wind_x, wind_z) = if wind.length_squared() < 1e-6 {
            (1.0f64, 0.0)
        } else {
            (wind.x as f64, wind.y as f64)
        };
        let xf = x as f64;
        let zf = z as f64;
        let along = xf * wind_x + zf * wind_z;
        let cross = -xf * wind_z + zf * wind_x;
        let ridge = self.billow_field(
            &self.dune,
            SamplePoint::Plane([
                along * DUNE_FREQUENCY * freq / DUNE_ELONGATION,
                cross * DUNE_FREQUENCY * freq,
            ]),
            1.0,
            3.0,
            0.5,
        );
        let wiggle = self.dune[3].eval(SamplePoint::Plane([
            cross * DUNE_WIGGLE_CROSS_FREQUENCY,
            along * DUNE_WIGGLE_ALONG_FREQUENCY,
        ]));
        let phase = along * DUNE_WAVE_FREQUENCY * freq + wiggle * DUNE_WIGGLE_PHASE_GAIN;
        let asym = stoss_slipface(phase) * 2.0 - 1.0;
        (DUNE_RIDGE_MIX * ridge + DUNE_ASYMMETRY_MIX * asym).clamp(-1.0, 1.0) as f32
    }

    fn dune_torus(&self, x: f32, z: f32, ctx: &NoiseContext) -> f32 {
        let tau = std::f64::consts::TAU;
        let k = tau / ctx.size as f64;
        let wind = ctx.wind.normalize_or_zero();
        let (wind_x, wind_z) = if wind.length_squared() < 1e-6 {
            (1.0f64, 0.0)
        } else {
            (wind.x as f64, wind.y as f64)
        };
        let xf = x as f64;
        let zf = z as f64;
        let mut sum = 0.0;
        let mut norm = 0.0;
        for nx in -DUNE_HARMONIC_RADIUS..=DUNE_HARMONIC_RADIUS {
            for nz in -DUNE_HARMONIC_RADIUS..=DUNE_HARMONIC_RADIUS {
                let length_sq = nx * nx + nz * nz;
                if length_sq == 0 || length_sq > DUNE_HARMONIC_RADIUS * DUNE_HARMONIC_RADIUS {
                    continue;
                }
                let length = (length_sq as f64).sqrt();
                let align = ((nx as f64 * wind_x + nz as f64 * wind_z) / length).abs();
                let weight = align.powi(DUNE_ALIGNMENT_EXPONENT as i32) / length;
                if weight < 1e-3 {
                    continue;
                }
                let phase = k * (nx as f64 * xf + nz as f64 * zf)
                    + wave_phase(self.seed, nx, nz);
                sum += weight * (phase.cos() - DUNE_SKEW * (2.0 * phase).sin());
                norm += weight * (1.0 + DUNE_SKEW);
            }
        }
        if norm < 1e-6 {
            0.0
        } else {
            (sum / norm).clamp(-1.0, 1.0) as f32
        }
    }

    #[allow(dead_code)]
    pub fn sample(&self, algorithm: NoiseAlgorithm, x: f32, z: f32, ctx: &NoiseContext) -> f32 {
        self.sample_with(algorithm, x, z, ctx, &NoiseShape::default())
    }

    pub fn sample_with(
        &self,
        algorithm: NoiseAlgorithm,
        x: f32,
        z: f32,
        ctx: &NoiseContext,
        shape: &NoiseShape,
    ) -> f32 {
        let octave_budget = shape
            .octave_budget
            .max(shape.octaves as f32)
            .clamp(MIN_OCTAVES as f32, MAX_OCTAVES as f32) as f64;
        let frequency = shape.frequency.max(0.05) as f64;
        let base = self.project(x, z, ctx);
        let point = self.warped(base, shape.warp as f64);
        match algorithm {
            NoiseAlgorithm::Hilly => {
                let broad = self.fbm(
                    &self.hilly,
                    point,
                    HILLY_FREQUENCY * frequency,
                    octave_budget,
                    0.42,
                );
                let swell = self.fbm(&self.swell, point, SWELL_FREQUENCY * frequency, 2.0, 0.5);
                (broad + swell * HILLY_SWELL_MIX).clamp(-1.0, 1.0) as f32
            }
            NoiseAlgorithm::Ridged => {
                self.ridged_field(point, RIDGED_FREQUENCY * frequency, octave_budget + 1.0) as f32
            }
            NoiseAlgorithm::Billow => {
                self.billow_field(&self.billow, point, BILOW_FREQUENCY * frequency, octave_budget, 0.5)
                    as f32
            }
            NoiseAlgorithm::Terraced => {
                let raw = (self.fbm(
                    &self.terrace,
                    point,
                    TERRACE_FREQUENCY * frequency,
                    octave_budget.min(4.0),
                    0.5,
                ) + 1.0)
                    * 0.5;
                let strata = self.fbm(
                    &self.strata,
                    point,
                    TERRACE_FREQUENCY * frequency * TERRACE_STRATA_FREQUENCY_RATIO,
                    2.0,
                    0.5,
                );
                let scaled = raw * TERRACE_STEPS + strata * TERRACE_STRATA_MIX;
                let step_index = scaled.floor();
                let frac = scaled - step_index;
                let riser_shape = riser_profile(frac, TERRACE_RISER);
                let scarp =
                    1.0 - ((frac - TERRACE_RISER) / (1.0 - TERRACE_RISER)).clamp(0.0, 1.0);
                let micro = self.fbm(
                    &self.cliff,
                    point,
                    TERRACE_MICRO_FREQUENCY * frequency,
                    2.0,
                    0.5,
                );
                let shaped = (step_index + riser_shape) / TERRACE_STEPS;
                ((shaped * 2.0 - 1.0) + micro * TERRACE_MICRO_MIX * scarp).clamp(-1.0, 1.0) as f32
            }
            NoiseAlgorithm::Dune => {
                if ctx.topology.wraps() && ctx.size > 0.0 {
                    self.dune_torus(x, z, ctx)
                } else {
                    self.dune_plane(x, z, ctx, frequency)
                }
            }
            NoiseAlgorithm::Cellular => {
                self.cracks(point, CELLULAR_FREQUENCY * frequency, octave_budget) as f32
            }
            NoiseAlgorithm::Plateau => {
                self.plateau_field(point, PLATEAU_FREQUENCY * frequency) as f32
            }
        }
    }

    #[allow(dead_code)]
    pub fn sample_blend(
        &self,
        contributions: &[(NoiseAlgorithm, f32)],
        x: f32,
        z: f32,
        ctx: &NoiseContext,
    ) -> f32 {
        self.sample_blend_with(contributions, x, z, ctx, &NoiseShape::default())
    }

    pub fn sample_blend_with(
        &self,
        contributions: &[(NoiseAlgorithm, f32)],
        x: f32,
        z: f32,
        ctx: &NoiseContext,
        shape: &NoiseShape,
    ) -> f32 {
        let mut total = 0.0f32;
        let mut weight_sum = 0.0f32;
        for (algorithm, weight) in contributions {
            if *weight <= 0.0 {
                continue;
            }
            total += self.sample_with(*algorithm, x, z, ctx, shape) * weight;
            weight_sum += weight;
        }
        if weight_sum <= f32::EPSILON {
            0.0
        } else {
            total / weight_sum
        }
    }
}

fn riser_profile(frac: f64, riser: f64) -> f64 {
    if frac <= 0.0 {
        0.0
    } else if frac >= riser {
        1.0
    } else {
        let t = frac / riser;
        t * t * (3.0 - 2.0 * t)
    }
}

fn stoss_slipface(phase: f64) -> f64 {
    let u = phase - phase.floor();
    let stoss = 0.78;
    if u < stoss {
        let t = u / stoss;
        t * t * (3.0 - 2.0 * t)
    } else {
        let t = (u - stoss) / (1.0 - stoss);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
}

fn wave_phase(seed: u32, nx: i32, nz: i32) -> f64 {
    let h = mix64(
        seed as u64 ^ ((nx as u32 as u64) << 32) ^ (nz as u32 as u64) ^ 0x52a7_f093,
    );
    (h >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::params::MacroContext;
    use crate::terrain::params::roll_cell_parameters;
    use crate::terrain::seed::WorldSeed;
    use noise::{Fbm, MultiFractal};

    fn torus_ctx() -> NoiseContext {
        NoiseContext {
            topology: WorldTopology::FiniteTorus,
            size: 200.0,
            wind: Vec2::X,
        }
    }

    #[test]
    fn every_algorithm_stays_in_the_unit_range() {
        let engine = NoiseEngine::new(42);
        let ctx = NoiseContext::default();
        for algorithm in NoiseAlgorithm::all() {
            for i in 0..64 {
                let x = i as f32 * 3.7 - 100.0;
                let z = i as f32 * -2.3 + 50.0;
                let value = engine.sample(algorithm, x, z, &ctx);
                assert!(
                    value.is_finite() && (-1.0..=1.0).contains(&value),
                    "{algorithm:?} produced {value} at ({x},{z})"
                );
            }
        }
    }

    #[test]
    fn sampling_is_deterministic() {
        let engine = NoiseEngine::new(7);
        let ctx = NoiseContext::default();
        for algorithm in NoiseAlgorithm::all() {
            assert_eq!(
                engine.sample(algorithm, 12.5, -8.25, &ctx),
                engine.sample(algorithm, 12.5, -8.25, &ctx)
            );
        }
    }

    #[test]
    fn different_seeds_produce_different_terrain() {
        let a = NoiseEngine::new(1);
        let b = NoiseEngine::new(2);
        let ctx = NoiseContext::default();
        assert_ne!(a.sample(NoiseAlgorithm::Hilly, 5.0, 5.0, &ctx), b.sample(NoiseAlgorithm::Hilly, 5.0, 5.0, &ctx));
    }

    #[test]
    fn algorithms_actually_differ_from_each_other() {
        let engine = NoiseEngine::new(11);
        let ctx = NoiseContext::default();
        let mut values = std::collections::HashSet::new();
        for algorithm in NoiseAlgorithm::all() {
            values.insert(engine.sample(algorithm, 33.0, -17.0, &ctx).to_bits());
        }
        assert_eq!(values.len(), 7, "each algorithm must contribute a distinct field");
    }

    #[test]
    fn toroidal_sampling_is_seamless_across_the_period() {
        let engine = NoiseEngine::new(3);
        let ctx = torus_ctx();
        for algorithm in NoiseAlgorithm::all() {
            let left = engine.sample(algorithm, -100.0, 12.0, &ctx);
            let right = engine.sample(algorithm, 100.0, 12.0, &ctx);
            assert!(
                (left - right).abs() < 1e-4,
                "{algorithm:?} discontinuous across the x seam: {left} vs {right}"
            );

            let bottom = engine.sample(algorithm, 7.0, -100.0, &ctx);
            let top = engine.sample(algorithm, 7.0, 100.0, &ctx);
            assert!(
                (bottom - top).abs() < 1e-4,
                "{algorithm:?} discontinuous across the z seam: {bottom} vs {top}"
            );
        }
    }

    #[test]
    fn toroidal_fields_repeat_exactly_over_full_periods() {
        let engine = NoiseEngine::new(29);
        let ctx = torus_ctx();
        for algorithm in NoiseAlgorithm::all() {
            for i in 0..12 {
                let x = i as f32 * 13.7 - 60.0;
                let z = i as f32 * 7.3 - 40.0;
                let here = engine.sample(algorithm, x, z, &ctx);
                let wrapped_x = engine.sample(algorithm, x + 200.0, z, &ctx);
                let wrapped_z = engine.sample(algorithm, x, z + 200.0, &ctx);
                assert!(
                    (here - wrapped_x).abs() < 1e-4 && (here - wrapped_z).abs() < 1e-4,
                    "{algorithm:?} is not periodic over the torus period"
                );
            }
        }
    }

    #[test]
    fn torus_warp_is_continuous_across_the_seam() {
        let engine = NoiseEngine::new(41);
        let ctx = NoiseContext {
            topology: WorldTopology::FiniteTorus,
            size: 120.0,
            wind: Vec2::X,
        };
        let half = ctx.size * 0.5;
        let gap = 1e-4f32;
        for algorithm in NoiseAlgorithm::all() {
            for i in 0..8 {
                let along = i as f32 * 11.0 - 44.0;
                let left = engine.sample(algorithm, half - gap, along, &ctx);
                let right = engine.sample(algorithm, -half + gap, along, &ctx);
                assert!(
                    (left - right).abs() < 1e-3,
                    "{algorithm:?} warp discontinuous across the x seam: {left} vs {right}"
                );
                let below = engine.sample(algorithm, along, half - gap, &ctx);
                let above = engine.sample(algorithm, along, -half + gap, &ctx);
                assert!(
                    (below - above).abs() < 1e-3,
                    "{algorithm:?} warp discontinuous across the z seam: {below} vs {above}"
                );
            }
        }
    }

    #[test]
    fn torus_warp_visibly_disturbs_the_compact_field() {
        let engine = NoiseEngine::new(37);
        let ctx = NoiseContext {
            topology: WorldTopology::FiniteTorus,
            size: 120.0,
            wind: Vec2::X,
        };
        let warped_shape = NoiseShape::default();
        let flat_shape = NoiseShape {
            warp: 0.0,
            ..Default::default()
        };
        let mut sum_sq = 0.0f64;
        let mut count = 0usize;
        for iz in 0..24 {
            for ix in 0..24 {
                let x = ix as f32 * 5.0 - 60.0;
                let z = iz as f32 * 5.0 - 60.0;
                let warped =
                    engine.sample_with(NoiseAlgorithm::Hilly, x, z, &ctx, &warped_shape) as f64;
                let flat = engine.sample_with(NoiseAlgorithm::Hilly, x, z, &ctx, &flat_shape) as f64;
                sum_sq += (warped - flat) * (warped - flat);
                count += 1;
            }
        }
        let rms = (sum_sq / count as f64).sqrt();
        assert!(
            rms > 0.01,
            "the torus warp must visibly disturb the compact field, not sit invisible: rms {rms}"
        );
    }

    #[test]
    fn blended_octaves_fade_continuously_across_cell_boundaries() {
        let engine = NoiseEngine::new(19);
        let ctx = NoiseContext::default();
        let coarse = NoiseShape {
            octaves: 3,
            octave_budget: 3.0,
            ..Default::default()
        };
        let fine = NoiseShape {
            octaves: 6,
            octave_budget: 6.0,
            ..Default::default()
        };
        let points = [(23.0f32, -14.0f32), (-41.0, 33.0), (7.5, 61.0)];
        let mut max_step = 0.0f32;
        let mut max_step_rounded = 0.0f32;
        for &(x, z) in &points {
            let mut previous: Option<f32> = None;
            let mut previous_rounded: Option<f32> = None;
            for step in 0..=400 {
                let t = step as f32 / 400.0;
                let shape = blend_shapes(&[(coarse, 1.0 - t), (fine, t)]);
                let value = engine.sample_with(NoiseAlgorithm::Hilly, x, z, &ctx, &shape);
                if let Some(prev) = previous {
                    max_step = max_step.max((value - prev).abs());
                }
                previous = Some(value);
                let rounded_count = shape.octave_budget.round();
                let legacy_shape = NoiseShape {
                    octaves: rounded_count as usize,
                    octave_budget: rounded_count,
                    ..shape
                };
                let legacy = engine.sample_with(NoiseAlgorithm::Hilly, x, z, &ctx, &legacy_shape);
                if let Some(prev) = previous_rounded {
                    max_step_rounded = max_step_rounded.max((legacy - prev).abs());
                }
                previous_rounded = Some(legacy);
            }
        }
        assert!(
            max_step < 0.01,
            "fractional octaves must fade smoothly, not pop: max step {max_step}"
        );
        assert!(
            max_step_rounded > 0.01,
            "the old rounded octave count must have produced a visible pop: max step {max_step_rounded}"
        );
    }

    #[test]
    fn finite_worlds_do_not_sample_four_dimensional_perlin_on_a_zero_plane() {
        let engine = NoiseEngine::new(42);
        let ctx = NoiseContext::default();
        let hilly: Fbm<Perlin> = Fbm::<Perlin>::new(42)
            .set_octaves(4)
            .set_frequency(0.009)
            .set_persistence(0.42)
            .set_lacunarity(2.0);
        let swell: Fbm<Perlin> = Fbm::<Perlin>::new(42 ^ 0x5bf0_3635)
            .set_octaves(2)
            .set_frequency(0.0032)
            .set_persistence(0.5);
        let warp: Fbm<Perlin> = Fbm::<Perlin>::new(42 ^ 0x1656_67b1)
            .set_octaves(2)
            .set_frequency(0.004);
        let mut matches = 0;
        for i in 0..16 {
            let x = i as f32 * 7.3 - 50.0;
            let z = i as f32 * -4.1 + 20.0;
            let base = [x as f64, z as f64, 0.0, 0.0];
            let wx = warp.get(base);
            let wz = warp.get([base[0] + 11.3, base[1] - 7.7, base[2] + 3.1, base[3]]);
            let warped = [
                base[0] + wx * 6.0,
                base[1] + wz * 6.0,
                base[2] + wx * 3.0,
                base[3] + wz * 3.0,
            ];
            let legacy = (hilly.get(warped) + swell.get(warped) * 0.35).clamp(-1.0, 1.0);
            if (engine.sample(NoiseAlgorithm::Hilly, x, z, &ctx) - legacy as f32).abs() < 1e-4 {
                matches += 1;
            }
        }
        assert_eq!(matches, 0, "finite sampling must not reuse the 4D zero-plane projection");
    }

    #[test]
    fn toroidal_sampling_varies_across_the_map() {
        let engine = NoiseEngine::new(19);
        let ctx = torus_ctx();
        let mut seen = 0;
        for i in 0..32 {
            if (engine.sample(NoiseAlgorithm::Hilly, i as f32 * 5.0 - 80.0, 3.0, &ctx)
                - engine.sample(NoiseAlgorithm::Hilly, i as f32 * 5.0 - 79.0, 3.0, &ctx))
                .abs()
                > 1e-6
            {
                seen += 1;
            }
        }
        assert!(seen > 10, "toroidal noise must not be constant");
    }

    #[test]
    fn dune_noise_is_anisotropic_along_the_wind() {
        let engine = NoiseEngine::new(5);
        let ctx = NoiseContext {
            topology: WorldTopology::Finite,
            size: 400.0,
            wind: Vec2::X,
        };
        let mut along_variation = 0.0;
        for i in 0..16 {
            along_variation += (engine.sample(NoiseAlgorithm::Dune, i as f32 * 2.0, 0.0, &ctx)
                - engine.sample(NoiseAlgorithm::Dune, i as f32 * 2.0 + 2.0, 0.0, &ctx))
                .abs();
        }
        let mut cross_variation = 0.0;
        for i in 0..16 {
            cross_variation += (engine.sample(NoiseAlgorithm::Dune, 0.0, i as f32 * 2.0, &ctx)
                - engine.sample(NoiseAlgorithm::Dune, 0.0, i as f32 * 2.0 + 2.0, &ctx))
                .abs();
        }
        assert!(
            cross_variation > along_variation,
            "dunes must vary faster across the wind than along it: cross {cross_variation} vs along {along_variation}"
        );
    }

    #[test]
    fn dune_field_follows_a_diagonal_wind() {
        let engine = NoiseEngine::new(5);
        let ctx = NoiseContext {
            topology: WorldTopology::Finite,
            size: 400.0,
            wind: Vec2::new(1.0, 1.0).normalize(),
        };
        let inv = 1.0 / 2.0f32.sqrt();
        let mut along_variation = 0.0;
        let mut cross_variation = 0.0;
        for i in 0..16 {
            let t = i as f32 * 2.0;
            along_variation += (engine.sample(
                NoiseAlgorithm::Dune,
                t * inv,
                t * inv,
                &ctx,
            ) - engine.sample(
                NoiseAlgorithm::Dune,
                (t + 2.0) * inv,
                (t + 2.0) * inv,
                &ctx,
            ))
                .abs();
            cross_variation += (engine.sample(
                NoiseAlgorithm::Dune,
                -t * inv,
                t * inv,
                &ctx,
            ) - engine.sample(
                NoiseAlgorithm::Dune,
                -(t + 2.0) * inv,
                (t + 2.0) * inv,
                &ctx,
            ))
                .abs();
        }
        assert!(
            cross_variation > along_variation * 1.3,
            "a diagonal wind must elongate dunes along itself: cross {cross_variation} vs along {along_variation}"
        );
    }

    #[test]
    fn torus_dune_field_follows_an_oblique_wind() {
        let engine = NoiseEngine::new(17);
        let base = torus_ctx();
        let oblique = NoiseContext {
            wind: Vec2::new(3.0f32.sqrt(), 1.0).normalize(),
            ..base
        };
        let mut difference = 0.0;
        for i in 0..24 {
            let x = i as f32 * 7.3 - 80.0;
            let z = i as f32 * 3.1 - 40.0;
            difference += (engine.sample(NoiseAlgorithm::Dune, x, z, &base)
                - engine.sample(NoiseAlgorithm::Dune, x, z, &oblique))
                .abs();
        }
        assert!(
            difference > 1.0,
            "an oblique wind must rotate the dune field, not snap to a cardinal axis: {difference}"
        );
    }

    #[test]
    fn torus_dune_field_is_periodic_under_oblique_wind() {
        let engine = NoiseEngine::new(23);
        let ctx = NoiseContext {
            topology: WorldTopology::FiniteTorus,
            size: 200.0,
            wind: Vec2::new(3.0f32.sqrt(), 1.0).normalize(),
        };
        for i in 0..12 {
            let z = i as f32 * 5.0 - 55.0;
            let left = engine.sample(NoiseAlgorithm::Dune, -100.0, z, &ctx);
            let right = engine.sample(NoiseAlgorithm::Dune, 100.0, z, &ctx);
            assert!(
                (left - right).abs() < 1e-4,
                "oblique dunes must stay periodic across the x seam: {left} vs {right}"
            );
            let x = i as f32 * 5.0 - 55.0;
            let bottom = engine.sample(NoiseAlgorithm::Dune, x, -100.0, &ctx);
            let top = engine.sample(NoiseAlgorithm::Dune, x, 100.0, &ctx);
            assert!(
                (bottom - top).abs() < 1e-4,
                "oblique dunes must stay periodic across the z seam: {bottom} vs {top}"
            );
        }
    }

    #[test]
    fn stoss_profile_rises_gently_and_drops_steeply() {
        let mut rise_start = None;
        let mut rise_end = None;
        let mut fall_start = None;
        let mut fall_end = None;
        for i in 0..10_000 {
            let u = i as f64 / 10_000.0;
            let v = stoss_slipface(u);
            if v >= 0.1 && rise_start.is_none() {
                rise_start = Some(u);
            }
            if v >= 0.9 && rise_end.is_none() {
                rise_end = Some(u);
            }
            if rise_end.is_some() && v <= 0.9 && fall_start.is_none() {
                fall_start = Some(u);
            }
            if fall_start.is_some() && v <= 0.1 && fall_end.is_none() {
                fall_end = Some(u);
            }
        }
        let rise = rise_end.unwrap() - rise_start.unwrap();
        let fall = fall_end.unwrap() - fall_start.unwrap();
        assert!(
            rise > fall * 3.0,
            "the stoss slope must span far more phase than the slipface: rise {rise} vs fall {fall}"
        );
    }

    #[test]
    fn terraced_noise_has_smooth_risers_instead_of_hard_steps() {
        let engine = NoiseEngine::new(8);
        let ctx = NoiseContext::default();
        let mut exact_runs = 0;
        let mut run = 0;
        let mut previous = f32::NAN;
        let mut off_grid = 0;
        for i in 0..300 {
            let value = engine.sample(NoiseAlgorithm::Terraced, i as f32 * 0.5, 4.0, &ctx);
            if (value - previous).abs() < 1e-6 {
                run += 1;
            } else {
                if run > 3 {
                    exact_runs += 1;
                }
                run = 0;
            }
            previous = value;
            let grid = (value * (TERRACE_STEPS as f32 / 2.0)).round() * 2.0 / TERRACE_STEPS as f32;
            if (value - grid).abs() > 0.02 {
                off_grid += 1;
            }
        }
        assert_eq!(exact_runs, 0, "hard steps produced exact flat plateaus");
        assert!(off_grid > 50, "terraced values must leave the hard staircase grid");
    }

    #[test]
    fn terrace_profile_is_monotone_and_continuous() {
        let mut previous = 0.0f64;
        let mut max_jump = 0.0f64;
        let mut interior = 0;
        for i in 0..2000 {
            let scaled = i as f64 * 0.01;
            let value = scaled.floor() + riser_profile(scaled - scaled.floor(), TERRACE_RISER);
            if i > 0 {
                assert!(value >= previous - 1e-12, "the riser shaping must never step backwards");
                max_jump = max_jump.max(value - previous);
            }
            let part = value - value.floor();
            if part > 1e-3 && part < 1.0 - 1e-3 {
                interior += 1;
            }
            previous = value;
        }
        assert!(max_jump < 0.1, "the riser must be smooth, not a hard cliff: {max_jump}");
        assert!(interior > 500, "the shaping must pass through intermediate heights");
    }

    #[test]
    fn ridged_noise_reaches_both_extremes_without_a_clamped_plateau() {
        let engine = NoiseEngine::new(23);
        let ctx = NoiseContext::default();
        let mut max = f32::MIN;
        let mut min = f32::MAX;
        let mut clamped = 0;
        let mut total = 0;
        for iz in 0..64 {
            for ix in 0..64 {
                let value = engine.sample(
                    NoiseAlgorithm::Ridged,
                    ix as f32 * 2.5 - 80.0,
                    iz as f32 * 1.9 - 60.0,
                    &ctx,
                );
                max = max.max(value);
                min = min.min(value);
                if value >= 1.0 - 1e-6 || value <= -1.0 + 1e-6 {
                    clamped += 1;
                }
                total += 1;
            }
        }
        assert!(max > 0.8, "ridged peaks must reach the top of the range: {max}");
        assert!(min < -0.8, "ridged valleys must reach the bottom of the range: {min}");
        assert!(
            clamped * 50 < total,
            "a hard clamp flattens too much of the field: {clamped}/{total} pinned"
        );
    }

    #[test]
    fn ridged_valleys_are_smooth_but_not_flat() {
        let engine = NoiseEngine::new(31);
        let ctx = NoiseContext::default();
        let mut valley_values = Vec::new();
        for i in 0..400 {
            let value = engine.sample(NoiseAlgorithm::Ridged, i as f32 * 1.7 - 100.0, 12.0, &ctx);
            if value < -0.5 {
                valley_values.push(value);
            }
        }
        assert!(!valley_values.is_empty(), "the field must contain valleys");
        let distinct: std::collections::HashSet<u32> =
            valley_values.iter().map(|v| v.to_bits()).collect();
        assert!(
            distinct.len() * 2 > valley_values.len(),
            "valleys collapsed onto a flat floor: {} distinct of {}",
            distinct.len(),
            valley_values.len()
        );
    }

    #[test]
    fn cellular_noise_is_a_crack_network_not_cones() {
        let engine = NoiseEngine::new(13);
        let ctx = NoiseContext::default();
        let shape = NoiseShape {
            frequency: 18.0,
            ..Default::default()
        };
        let mut high = 0;
        let mut total = 0;
        let mut min = f32::MAX;
        for iz in 0..120 {
            for ix in 0..120 {
                let value = engine.sample_with(
                    NoiseAlgorithm::Cellular,
                    ix as f32 * 0.25 - 15.0,
                    iz as f32 * 0.25 - 9.0,
                    &ctx,
                    &shape,
                );
                if value > 0.8 {
                    high += 1;
                }
                min = min.min(value);
                total += 1;
            }
        }
        assert!(
            high as f32 / total as f32 > 0.35,
            "plateaus between cracks must dominate the field: {high}/{total}"
        );
        assert!(min < -0.5, "cracks must reach the low end: {min}");
    }

    #[test]
    fn cellular_cracks_are_thin_zero_width_ridges() {
        let engine = NoiseEngine::new(13);
        let mut min_gap = f64::MAX;
        let mut near_border = 0;
        let mut total = 0;
        for iz in 0..200 {
            for ix in 0..200 {
                let point = [ix as f64 * 0.05 - 5.0, iz as f64 * 0.05 - 3.0];
                let gap = engine.f2_minus_f1(&point, 1.0);
                min_gap = min_gap.min(gap);
                if gap < 0.05 {
                    near_border += 1;
                }
                total += 1;
            }
        }
        assert!(
            min_gap < 0.05,
            "F2 minus F1 must collapse to zero on cell borders: {min_gap}"
        );
        assert!(near_border > 0, "the sample grid must cross cell borders");
        assert!(
            near_border * 4 < total,
            "the zero band must be a thin ridge, not a broad cone: {near_border}/{total}"
        );
    }

    #[test]
    fn plateau_field_forms_flat_mesa_tops_with_steep_risers() {
        let engine = NoiseEngine::new(23);
        let ctx = NoiseContext::default();
        let mut flat_runs = 0;
        let mut run = 0;
        let mut previous = f32::NAN;
        let mut max_step = 0.0f32;
        let mut distinct = std::collections::HashSet::new();
        for i in 0..4000 {
            let value = engine.sample(NoiseAlgorithm::Plateau, i as f32 * 0.4, 7.0, &ctx);
            distinct.insert(value.to_bits());
            if (value - previous).abs() < 1e-6 {
                run += 1;
            } else {
                if run > 4 {
                    flat_runs += 1;
                }
                run = 0;
            }
            if previous.is_finite() {
                max_step = max_step.max((value - previous).abs());
            }
            previous = value;
        }
        assert!(flat_runs > 5, "plateaus must produce flat mesa tops: {flat_runs}");
        assert!(max_step > 0.05, "risers between mesas must be steep: {max_step}");
        assert!(
            distinct.len() > 100,
            "mesa risers must be jagged, not a hard staircase: {}",
            distinct.len()
        );
    }

    #[test]
    fn plateau_selection_follows_crystalline_bedrock() {
        let mut params = roll_cell_parameters(WorldSeed::new(1), 0, &MacroContext::default());
        params.geothermal = Geothermal::None;
        params.substrate = Substrate::RockCrystalline;
        params.base_height = BaseHeightBand::Lowland;
        assert_eq!(select_algorithm(&params, ""), NoiseAlgorithm::Plateau);
        params.base_height = BaseHeightBand::Peak;
        assert_eq!(select_algorithm(&params, ""), NoiseAlgorithm::Ridged);
    }

    #[test]
    fn shape_orders_relief_from_abyssal_to_alpine() {
        let abyss = shape_for(
            &Region::DeepOceanBasin.prototype(),
            Region::DeepOceanBasin,
            "",
        );
        let coastal = shape_for(&Region::TidalFlat.prototype(), Region::TidalFlat, "");
        let lowland = shape_for(
            &Region::TemperateForest.prototype(),
            Region::TemperateForest,
            "",
        );
        let alpine = shape_for(&Region::ScreeTalus.prototype(), Region::ScreeTalus, "");
        assert!(abyss.amplitude < 2.0, "abyssal basins must be near flat: {}", abyss.amplitude);
        assert!(
            (1.0..=2.5).contains(&coastal.amplitude),
            "tidal bands must be gentle ripples: {}",
            coastal.amplitude
        );
        assert!(lowland.amplitude > coastal.amplitude);
        assert!(alpine.amplitude > 25.0, "alpine relief must be sharp: {}", alpine.amplitude);
        assert!(alpine.octaves > coastal.octaves);
        assert!(alpine.warp > coastal.warp);
    }

    #[test]
    fn blended_shapes_interpolate_between_landforms() {
        let alpine = shape_for(&Region::ScreeTalus.prototype(), Region::ScreeTalus, "");
        let coastal = shape_for(&Region::TidalFlat.prototype(), Region::TidalFlat, "");
        let mid = blend_shapes(&[(alpine, 0.5), (coastal, 0.5)]);
        assert!(mid.amplitude > coastal.amplitude);
        assert!(mid.amplitude < alpine.amplitude);
        let weighted = blend_shapes(&[(alpine, 3.0), (coastal, 1.0)]);
        assert!(weighted.amplitude > mid.amplitude);
    }

    #[test]
    fn blend_of_one_algorithm_equals_that_algorithm() {
        let engine = NoiseEngine::new(2);
        let ctx = NoiseContext::default();
        let direct = engine.sample(NoiseAlgorithm::Ridged, 9.0, 9.0, &ctx);
        let blended = engine.sample_blend(&[(NoiseAlgorithm::Ridged, 1.0)], 9.0, 9.0, &ctx);
        assert!((direct - blended).abs() < 1e-6);
    }

    #[test]
    fn blend_is_weight_averaged_and_continuous() {
        let engine = NoiseEngine::new(6);
        let ctx = NoiseContext::default();
        let mut previous: Option<f32> = None;
        for step in 0..=20 {
            let t = step as f32 / 20.0;
            let value = engine.sample_blend(
                &[(NoiseAlgorithm::Hilly, 1.0 - t), (NoiseAlgorithm::Ridged, t)],
                14.0,
                -22.0,
                &ctx,
            );
            if let Some(prev) = previous {
                assert!((value - prev).abs() < 0.2, "cross-algorithm blend jumped: {prev} -> {value}");
            }
            previous = Some(value);
        }
    }

    #[test]
    fn selection_covers_every_algorithm() {
        let seed = WorldSeed::new(1234);
        let mut seen = std::collections::HashSet::new();
        for cell in 0..500u64 {
            for band in BaseHeightBand::ALL {
                let ctx = MacroContext {
                    band: *band,
                    latitude: (cell % 50) as f32 / 49.0,
                    sea_proximity: (cell % 4) as f32 / 3.0,
                    rain_shadow: (cell % 3) as f32 / 2.0,
                    geothermal_intensity: (cell % 5) as f32 / 4.0,
                    prevailing_wind: Vec2::X,
                    ..Default::default()
                };
                seen.insert(select_algorithm(&roll_cell_parameters(seed, cell, &ctx), ""));
            }
        }
        for algorithm in NoiseAlgorithm::all() {
            assert!(seen.contains(&algorithm), "{algorithm:?} was never selected");
        }
    }

    #[test]
    fn selection_follows_the_documented_intents() {
        let mut params = roll_cell_parameters(WorldSeed::new(1), 0, &MacroContext::default());
        params.geothermal = Geothermal::ActiveVolcanic;
        assert_eq!(select_algorithm(&params, ""), NoiseAlgorithm::Cellular);

        params = roll_cell_parameters(WorldSeed::new(1), 0, &MacroContext::default());
        params.base_height = BaseHeightBand::Peak;
        assert_eq!(select_algorithm(&params, ""), NoiseAlgorithm::Ridged);

        params = roll_cell_parameters(WorldSeed::new(1), 0, &MacroContext::default());
        params.substrate = Substrate::LimestoneKarst;
        assert_eq!(select_algorithm(&params, ""), NoiseAlgorithm::Terraced);
    }

    #[test]
    fn algorithm_labels_round_trip() {
        for algorithm in NoiseAlgorithm::all() {
            assert_eq!(NoiseAlgorithm::parse(algorithm.label()), Some(algorithm));
        }
        assert_eq!(NoiseAlgorithm::parse("nope"), None);
    }
}