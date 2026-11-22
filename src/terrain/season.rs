use bevy::color::Mix;
use bevy::prelude::{Color, Resource, Vec2};

use crate::terrain::field::ScalarField;
use crate::terrain::material::SurfaceLayer;
use crate::terrain::params::{
    CellParameters, Geothermal, ParamLevel, Precipitation, Salinity, Temperature,
};
use crate::terrain::region::RegionTier;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    Spring,
    Summer,
    Fall,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [
        Season::Spring,
        Season::Summer,
        Season::Fall,
        Season::Winter,
    ];

    pub fn index(self) -> usize {
        Season::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Fall => "fall",
            Season::Winter => "winter",
        }
    }

    pub fn parse(input: &str) -> Option<Season> {
        Season::ALL.iter().copied().find(|s| s.label() == input)
    }

    pub fn next(self) -> Season {
        Season::ALL[(self.index() + 1) % Season::ALL.len()]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeasonDelta {
    pub temperature: i32,
    pub humidity: i32,
    pub precipitation: i32,
    pub wind: i32,
    pub sun_exposure: i32,
}

impl SeasonDelta {
    pub const ZERO: SeasonDelta = SeasonDelta {
        temperature: 0,
        humidity: 0,
        precipitation: 0,
        wind: 0,
        sun_exposure: 0,
    };

    pub fn apply(&self, params: &CellParameters) -> CellParameters {
        let mut next = *params;
        next.temperature = params.temperature.shift(self.temperature);
        next.humidity = params.humidity.shift(self.humidity);
        next.precipitation = params.precipitation.shift(self.precipitation);
        next.wind = params.wind.shift(self.wind);
        next.sun_exposure = params.sun_exposure.shift(self.sun_exposure);
        next
    }

    #[allow(dead_code)]
    pub fn magnitude(&self) -> i32 {
        self.temperature.abs()
            + self.humidity.abs()
            + self.precipitation.abs()
            + self.wind.abs()
            + self.sun_exposure.abs()
    }

    pub fn blend(&self, other: &SeasonDelta, t: f32) -> SeasonDelta {
        let t = t.clamp(0.0, 1.0);
        let mix_field = |a: i32, b: i32| (a as f32 + (b - a) as f32 * t).round() as i32;
        SeasonDelta {
            temperature: mix_field(self.temperature, other.temperature),
            humidity: mix_field(self.humidity, other.humidity),
            precipitation: mix_field(self.precipitation, other.precipitation),
            wind: mix_field(self.wind, other.wind),
            sun_exposure: mix_field(self.sun_exposure, other.sun_exposure),
        }
    }
}

const fn delta(
    temperature: i32,
    humidity: i32,
    precipitation: i32,
    wind: i32,
    sun_exposure: i32,
) -> SeasonDelta {
    SeasonDelta {
        temperature,
        humidity,
        precipitation,
        wind,
        sun_exposure,
    }
}

static SEASON_DELTAS: [(Season, RegionTier, SeasonDelta); 20] = [
    (Season::Spring, RegionTier::Submerged, delta(-1, 0, -1, 0, -1)),
    (Season::Summer, RegionTier::Submerged, delta(-2, 0, -1, 1, -1)),
    (Season::Fall, RegionTier::Submerged, delta(1, 0, 0, -1, 1)),
    (Season::Winter, RegionTier::Submerged, delta(2, 0, 1, -1, 2)),
    (Season::Spring, RegionTier::Coastal, delta(-1, -1, -1, 0, -1)),
    (Season::Summer, RegionTier::Coastal, delta(-2, -1, -1, 1, -1)),
    (Season::Fall, RegionTier::Coastal, delta(1, -1, 0, -1, 1)),
    (Season::Winter, RegionTier::Coastal, delta(2, 1, 1, -1, 2)),
    (Season::Spring, RegionTier::Lowland, delta(-1, -1, -1, 0, -1)),
    (Season::Summer, RegionTier::Lowland, delta(-2, 1, 1, 1, -1)),
    (Season::Fall, RegionTier::Lowland, delta(1, -1, -1, -1, 1)),
    (Season::Winter, RegionTier::Lowland, delta(2, 1, 1, -1, 2)),
    (Season::Spring, RegionTier::Highland, delta(-1, -1, -1, 0, -1)),
    (Season::Summer, RegionTier::Highland, delta(-2, 1, 0, 1, -1)),
    (Season::Fall, RegionTier::Highland, delta(2, -1, -1, -1, 1)),
    (Season::Winter, RegionTier::Highland, delta(3, 1, 1, -1, 2)),
    (Season::Spring, RegionTier::Alpine, delta(-2, -1, -1, 0, -1)),
    (Season::Summer, RegionTier::Alpine, delta(-3, 1, 0, 1, -1)),
    (Season::Fall, RegionTier::Alpine, delta(2, -1, -1, -1, 1)),
    (Season::Winter, RegionTier::Alpine, delta(3, 1, 1, -1, 2)),
];

pub fn seasonal_delta(season: Season, tier: RegionTier) -> SeasonDelta {
    SEASON_DELTAS
        .iter()
        .find(|(s, t, _)| *s == season && *t == tier)
        .map(|(_, _, d)| *d)
        .unwrap_or(SeasonDelta::ZERO)
}

#[allow(dead_code)]
pub fn seasonal_params(
    params: &CellParameters,
    season: Season,
    tier: RegionTier,
) -> CellParameters {
    seasonal_delta(season, tier).apply(params)
}

pub fn blended_delta(clock: &SeasonClock, tier: RegionTier) -> SeasonDelta {
    seasonal_delta(clock.season, tier).blend(
        &seasonal_delta(clock.season.next(), tier),
        clock.blend_to_next(),
    )
}

#[derive(Clone, Copy, Debug)]
pub struct SeaLevelSeason {
    pub base: f32,
    pub amplitude: f32,
}

impl Default for SeaLevelSeason {
    fn default() -> Self {
        Self {
            base: 0.0,
            amplitude: 1.6,
        }
    }
}

impl SeaLevelSeason {
    pub fn offset(&self, season: Season) -> f32 {
        let factor = match season {
            Season::Spring => 0.25,
            Season::Summer => 1.0,
            Season::Fall => 0.15,
            Season::Winter => -1.0,
        };
        self.base + self.amplitude * factor
    }

    pub fn level_at(&self, season: Season) -> f32 {
        self.offset(season)
    }

    #[allow(dead_code)]
    pub fn min(&self) -> f32 {
        Season::ALL
            .iter()
            .map(|s| self.offset(*s))
            .fold(f32::INFINITY, f32::min)
    }

    pub fn max(&self) -> f32 {
        Season::ALL
            .iter()
            .map(|s| self.offset(*s))
            .fold(f32::NEG_INFINITY, f32::max)
    }

    pub fn continuous_level(&self, cycle_progress: f32) -> f32 {
        let tau = std::f32::consts::TAU;
        self.base + self.amplitude * (cycle_progress * tau).sin()
    }
}

#[derive(Resource, Clone, Debug)]
pub struct SeasonClock {
    pub season: Season,
    pub progress: f32,
    pub cycle_seconds: f32,
    pub running: bool,
}

impl Default for SeasonClock {
    fn default() -> Self {
        Self {
            season: Season::Summer,
            progress: 0.0,
            cycle_seconds: 240.0,
            running: false,
        }
    }
}

impl SeasonClock {
    pub fn season_seconds(&self) -> f32 {
        (self.cycle_seconds / Season::ALL.len() as f32).max(1e-3)
    }

    pub fn tick(&mut self, delta_seconds: f32) -> bool {
        if !self.running {
            return false;
        }
        self.progress += delta_seconds / self.season_seconds();
        let mut changed = false;
        while self.progress >= 1.0 {
            self.progress -= 1.0;
            self.season = self.season.next();
            changed = true;
        }
        changed
    }

    pub fn set_season(&mut self, season: Season) {
        self.season = season;
        self.progress = 0.0;
    }

    pub fn cycle_progress(&self) -> f32 {
        let base = self.season.index() as f32 + self.progress.clamp(0.0, 1.0);
        (base / Season::ALL.len() as f32) % 1.0
    }

    pub fn blend_to_next(&self) -> f32 {
        let tail = 0.25;
        if self.progress <= 1.0 - tail {
            return 0.0;
        }
        let t = (self.progress - (1.0 - tail)) / tail;
        t * t * (3.0 - 2.0 * t)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloodZone {
    pub low: f32,
    pub high: f32,
}

impl FloodZone {
    pub fn new(low: f32, high: f32) -> Self {
        let (lo, hi) = if low <= high { (low, high) } else { (high, low) };
        Self { low: lo, high: hi }
    }

    pub fn around_sea_levels(levels: &[f32], margin: f32) -> Self {
        let low = levels
            .iter()
            .cloned()
            .fold(f32::INFINITY, f32::min)
            - margin;
        let high = levels
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max)
            + margin;
        Self::new(low, high)
    }

    pub fn contains(&self, height: f32) -> bool {
        height >= self.low && height <= self.high
    }

    #[allow(dead_code)]
    pub fn classify(&self, field: &ScalarField) -> Vec<bool> {
        field
            .values
            .iter()
            .map(|v| self.contains(*v))
            .collect()
    }

    #[allow(dead_code)]
    pub fn coverage(&self, field: &ScalarField) -> f32 {
        if field.values.is_empty() {
            return 0.0;
        }
        let inside = self.classify(field).iter().filter(|b| **b).count();
        inside as f32 / field.values.len() as f32
    }
}

pub fn coastal_erosion(
    field: &mut ScalarField,
    zone: &FloodZone,
    strength: f32,
    hardness: Option<&ScalarField>,
) {
    if strength <= 0.0 {
        return;
    }
    let snapshot = field.values.clone();
    let mut targets: Vec<Option<f32>> = vec![None; snapshot.len()];

    for index in 0..snapshot.len() {
        if !zone.contains(snapshot[index]) {
            continue;
        }
        let erodibility = hardness
            .map(|field| 1.0 - field.get_at(index).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        let local_strength = (strength * erodibility).min(1.0);
        let (x, z) = field.coords(index);
        let mut highest_neighbour = snapshot[index];
        for neighbour in field.neighbours8(x, z) {
            highest_neighbour = highest_neighbour.max(snapshot[neighbour]);
        }
        let pull = zone.high + (highest_neighbour - zone.high).max(0.0) * 0.15;
        let blended = snapshot[index] + (pull - snapshot[index]) * local_strength;
        targets[index] = Some(blended.min(highest_neighbour));
    }

    for (index, target) in targets.iter().enumerate() {
        if let Some(value) = target {
            field.set_at(index, *value);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShoreSurface {
    WetSand,
    Mudflat,
    SaltPan,
    FrozenShore,
}

impl ShoreSurface {
    pub fn layer(self) -> Option<SurfaceLayer> {
        match self {
            ShoreSurface::WetSand => Some(SurfaceLayer::Sand),
            ShoreSurface::Mudflat => Some(SurfaceLayer::Silt),
            ShoreSurface::SaltPan => Some(SurfaceLayer::Gravel),
            ShoreSurface::FrozenShore => Some(SurfaceLayer::Snow),
        }
    }
}

pub fn shore_surface(
    season: Season,
    salinity: Salinity,
    precipitation: Precipitation,
    submerged_now: bool,
) -> ShoreSurface {
    if season == Season::Winter {
        return ShoreSurface::FrozenShore;
    }
    if submerged_now {
        return match salinity {
            Salinity::Freshwater | Salinity::Brackish => ShoreSurface::Mudflat,
            Salinity::Marine => ShoreSurface::WetSand,
            Salinity::Hypersaline => ShoreSurface::SaltPan,
        };
    }
    match salinity {
        Salinity::Hypersaline => ShoreSurface::SaltPan,
        Salinity::Marine | Salinity::Brackish if precipitation.index() <= Precipitation::Moderate.index() => {
            ShoreSurface::WetSand
        }
        _ => ShoreSurface::Mudflat,
    }
}

pub const HARD_SHORE_HARDNESS: f32 = 0.5;

pub fn shore_override_with_hardness(
    season: Season,
    zone: &FloodZone,
    height: f32,
    salinity: Salinity,
    precipitation: Precipitation,
    sea_level: f32,
    hardness: f32,
) -> Option<SurfaceLayer> {
    if !zone.contains(height) {
        return None;
    }
    if hardness >= HARD_SHORE_HARDNESS {
        if season == Season::Winter {
            return Some(SurfaceLayer::Snow);
        }
        if salinity == Salinity::Hypersaline {
            return Some(SurfaceLayer::Gravel);
        }
        return None;
    }
    shore_surface(season, salinity, precipitation, height <= sea_level).layer()
}

pub fn is_water_frozen(
    season: Season,
    temperature: Temperature,
    salinity: Salinity,
    geothermal: Geothermal,
) -> bool {
    if geothermal != Geothermal::None {
        return false;
    }
    let temperature_step = temperature.index();
    match salinity {
        Salinity::Freshwater => {
            temperature_step >= Temperature::Cold.index()
                || (season == Season::Winter
                    && temperature_step >= Temperature::Temperate.index())
        }
        Salinity::Brackish => {
            temperature_step >= Temperature::Freezing.index()
                || (season == Season::Winter && temperature_step >= Temperature::Cold.index())
        }
        Salinity::Marine => temperature_step >= Temperature::Freezing.index(),
        Salinity::Hypersaline => {
            season == Season::Winter && temperature_step >= Temperature::Freezing.index()
        }
    }
}

pub const DEFAULT_SNOWLINE_METRES: f32 = 60.0;

pub const SNOW_SHED_SLOPE: f32 = 0.5;

const SNOW_COLD_RAMP_BANDS: f32 = 2.0;
const SNOW_ALTITUDE_RAMP_METRES: f32 = 40.0;

pub fn snow_coverage(season: Season, temperature: Temperature, height: f32, snowline: f32) -> f32 {
    let cold = ((temperature.index() as f32 - Temperature::Temperate.index() as f32)
        / SNOW_COLD_RAMP_BANDS)
        .clamp(0.0, 1.0);
    let altitude = ((height - snowline) / SNOW_ALTITUDE_RAMP_METRES).clamp(0.0, 1.0);
    let seasonal: f32 = match season {
        Season::Winter => 1.0,
        Season::Spring => 0.35,
        Season::Fall => 0.45,
        Season::Summer => 0.0,
    };
    (seasonal.max(cold).max(altitude)).clamp(0.0, 1.0)
}

pub fn snow_coverage_blend(
    clock: &SeasonClock,
    temperature: Temperature,
    height: f32,
    snowline: f32,
) -> f32 {
    let current = snow_coverage(clock.season, temperature, height, snowline);
    let t = clock.blend_to_next();
    if t <= 0.0 {
        return current;
    }
    let next = snow_coverage(clock.season.next(), temperature, height, snowline);
    (current + (next - current) * t).clamp(0.0, 1.0)
}

pub const MAX_SNOW_SHED_FRACTION: f32 = 0.85;

pub fn snow_coverage_with_slope(
    clock: &SeasonClock,
    temperature: Temperature,
    height: f32,
    snowline: f32,
    slope: f32,
) -> f32 {
    let coverage = snow_coverage_blend(clock, temperature, height, snowline);
    let shed = (slope / SNOW_SHED_SLOPE).clamp(0.0, 1.0) * MAX_SNOW_SHED_FRACTION;
    (coverage * (1.0 - shed)).clamp(0.0, 1.0)
}

const SNOWLINE_SEASON_SHIFT: [f32; 4] = [-6.0, 8.0, 0.0, -18.0];

pub fn snowline_for(clock: &SeasonClock, base: f32) -> f32 {
    let current = SNOWLINE_SEASON_SHIFT[clock.season.index()];
    let next = SNOWLINE_SEASON_SHIFT[clock.season.next().index()];
    base + current + (next - current) * clock.blend_to_next()
}

pub fn ground_tint(base: Color, snow: f32) -> Color {
    if snow <= 0.0 {
        return base;
    }
    let frost = Color::srgb(0.92, 0.95, 1.0);
    base.mix(&frost, snow * 0.75)
}

#[allow(dead_code)]
pub fn seasonal_wind_bias(season: Season) -> f32 {
    match season {
        Season::Spring => 1.0,
        Season::Summer => 0.7,
        Season::Fall => 1.15,
        Season::Winter => 1.35,
    }
}

#[allow(dead_code)]
pub fn prevailing_wind_for_season(base: Vec2, season: Season) -> Vec2 {
    let rotated = if season == Season::Fall || season == Season::Winter {
        Vec2::new(-base.y, base.x)
    } else {
        base
    };
    rotated.normalize_or_zero() * seasonal_wind_bias(season)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::params::{MacroContext, roll_cell_parameters};
    use crate::terrain::seed::WorldSeed;
    use crate::terrain::topology::WorldTopology;

    #[test]
    fn season_order_cycles() {
        let mut season = Season::Spring;
        for _ in 0..4 {
            season = season.next();
        }
        assert_eq!(season, Season::Spring);
        assert_eq!(Season::parse("winter"), Some(Season::Winter));
        assert_eq!(Season::parse("nope"), None);
    }

    #[test]
    fn seasonal_deltas_cover_every_season_and_tier() {
        for season in Season::ALL {
            for tier in RegionTier::all() {
                let delta = seasonal_delta(season, tier);
                assert!(delta.magnitude() > 0, "{season:?} {tier:?} must differ");
                assert!(delta.magnitude() <= 10);
            }
        }
    }

    #[test]
    fn parameter_deltas_clamp_instead_of_wrapping() {
        let params = roll_cell_parameters(WorldSeed::new(3), 0, &MacroContext::default());
        for season in Season::ALL {
            for tier in RegionTier::all() {
                let shifted = seasonal_params(&params, season, tier);
                assert!(
                    shifted.temperature.index() < Temperature::COUNT,
                    "temperature escaped its range"
                );
                assert!(shifted.precipitation.index() < Precipitation::COUNT);
            }
        }

        let scorching = CellParameters {
            temperature: Temperature::Scorching,
            ..params
        };
        let hotter = seasonal_delta(Season::Summer, RegionTier::Alpine).apply(&scorching);
        assert_eq!(hotter.temperature, Temperature::Scorching, "must clamp at the top");

        let freezing = CellParameters {
            temperature: Temperature::Freezing,
            ..params
        };
        let colder = seasonal_delta(Season::Winter, RegionTier::Alpine).apply(&freezing);
        assert_eq!(colder.temperature, Temperature::Freezing, "must clamp at the bottom");
    }

    #[test]
    fn sea_level_stays_between_extremes_and_is_continuous() {
        let levels = SeaLevelSeason::default();
        let winter = levels.level_at(Season::Winter);
        let summer = levels.level_at(Season::Summer);
        assert!(winter < summer, "winter sea level must sit lower");
        assert!((levels.min() - winter).abs() < 1e-5);
        assert!((levels.max() - summer).abs() < 1e-5);

        let mut previous = levels.continuous_level(0.0);
        for step in 1..=200 {
            let t = step as f32 / 200.0;
            let value = levels.continuous_level(t);
            assert!(value >= levels.min() - 1e-4 && value <= levels.max() + 1e-4);
            assert!(
                (value - previous).abs() < 0.1,
                "sea level jumped at t={t}: {previous} -> {value}"
            );
            previous = value;
        }
        assert!(
            (levels.continuous_level(0.0) - levels.continuous_level(1.0)).abs() < 1e-4,
            "the cycle must close on itself"
        );
    }

    #[test]
    fn the_tide_moves_the_sea_within_the_seasonal_band() {
        let levels = SeaLevelSeason::default();
        for step in 0..16 {
            let level = levels.continuous_level(step as f32 / 16.0);
            assert!(
                level >= levels.min() - 1e-4 && level <= levels.max() + 1e-4,
                "tide {level} escapes the seasonal band"
            );
        }
        assert_ne!(
            levels.continuous_level(0.25),
            levels.continuous_level(0.75),
            "the tide curve must actually move"
        );
    }

    #[test]
    fn clock_advances_and_wraps_seasons() {
        let mut clock = SeasonClock {
            cycle_seconds: 40.0,
            running: true,
            ..Default::default()
        };
        clock.set_season(Season::Spring);
        assert!(!clock.tick(5.0), "a fifth of a season is not a change");
        assert_eq!(clock.season, Season::Spring);
        assert!(clock.tick(6.0), "crossing the boundary must report a change");
        assert_eq!(clock.season, Season::Summer);
        assert!(clock.progress < 0.5);

        for _ in 0..4 {
            clock.tick(10.0);
        }
        assert_eq!(clock.season, Season::Summer, "a full cycle returns to the start");
        assert!((clock.cycle_progress() - 0.25).abs() < 0.05);
    }

    #[test]
    fn a_paused_clock_does_not_move() {
        let mut clock = SeasonClock {
            cycle_seconds: 10.0,
            running: false,
            ..Default::default()
        };
        assert!(!clock.tick(1000.0));
        assert_eq!(clock.season, Season::Summer);
    }

    #[test]
    fn blend_to_next_ramps_only_at_the_season_tail() {
        let mut clock = SeasonClock {
            cycle_seconds: 400.0,
            running: true,
            ..Default::default()
        };
        clock.progress = 0.5;
        assert_eq!(clock.blend_to_next(), 0.0);
        clock.progress = 0.9;
        assert!(clock.blend_to_next() > 0.4);
        clock.progress = 1.0;
        assert!((clock.blend_to_next() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn blended_delta_follows_the_clock_across_the_tail() {
        let mut clock = SeasonClock {
            cycle_seconds: 400.0,
            running: true,
            ..Default::default()
        };
        clock.set_season(Season::Winter);
        assert_eq!(
            blended_delta(&clock, RegionTier::Alpine),
            seasonal_delta(Season::Winter, RegionTier::Alpine),
            "at the season start the blend must be the current delta"
        );
        clock.progress = 1.0;
        assert_eq!(
            blended_delta(&clock, RegionTier::Alpine),
            seasonal_delta(Season::Spring, RegionTier::Alpine),
            "at the season end the blend must reach the next delta"
        );
        clock.progress = 0.9;
        let mid = blended_delta(&clock, RegionTier::Alpine);
        let winter = seasonal_delta(Season::Winter, RegionTier::Alpine);
        let spring = seasonal_delta(Season::Spring, RegionTier::Alpine);
        assert!(mid.temperature < winter.temperature);
        assert!(mid.temperature >= spring.temperature);
    }

    #[test]
    fn snowline_and_snow_coverage_slide_with_the_clock() {
        let mut winter = SeasonClock {
            cycle_seconds: 400.0,
            running: false,
            ..Default::default()
        };
        winter.set_season(Season::Winter);
        let mut summer = SeasonClock {
            cycle_seconds: 400.0,
            running: false,
            ..Default::default()
        };
        summer.set_season(Season::Summer);
        assert!(
            snowline_for(&winter, 60.0) < snowline_for(&summer, 60.0),
            "the snowline must drop in winter"
        );
        let winter_snow = snow_coverage_blend(&winter, Temperature::Temperate, 0.0, 60.0);
        let summer_snow = snow_coverage_blend(&summer, Temperature::Temperate, 0.0, 60.0);
        assert!(winter_snow > summer_snow);

        summer.progress = 0.95;
        let thawing = snow_coverage_blend(&summer, Temperature::Temperate, 0.0, 60.0);
        assert!(
            thawing > summer_snow,
            "the tail of a season must already lean toward the next one"
        );
        assert!(thawing < 1.0);
    }

    #[test]
    fn flood_zone_classifies_only_the_interband() {
        let zone = FloodZone::around_sea_levels(
            &[
                SeaLevelSeason::default().level_at(Season::Winter),
                SeaLevelSeason::default().level_at(Season::Summer),
            ],
            0.5,
        );
        assert!(zone.low < zone.high);
        assert!(zone.contains(0.0));
        assert!(!zone.contains(zone.high + 5.0));
        assert!(!zone.contains(zone.low - 5.0));

        let mut field = ScalarField::new(10, 100.0, WorldTopology::Finite);
        for index in 0..field.len() {
            field.set_at(index, index as f32 * 0.5 - 10.0);
        }
        let inside = zone.classify(&field).iter().filter(|b| **b).count();
        assert!(inside > 0, "some vertices must fall in the flood zone");
        assert!(inside < field.len(), "not everything can be in the flood zone");
        assert!(zone.coverage(&field) > 0.0 && zone.coverage(&field) < 1.0);
    }

    #[test]
    fn coastal_erosion_flattens_the_flood_band_monotonically() {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, (x as f32 - 2.0) * 1.4);
            }
        }
        let zone = FloodZone::new(-1.0, 3.0);
        let before = field.clone();
        coastal_erosion(&mut field, &zone, 0.8, None);

        for z in 0..16 {
            let mut previous = f32::NEG_INFINITY;
            for x in 0..16 {
                let value = field.get(x, z);
                let original = before.get(x, z);
                if zone.contains(original) {
                    assert!(value >= original - 1e-4, "erosion must not sink flood vertices");
                }
                assert!(
                    value >= previous - 1e-3,
                    "the shore must rise monotonically inland at ({x},{z})"
                );
                previous = value;
            }
        }
        assert!(
            field.mean() > before.mean(),
            "the flood band should be smoothed up toward the beach"
        );
    }

    fn shore_ramp() -> ScalarField {
        let mut field = ScalarField::new(16, 160.0, WorldTopology::Finite);
        for z in 0..16 {
            for x in 0..16 {
                field.set(x, z, (x as f32 - 2.0) * 1.4);
            }
        }
        field
    }

    #[test]
    fn hard_coastal_bedrock_resists_beach_flattening() {
        let zone = FloodZone::new(-1.0, 3.0);
        let mut soft = shore_ramp();
        let mut hard = shore_ramp();
        let mut unmasked = shore_ramp();
        let soft_hardness = {
            let mut f = ScalarField::new(16, 160.0, WorldTopology::Finite);
            f.fill(0.1);
            f
        };
        let hard_hardness = {
            let mut f = ScalarField::new(16, 160.0, WorldTopology::Finite);
            f.fill(0.9);
            f
        };
        coastal_erosion(&mut soft, &zone, 0.8, Some(&soft_hardness));
        coastal_erosion(&mut hard, &zone, 0.8, Some(&hard_hardness));
        coastal_erosion(&mut unmasked, &zone, 0.8, None);

        let change = |field: &ScalarField| -> f32 {
            field
                .values
                .iter()
                .zip(shore_ramp().values.iter())
                .map(|(a, b)| (a - b).abs())
                .sum()
        };
        let soft_change = change(&soft);
        let hard_change = change(&hard);
        assert!(soft_change > 1.0, "soft shores must be flattened into beaches");
        assert!(
            hard_change < soft_change * 0.25,
            "hard bedrock cliffs must keep their steepness: {hard_change} vs {soft_change}"
        );
        let unmasked_change = change(&unmasked);
        assert!(
            unmasked_change > hard_change,
            "unmasked erosion must still flatten what hard rock resists"
        );
    }

    #[test]
    fn hard_shore_substrates_keep_rock_instead_of_wet_sand() {
        let zone = FloodZone::new(-2.0, 2.0);
        assert_eq!(
            shore_override_with_hardness(
                Season::Summer, &zone, -0.5, Salinity::Marine, Precipitation::Low, 0.0, 0.9,
            ),
            None,
            "crystalline sea cliffs stay bedrock, never wet sand"
        );
        assert_eq!(
            shore_override_with_hardness(
                Season::Winter, &zone, -0.5, Salinity::Marine, Precipitation::Low, 0.0, 0.9,
            ),
            Some(SurfaceLayer::Snow),
            "hard shores still ice over in winter"
        );
        assert_eq!(
            shore_override_with_hardness(
                Season::Summer, &zone, -0.5, Salinity::Hypersaline, Precipitation::Low, 0.0, 0.72,
            ),
            Some(SurfaceLayer::Gravel),
            "hypersaline hard shores get salt pan gravel, not mudflat"
        );
        assert_eq!(
            shore_override_with_hardness(
                Season::Summer, &zone, -0.5, Salinity::Marine, Precipitation::Low, 0.0, 0.1,
            ),
            Some(SurfaceLayer::Sand),
            "sandy shores still become wet sand"
        );
        assert_eq!(
            shore_override_with_hardness(
                Season::Summer, &zone, 9.0, Salinity::Marine, Precipitation::Low, 0.0, 0.9,
            ),
            None,
            "outside the flood zone there is never an override"
        );
    }

    #[test]
    fn shore_material_overrides_follow_season_and_salinity() {
        let zone = FloodZone::new(-2.0, 2.0);
        assert_eq!(
            shore_override_with_hardness(
                Season::Winter,
                &zone,
                0.0,
                Salinity::Marine,
                Precipitation::Low,
                0.0,
                0.0
            ),
            Some(SurfaceLayer::Snow)
        );
        assert_eq!(
            shore_override_with_hardness(
                Season::Summer,
                &zone,
                -0.5,
                Salinity::Marine,
                Precipitation::Low,
                0.0,
                0.0
            ),
            Some(SurfaceLayer::Sand)
        );
        assert_eq!(
            shore_override_with_hardness(
                Season::Summer,
                &zone,
                -0.5,
                Salinity::Hypersaline,
                Precipitation::Low,
                0.0,
                0.0
            ),
            Some(SurfaceLayer::Gravel)
        );
        assert_eq!(
            shore_override_with_hardness(
                Season::Summer,
                &zone,
                9.0,
                Salinity::Marine,
                Precipitation::Low,
                0.0,
                0.0
            ),
            None,
            "outside the flood zone there is no override"
        );
    }

    #[test]
    fn freezing_and_snow_coverage_track_season_and_altitude() {
        assert!(is_water_frozen(
            Season::Winter,
            Temperature::Temperate,
            Salinity::Freshwater,
            Geothermal::None
        ));
        assert!(!is_water_frozen(
            Season::Summer,
            Temperature::Warm,
            Salinity::Freshwater,
            Geothermal::None
        ));
        assert!(is_water_frozen(
            Season::Summer,
            Temperature::Freezing,
            Salinity::Freshwater,
            Geothermal::None
        ));

        assert!(snow_coverage(Season::Winter, Temperature::Cold, 0.0, 50.0) > 0.9);
        assert!(snow_coverage(Season::Summer, Temperature::Warm, 0.0, 50.0) < 0.01);
        assert!(
            snow_coverage(Season::Summer, Temperature::Warm, 200.0, 50.0) > 0.9,
            "high peaks keep snow in summer"
        );
        for season in Season::ALL {
            let coverage = snow_coverage(season, Temperature::Cold, 10.0, 50.0);
            assert!((0.0..=1.0).contains(&coverage));
        }
    }

    #[test]
    fn open_sea_stays_liquid_through_mild_seasons() {
        assert!(!is_water_frozen(
            Season::Summer,
            Temperature::Temperate,
            Salinity::Marine,
            Geothermal::None
        ));
        assert!(!is_water_frozen(
            Season::Winter,
            Temperature::Cold,
            Salinity::Marine,
            Geothermal::None
        ));
        assert!(
            is_water_frozen(
                Season::Winter,
                Temperature::Freezing,
                Salinity::Marine,
                Geothermal::None
            ),
            "marine water only freezes at true freezing temperatures"
        );
    }

    #[test]
    fn shallow_inland_pool_freezes_in_winter_and_thaws_in_summer() {
        assert!(
            is_water_frozen(
                Season::Winter,
                Temperature::Freezing,
                Salinity::Freshwater,
                Geothermal::None
            ),
            "a freezing inland pool must ice over in winter"
        );
        assert!(is_water_frozen(
            Season::Winter,
            Temperature::Temperate,
            Salinity::Freshwater,
            Geothermal::None
        ));
        assert!(
            !is_water_frozen(
                Season::Summer,
                Temperature::Temperate,
                Salinity::Freshwater,
                Geothermal::None
            ),
            "the same pool must be liquid in summer"
        );
        assert!(
            is_water_frozen(
                Season::Summer,
                Temperature::Cold,
                Salinity::Freshwater,
                Geothermal::None
            ),
            "freshwater freezes at cold temperatures even outside winter"
        );
    }

    #[test]
    fn geothermal_heat_keeps_water_liquid() {
        for season in Season::ALL {
            assert!(
                !is_water_frozen(
                    season,
                    Temperature::Freezing,
                    Salinity::Freshwater,
                    Geothermal::Geothermal
                ),
                "geothermal cells never freeze in {season:?}"
            );
            assert!(
                !is_water_frozen(
                    season,
                    Temperature::Freezing,
                    Salinity::Marine,
                    Geothermal::ActiveVolcanic
                ),
                "active volcanic cells never freeze in {season:?}"
            );
        }
    }

    #[test]
    fn salinity_shifts_the_freezing_point() {
        assert!(
            is_water_frozen(
                Season::Winter,
                Temperature::Cold,
                Salinity::Brackish,
                Geothermal::None
            ),
            "brackish water freezes at cold in winter"
        );
        assert!(
            !is_water_frozen(
                Season::Summer,
                Temperature::Cold,
                Salinity::Brackish,
                Geothermal::None
            ),
            "brackish water survives a cold summer"
        );
        assert!(!is_water_frozen(
            Season::Summer,
            Temperature::Freezing,
            Salinity::Hypersaline,
            Geothermal::None
        ));
        assert!(is_water_frozen(
            Season::Winter,
            Temperature::Freezing,
            Salinity::Hypersaline,
            Geothermal::None
        ));
    }

    #[test]
    fn ground_tint_lightens_with_snow() {
        let base = Color::srgb(0.2, 0.4, 0.2);
        assert_eq!(ground_tint(base, 0.0), base);
        let snowy = ground_tint(base, 1.0);
        assert!(snowy.to_linear().green > base.to_linear().green);
    }

    #[test]
    fn seasonal_wind_shifts_direction_and_strength() {
        let base = Vec2::X;
        let summer = prevailing_wind_for_season(base, Season::Summer);
        let winter = prevailing_wind_for_season(base, Season::Winter);
        assert!(summer.length() < 1.0);
        assert!(winter.length() > summer.length(), "winter wind must be stronger");
        assert!(winter.dot(base).abs() < 1e-4, "winter wind turns across the prevailing axis");
    }
}
