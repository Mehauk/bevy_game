#![allow(dead_code)]

use bevy::prelude::Vec2;

use crate::terrain::material::SurfaceLayer;
use crate::terrain::params::{Precipitation, Salinity, Substrate, Temperature};
use crate::terrain::region::Region;
use crate::terrain::season::{
    DEFAULT_SNOWLINE_METRES, FloodZone, SeaLevelSeason, SeasonClock,
    shore_override_with_hardness, snow_coverage_with_slope, snowline_for,
};
use crate::terrain::water;

use super::TerrainMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceWater {
    Dry,
    Sea,
    Lake,
    River,
    Waterfall,
}

impl SurfaceWater {
    pub fn from_kind(kind: water::WaterKind) -> Self {
        match kind {
            water::WaterKind::Sea => SurfaceWater::Sea,
            water::WaterKind::Lake => SurfaceWater::Lake,
            water::WaterKind::River => SurfaceWater::River,
            water::WaterKind::Waterfall => SurfaceWater::Waterfall,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SurfaceInfo {
    pub point: Vec2,
    pub height: f32,
    pub slope: f32,
    pub cell: Option<usize>,
    pub region: Option<Region>,
    pub substrate: Option<Substrate>,
    pub hardness: f32,
    pub temperature: Option<Temperature>,
    pub precipitation: Option<Precipitation>,
    pub moisture: f32,
    pub snow: f32,
    pub water: SurfaceWater,
    pub depth: f32,
    pub frozen: bool,
    pub salinity: Option<Salinity>,
    pub flow: Vec2,
    pub shore_layer: Option<SurfaceLayer>,
}

impl SurfaceInfo {
    pub fn is_dry(&self) -> bool {
        matches!(self.water, SurfaceWater::Dry)
    }

    pub fn is_submerged(&self) -> bool {
        !self.is_dry()
    }

    pub fn walkable(&self, max_slope: f32) -> bool {
        self.is_dry() && !self.frozen && self.slope <= max_slope
    }
}

impl TerrainMap {
    pub fn water_surface_at(&self, x: f32, z: f32) -> Option<water::WaterSample> {
        self.water.sample_at(&self.heights, Vec2::new(x, z))
    }
    pub fn season_clock(&self) -> &SeasonClock {
        &self.season_clock
    }

    pub fn snowline_base(&self) -> f32 {
        DEFAULT_SNOWLINE_METRES
    }

    pub fn snowline(&self) -> f32 {
        snowline_for(&self.season_clock, self.snowline_base())
    }

    pub fn snow_coverage_at(&self, x: f32, z: f32) -> f32 {
        let Some(parameters) = self.parameters_at(x, z) else {
            return 0.0;
        };
        snow_coverage_with_slope(
            &self.season_clock,
            parameters.temperature,
            self.height_at(x, z),
            self.snowline(),
            self.slope_at(x, z),
        )
    }

    pub fn shore_override_at(&self, x: f32, z: f32) -> Option<SurfaceLayer> {
        let parameters = self.parameters_at(x, z)?;
        shore_override_with_hardness(
            self.season,
            &self.flood_zone,
            self.height_at(x, z),
            parameters.salinity,
            parameters.precipitation,
            self.sea_level(),
            self.hardness_at(x, z),
        )
    }

    pub fn surface_info_at(&self, x: f32, z: f32) -> SurfaceInfo {
        let parameters = self.parameters_at(x, z);
        let sample = self.water_surface_at(x, z);
        let (water, depth, frozen, salinity, flow) = match sample {
            Some(sample) => (
                SurfaceWater::from_kind(sample.kind),
                sample.depth,
                sample.frozen,
                Some(sample.salinity),
                sample.flow,
            ),
            None => (SurfaceWater::Dry, 0.0, false, None, Vec2::ZERO),
        };
        SurfaceInfo {
            point: Vec2::new(x, z),
            height: self.height_at(x, z),
            slope: self.slope_at(x, z),
            cell: self.cell_id_at(x, z),
            region: self.region_at(x, z),
            substrate: parameters.map(|parameters| parameters.substrate),
            hardness: self.hardness_at(x, z),
            temperature: parameters.map(|parameters| parameters.temperature),
            precipitation: parameters.map(|parameters| parameters.precipitation),
            moisture: self.precipitation_at(x, z),
            snow: self.snow_coverage_at(x, z),
            water,
            depth,
            frozen,
            salinity,
            flow,
            shore_layer: self.shore_override_at(x, z),
        }
    }

    pub fn sea_level(&self) -> f32 {
        self.sea_level_now
    }

    pub fn prevailing_wind(&self) -> Vec2 {
        self.macro_field.prevailing_wind
    }

    pub fn sea_levels(&self) -> &SeaLevelSeason {
        &self.sea_levels
    }

    pub fn flood_zone(&self) -> &FloodZone {
        &self.flood_zone
    }

    pub fn is_underwater(&self, x: f32, z: f32) -> bool {
        self.height_at(x, z) < self.sea_level()
    }

    pub fn depth_below_sea(&self, x: f32, z: f32) -> f32 {
        (self.sea_level() - self.height_at(x, z)).max(0.0)
    }
}
