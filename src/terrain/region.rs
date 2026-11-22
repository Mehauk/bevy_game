use crate::terrain::params::{
    BaseGradient, BaseHeightBand, CellParameters, Geothermal, Humidity, Hydrology, ParamLevel,
    ParamRange, Precipitation, Salinity, SlopeMagnitude, Substrate, SunExposure, Temperature, Wind,
};
use crate::terrain::seed::{WorldSeed, STAGE_PARAMETERS};

const fn bit(index: usize) -> u32 {
    1 << index
}

const HYD_ALL: u32 = bit(0) | bit(1) | bit(2) | bit(3) | bit(4);
const HYD_FLOODED: u32 = bit(4);
const HYD_WET: u32 = bit(2) | bit(3) | bit(4);
const HYD_DRAINED: u32 = bit(0) | bit(1);
const HYD_FLUVIAL: u32 = bit(3) | bit(4);
const HYD_NOT_DRY: u32 = bit(1) | bit(2) | bit(3) | bit(4);

const SUB_ALL: u32 = bit(0) | bit(1) | bit(2) | bit(3) | bit(4);
const SUB_SAND: u32 = bit(0);
const SUB_SAND_SILT: u32 = bit(0) | bit(1);
const SUB_SILT: u32 = bit(1);
const SUB_LIMESTONE: u32 = bit(2);
const SUB_ROCK: u32 = bit(3);
const SUB_VOLCANIC: u32 = bit(4);
const SUB_SAND_SILT_LIMESTONE: u32 = bit(0) | bit(1) | bit(2);

const GEO_NONE: u32 = bit(0);
const GEO_ANY: u32 = bit(0) | bit(1) | bit(2);
const GEO_HOT: u32 = bit(1) | bit(2);

const SAL_FRESH: u32 = bit(0);
const SAL_MARINE: u32 = bit(2);
const SAL_MARINE_BRACKISH: u32 = bit(1) | bit(2);
const SAL_ALL: u32 = bit(0) | bit(1) | bit(2) | bit(3);

const SLOPE_ALL: u32 = bit(0) | bit(1) | bit(2) | bit(3) | bit(4);
const SLOPE_FLAT_GENTLE: u32 = bit(0) | bit(1);
const SLOPE_STEEP_SHEER: u32 = bit(3) | bit(4);
const SLOPE_ROUGH: u32 = bit(2) | bit(3) | bit(4);

const WIND_ALL: u32 = bit(0) | bit(1) | bit(2) | bit(3);
const WIND_CALM: u32 = bit(2) | bit(3);
const WIND_WILD: u32 = bit(0) | bit(1);
const WIND_NOT_GALE: u32 = bit(1) | bit(2) | bit(3);

const SUN_ALL: u32 = bit(0) | bit(1) | bit(2) | bit(3) | bit(4);
const SUN_HARSH: u32 = bit(0) | bit(1);
const SUN_MILD: u32 = bit(2) | bit(3);
const SUN_LOW: u32 = bit(3) | bit(4);
const SUN_NOT_DARK: u32 = bit(0) | bit(1) | bit(2) | bit(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegionTier {
    Submerged,
    Coastal,
    Lowland,
    Highland,
    Alpine,
}

impl RegionTier {
    #[allow(dead_code)]
    pub fn all() -> [RegionTier; 5] {
        [
            RegionTier::Submerged,
            RegionTier::Coastal,
            RegionTier::Lowland,
            RegionTier::Highland,
            RegionTier::Alpine,
        ]
    }

    #[cfg(test)]
    pub fn expected_region_count(self) -> usize {
        match self {
            RegionTier::Submerged => 8,
            RegionTier::Coastal => 6,
            RegionTier::Lowland => 11,
            RegionTier::Highland => 6,
            RegionTier::Alpine => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Region {
    AbyssalTrench,
    DeepOceanBasin,
    HydrothermalVentField,
    KelpForest,
    CoralReef,
    SeagrassMeadow,
    BarrenSeabed,
    SeaIce,

    MangroveSwamp,
    TidalFlat,
    SandyBeachDunes,
    RockyCoast,
    RiverDelta,
    FrozenShore,

    TropicalRainforest,
    TropicalMonsoonForest,
    TropicalDeciduousForest,
    Savanna,
    DesertBarren,
    WetlandMarsh,
    TemperateGrassland,
    TemperateForest,
    MediterraneanScrubland,
    Taiga,
    ArcticTundra,

    PlateauMesa,
    BadlandsCanyonlands,
    HighlandSteppeDesert,
    MontaneForest,
    HighlandMoorland,
    KarstHills,

    SubalpineForest,
    AlpineMeadow,
    ScreeTalus,
    GlacierIcefield,
    VolcanicCaldera,
}

#[derive(Clone, Copy, Debug)]
pub struct RegionRule {
    pub elevation: ParamRange<BaseHeightBand>,
    pub temperature: ParamRange<Temperature>,
    pub humidity: ParamRange<Humidity>,
    pub precipitation: ParamRange<Precipitation>,
    pub hydrology: u32,
    pub substrate: u32,
    pub geothermal: u32,
    pub salinity: u32,
    pub slope: u32,
    pub wind: u32,
    pub sun: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RegionDefinition {
    pub region: Region,
    pub tier: RegionTier,
    pub label: &'static str,
    pub sub_variants: &'static [&'static str],
    pub rule: RegionRule,
    pub prototype: CellParameters,
}

const FLAT_ASPECT: bevy::prelude::Vec2 = bevy::prelude::Vec2::new(1.0, 0.0);

const fn gradient_none() -> BaseGradient {
    BaseGradient {
        slope: SlopeMagnitude::Flat,
        aspect: FLAT_ASPECT,
    }
}

const fn proto(
    base_height: BaseHeightBand,
    temperature: Temperature,
    humidity: Humidity,
    wind: Wind,
    sun_exposure: SunExposure,
    precipitation: Precipitation,
    hydrology: Hydrology,
    substrate: Substrate,
    geothermal: Geothermal,
    salinity: Salinity,
) -> CellParameters {
    CellParameters {
        temperature,
        humidity,
        wind,
        sun_exposure,
        precipitation,
        base_height,
        base_gradient: gradient_none(),
        hydrology,
        substrate,
        geothermal,
        salinity,
    }
}

const fn rule(
    elevation: ParamRange<BaseHeightBand>,
    temperature: ParamRange<Temperature>,
    humidity: ParamRange<Humidity>,
    precipitation: ParamRange<Precipitation>,
    hydrology: u32,
    substrate: u32,
    geothermal: u32,
    salinity: u32,
    slope: u32,
    wind: u32,
    sun: u32,
) -> RegionRule {
    RegionRule {
        elevation,
        temperature,
        humidity,
        precipitation,
        hydrology,
        substrate,
        geothermal,
        salinity,
        slope,
        wind,
        sun,
    }
}

const fn band(first: BaseHeightBand, last: BaseHeightBand) -> ParamRange<BaseHeightBand> {
    if first.ordinal() <= last.ordinal() {
        ParamRange { min: first, max: last }
    } else {
        ParamRange { min: last, max: first }
    }
}

const fn temp(first: Temperature, last: Temperature) -> ParamRange<Temperature> {
    if first.ordinal() <= last.ordinal() {
        ParamRange { min: first, max: last }
    } else {
        ParamRange { min: last, max: first }
    }
}

const fn hum(first: Humidity, last: Humidity) -> ParamRange<Humidity> {
    if first.ordinal() <= last.ordinal() {
        ParamRange { min: first, max: last }
    } else {
        ParamRange { min: last, max: first }
    }
}

const fn prec(first: Precipitation, last: Precipitation) -> ParamRange<Precipitation> {
    if first.ordinal() <= last.ordinal() {
        ParamRange { min: first, max: last }
    } else {
        ParamRange { min: last, max: first }
    }
}

const SUBMERGED_BAND: ParamRange<BaseHeightBand> =
    band(BaseHeightBand::Abyssal, BaseHeightBand::CoastalShelf);
const COASTAL_BAND: ParamRange<BaseHeightBand> =
    band(BaseHeightBand::CoastalShelf, BaseHeightBand::SeaLevel);
const LOWLAND_BAND: ParamRange<BaseHeightBand> =
    band(BaseHeightBand::SeaLevel, BaseHeightBand::Lowland);
const HIGHLAND_BAND: ParamRange<BaseHeightBand> =
    band(BaseHeightBand::Highland, BaseHeightBand::Alpine);
const ALPINE_BAND: ParamRange<BaseHeightBand> =
    band(BaseHeightBand::Alpine, BaseHeightBand::Peak);

pub static REGION_TABLE: [RegionDefinition; 36] = [
    RegionDefinition {
        region: Region::AbyssalTrench,
        tier: RegionTier::Submerged,
        label: "abyssal trench",
        sub_variants: &["hadal trench", "fracture zone"],
        rule: rule(
            band(BaseHeightBand::Abyssal, BaseHeightBand::Abyssal),
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_FLOODED,
            SUB_SILT,
            GEO_ANY,
            SAL_MARINE,
            SLOPE_STEEP_SHEER,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Abyssal,
            Temperature::Cold,
            Humidity::Saturated,
            Wind::NotWindy,
            SunExposure::NoSun,
            Precipitation::Moderate,
            Hydrology::FloodedBasin,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::DeepOceanBasin,
        tier: RegionTier::Submerged,
        label: "deep ocean basin",
        sub_variants: &["abyssal plain", "seamount", "cold-water reef"],
        rule: rule(
            band(BaseHeightBand::Abyssal, BaseHeightBand::DeepOcean),
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_FLOODED,
            SUB_SILT | SUB_ROCK,
            GEO_ANY,
            SAL_MARINE,
            SLOPE_FLAT_GENTLE,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::DeepOcean,
            Temperature::Cold,
            Humidity::Saturated,
            Wind::ModerateWindy,
            SunExposure::NoSun,
            Precipitation::Moderate,
            Hydrology::FloodedBasin,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::HydrothermalVentField,
        tier: RegionTier::Submerged,
        label: "hydrothermal vent field",
        sub_variants: &["vents", "cold seep", "submarine lava"],
        rule: rule(
            band(BaseHeightBand::Abyssal, BaseHeightBand::DeepOcean),
            temp(Temperature::Warm, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::ModerateHumidity),
            prec(Precipitation::Torrential, Precipitation::Low),
            HYD_FLOODED,
            SUB_VOLCANIC,
            GEO_HOT,
            SAL_MARINE,
            SLOPE_ROUGH,
            WIND_CALM,
            SUN_LOW,
        ),
        prototype: proto(
            BaseHeightBand::DeepOcean,
            Temperature::Chilly,
            Humidity::Saturated,
            Wind::NotWindy,
            SunExposure::NoSun,
            Precipitation::Moderate,
            Hydrology::FloodedBasin,
            Substrate::Volcanic,
            Geothermal::ActiveVolcanic,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::KelpForest,
        tier: RegionTier::Submerged,
        label: "kelp forest",
        sub_variants: &[],
        rule: rule(
            band(BaseHeightBand::CoastalShelf, BaseHeightBand::CoastalShelf),
            temp(Temperature::Chilly, Temperature::Warm),
            hum(Humidity::Saturated, Humidity::Humid),
            prec(Precipitation::Torrential, Precipitation::Low),
            HYD_FLOODED,
            SUB_SAND_SILT_LIMESTONE | SUB_ROCK,
            GEO_NONE,
            SAL_MARINE,
            SLOPE_FLAT_GENTLE,
            WIND_NOT_GALE,
            SUN_MILD,
        ),
        prototype: proto(
            BaseHeightBand::CoastalShelf,
            Temperature::Chilly,
            Humidity::Saturated,
            Wind::ModerateWindy,
            SunExposure::ModerateSun,
            Precipitation::Heavy,
            Hydrology::FloodedBasin,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::CoralReef,
        tier: RegionTier::Submerged,
        label: "coral reef",
        sub_variants: &["fringing reef", "atoll"],
        rule: rule(
            band(BaseHeightBand::CoastalShelf, BaseHeightBand::CoastalShelf),
            temp(Temperature::Scorching, Temperature::Warm),
            hum(Humidity::Saturated, Humidity::Humid),
            prec(Precipitation::Torrential, Precipitation::Low),
            HYD_FLOODED,
            SUB_LIMESTONE | SUB_SAND,
            GEO_NONE,
            SAL_MARINE,
            SLOPE_FLAT_GENTLE,
            WIND_NOT_GALE,
            SUN_HARSH,
        ),
        prototype: proto(
            BaseHeightBand::CoastalShelf,
            Temperature::Hot,
            Humidity::Saturated,
            Wind::ModerateWindy,
            SunExposure::HarshSun,
            Precipitation::Heavy,
            Hydrology::FloodedBasin,
            Substrate::LimestoneKarst,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::SeagrassMeadow,
        tier: RegionTier::Submerged,
        label: "seagrass meadow",
        sub_variants: &[],
        rule: rule(
            band(BaseHeightBand::CoastalShelf, BaseHeightBand::CoastalShelf),
            temp(Temperature::Warm, Temperature::Temperate),
            hum(Humidity::Saturated, Humidity::Humid),
            prec(Precipitation::Torrential, Precipitation::Low),
            HYD_FLOODED,
            SUB_SAND_SILT,
            GEO_NONE,
            SAL_MARINE_BRACKISH,
            SLOPE_FLAT_GENTLE,
            WIND_CALM,
            SUN_MILD,
        ),
        prototype: proto(
            BaseHeightBand::CoastalShelf,
            Temperature::Warm,
            Humidity::Saturated,
            Wind::NotWindy,
            SunExposure::ModerateSun,
            Precipitation::Moderate,
            Hydrology::FloodedBasin,
            Substrate::Sand,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::BarrenSeabed,
        tier: RegionTier::Submerged,
        label: "barren seabed / rocky shoal",
        sub_variants: &["rocky shoal"],
        rule: rule(
            SUBMERGED_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_FLOODED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::CoastalShelf,
            Temperature::Cold,
            Humidity::Saturated,
            Wind::ModerateWindy,
            SunExposure::LowSun,
            Precipitation::Low,
            Hydrology::FloodedBasin,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::SeaIce,
        tier: RegionTier::Submerged,
        label: "sea ice / pack ice / ice shelf",
        sub_variants: &["pack ice", "ice shelf"],
        rule: rule(
            SUBMERGED_BAND,
            temp(Temperature::Freezing, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Dry),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_FLOODED,
            SUB_ALL,
            GEO_NONE,
            SAL_MARINE,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::CoastalShelf,
            Temperature::Freezing,
            Humidity::Saturated,
            Wind::Windy,
            SunExposure::LowSun,
            Precipitation::Low,
            Hydrology::FloodedBasin,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::MangroveSwamp,
        tier: RegionTier::Coastal,
        label: "mangrove swamp",
        sub_variants: &[],
        rule: rule(
            COASTAL_BAND,
            temp(Temperature::Scorching, Temperature::Warm),
            hum(Humidity::Saturated, Humidity::Humid),
            prec(Precipitation::Torrential, Precipitation::Moderate),
            HYD_FLUVIAL,
            SUB_SILT,
            GEO_NONE,
            SAL_MARINE_BRACKISH,
            SLOPE_FLAT_GENTLE,
            WIND_CALM,
            SUN_HARSH,
        ),
        prototype: proto(
            BaseHeightBand::SeaLevel,
            Temperature::Warm,
            Humidity::Saturated,
            Wind::NotWindy,
            SunExposure::HarshSun,
            Precipitation::Heavy,
            Hydrology::FloodedBasin,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Brackish,
        ),
    },
    RegionDefinition {
        region: Region::TidalFlat,
        tier: RegionTier::Coastal,
        label: "tidal flat / salt marsh",
        sub_variants: &["mudflat", "coastal salt pan"],
        rule: rule(
            COASTAL_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_SAND_SILT,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::SeaLevel,
            Temperature::Temperate,
            Humidity::Humid,
            Wind::ModerateWindy,
            SunExposure::ExposedSun,
            Precipitation::Low,
            Hydrology::PoorlyDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::SandyBeachDunes,
        tier: RegionTier::Coastal,
        label: "sandy beach / dunes",
        sub_variants: &["barrier spit", "fog coast"],
        rule: rule(
            COASTAL_BAND,
            temp(Temperature::Hot, Temperature::Chilly),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_DRAINED,
            SUB_SAND,
            GEO_NONE,
            SAL_MARINE_BRACKISH,
            SLOPE_FLAT_GENTLE,
            WIND_ALL,
            SUN_HARSH,
        ),
        prototype: proto(
            BaseHeightBand::SeaLevel,
            Temperature::Warm,
            Humidity::Dry,
            Wind::Windy,
            SunExposure::HarshSun,
            Precipitation::Low,
            Hydrology::Dry,
            Substrate::Sand,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::RockyCoast,
        tier: RegionTier::Coastal,
        label: "rocky coast / sea cliffs",
        sub_variants: &["shingle beach", "sea caves", "headland"],
        rule: rule(
            COASTAL_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_DRAINED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::SeaLevel,
            Temperature::Temperate,
            Humidity::Humid,
            Wind::Gale,
            SunExposure::ModerateSun,
            Precipitation::Moderate,
            Hydrology::WellDrained,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::RiverDelta,
        tier: RegionTier::Coastal,
        label: "river delta / estuary",
        sub_variants: &[],
        rule: rule(
            COASTAL_BAND,
            temp(Temperature::Hot, Temperature::Cold),
            hum(Humidity::Saturated, Humidity::Humid),
            prec(Precipitation::Torrential, Precipitation::Moderate),
            HYD_FLUVIAL,
            SUB_SILT,
            GEO_NONE,
            SAL_MARINE_BRACKISH,
            SLOPE_FLAT_GENTLE,
            WIND_CALM,
            SUN_NOT_DARK,
        ),
        prototype: proto(
            BaseHeightBand::SeaLevel,
            Temperature::Temperate,
            Humidity::Saturated,
            Wind::NotWindy,
            SunExposure::ExposedSun,
            Precipitation::Heavy,
            Hydrology::FluvialChannel,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Brackish,
        ),
    },
    RegionDefinition {
        region: Region::FrozenShore,
        tier: RegionTier::Coastal,
        label: "frozen shore / glacial coast",
        sub_variants: &[],
        rule: rule(
            COASTAL_BAND,
            temp(Temperature::Freezing, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Dry),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::SeaLevel,
            Temperature::Freezing,
            Humidity::Dry,
            Wind::Gale,
            SunExposure::NoSun,
            Precipitation::Low,
            Hydrology::Dry,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Marine,
        ),
    },
    RegionDefinition {
        region: Region::TropicalRainforest,
        tier: RegionTier::Lowland,
        label: "tropical rainforest",
        sub_variants: &[],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Scorching, Temperature::Warm),
            hum(Humidity::Saturated, Humidity::Humid),
            prec(Precipitation::Torrential, Precipitation::Heavy),
            HYD_NOT_DRY,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Hot,
            Humidity::Saturated,
            Wind::NotWindy,
            SunExposure::HarshSun,
            Precipitation::Torrential,
            Hydrology::WellDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::TropicalMonsoonForest,
        tier: RegionTier::Lowland,
        label: "tropical monsoon forest",
        sub_variants: &[],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Scorching, Temperature::Warm),
            hum(Humidity::Humid, Humidity::ModerateHumidity),
            prec(Precipitation::Heavy, Precipitation::Moderate),
            HYD_WET,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Hot,
            Humidity::Humid,
            Wind::NotWindy,
            SunExposure::HarshSun,
            Precipitation::Heavy,
            Hydrology::PoorlyDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::TropicalDeciduousForest,
        tier: RegionTier::Lowland,
        label: "tropical deciduous forest",
        sub_variants: &[],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Scorching, Temperature::Warm),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::Low),
            HYD_DRAINED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Warm,
            Humidity::ModerateHumidity,
            Wind::NotWindy,
            SunExposure::HarshSun,
            Precipitation::Moderate,
            Hydrology::WellDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::Savanna,
        tier: RegionTier::Lowland,
        label: "savanna",
        sub_variants: &["tropical grassland", "thornbush"],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Scorching, Temperature::Warm),
            hum(Humidity::ModerateHumidity, Humidity::Arid),
            prec(Precipitation::Moderate, Precipitation::Low),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Hot,
            Humidity::Dry,
            Wind::Windy,
            SunExposure::HarshSun,
            Precipitation::Moderate,
            Hydrology::WellDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::DesertBarren,
        tier: RegionTier::Lowland,
        label: "desert / barren",
        sub_variants: &["sand dunes", "stony reg", "fog desert", "salt flat", "cold desert"],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Scorching, Temperature::Cold),
            hum(Humidity::Dry, Humidity::Arid),
            prec(Precipitation::Low, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Hot,
            Humidity::Arid,
            Wind::Windy,
            SunExposure::HarshSun,
            Precipitation::None,
            Hydrology::Dry,
            Substrate::Sand,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::WetlandMarsh,
        tier: RegionTier::Lowland,
        label: "wetland / marsh / bog",
        sub_variants: &["tropical swamp", "floodplain", "peat muskeg", "lake"],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_WET,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Temperate,
            Humidity::Saturated,
            Wind::NotWindy,
            SunExposure::ModerateSun,
            Precipitation::Heavy,
            Hydrology::PoorlyDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::TemperateGrassland,
        tier: RegionTier::Lowland,
        label: "temperate grassland",
        sub_variants: &["prairie", "pampas", "oak savanna", "steppe"],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Warm, Temperature::Cold),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Moderate, Precipitation::Low),
            HYD_DRAINED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Temperate,
            Humidity::ModerateHumidity,
            Wind::Windy,
            SunExposure::ExposedSun,
            Precipitation::Moderate,
            Hydrology::WellDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::TemperateForest,
        tier: RegionTier::Lowland,
        label: "temperate forest",
        sub_variants: &["temperate rainforest", "deciduous", "coniferous"],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Warm, Temperature::Chilly),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::Moderate),
            HYD_DRAINED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Temperate,
            Humidity::Humid,
            Wind::NotWindy,
            SunExposure::ModerateSun,
            Precipitation::Heavy,
            Hydrology::WellDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::MediterraneanScrubland,
        tier: RegionTier::Lowland,
        label: "mediterranean scrubland",
        sub_variants: &["chaparral", "maquis"],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Hot, Temperature::Temperate),
            hum(Humidity::ModerateHumidity, Humidity::Dry),
            prec(Precipitation::Moderate, Precipitation::Low),
            HYD_DRAINED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Warm,
            Humidity::Dry,
            Wind::ModerateWindy,
            SunExposure::HarshSun,
            Precipitation::Low,
            Hydrology::WellDrained,
            Substrate::LimestoneKarst,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::Taiga,
        tier: RegionTier::Lowland,
        label: "taiga",
        sub_variants: &["boreal forest", "lichen woodland"],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Cold, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Cold,
            Humidity::Humid,
            Wind::NotWindy,
            SunExposure::LowSun,
            Precipitation::Low,
            Hydrology::PoorlyDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::ArcticTundra,
        tier: RegionTier::Lowland,
        label: "arctic tundra",
        sub_variants: &[],
        rule: rule(
            LOWLAND_BAND,
            temp(Temperature::Freezing, Temperature::Freezing),
            hum(Humidity::Humid, Humidity::Dry),
            prec(Precipitation::Moderate, Precipitation::None),
            HYD_WET,
            SUB_SILT | SUB_SAND,
            GEO_NONE,
            SAL_FRESH,
            SLOPE_FLAT_GENTLE,
            WIND_WILD,
            SUN_LOW,
        ),
        prototype: proto(
            BaseHeightBand::Lowland,
            Temperature::Freezing,
            Humidity::Dry,
            Wind::Gale,
            SunExposure::NoSun,
            Precipitation::Low,
            Hydrology::PoorlyDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::PlateauMesa,
        tier: RegionTier::Highland,
        label: "plateau / mesa",
        sub_variants: &["arid mesa", "salar", "snow plateau"],
        rule: rule(
            HIGHLAND_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_DRAINED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Highland,
            Temperature::Warm,
            Humidity::Arid,
            Wind::Windy,
            SunExposure::HarshSun,
            Precipitation::Low,
            Hydrology::Dry,
            Substrate::Sand,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::BadlandsCanyonlands,
        tier: RegionTier::Highland,
        label: "badlands / canyonlands",
        sub_variants: &["gorge", "rocky escarpment"],
        rule: rule(
            HIGHLAND_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_DRAINED,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Highland,
            Temperature::Warm,
            Humidity::Dry,
            Wind::ModerateWindy,
            SunExposure::HarshSun,
            Precipitation::Low,
            Hydrology::WellDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::HighlandSteppeDesert,
        tier: RegionTier::Highland,
        label: "highland steppe / desert",
        sub_variants: &["highland savanna", "temperate steppe", "cold desert"],
        rule: rule(
            HIGHLAND_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Highland,
            Temperature::Chilly,
            Humidity::Dry,
            Wind::Gale,
            SunExposure::HarshSun,
            Precipitation::Low,
            Hydrology::Dry,
            Substrate::Sand,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::MontaneForest,
        tier: RegionTier::Highland,
        label: "montane forest",
        sub_variants: &["cloud forest", "montane rainforest", "coniferous", "dry woodland"],
        rule: rule(
            HIGHLAND_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Highland,
            Temperature::Chilly,
            Humidity::Humid,
            Wind::NotWindy,
            SunExposure::ModerateSun,
            Precipitation::Heavy,
            Hydrology::WellDrained,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::HighlandMoorland,
        tier: RegionTier::Highland,
        label: "highland moorland / heath",
        sub_variants: &["blanket bog"],
        rule: rule(
            HIGHLAND_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Highland,
            Temperature::Cold,
            Humidity::Saturated,
            Wind::Gale,
            SunExposure::LowSun,
            Precipitation::Heavy,
            Hydrology::PoorlyDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::KarstHills,
        tier: RegionTier::Highland,
        label: "karst / limestone hills",
        sub_variants: &[],
        rule: rule(
            HIGHLAND_BAND,
            temp(Temperature::Scorching, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_DRAINED,
            SUB_LIMESTONE,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Highland,
            Temperature::Warm,
            Humidity::Humid,
            Wind::ModerateWindy,
            SunExposure::ExposedSun,
            Precipitation::Moderate,
            Hydrology::WellDrained,
            Substrate::LimestoneKarst,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::SubalpineForest,
        tier: RegionTier::Alpine,
        label: "subalpine forest",
        sub_variants: &["coniferous", "wind-stunted krummholz"],
        rule: rule(
            ALPINE_BAND,
            temp(Temperature::Cold, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Alpine,
            Temperature::Cold,
            Humidity::Humid,
            Wind::Windy,
            SunExposure::ModerateSun,
            Precipitation::Low,
            Hydrology::WellDrained,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::AlpineMeadow,
        tier: RegionTier::Alpine,
        label: "alpine meadow",
        sub_variants: &["meadow", "tropical paramo", "dry puna", "alpine tundra", "tarn"],
        rule: rule(
            ALPINE_BAND,
            temp(Temperature::Hot, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Alpine,
            Temperature::Chilly,
            Humidity::Humid,
            Wind::Gale,
            SunExposure::HarshSun,
            Precipitation::Moderate,
            Hydrology::WellDrained,
            Substrate::SiltClay,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::ScreeTalus,
        tier: RegionTier::Alpine,
        label: "scree / talus slope",
        sub_variants: &["sheer rock cliff", "desert mountain"],
        rule: rule(
            ALPINE_BAND,
            temp(Temperature::Hot, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Alpine,
            Temperature::Cold,
            Humidity::Dry,
            Wind::Gale,
            SunExposure::HarshSun,
            Precipitation::Low,
            Hydrology::Dry,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::GlacierIcefield,
        tier: RegionTier::Alpine,
        label: "glacier / icefield",
        sub_variants: &["perennial snowfield", "valley glacier", "polar ice cap"],
        rule: rule(
            ALPINE_BAND,
            temp(Temperature::Freezing, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_ALL,
            GEO_ANY,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Peak,
            Temperature::Freezing,
            Humidity::Dry,
            Wind::Gale,
            SunExposure::HarshSun,
            Precipitation::Low,
            Hydrology::Dry,
            Substrate::RockCrystalline,
            Geothermal::None,
            Salinity::Freshwater,
        ),
    },
    RegionDefinition {
        region: Region::VolcanicCaldera,
        tier: RegionTier::Alpine,
        label: "volcanic caldera / lava field",
        sub_variants: &["fumarole", "geothermal peak"],
        rule: rule(
            ALPINE_BAND,
            temp(Temperature::Hot, Temperature::Freezing),
            hum(Humidity::Saturated, Humidity::Arid),
            prec(Precipitation::Torrential, Precipitation::None),
            HYD_ALL,
            SUB_VOLCANIC,
            GEO_HOT,
            SAL_ALL,
            SLOPE_ALL,
            WIND_ALL,
            SUN_ALL,
        ),
        prototype: proto(
            BaseHeightBand::Peak,
            Temperature::Chilly,
            Humidity::Dry,
            Wind::Windy,
            SunExposure::ExposedSun,
            Precipitation::Low,
            Hydrology::Dry,
            Substrate::Volcanic,
            Geothermal::ActiveVolcanic,
            Salinity::Freshwater,
        ),
    },
];

impl Region {
    pub fn all() -> &'static [RegionDefinition] {
        &REGION_TABLE
    }

    pub fn count() -> usize {
        REGION_TABLE.len()
    }

    pub fn definition(self) -> &'static RegionDefinition {
        REGION_TABLE
            .iter()
            .find(|d| d.region == self)
            .unwrap_or(&REGION_TABLE[0])
    }

    pub fn index(self) -> usize {
        REGION_TABLE
            .iter()
            .position(|d| d.region == self)
            .unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn from_index(index: usize) -> Region {
        REGION_TABLE[index % REGION_TABLE.len()].region
    }

    pub fn label(self) -> &'static str {
        self.definition().label
    }

    pub fn sub_variants(self) -> &'static [&'static str] {
        self.definition().sub_variants
    }

    #[allow(dead_code)]
    pub fn rule(self) -> &'static RegionRule {
        &self.definition().rule
    }

    pub fn prototype(self) -> CellParameters {
        self.definition().prototype
    }
}

fn mask_allows(mask: u32, index: usize) -> bool {
    mask == u32::MAX || (mask & bit(index)) != 0
}

pub fn rule_matches(rule: &RegionRule, params: &CellParameters) -> bool {
    rule.elevation.contains(params.base_height)
        && rule.temperature.contains(params.temperature)
        && rule.humidity.contains(params.humidity)
        && rule.precipitation.contains(params.precipitation)
        && mask_allows(rule.hydrology, params.hydrology.index())
        && mask_allows(rule.substrate, params.substrate.index())
        && mask_allows(rule.geothermal, params.geothermal.index())
        && mask_allows(rule.salinity, params.salinity.index())
        && mask_allows(rule.slope, params.base_gradient.slope.index())
        && mask_allows(rule.wind, params.wind.index())
        && mask_allows(rule.sun, params.sun_exposure.index())
}

pub fn rule_violations(rule: &RegionRule, params: &CellParameters) -> usize {
    let mut violations = 0;
    violations += (rule.elevation.distance(params.base_height) > 0) as usize;
    violations += (rule.temperature.distance(params.temperature) > 0) as usize;
    violations += (rule.humidity.distance(params.humidity) > 0) as usize;
    violations += (rule.precipitation.distance(params.precipitation) > 0) as usize;
    violations += (!mask_allows(rule.hydrology, params.hydrology.index())) as usize;
    violations += (!mask_allows(rule.substrate, params.substrate.index())) as usize;
    violations += (!mask_allows(rule.geothermal, params.geothermal.index())) as usize;
    violations += (!mask_allows(rule.salinity, params.salinity.index())) as usize;
    violations += (!mask_allows(rule.slope, params.base_gradient.slope.index())) as usize;
    violations += (!mask_allows(rule.wind, params.wind.index())) as usize;
    violations += (!mask_allows(rule.sun, params.sun_exposure.index())) as usize;
    violations
}

pub fn prototype_distance(prototype: &CellParameters, params: &CellParameters) -> i32 {
    prototype.base_height.distance(params.base_height)
        + prototype.temperature.distance(params.temperature)
        + prototype.humidity.distance(params.humidity)
        + prototype.precipitation.distance(params.precipitation)
        + prototype.hydrology.distance(params.hydrology)
        + prototype.substrate.distance(params.substrate)
        + prototype.geothermal.distance(params.geothermal)
        + prototype.salinity.distance(params.salinity)
        + prototype.wind.distance(params.wind)
        + prototype.sun_exposure.distance(params.sun_exposure)
}

pub fn resolve_region(params: &CellParameters) -> Region {
    let mut best: Option<(i32, usize, Region)> = None;
    for definition in REGION_TABLE.iter() {
        if !rule_matches(&definition.rule, params) {
            continue;
        }
        let distance = prototype_distance(&definition.prototype, params);
        let candidate = (distance, definition.region.index(), definition.region);
        if best.is_none_or(|(best_distance, best_index, _)| {
            distance < best_distance || (distance == best_distance && definition.region.index() < best_index)
        }) {
            best = Some(candidate);
        }
    }
    if let Some((_, _, region)) = best {
        return region;
    }

    let mut fallback: Option<(usize, i32, usize, Region)> = None;
    for definition in REGION_TABLE.iter() {
        let violations = rule_violations(&definition.rule, params);
        let distance = prototype_distance(&definition.prototype, params);
        let index = definition.region.index();
        let better = fallback.is_none_or(|(fv, fd, fi, _)| {
            violations < fv || (violations == fv && (distance < fd || (distance == fd && index < fi)))
        });
        if better {
            fallback = Some((violations, distance, index, definition.region));
        }
    }
    fallback
        .map(|(_, _, _, region)| region)
        .unwrap_or(Region::BarrenSeabed)
}

pub fn eligible_regions(params: &CellParameters) -> Vec<Region> {
    REGION_TABLE
        .iter()
        .filter(|d| rule_matches(&d.rule, params))
        .map(|d| d.region)
        .collect()
}

pub fn sub_variant_index(
    region: Region,
    params: &CellParameters,
    seed: WorldSeed,
    cell_index: u64,
) -> usize {
    let variants = region.sub_variants();
    if variants.is_empty() {
        return 0;
    }
    let bias = params.salinity.index() as u64
        + params.substrate.index() as u64 * 2
        + params.base_gradient.slope.index() as u64 * 3
        + params.temperature.index() as u64 * 5;
    let roll = seed.unit(STAGE_PARAMETERS, cell_index, 60 + region.index() as u64);
    let pick = (roll * variants.len() as f32) as usize;
    (pick + bias as usize) % variants.len()
}

#[cfg(test)]
pub fn is_eligible(region: Region, params: &CellParameters) -> bool {
    rule_matches(region.rule(), params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::params::{BaseGradient, ParamLevel, SlopeMagnitude};

    fn params_with_slopes(params: &CellParameters, slope: SlopeMagnitude) -> CellParameters {
        let mut p = *params;
        p.base_gradient = BaseGradient {
            slope,
            aspect: bevy::prelude::Vec2::X,
        };
        p
    }

    #[test]
    fn the_table_covers_thirty_six_regions_in_five_tiers() {
        assert_eq!(Region::count(), 36);
        for tier in RegionTier::all() {
            let count = REGION_TABLE.iter().filter(|d| d.tier == tier).count();
            assert_eq!(count, tier.expected_region_count(), "tier {tier:?}");
        }
        let mut labels: Vec<&str> = REGION_TABLE.iter().map(|d| d.label).collect();
        let unique = labels.len();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), unique, "region labels must be unique");
    }

    #[test]
    fn every_prototype_satisfies_its_own_rule() {
        for definition in Region::all() {
            let params = params_with_slopes(&definition.prototype, slope_for(definition.region));
            assert!(
                rule_matches(&definition.rule, &params),
                "{} prototype violates its own rule: {}",
                definition.label,
                params.describe()
            );
        }
    }

    fn slope_for(region: Region) -> SlopeMagnitude {
        let rule = region.rule();
        for slope in SlopeMagnitude::ALL {
            if mask_allows(rule.slope, slope.index()) {
                return *slope;
            }
        }
        SlopeMagnitude::Flat
    }

    #[test]
    fn every_region_is_reachable_from_its_prototype() {
        for definition in Region::all() {
            let params = params_with_slopes(&definition.prototype, slope_for(definition.region));
            let resolved = resolve_region(&params);
            assert_eq!(
                resolved, definition.region,
                "prototype for {} resolved to {}",
                definition.label,
                resolved.label()
            );
        }
    }

    #[test]
    fn resolution_never_fails_even_for_absurd_parameters() {
        let absurd = CellParameters {
            temperature: Temperature::Scorching,
            humidity: Humidity::Saturated,
            wind: Wind::Gale,
            sun_exposure: SunExposure::NoSun,
            precipitation: Precipitation::Torrential,
            base_height: BaseHeightBand::Peak,
            base_gradient: BaseGradient {
                slope: SlopeMagnitude::Flat,
                aspect: bevy::prelude::Vec2::X,
            },
            hydrology: Hydrology::FloodedBasin,
            substrate: Substrate::Volcanic,
            geothermal: Geothermal::ActiveVolcanic,
            salinity: Salinity::Hypersaline,
        };
        assert!(
            eligible_regions(&absurd).is_empty(),
            "a scorching flooded peak is meant to match no rule at all"
        );
        let resolved = resolve_region(&absurd);
        assert_eq!(
            rule_violations(resolved.rule(), &absurd),
            Region::all()
                .iter()
                .map(|d| rule_violations(&d.rule, &absurd))
                .min()
                .unwrap(),
            "the fallback must pick the least-ineligible region"
        );
    }

    #[test]
    fn a_plausible_volcanic_peak_resolves_to_the_volcanic_region() {
        let mut params = Region::VolcanicCaldera.prototype();
        params.base_gradient = BaseGradient {
            slope: SlopeMagnitude::Moderate,
            aspect: bevy::prelude::Vec2::X,
        };
        assert_eq!(resolve_region(&params), Region::VolcanicCaldera);
    }

    #[test]
    fn resolution_is_deterministic_and_total_over_random_rolls() {
        use crate::terrain::params::{roll_cell_parameters, MacroContext};
        use crate::terrain::seed::WorldSeed;

        let seed = WorldSeed::new(90210);
        let mut seen = std::collections::HashSet::new();
        for cell in 0..600u64 {
            for band in BaseHeightBand::ALL {
                let ctx = MacroContext {
                    band: *band,
                    latitude: (cell % 100) as f32 / 99.0,
                    sea_proximity: (cell % 7) as f32 / 6.0,
                    rain_shadow: (cell % 5) as f32 / 4.0,
                    geothermal_intensity: (cell % 6) as f32 / 5.0,
                    prevailing_wind: bevy::prelude::Vec2::new(1.0, 0.0),
                    downslope: bevy::prelude::Vec2::new(((cell % 6) as f32 - 3.0) / 3.0, ((cell % 4) as f32 - 2.0) / 2.0),
                    slope_magnitude: (cell % 8) as f32 / 2.5,
                };
                let params = roll_cell_parameters(seed, cell, &ctx);
                let first = resolve_region(&params);
                let second = resolve_region(&params);
                assert_eq!(first, second, "resolution must be pure");
                seen.insert(first);
            }
        }
        assert!(seen.len() >= 12, "rolls must reach many regions, got {}", seen.len());
    }

    #[test]
    fn resolution_never_returns_an_ineligible_region_when_eligible_ones_exist() {
        use crate::terrain::params::{roll_cell_parameters, MacroContext};
        use crate::terrain::seed::WorldSeed;

        let seed = WorldSeed::new(5150);
        for cell in 0..400u64 {
            for band in BaseHeightBand::ALL {
                let ctx = MacroContext {
                    band: *band,
                    latitude: (cell % 50) as f32 / 49.0,
                    sea_proximity: (cell % 3) as f32 / 2.0,
                    geothermal_intensity: (cell % 4) as f32 / 3.0,
                    ..Default::default()
                };
                let params = roll_cell_parameters(seed, cell, &ctx);
                let resolved = resolve_region(&params);
                let eligible = eligible_regions(&params);
                if !eligible.is_empty() {
                    assert!(
                        eligible.contains(&resolved),
                        "resolved {} is not eligible for {}",
                        resolved.label(),
                        params.describe()
                    );
                }
            }
        }
    }

    #[test]
    fn gating_rejects_impossible_combinations() {
        let reef = Region::CoralReef;
        let mut params = reef.prototype();
        params.base_height = BaseHeightBand::Peak;
        assert!(!is_eligible(reef, &params), "coral cannot grow on a peak");

        params = reef.prototype();
        params.temperature = Temperature::Freezing;
        assert!(!is_eligible(reef, &params), "coral cannot grow in freezing water");

        let glacier = Region::GlacierIcefield;
        params = glacier.prototype();
        params.temperature = Temperature::Scorching;
        assert!(!is_eligible(glacier, &params), "glaciers need freezing temperatures");

        let vents = Region::HydrothermalVentField;
        params = vents.prototype();
        params.geothermal = Geothermal::None;
        assert!(!is_eligible(vents, &params), "vents need geothermal activity");
    }

    #[test]
    fn elevation_tiers_gate_the_matrix() {
        for definition in Region::all() {
            let mut params = definition.prototype;
            params.base_gradient = BaseGradient {
                slope: slope_for(definition.region),
                aspect: bevy::prelude::Vec2::X,
            };
            match definition.tier {
                RegionTier::Submerged => assert!(
                    definition.rule.elevation.max.index() <= BaseHeightBand::CoastalShelf.index(),
                    "{} must be submerged",
                    definition.label
                ),
                RegionTier::Alpine => assert!(
                    definition.rule.elevation.min.index() >= BaseHeightBand::Alpine.index(),
                    "{} must be alpine or higher",
                    definition.label
                ),
                _ => {}
            }
            assert!(definition.rule.elevation.contains(params.base_height));
        }
    }

    #[test]
    fn violation_counting_is_zero_exactly_when_eligible() {
        for definition in Region::all() {
            let params = params_with_slopes(&definition.prototype, slope_for(definition.region));
            assert_eq!(rule_violations(&definition.rule, &params), 0);
        }
        let mut wrong = Region::CoralReef.prototype();
        wrong.base_height = BaseHeightBand::Peak;
        wrong.temperature = Temperature::Freezing;
        assert!(rule_violations(Region::CoralReef.rule(), &wrong) >= 2);
    }

    #[test]
    fn region_index_round_trips() {
        for definition in Region::all() {
            assert_eq!(Region::from_index(definition.region.index()), definition.region);
        }
        assert_eq!(Region::from_index(usize::MAX), Region::from_index(usize::MAX % 36));
    }

    #[test]
    fn sub_variants_are_recorded_where_the_design_specifies_them() {
        assert!(!Region::DeepOceanBasin.sub_variants().is_empty());
        assert!(!Region::DesertBarren.sub_variants().is_empty());
        assert!(Region::KelpForest.sub_variants().is_empty());
        assert!(Region::GlacierIcefield.sub_variants().contains(&"polar ice cap"));
    }

    #[test]
    fn sub_variants_resolve_inside_their_regions_declared_list() {
        use crate::terrain::params::{roll_cell_parameters, MacroContext};
        use crate::terrain::seed::WorldSeed;

        let seed = WorldSeed::new(4242);
        let mut distinct = std::collections::HashMap::new();
        for cell in 0..4000u64 {
            let ctx = MacroContext {
                band: BaseHeightBand::ALL[(cell % 8) as usize],
                latitude: (cell % 7) as f32 / 6.0,
                sea_proximity: (cell % 5) as f32 / 4.0,
                rain_shadow: (cell % 3) as f32 / 2.0,
                geothermal_intensity: (cell % 4) as f32 / 3.0,
                slope_magnitude: (cell % 6) as f32 / 2.0,
                ..Default::default()
            };
            let params = roll_cell_parameters(seed, cell, &ctx);
            let region = resolve_region(&params);
            let variants = region.sub_variants();
            let index = sub_variant_index(region, &params, seed, cell);
            assert!(
                index < variants.len().max(1),
                "{region:?} index {index} outside {} variants",
                variants.len()
            );
            assert_eq!(
                sub_variant_index(region, &params, seed, cell),
                index,
                "sub-variant pick must be deterministic"
            );
            if !variants.is_empty() {
                distinct
                    .entry(region)
                    .or_insert_with(std::collections::HashSet::new)
                    .insert(index);
            }
        }
        let multi = distinct.values().filter(|s| s.len() > 1).count();
        assert!(
            multi >= distinct.len() / 2,
            "regions with declared variants should not collapse to one pick: {multi}/{}",
            distinct.len()
        );
    }

    #[test]
    fn every_valid_roll_resolves_without_fallback() {
        use crate::terrain::params::{roll_cell_parameters, MacroContext};
        use crate::terrain::seed::WorldSeed;

        let slopes = [0.0f32, 0.2, 0.6, 1.0, 1.6, 2.4];
        let mut rolls = 0usize;
        let mut fallbacks = 0usize;
        let mut examples: Vec<String> = Vec::new();
        for seed_index in 0..400u64 {
            let seed = WorldSeed::new(seed_index.wrapping_mul(7919).wrapping_add(13));
            for band in BaseHeightBand::ALL {
                for lat in 0..7 {
                    for prox in 0..3 {
                        for shadow in 0..3 {
                            for geo in 0..4 {
                                for slope in 0..6 {
                                    let ctx = MacroContext {
                                        band: *band,
                                        latitude: lat as f32 / 6.0,
                                        sea_proximity: prox as f32 / 2.0,
                                        rain_shadow: shadow as f32 / 2.0,
                                        geothermal_intensity: geo as f32 / 3.0,
                                        slope_magnitude: slopes[slope],
                                        ..Default::default()
                                    };
                                    let params = roll_cell_parameters(seed, seed_index, &ctx);
                                    rolls += 1;
                                    if eligible_regions(&params).is_empty() {
                                        fallbacks += 1;
                                        if examples.len() < 16 {
                                            examples.push(format!(
                                                "{band:?} lat{lat} prox{prox} shadow{shadow} geo{geo} slope{slope} => {}",
                                                params.describe()
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let rate = fallbacks as f32 / rolls as f32;
        eprintln!("rule coverage: {rolls} rolls, {fallbacks} fallbacks, rate {rate:.6}");
        assert_eq!(fallbacks, 0, "fallback rate {rate}: {examples:?}");
    }
}
