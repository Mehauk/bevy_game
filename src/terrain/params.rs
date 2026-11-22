use bevy::prelude::Vec2;

use crate::terrain::seed::{WorldSeed, STAGE_PARAMETERS};

macro_rules! param_enum {
    ($name:ident, $display_name:expr, [$($variant:ident => $label:literal),+ $(,)?]) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(usize)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            #[allow(dead_code)]
            pub const COUNT: usize = [$(Self::$variant),+].len();

            pub const fn ordinal(self) -> usize {
                self as usize
            }

            pub fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label),+
                }
            }
        }

        impl $crate::terrain::params::ParamLevel for $name {
            const ALL: &'static [Self] = &[$(Self::$variant),+];

            fn index(self) -> usize {
                self.ordinal()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str($name::label(*self))
            }
        }
    };
}

pub trait ParamLevel: Copy + Eq + std::fmt::Debug + 'static {
    const ALL: &'static [Self];

    fn index(self) -> usize;

    fn from_index(index: usize) -> Self {
        Self::ALL[index.min(Self::ALL.len() - 1)]
    }

    fn shift(self, delta: i32) -> Self {
        let last = Self::ALL.len() as i32 - 1;
        let raw = (self.index() as i32 + delta).clamp(0, last);
        Self::ALL[raw as usize]
    }

    fn distance(self, other: Self) -> i32 {
        (self.index() as i32 - other.index() as i32).abs()
    }
}

param_enum!(Temperature, "temperature", [
    Scorching => "scorching",
    Hot => "hot",
    Warm => "warm",
    Temperate => "temperate",
    Chilly => "chilly",
    Cold => "cold",
    Freezing => "freezing",
]);

param_enum!(Humidity, "humidity", [
    Saturated => "saturated",
    Humid => "humid",
    ModerateHumidity => "moderate humidity",
    Dry => "dry",
    Arid => "arid",
]);

param_enum!(Wind, "wind", [
    Gale => "gale",
    Windy => "windy",
    ModerateWindy => "moderate windy",
    NotWindy => "not windy",
]);

param_enum!(SunExposure, "sun exposure", [
    HarshSun => "harsh sun",
    ExposedSun => "exposed sun",
    ModerateSun => "moderate sun",
    LowSun => "low sun",
    NoSun => "no sun",
]);

param_enum!(Precipitation, "precipitation", [
    Torrential => "torrential precipitation",
    Heavy => "heavy precipitation",
    Moderate => "moderate precipitation",
    Low => "low precipitation",
    None => "no precipitation",
]);

param_enum!(BaseHeightBand, "base height", [
    Abyssal => "abyssal",
    DeepOcean => "deep ocean",
    CoastalShelf => "coastal shelf",
    SeaLevel => "sea level",
    Lowland => "lowland",
    Highland => "highland",
    Alpine => "alpine",
    Peak => "peak",
]);

param_enum!(SlopeMagnitude, "slope", [
    Flat => "flat",
    Gentle => "gentle",
    Moderate => "moderate",
    Steep => "steep",
    Sheer => "sheer",
]);

param_enum!(Hydrology, "hydrology", [
    Dry => "dry",
    WellDrained => "well drained",
    PoorlyDrained => "poorly drained",
    FluvialChannel => "fluvial / channel",
    FloodedBasin => "flooded / basin",
]);

param_enum!(Substrate, "substrate", [
    Sand => "sand",
    SiltClay => "silt / clay",
    LimestoneKarst => "limestone / karst",
    RockCrystalline => "rock / crystalline",
    Volcanic => "volcanic",
]);

impl Substrate {
    pub fn hardness(self) -> f32 {
        match self {
            Substrate::Sand => 0.10,
            Substrate::SiltClay => 0.25,
            Substrate::LimestoneKarst => 0.60,
            Substrate::Volcanic => 0.72,
            Substrate::RockCrystalline => 0.90,
        }
    }

    pub fn erodibility(self) -> f32 {
        1.0 - self.hardness()
    }
}

param_enum!(Geothermal, "geothermal", [
    None => "none",
    Geothermal => "geothermal",
    ActiveVolcanic => "active volcanic",
]);

param_enum!(Salinity, "salinity", [
    Freshwater => "freshwater",
    Brackish => "brackish",
    Marine => "marine",
    Hypersaline => "hypersaline",
]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamRange<T: ParamLevel> {
    pub min: T,
    pub max: T,
}

impl<T: ParamLevel> ParamRange<T> {
    #[allow(dead_code)]
    pub fn new(first: T, second: T) -> Self {
        let (lo, hi) = if first.index() <= second.index() {
            (first, second)
        } else {
            (second, first)
        };
        Self { min: lo, max: hi }
    }

    #[allow(dead_code)]
    pub fn exact(value: T) -> Self {
        Self { min: value, max: value }
    }

    #[allow(dead_code)]
    pub fn full() -> Self {
        Self {
            min: T::ALL[0],
            max: T::ALL[T::ALL.len() - 1],
        }
    }

    pub fn contains(self, value: T) -> bool {
        value.index() >= self.min.index() && value.index() <= self.max.index()
    }

    pub fn distance(self, value: T) -> i32 {
        if value.index() < self.min.index() {
            self.min.index() as i32 - value.index() as i32
        } else if value.index() > self.max.index() {
            value.index() as i32 - self.max.index() as i32
        } else {
            0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BaseGradient {
    pub slope: SlopeMagnitude,
    pub aspect: Vec2,
}

impl Default for BaseGradient {
    fn default() -> Self {
        Self {
            slope: SlopeMagnitude::Flat,
            aspect: Vec2::X,
        }
    }
}

impl BaseGradient {
    pub fn slope_tan(slope: SlopeMagnitude) -> f32 {
        match slope {
            SlopeMagnitude::Flat => 0.0,
            SlopeMagnitude::Gentle => 0.035,
            SlopeMagnitude::Moderate => 0.105,
            SlopeMagnitude::Steep => 0.249,
            SlopeMagnitude::Sheer => 0.577,
        }
    }

    #[cfg(test)]
    pub fn slope_angle_deg(slope: SlopeMagnitude) -> f32 {
        match slope {
            SlopeMagnitude::Flat => 0.0,
            SlopeMagnitude::Gentle => 2.0,
            SlopeMagnitude::Moderate => 6.0,
            SlopeMagnitude::Steep => 14.0,
            SlopeMagnitude::Sheer => 30.0,
        }
    }

    #[cfg(test)]
    pub fn tilt_at(&self, delta: Vec2) -> f32 {
        delta.dot(self.aspect) * Self::slope_tan(self.slope)
    }

    #[cfg(test)]
    pub fn is_flat(&self) -> bool {
        self.slope == SlopeMagnitude::Flat
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellParameters {
    pub temperature: Temperature,
    pub humidity: Humidity,
    pub wind: Wind,
    pub sun_exposure: SunExposure,
    pub precipitation: Precipitation,
    pub base_height: BaseHeightBand,
    pub base_gradient: BaseGradient,
    pub hydrology: Hydrology,
    pub substrate: Substrate,
    pub geothermal: Geothermal,
    pub salinity: Salinity,
}

impl CellParameters {
    pub fn is_arid(&self) -> bool {
        self.precipitation.index() >= Precipitation::Low.index()
            && self.humidity.index() >= Humidity::Dry.index()
    }

    #[allow(dead_code)]
    pub fn describe(&self) -> String {
        format!(
            "temp={} humidity={} wind={} sun={} precip={} height={} slope={} hydro={} substrate={} geothermal={} salinity={}",
            self.temperature.label(),
            self.humidity.label(),
            self.wind.label(),
            self.sun_exposure.label(),
            self.precipitation.label(),
            self.base_height.label(),
            self.base_gradient.slope.label(),
            self.hydrology.label(),
            self.substrate.label(),
            self.geothermal.label(),
            self.salinity.label()
        )
    }

    #[cfg(test)]
    pub fn step_distance(&self, other: &CellParameters) -> i32 {
        self.temperature.distance(other.temperature)
            + self.humidity.distance(other.humidity)
            + self.wind.distance(other.wind)
            + self.sun_exposure.distance(other.sun_exposure)
            + self.precipitation.distance(other.precipitation)
            + self.base_height.distance(other.base_height)
            + self.hydrology.distance(other.hydrology)
            + self.substrate.distance(other.substrate)
            + self.geothermal.distance(other.geothermal)
            + self.salinity.distance(other.salinity)
    }

    #[cfg(test)]
    pub fn max_single_step(&self, other: &CellParameters) -> i32 {
        [
            self.temperature.distance(other.temperature),
            self.humidity.distance(other.humidity),
            self.wind.distance(other.wind),
            self.sun_exposure.distance(other.sun_exposure),
            self.precipitation.distance(other.precipitation),
            self.base_height.distance(other.base_height),
            self.hydrology.distance(other.hydrology),
            self.substrate.distance(other.substrate),
            self.geothermal.distance(other.geothermal),
            self.salinity.distance(other.salinity),
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    #[cfg(test)]
    pub fn is_submerged_band(&self) -> bool {
        self.base_height.index() <= BaseHeightBand::CoastalShelf.index()
    }

    #[cfg(test)]
    pub fn is_coastal_band(&self) -> bool {
        self.base_height == BaseHeightBand::CoastalShelf || self.base_height == BaseHeightBand::SeaLevel
    }

    #[cfg(test)]
    pub fn is_high_elevation(&self) -> bool {
        self.base_height.index() >= BaseHeightBand::Highland.index()
    }

    #[cfg(test)]
    pub fn is_frozen(&self) -> bool {
        self.temperature == Temperature::Freezing
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroContext {
    pub band: BaseHeightBand,
    pub latitude: f32,
    pub prevailing_wind: Vec2,
    pub rain_shadow: f32,
    pub geothermal_intensity: f32,
    pub sea_proximity: f32,
    pub downslope: Vec2,
    pub slope_magnitude: f32,
}

impl Default for MacroContext {
    fn default() -> Self {
        Self {
            band: BaseHeightBand::Lowland,
            latitude: 0.4,
            prevailing_wind: Vec2::X,
            rain_shadow: 0.0,
            geothermal_intensity: 0.0,
            sea_proximity: 0.5,
            downslope: Vec2::X,
            slope_magnitude: 0.3,
        }
    }
}

pub fn temperature_at_latitude(latitude: f32) -> Temperature {
    let last = Temperature::COUNT as f32 - 1.0;
    Temperature::from_index((latitude.clamp(0.0, 1.0) * last).round() as usize)
}

pub fn precipitation_at_latitude(latitude: f32) -> Precipitation {
    let lat = latitude.clamp(0.0, 1.0);
    let wetness = (1.0 - (lat - 0.15).abs() * 1.15 - 0.15 * (lat - 0.6).abs()).clamp(0.0, 1.0);
    let last = Precipitation::COUNT as f32 - 1.0;
    Precipitation::from_index(((1.0 - wetness) * last).round() as usize)
}

pub fn elevation_cooling(band: BaseHeightBand) -> i32 {
    match band {
        BaseHeightBand::Abyssal | BaseHeightBand::DeepOcean => 1,
        BaseHeightBand::CoastalShelf | BaseHeightBand::SeaLevel | BaseHeightBand::Lowland => 0,
        BaseHeightBand::Highland => 1,
        BaseHeightBand::Alpine => 2,
        BaseHeightBand::Peak => 3,
    }
}

pub fn geothermal_warming(geothermal: Geothermal) -> i32 {
    match geothermal {
        Geothermal::None => 0,
        Geothermal::Geothermal => 1,
        Geothermal::ActiveVolcanic => 2,
    }
}

pub fn roll_geothermal(seed: WorldSeed, cell_index: u64, intensity: f32) -> Geothermal {
    if intensity <= 0.0 {
        return Geothermal::None;
    }
    let roll = seed.unit(STAGE_PARAMETERS, cell_index, 40);
    if roll < intensity * 0.25 {
        Geothermal::ActiveVolcanic
    } else if roll < intensity * 0.7 {
        Geothermal::Geothermal
    } else {
        Geothermal::None
    }
}

pub fn roll_salinity(
    seed: WorldSeed,
    cell_index: u64,
    band: BaseHeightBand,
    hydrology: Hydrology,
    arid: bool,
) -> Salinity {
    let roll = seed.unit(STAGE_PARAMETERS, cell_index, 41);
    match band {
        BaseHeightBand::Abyssal | BaseHeightBand::DeepOcean => Salinity::Marine,
        BaseHeightBand::CoastalShelf => {
            if roll < 0.25 {
                Salinity::Brackish
            } else {
                Salinity::Marine
            }
        }
        BaseHeightBand::SeaLevel => {
            if hydrology == Hydrology::FloodedBasin {
                if arid {
                    Salinity::Hypersaline
                } else if roll < 0.4 {
                    Salinity::Brackish
                } else {
                    Salinity::Freshwater
                }
            } else if roll < 0.2 {
                Salinity::Brackish
            } else {
                Salinity::Freshwater
            }
        }
        _ => {
            if hydrology == Hydrology::FloodedBasin && arid {
                Salinity::Hypersaline
            } else {
                Salinity::Freshwater
            }
        }
    }
}

pub fn roll_substrate(
    seed: WorldSeed,
    cell_index: u64,
    band: BaseHeightBand,
    geothermal: Geothermal,
) -> Substrate {
    if geothermal == Geothermal::ActiveVolcanic {
        return Substrate::Volcanic;
    }
    let roll = seed.unit(STAGE_PARAMETERS, cell_index, 42);
    match band {
        BaseHeightBand::Abyssal | BaseHeightBand::DeepOcean => {
            if roll < 0.7 {
                Substrate::SiltClay
            } else {
                Substrate::RockCrystalline
            }
        }
        BaseHeightBand::CoastalShelf => {
            if roll < 0.6 {
                Substrate::Sand
            } else if roll < 0.85 {
                Substrate::SiltClay
            } else {
                Substrate::LimestoneKarst
            }
        }
        BaseHeightBand::SeaLevel | BaseHeightBand::Lowland => {
            if roll < 0.35 {
                Substrate::SiltClay
            } else if roll < 0.6 {
                Substrate::Sand
            } else if roll < 0.8 {
                Substrate::LimestoneKarst
            } else {
                Substrate::RockCrystalline
            }
        }
        BaseHeightBand::Highland => {
            if roll < 0.5 {
                Substrate::RockCrystalline
            } else if roll < 0.75 {
                Substrate::LimestoneKarst
            } else {
                Substrate::SiltClay
            }
        }
        BaseHeightBand::Alpine | BaseHeightBand::Peak => {
            if roll < 0.75 {
                Substrate::RockCrystalline
            } else {
                Substrate::Sand
            }
        }
    }
}

pub fn slope_from_relief(relief_slope: f32) -> SlopeMagnitude {
    if relief_slope < 0.15 {
        SlopeMagnitude::Flat
    } else if relief_slope < 0.40 {
        SlopeMagnitude::Gentle
    } else if relief_slope < 0.80 {
        SlopeMagnitude::Moderate
    } else if relief_slope < 1.40 {
        SlopeMagnitude::Steep
    } else {
        SlopeMagnitude::Sheer
    }
}

pub fn roll_gradient(
    seed: WorldSeed,
    cell_index: u64,
    ctx: &MacroContext,
    substrate: Substrate,
) -> BaseGradient {
    let mut aspect = ctx.downslope;
    if aspect.length_squared() < 1e-6 {
        aspect = seed.direction(STAGE_PARAMETERS, cell_index, 5);
    }
    let jitter_angle = (seed.unit(STAGE_PARAMETERS, cell_index, 6) - 0.5) * 0.7;
    let (s, c) = jitter_angle.sin_cos();
    aspect = Vec2::new(aspect.x * c - aspect.y * s, aspect.x * s + aspect.y * c)
        .normalize_or_zero();
    if aspect.length_squared() < 1e-6 {
        aspect = Vec2::X;
    }
    let hardness = match substrate {
        s if s.hardness() < 0.2 => -1,
        s if s.hardness() < 0.7 => 0,
        _ => 1,
    };
    let drift = ((seed.unit(STAGE_PARAMETERS, cell_index, 43) * 3.0).floor() as i32) - 1;
    let slope = slope_from_relief(ctx.slope_magnitude).shift(hardness + drift);
    BaseGradient { slope, aspect }
}

pub fn roll_hydrology(
    seed: WorldSeed,
    cell_index: u64,
    band: BaseHeightBand,
    precipitation: Precipitation,
    lowland_bias: bool,
) -> Hydrology {
    if band.index() <= BaseHeightBand::CoastalShelf.index() {
        return Hydrology::FloodedBasin;
    }
    let roll = seed.unit(STAGE_PARAMETERS, cell_index, 44);
    let wetness = Precipitation::COUNT - 1 - precipitation.index();
    if wetness >= 3 {
        if roll < 0.4 {
            return Hydrology::PoorlyDrained;
        }
        return Hydrology::WellDrained;
    }
    if wetness <= 1 {
        return Hydrology::Dry;
    }
    if lowland_bias && roll < 0.3 {
        return Hydrology::FluvialChannel;
    }
    if roll < 0.15 {
        return Hydrology::PoorlyDrained;
    }
    Hydrology::WellDrained
}

fn relax_level<T: ParamLevel>(current: T, neighbour_mean: f32) -> T {
    let delta = neighbour_mean.round() as i32 - current.index() as i32;
    current.shift(delta.clamp(-1, 1))
}

fn mean_index<T: ParamLevel, F>(
    neighbours: &[usize],
    params: &[CellParameters],
    read: F,
) -> Option<f32>
where
    F: Fn(&CellParameters) -> T,
{
    if neighbours.is_empty() {
        return None;
    }
    let mut sum = 0usize;
    let mut count = 0usize;
    for &n in neighbours {
        let Some(other) = params.get(n) else {
            continue;
        };
        sum += read(other).index();
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some(sum as f32 / count as f32)
    }
}

fn majority_level<T: ParamLevel, F>(
    neighbours: &[usize],
    params: &[CellParameters],
    read: F,
    current: T,
) -> Option<T>
where
    F: Fn(&CellParameters) -> T,
{
    let mut counts = vec![0usize; T::ALL.len()];
    let mut total = 0usize;
    for &n in neighbours {
        let Some(other) = params.get(n) else {
            continue;
        };
        counts[read(other).index()] += 1;
        total += 1;
    }
    if total == 0 {
        return None;
    }
    let mut best = current.index();
    let mut best_count = counts[best.min(counts.len() - 1)];
    for (index, count) in counts.iter().enumerate() {
        if *count * 2 > total && *count > best_count {
            best = index;
            best_count = *count;
        }
    }
    Some(T::from_index(best))
}

pub fn relax_parameters(
    seed: WorldSeed,
    params: &mut [CellParameters],
    neighbours: &[Vec<usize>],
    iterations: usize,
) {
    for _pass in 0..iterations {
        let mut next = params.to_vec();
        for index in 0..params.len() {
            let empty = Vec::new();
            let neighbour_list = neighbours.get(index).unwrap_or(&empty);
            if neighbour_list.is_empty() {
                continue;
            }
            let current = params[index];
            if let Some(mean) = mean_index(neighbour_list, params, |p| p.temperature) {
                next[index].temperature = relax_level(current.temperature, mean);
            }
            if let Some(mean) = mean_index(neighbour_list, params, |p| p.humidity) {
                next[index].humidity = relax_level(current.humidity, mean);
            }
            if let Some(mean) = mean_index(neighbour_list, params, |p| p.precipitation) {
                next[index].precipitation = relax_level(current.precipitation, mean);
            }
            if let Some(mean) = mean_index(neighbour_list, params, |p| p.wind) {
                next[index].wind = relax_level(current.wind, mean);
            }
            if let Some(mean) = mean_index(neighbour_list, params, |p| p.sun_exposure) {
                next[index].sun_exposure = relax_level(current.sun_exposure, mean);
            }
            if let Some(mode) = majority_level(neighbour_list, params, |p| p.substrate, current.substrate) {
                next[index].substrate = mode;
            }
            if let Some(mode) = majority_level(neighbour_list, params, |p| p.hydrology, current.hydrology) {
                next[index].hydrology = mode;
            }
            let arid_now = next[index].precipitation.index() >= Precipitation::Low.index();
            let arid_before = current.precipitation.index() >= Precipitation::Low.index();
            if next[index].hydrology != current.hydrology || arid_now != arid_before {
                next[index].salinity = roll_salinity(
                    seed,
                    index as u64,
                    current.base_height,
                    next[index].hydrology,
                    arid_now,
                );
            }
        }
        params.copy_from_slice(&next);
    }
}

pub fn roll_cell_parameters(
    seed: WorldSeed,
    cell_index: u64,
    ctx: &MacroContext,
) -> CellParameters {
    let jitter = |channel: u64| {
        let step = seed.unit(STAGE_PARAMETERS, cell_index, channel) * 3.0;
        step.floor() as i32 - 1
    };

    let geothermal = roll_geothermal(seed, cell_index, ctx.geothermal_intensity);

    let temperature = temperature_at_latitude(ctx.latitude)
        .shift(elevation_cooling(ctx.band))
        .shift(geothermal_warming(geothermal))
        .shift(jitter(0));

    let wetness_bias = (ctx.sea_proximity * 2.0).round() as i32
        - (ctx.rain_shadow * 2.0).round() as i32;
    let precipitation = precipitation_at_latitude(ctx.latitude)
        .shift((ctx.rain_shadow * 2.0).round() as i32)
        .shift(-(ctx.sea_proximity * 1.5).round() as i32)
        .shift(jitter(1));
    let humidity = Humidity::from_index(precipitation.index())
        .shift(-wetness_bias)
        .shift(jitter(2));

    let exposure_bonus = match ctx.band {
        BaseHeightBand::CoastalShelf | BaseHeightBand::SeaLevel | BaseHeightBand::Highland => 1,
        BaseHeightBand::Alpine | BaseHeightBand::Peak => 2,
        _ => 0,
    };
    let coast_exposure = (ctx.sea_proximity * 2.0).round() as i32;
    let windward = ctx.prevailing_wind.dot(-ctx.downslope).clamp(-1.0, 1.0);
    let windward_bonus = (windward * 2.0).round() as i32;
    let wind_strength = exposure_bonus as i32 + coast_exposure + windward_bonus + jitter(3);
    let wind = Wind::from_index(
        ((Wind::COUNT as i32 - 1) - wind_strength).clamp(0, Wind::COUNT as i32 - 1) as usize,
    );

    let humid_sky = (humidity.index() > Humidity::ModerateHumidity.index()) as usize;
    let rain_shaded = (ctx.rain_shadow > 0.5) as usize;
    let sunward = (-ctx.downslope.y).clamp(-1.0, 1.0);
    let aspect_shift = sunward.round() as i32;
    let sun_index = (ctx.latitude * SunExposure::COUNT as f32).round() as i32
        + humid_sky as i32
        - rain_shaded as i32
        - aspect_shift;
    let sun_index = sun_index.clamp(0, SunExposure::COUNT as i32 - 1) as usize;

    let substrate = roll_substrate(seed, cell_index, ctx.band, geothermal);
    let lowland = ctx.band == BaseHeightBand::Lowland;
    let hydrology = roll_hydrology(seed, cell_index, ctx.band, precipitation, lowland);
    let salinity = roll_salinity(
        seed,
        cell_index,
        ctx.band,
        hydrology,
        precipitation.index() >= Precipitation::Low.index(),
    );

    let gradient = roll_gradient(seed, cell_index, ctx, substrate);

    CellParameters {
        temperature,
        humidity,
        wind,
        sun_exposure: SunExposure::from_index(sun_index),
        precipitation,
        base_height: ctx.band,
        base_gradient: gradient,
        hydrology,
        substrate,
        geothermal,
        salinity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_sizes_match_the_design_document() {
        assert_eq!(Temperature::COUNT, 7);
        assert_eq!(Humidity::COUNT, 5);
        assert_eq!(Wind::COUNT, 4);
        assert_eq!(SunExposure::COUNT, 5);
        assert_eq!(Precipitation::COUNT, 5);
        assert_eq!(BaseHeightBand::COUNT, 8);
        assert_eq!(SlopeMagnitude::COUNT, 5);
        assert_eq!(Hydrology::COUNT, 5);
        assert_eq!(Substrate::COUNT, 5);
        assert_eq!(Geothermal::COUNT, 3);
        assert_eq!(Salinity::COUNT, 4);
    }

    #[test]
    fn substrate_hardness_is_ordered_and_bounded() {
        let order = [
            Substrate::Sand,
            Substrate::SiltClay,
            Substrate::LimestoneKarst,
            Substrate::Volcanic,
            Substrate::RockCrystalline,
        ];
        for pair in order.windows(2) {
            assert!(
                pair[0].hardness() < pair[1].hardness(),
                "{:?} must be softer than {:?}",
                pair[0],
                pair[1]
            );
        }
        for substrate in Substrate::ALL {
            assert!(
                (0.0..=1.0).contains(&substrate.hardness()),
                "{substrate:?} hardness out of [0, 1]"
            );
            assert!(
                (substrate.erodibility() - (1.0 - substrate.hardness())).abs() < 1e-6,
                "{substrate:?} erodibility must be the complement of hardness"
            );
        }
        assert!(Substrate::Sand.hardness() < 0.2);
        assert!(Substrate::RockCrystalline.hardness() > 0.8);
    }

    #[test]
    fn harder_substrates_roll_steeper_base_slopes() {
        let seed = WorldSeed::new(9);
        let ctx = MacroContext {
            slope_magnitude: 0.5,
            ..Default::default()
        };
        let sand = roll_gradient(seed, 3, &ctx, Substrate::Sand);
        let rock = roll_gradient(seed, 3, &ctx, Substrate::RockCrystalline);
        assert!(
            rock.slope.index() > sand.slope.index(),
            "hard rock must hold steeper base slopes than sand"
        );
    }

    #[test]
    fn index_round_trips_for_every_level() {
        for level in 0..Temperature::COUNT {
            let t = Temperature::from_index(level);
            assert_eq!(t.index(), level);
            assert!(!t.label().is_empty());
        }
        for band in BaseHeightBand::ALL {
            assert_eq!(BaseHeightBand::from_index(band.index()), *band);
        }
    }

    #[test]
    fn from_index_clamps_instead_of_panicking() {
        assert_eq!(Temperature::from_index(999), Temperature::Freezing);
        assert_eq!(Temperature::from_index(usize::MAX), Temperature::Freezing);
        assert_eq!(Salinity::from_index(usize::MAX), Salinity::Hypersaline);
    }

    #[test]
    fn shift_clamps_at_both_ends() {
        assert_eq!(Temperature::Scorching.shift(-5), Temperature::Scorching);
        assert_eq!(Temperature::Freezing.shift(5), Temperature::Freezing);
        assert_eq!(Temperature::Warm.shift(1), Temperature::Temperate);
        assert_eq!(Temperature::Warm.shift(-1), Temperature::Hot);
    }

    #[test]
    fn display_uses_the_document_labels() {
        assert_eq!(Temperature::Freezing.to_string(), "freezing");
        assert_eq!(Hydrology::FluvialChannel.to_string(), "fluvial / channel");
        assert_eq!(Precipitation::None.to_string(), "no precipitation");
    }

    #[test]
    fn ranges_are_order_independent() {
        let a = ParamRange::new(Temperature::Cold, Temperature::Hot);
        let b = ParamRange::new(Temperature::Hot, Temperature::Cold);
        assert_eq!(a, b);
        assert!(a.contains(Temperature::Warm));
        assert!(!a.contains(Temperature::Scorching));
        assert_eq!(a.distance(Temperature::Scorching), 1);
        assert_eq!(a.distance(Temperature::Freezing), 1);
        assert_eq!(a.distance(Temperature::Temperate), 0);
    }

    #[test]
    fn full_range_contains_every_level() {
        for level in Temperature::ALL {
            assert!(ParamRange::<Temperature>::full().contains(*level));
        }
        assert_eq!(ParamRange::<Salinity>::exact(Salinity::Marine).distance(Salinity::Marine), 0);
    }

    #[test]
    fn gradient_tilt_follows_aspect_and_slope() {
        let flat = BaseGradient {
            slope: SlopeMagnitude::Flat,
            aspect: Vec2::X,
        };
        assert_eq!(flat.tilt_at(Vec2::new(10.0, 0.0)), 0.0);
        assert!(flat.is_flat());

        let steep = BaseGradient {
            slope: SlopeMagnitude::Steep,
            aspect: Vec2::X,
        };
        assert!(steep.tilt_at(Vec2::new(10.0, 0.0)) > 0.0, "down-aspect rises");
        assert!(steep.tilt_at(Vec2::new(-10.0, 0.0)) < 0.0, "up-aspect falls");
        assert!(steep.tilt_at(Vec2::new(0.0, 10.0)).abs() < 1e-6, "perpendicular motion does not tilt");
        assert!(
            BaseGradient::slope_tan(SlopeMagnitude::Sheer) > BaseGradient::slope_tan(SlopeMagnitude::Moderate)
        );
    }

    #[test]
    fn slope_angles_increase_monotonically() {
        let angles: Vec<f32> = SlopeMagnitude::ALL
            .iter()
            .map(|s| BaseGradient::slope_angle_deg(*s))
            .collect();
        for pair in angles.windows(2) {
            assert!(pair[1] > pair[0], "slope angles must increase: {angles:?}");
        }
    }

    #[test]
    fn latitude_curves_run_the_right_directions() {
        assert!(temperature_at_latitude(0.0).index() < temperature_at_latitude(1.0).index());
        assert_eq!(temperature_at_latitude(0.0), Temperature::Scorching);
        assert_eq!(temperature_at_latitude(1.0), Temperature::Freezing);
        assert!(
            precipitation_at_latitude(0.15).index() < precipitation_at_latitude(0.95).index(),
            "tropics must be wetter than the polar desert"
        );
    }

    #[test]
    fn elevation_cools_and_geothermal_warms() {
        assert!(elevation_cooling(BaseHeightBand::Peak) > elevation_cooling(BaseHeightBand::Lowland));
        assert_eq!(geothermal_warming(Geothermal::None), 0);
        assert!(geothermal_warming(Geothermal::ActiveVolcanic) > geothermal_warming(Geothermal::Geothermal));

        let peak_ctx = MacroContext {
            band: BaseHeightBand::Peak,
            latitude: 0.0,
            ..Default::default()
        };
        let vent_ctx = MacroContext {
            band: BaseHeightBand::Lowland,
            latitude: 0.0,
            geothermal_intensity: 1.0,
            ..Default::default()
        };
        let seed = WorldSeed::new(11);
        let peak = roll_cell_parameters(seed, 0, &peak_ctx);
        let vent = roll_cell_parameters(seed, 0, &vent_ctx);
        assert!(peak.temperature.index() > vent.temperature.index());
        assert_eq!(vent.substrate, Substrate::Volcanic, "active volcanism forces volcanic substrate");
    }

    #[test]
    fn salinity_respects_band_coupling() {
        let seed = WorldSeed::new(4);
        for cell in 0..32u64 {
            assert_eq!(
                roll_salinity(seed, cell, BaseHeightBand::DeepOcean, Hydrology::FloodedBasin, true),
                Salinity::Marine,
                "open ocean is always marine"
            );
            assert_eq!(
                roll_salinity(seed, cell, BaseHeightBand::Lowland, Hydrology::WellDrained, false),
                Salinity::Freshwater,
                "drained inland is fresh"
            );
            assert_eq!(
                roll_salinity(seed, cell, BaseHeightBand::Lowland, Hydrology::FloodedBasin, true),
                Salinity::Hypersaline,
                "closed arid basins are hypersaline"
            );
        }
    }

    #[test]
    fn rolling_is_deterministic() {
        let seed = WorldSeed::new(77);
        let ctx = MacroContext::default();
        assert_eq!(roll_cell_parameters(seed, 3, &ctx), roll_cell_parameters(seed, 3, &ctx));
        assert_ne!(roll_cell_parameters(seed, 3, &ctx), roll_cell_parameters(seed, 4, &ctx));
    }

    #[test]
    fn rolling_covers_the_parameter_space() {
        let seed = WorldSeed::new(2024);
        let mut temps = std::collections::HashSet::new();
        let mut substrates = std::collections::HashSet::new();
        for cell in 0..400u64 {
            for band in BaseHeightBand::ALL {
                let ctx = MacroContext {
                    band: *band,
                    latitude: cell as f32 / 400.0,
                    sea_proximity: (cell % 5) as f32 / 4.0,
                    rain_shadow: (cell % 3) as f32 / 2.0,
                    geothermal_intensity: (cell % 4) as f32 / 3.0,
                    prevailing_wind: Vec2::X,
                    downslope: Vec2::new(((cell % 8) as f32 - 4.0) / 4.0, ((cell % 5) as f32 - 2.0) / 3.0),
                    slope_magnitude: (cell % 7) as f32 / 2.0,
                };
                let p = roll_cell_parameters(seed, cell, &ctx);
                assert_eq!(p.base_height, *band, "band must be honoured");
                assert!(p.base_gradient.aspect.length() < 2.0);
                temps.insert(p.temperature);
                substrates.insert(p.substrate);
            }
        }
        assert!(temps.len() > 4, "temperature must spread across latitudes");
        assert!(substrates.len() > 3, "substrate must vary");
    }

    #[test]
    fn rain_shadow_shifts_sun_exposure_toward_more_sun() {
        let seed = WorldSeed::new(404);
        let roll = |shadow: f32| -> usize {
            (0..200u64)
                .map(|cell| {
                    let ctx = MacroContext {
                        band: BaseHeightBand::Lowland,
                        latitude: (cell % 40) as f32 / 39.0,
                        sea_proximity: (cell % 5) as f32 / 4.0,
                        rain_shadow: shadow,
                        ..Default::default()
                    };
                    roll_cell_parameters(seed, cell, &ctx).sun_exposure.index()
                })
                .sum::<usize>()
        };
        let exposed = roll(0.0);
        let shaded = roll(1.0);
        assert!(
            shaded < exposed,
            "leeward rain-shadow cells must be sunnier, not cloudier: shaded {shaded} vs exposed {exposed}"
        );
    }

    #[test]
    fn categorical_relaxation_uses_majority_vote() {
        let seed = WorldSeed::new(91);
        let base = roll_cell_parameters(seed, 5, &MacroContext::default());
        let mut params = vec![base; 3];
        params[0].substrate = Substrate::Sand;
        params[1].substrate = Substrate::Volcanic;
        params[2].substrate = Substrate::Sand;
        params[0].hydrology = Hydrology::Dry;
        params[1].hydrology = Hydrology::FloodedBasin;
        params[2].hydrology = Hydrology::Dry;
        let neighbours = vec![vec![1usize], vec![0, 2], vec![1]];

        relax_parameters(seed, &mut params, &neighbours, 1);

        assert_eq!(
            params[1].substrate,
            Substrate::Sand,
            "the odd cell must adopt the neighbours' category, never an ordinal average"
        );
        assert_ne!(
            params[1].substrate,
            Substrate::LimestoneKarst,
            "categorical levels must not be averaged by ordinal index"
        );
        assert_eq!(params[1].hydrology, Hydrology::Dry);
    }

    #[test]
    fn categorical_relaxation_needs_a_strict_majority() {
        let seed = WorldSeed::new(77);
        let base = roll_cell_parameters(seed, 5, &MacroContext::default());
        let mut params = vec![base; 3];
        params[0].substrate = Substrate::Sand;
        params[1].substrate = Substrate::Volcanic;
        params[2].substrate = Substrate::RockCrystalline;
        let neighbours = vec![vec![1usize], vec![0, 2], vec![1]];

        relax_parameters(seed, &mut params, &neighbours, 1);

        assert_eq!(
            params[1].substrate,
            Substrate::Volcanic,
            "split neighbours (no strict majority) must leave the categorical level untouched"
        );
    }

    #[test]
    fn relaxation_reduces_neighbour_disagreement() {
        let seed = WorldSeed::new(31);
        let mut params: Vec<CellParameters> = (0..9u64)
            .map(|i| roll_cell_parameters(seed, i * 7919, &MacroContext::default()))
            .collect();
        let neighbours: Vec<Vec<usize>> = (0..9)
            .map(|i| (0..9).filter(|j| *j != i && (*j as i32 - i as i32).abs() == 1).collect())
            .collect();

        let disagreement = |p: &[CellParameters]| -> i32 {
            (0..9)
                .flat_map(|i| neighbours[i].iter().map(move |&j| (i, j)))
                .map(|(i, j)| p[i].max_single_step(&p[j]))
                .sum()
        };

        let before = disagreement(&params);
        relax_parameters(seed, &mut params, &neighbours, 8);
        let after = disagreement(&params);
        assert!(after <= before, "relaxation must not increase disagreement: {before} -> {after}");
    }

    #[test]
    fn relaxation_is_deterministic_and_idempotent_when_smooth() {
        let seed = WorldSeed::new(8);
        let build = || {
            let params: Vec<CellParameters> = (0..6u64)
                .map(|i| roll_cell_parameters(seed, i, &MacroContext::default()))
                .collect();
            let neighbours = vec![vec![1usize], vec![0, 2], vec![1, 3], vec![2, 4], vec![3, 5], vec![4]];
            (params, neighbours)
        };
        let (mut a, na) = build();
        let (mut b, nb) = build();
        relax_parameters(seed, &mut a, &na, 3);
        relax_parameters(seed, &mut b, &nb, 3);
        assert_eq!(a, b);

        let uniform = vec![roll_cell_parameters(seed, 1, &MacroContext::default()); 4];
        let mut settled = uniform.clone();
        relax_parameters(seed, &mut settled, &vec![vec![1, 2], vec![0, 3], vec![0, 3], vec![1, 2]], 4);
        assert_eq!(settled, uniform, "an already uniform field must not move");
    }

    #[test]
    fn wind_varies_with_coast_proximity_aspect_and_jitter() {
        let seed = WorldSeed::new(2026);
        let mut levels = std::collections::HashSet::new();
        let mut coast_sum = 0i64;
        let mut coast_count = 0i64;
        let mut inland_sum = 0i64;
        let mut inland_count = 0i64;
        for cell in 0..4000u64 {
            let coastal = cell % 2 == 0;
            let ctx = MacroContext {
                band: BaseHeightBand::ALL[(cell % 8) as usize],
                latitude: (cell % 11) as f32 / 10.0,
                sea_proximity: if coastal { 1.0 } else { 0.0 },
                prevailing_wind: Vec2::X,
                downslope: Vec2::new(
                    ((cell % 7) as f32 - 3.0) / 3.0,
                    ((cell % 9) as f32 - 4.0) / 4.0,
                ),
                slope_magnitude: (cell % 6) as f32 / 2.0,
                ..Default::default()
            };
            let p = roll_cell_parameters(seed, cell, &ctx);
            levels.insert(p.wind);
            if coastal {
                coast_sum += p.wind.index() as i64;
                coast_count += 1;
            } else {
                inland_sum += p.wind.index() as i64;
                inland_count += 1;
            }
        }
        assert!(
            levels.len() >= 4,
            "wind must take at least 4 distinct levels, got {levels:?}"
        );
        assert!(
            coast_sum * inland_count < inland_sum * coast_count,
            "wind must be stronger near coasts: coast mean {coast_sum}/{coast_count}, inland mean {inland_sum}/{inland_count}"
        );
    }

    #[test]
    fn sun_exposure_follows_slope_aspect() {
        let seed = WorldSeed::new(313);
        for cell in 0..200u64 {
            let base = MacroContext {
                band: BaseHeightBand::Lowland,
                latitude: (cell % 7) as f32 / 10.0,
                ..Default::default()
            };
            let facing = MacroContext {
                downslope: Vec2::NEG_Y,
                ..base
            };
            let shaded = MacroContext {
                downslope: Vec2::Y,
                ..base
            };
            let sun_facing = roll_cell_parameters(seed, cell, &facing);
            let sun_away = roll_cell_parameters(seed, cell, &shaded);
            assert_ne!(
                sun_facing.sun_exposure, sun_away.sun_exposure,
                "cell {cell}: identical cells differing only in aspect must not share sun exposure"
            );
            assert!(
                sun_facing.sun_exposure.index() < sun_away.sun_exposure.index(),
                "cell {cell}: the sun-facing slope must be more exposed"
            );
        }
    }

    #[test]
    fn relaxation_rederives_salinity_when_hydrology_moves() {
        let seed = WorldSeed::new(64);
        let base = roll_cell_parameters(seed, 0, &MacroContext::default());
        let mut params = vec![base; 3];
        for cell in params.iter_mut() {
            cell.base_height = BaseHeightBand::Lowland;
            cell.precipitation = Precipitation::Low;
        }
        params[1].hydrology = Hydrology::WellDrained;
        params[1].salinity = Salinity::Freshwater;
        for &i in &[0usize, 2] {
            params[i].hydrology = Hydrology::FloodedBasin;
            params[i].salinity = Salinity::Hypersaline;
        }
        let neighbours = vec![vec![1usize], vec![0, 2], vec![1]];

        relax_parameters(seed, &mut params, &neighbours, 1);

        assert_eq!(params[1].hydrology, Hydrology::FloodedBasin);
        assert_eq!(
            params[1].salinity,
            Salinity::Hypersaline,
            "a cell relaxed into a flooded arid basin must not keep its stale freshwater"
        );
    }

    #[test]
    fn step_distance_is_symmetric_and_zero_only_for_identical() {
        let seed = WorldSeed::new(2);
        let a = roll_cell_parameters(seed, 1, &MacroContext::default());
        let b = roll_cell_parameters(seed, 2, &MacroContext::default());
        assert_eq!(a.step_distance(&b), b.step_distance(&a));
        assert_eq!(a.step_distance(&a), 0);
        assert!(a.step_distance(&b) > 0);
    }

    #[test]
    fn band_predicates_partition_elevation() {
        let mut p = roll_cell_parameters(WorldSeed::new(1), 0, &MacroContext::default());
        p.base_height = BaseHeightBand::DeepOcean;
        assert!(p.is_submerged_band() && !p.is_high_elevation());
        p.base_height = BaseHeightBand::Alpine;
        assert!(p.is_high_elevation() && !p.is_submerged_band());
        p.base_height = BaseHeightBand::SeaLevel;
        assert!(p.is_coastal_band());
        p.temperature = Temperature::Freezing;
        assert!(p.is_frozen());
    }
}
