use bevy::color::Mix;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::{
    Camera3d, ClearColor, Color, GlobalTransform, Query, Res, ResMut, Resource, Vec3, With,
};

use crate::terrain::map::TerrainMap;
use crate::terrain::params::{Humidity, ParamLevel, Temperature};
use crate::terrain::region::{REGION_TABLE, Region, RegionDefinition, RegionTier};
use crate::terrain::sky::{TerrainAtmosphere, TerrainFog};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionAtmosphereProfile {
    pub region: Region,
    pub fog_colour: Color,
    pub visibility: f32,
    pub light_colour: Color,
    pub light_exponent: f32,
    pub sky_zenith: Color,
    pub sky_horizon: Color,
    pub ambient_boost: f32,
}

const fn grey(hex: u32) -> Color {
    Color::srgb(
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    )
}

const BASE_FOG: u32 = 0x9db0be;
const BASE_LIGHT: u32 = 0xfff1da;
const BASE_ZENITH: u32 = 0x46759f;
const BASE_HORIZON: u32 = 0xc2d2de;

fn wetness_of(region: Region) -> f32 {
    let index = region.prototype().humidity.index();
    (Humidity::COUNT - 1 - index) as f32 / (Humidity::COUNT - 1) as f32
}

fn coldness_of(region: Region) -> f32 {
    let index = region.prototype().temperature.index();
    index as f32 / (Temperature::COUNT - 1) as f32
}

fn altitude_of(region: Region) -> f32 {
    match region.definition().tier {
        RegionTier::Submerged | RegionTier::Coastal => 0.0,
        RegionTier::Lowland => 0.1,
        RegionTier::Highland => 0.6,
        RegionTier::Alpine => 1.0,
    }
}

fn profile_for(definition: &RegionDefinition) -> RegionAtmosphereProfile {
    let region = definition.region;
    let wet = wetness_of(region);
    let cold = coldness_of(region);
    let alt = altitude_of(region);

    let fog_colour = grey(BASE_FOG)
        .mix(&grey(0x8b98a4), wet * 0.35)
        .mix(&grey(0xa8bccb), cold * 0.30)
        .mix(&grey(0xbccdd8), alt * 0.25);
    let light_colour = grey(BASE_LIGHT)
        .mix(&grey(0xdde9f2), cold * 0.35)
        .mix(&grey(0xe6e2d8), wet * 0.25);
    let sky_zenith = grey(BASE_ZENITH)
        .mix(&grey(0x5d7f9e), wet * 0.40)
        .mix(&grey(0x33608f), alt * 0.50);
    let sky_horizon = grey(BASE_HORIZON)
        .mix(&grey(0xa9b4bc), wet * 0.40)
        .mix(&grey(0xd8e4ee), cold * 0.25);

    RegionAtmosphereProfile {
        region,
        fog_colour,
        visibility: 140.0 - 16.0 * wet - 8.0 * cold + 6.0 * alt,
        light_colour,
        light_exponent: 27.0 - 2.0 * wet + 1.5 * alt,
        sky_zenith,
        sky_horizon,
        ambient_boost: 0.95 - 0.04 * wet - 0.02 * cold + 0.03 * alt,
    }
}

pub static REGION_ATMOSPHERE_TABLE: std::sync::LazyLock<Vec<RegionAtmosphereProfile>> =
    std::sync::LazyLock::new(|| REGION_TABLE.iter().map(profile_for).collect());

pub fn atmosphere_profile(region: Region) -> &'static RegionAtmosphereProfile {
    REGION_ATMOSPHERE_TABLE
        .iter()
        .find(|entry| entry.region == region)
        .unwrap_or(&REGION_ATMOSPHERE_TABLE[0])
}

pub fn blend_atmosphere(contributions: &[(f32, RegionAtmosphereProfile)]) -> RegionAtmosphereProfile {
    let total: f32 = contributions.iter().map(|(weight, _)| *weight).sum();
    if total <= f32::EPSILON || contributions.is_empty() {
        return *atmosphere_profile(Region::TemperateGrassland);
    }

    let mut fog = empty_linear(contributions[0].1.fog_colour);
    let mut light = empty_linear(contributions[0].1.light_colour);
    let mut zenith = empty_linear(contributions[0].1.sky_zenith);
    let mut horizon = empty_linear(contributions[0].1.sky_horizon);
    let mut visibility = 0.0f32;
    let mut exponent = 0.0f32;
    let mut ambient = 0.0f32;
    let mut region = contributions[0].1.region;
    let mut best = f32::NEG_INFINITY;

    for (weight, profile) in contributions {
        let share = weight / total;
        accumulate(&mut fog, profile.fog_colour, share);
        accumulate(&mut light, profile.light_colour, share);
        accumulate(&mut zenith, profile.sky_zenith, share);
        accumulate(&mut horizon, profile.sky_horizon, share);
        visibility += profile.visibility * share;
        exponent += profile.light_exponent * share;
        ambient += profile.ambient_boost * share;
        if *weight > best {
            best = *weight;
            region = profile.region;
        }
    }

    RegionAtmosphereProfile {
        region,
        fog_colour: Color::from(fog),
        visibility,
        light_colour: Color::from(light),
        light_exponent: exponent,
        sky_zenith: Color::from(zenith),
        sky_horizon: Color::from(horizon),
        ambient_boost: ambient,
    }
}

fn empty_linear(colour: Color) -> bevy::prelude::LinearRgba {
    let mut base = colour.to_linear();
    base.red = 0.0;
    base.green = 0.0;
    base.blue = 0.0;
    base
}

fn accumulate(target: &mut bevy::prelude::LinearRgba, colour: Color, share: f32) {
    let source = colour.to_linear();
    target.red += source.red * share;
    target.green += source.green * share;
    target.blue += source.blue * share;
}

pub fn to_distance_fog(profile: &RegionAtmosphereProfile) -> DistanceFog {
    DistanceFog {
        color: profile.fog_colour,
        directional_light_color: profile.light_colour,
        directional_light_exponent: profile.light_exponent,
        falloff: FogFalloff::from_visibility_colors(
            profile.visibility,
            profile.fog_colour,
            profile.sky_horizon,
        ),
    }
}

pub fn underwater_tint(depth: f32) -> Color {
    let t = (depth / 24.0).clamp(0.0, 1.0);
    let shallow = Color::srgb(0.20, 0.46, 0.52);
    let deep = Color::srgb(0.02, 0.10, 0.20);
    shallow.mix(&deep, t)
}

pub fn underwater_visibility(depth: f32) -> f32 {
    (26.0 - depth * 0.9).clamp(4.0, 26.0)
}

pub const UNDERWATER_BLEND_METRES: f32 = 2.0;

pub fn underwater_blend(depth: f32) -> f32 {
    let t = (depth / UNDERWATER_BLEND_METRES).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn underwater_visibility_at(surface: f32, depth: f32) -> f32 {
    let blend = underwater_blend(depth);
    surface + (underwater_visibility(depth) - surface) * blend
}

pub const AXIAL_TILT_RADIANS: f32 = 23.44f32.to_radians();
pub const OBSERVER_LATITUDE_RADIANS: f32 = 45.0f32.to_radians();
pub const SUN_PEAK_LUX: f32 = 6000.0;

pub fn solar_declination(cycle_progress: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    AXIAL_TILT_RADIANS * (cycle_progress * tau).sin()
}

pub fn solar_elevation(cycle_progress: f32) -> f32 {
    let meridian = (OBSERVER_LATITUDE_RADIANS - solar_declination(cycle_progress)).abs();
    (std::f32::consts::FRAC_PI_2 - meridian).clamp(-1.4, 1.4)
}

pub fn sun_direction(cycle_progress: f32) -> Vec3 {
    let elevation = solar_elevation(cycle_progress);
    let azimuth = 2.35 + 0.6 * solar_declination(cycle_progress);
    let horizontal = elevation.cos();
    Vec3::new(azimuth.sin() * horizontal, elevation.sin(), azimuth.cos() * horizontal)
        .try_normalize()
        .unwrap_or(Vec3::Y)
}

pub fn daylight_factor(elevation: f32) -> f32 {
    (elevation.sin().max(0.0) * 0.85 + 0.15).clamp(0.0, 1.0)
}

pub fn sun_illuminance(profile: &RegionAtmosphereProfile, elevation: f32) -> f32 {
    SUN_PEAK_LUX * profile.ambient_boost * daylight_factor(elevation)
}

pub fn sun_colour(profile: &RegionAtmosphereProfile, elevation: f32) -> Color {
    let warm = Color::srgb(1.0, 0.62, 0.35);
    warm.mix(&profile.light_colour, daylight_factor(elevation))
}

pub fn ambient_colour(profile: &RegionAtmosphereProfile) -> Color {
    profile.sky_zenith
}

pub fn ambient_brightness(profile: &RegionAtmosphereProfile, elevation: f32) -> f32 {
    80.0 * profile.ambient_boost * (0.35 + 0.65 * daylight_factor(elevation))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Resource)]
pub struct UnderwaterState {
    pub submerged: bool,
    pub depth: f32,
    pub blend: f32,
}

pub fn sync_underwater_view(
    map: Res<TerrainMap>,
    atmosphere_state: Res<TerrainAtmosphere>,
    mut state: ResMut<UnderwaterState>,
    mut fog: ResMut<TerrainFog>,
    mut clear: ResMut<ClearColor>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
) {
    let Some(transform) = cameras.iter().next() else {
        return;
    };
    let position = transform.translation();
    let sample = map.water_surface_at(position.x, position.z);
    let depth = match &sample {
        Some(sample) if !sample.frozen => (sample.surface_y - position.y).max(0.0),
        _ => 0.0,
    };
    state.depth = depth;
    state.blend = underwater_blend(depth);
    state.submerged = depth > 0.0;
    if state.blend <= 0.0 {
        return;
    }
    let tint = underwater_tint(depth);
    let visibility = underwater_visibility_at(atmosphere_state.profile.visibility, depth);
    let blend = state.blend;
    fog.0.color = fog.0.color.mix(&tint, blend);
    fog.0.directional_light_color = fog.0.directional_light_color.mix(&tint, blend);
    fog.0.falloff = FogFalloff::from_visibility_colors(visibility, fog.0.color, fog.0.color);
    clear.0 = clear.0.mix(&tint, blend);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::region::REGION_TABLE;

    #[test]
    fn underwater_state_tracks_the_camera_depth() {
        let map = TerrainMap::compact();
        let deepest = (0..map.heights().len())
            .map(|index| {
                let (x, z) = map.heights().coords(index);
                let point = map.heights().world_of(x, z);
                map.depth_below_sea(point.x, point.y)
            })
            .fold(0.0f32, f32::max);
        assert!(deepest > 0.0, "a compact world must have submerged ground");
        assert!(
            underwater_visibility(deepest) > 0.0,
            "underwater visibility must stay positive"
        );
        assert_eq!(
            underwater_blend(0.0),
            0.0,
            "grazing the surface must not snap the view underwater"
        );
        assert!(underwater_blend(0.5) < 0.35, "the first half metre must ease in");
        assert_eq!(underwater_blend(2.0), 1.0, "two metres must be fully submerged");
    }

    #[test]
    fn every_region_has_an_atmosphere_profile() {
        assert_eq!(REGION_ATMOSPHERE_TABLE.len(), REGION_TABLE.len());
        for definition in REGION_TABLE.iter() {
            let profile = atmosphere_profile(definition.region);
            assert_eq!(profile.region, definition.region);
            assert!(profile.visibility > 0.0);
            assert!(profile.light_exponent > 0.0);
            assert!(profile.ambient_boost > 0.0);
        }
    }

    #[test]
    fn region_profiles_stay_inside_one_palette() {
        let mut min_vis = f32::INFINITY;
        let mut max_vis = f32::NEG_INFINITY;
        let mut min_amb = f32::INFINITY;
        let mut max_amb = f32::NEG_INFINITY;
        let mut min_lum = f32::INFINITY;
        let mut max_lum = f32::NEG_INFINITY;
        for profile in REGION_ATMOSPHERE_TABLE.iter() {
            min_vis = min_vis.min(profile.visibility);
            max_vis = max_vis.max(profile.visibility);
            min_amb = min_amb.min(profile.ambient_boost);
            max_amb = max_amb.max(profile.ambient_boost);
            let linear = profile.fog_colour.to_linear();
            let luminance =
                0.2126 * linear.red + 0.7152 * linear.green + 0.0722 * linear.blue;
            min_lum = min_lum.min(luminance);
            max_lum = max_lum.max(luminance);
        }
        assert!(
            max_vis - min_vis < 40.0,
            "visibility band too wide: {min_vis}..{max_vis}"
        );
        assert!(
            min_vis > 100.0 && max_vis < 180.0,
            "visibility must stay near the baseline: {min_vis}..{max_vis}"
        );
        assert!(
            max_amb - min_amb < 0.15,
            "ambient band too wide: {min_amb}..{max_amb}"
        );
        assert!(
            max_lum - min_lum < 0.25,
            "fog colours must not jump between regions: luminance {min_lum}..{max_lum}"
        );
    }

    #[test]
    fn blending_one_profile_is_the_identity() {
        let profile = *atmosphere_profile(Region::DesertBarren);
        let blended = blend_atmosphere(&[(1.0, profile)]);
        assert!(
            colour_close(blended.fog_colour, profile.fog_colour),
            "a unit blend must preserve the fog colour"
        );
        assert_eq!(blended.region, profile.region);
        assert!((blended.visibility - profile.visibility).abs() < 1e-4);
    }

    fn colour_close(a: Color, b: Color) -> bool {
        let (a, b) = (a.to_linear(), b.to_linear());
        (a.red - b.red).abs() < 1e-4
            && (a.green - b.green).abs() < 1e-4
            && (a.blue - b.blue).abs() < 1e-4
    }

    #[test]
    fn blending_is_weight_ordered() {
        let cold = *atmosphere_profile(Region::GlacierIcefield);
        let hot = *atmosphere_profile(Region::DesertBarren);
        let mostly_cold = blend_atmosphere(&[(0.9, cold), (0.1, hot)]);
        let mostly_hot = blend_atmosphere(&[(0.1, cold), (0.9, hot)]);
        assert!(mostly_cold.visibility < mostly_hot.visibility);
        assert!(mostly_cold.sky_zenith != mostly_hot.sky_zenith);
    }

    #[test]
    fn blending_is_weight_normalised() {
        let cold = *atmosphere_profile(Region::GlacierIcefield);
        let hot = *atmosphere_profile(Region::DesertBarren);
        let unit = blend_atmosphere(&[(1.0, cold), (1.0, hot)]);
        let doubled = blend_atmosphere(&[(2.0, cold), (2.0, hot)]);
        assert!((unit.visibility - doubled.visibility).abs() < 1e-4);
        assert_eq!(unit.fog_colour, doubled.fog_colour);
    }

    #[test]
    fn empty_blend_falls_back_instead_of_panicking() {
        let fallback = blend_atmosphere(&[]);
        assert!(fallback.visibility > 0.0);
        let zeroed = blend_atmosphere(&[(0.0, *atmosphere_profile(Region::Taiga))]);
        assert!(zeroed.visibility > 0.0);
    }

    #[test]
    fn fog_conversion_preserves_visibility_and_colour() {
        let profile = *atmosphere_profile(Region::Taiga);
        let fog = to_distance_fog(&profile);
        assert_eq!(fog.color, profile.fog_colour);
        assert_eq!(fog.directional_light_color, profile.light_colour);
        assert!((fog.directional_light_exponent - profile.light_exponent).abs() < 1e-6);
    }

    #[test]
    fn the_sun_rides_higher_in_summer_than_winter() {
        let summer = solar_elevation(0.25);
        let winter = solar_elevation(0.75);
        assert!(summer > winter, "the declination sweep must lift the summer sun");
        assert!(summer > 1.0 && summer < 1.45);
        assert!(winter > 0.2 && winter < 0.6);

        let profile = *atmosphere_profile(Region::TemperateGrassland);
        assert!(sun_illuminance(&profile, summer) > sun_illuminance(&profile, winter));
        assert!(sun_direction(0.0).length() - 1.0 < 1e-4);
        assert!(sun_direction(0.25).y > sun_direction(0.75).y);
        assert!(sun_direction(0.75).y > 0.0, "the sun must stay above the horizon");

        let summer_colour = sun_colour(&profile, summer).to_linear();
        let winter_colour = sun_colour(&profile, winter).to_linear();
        let winter_amber = winter_colour.red - winter_colour.green;
        let summer_amber = summer_colour.red - summer_colour.green;
        assert!(
            winter_amber > summer_amber,
            "low winter sun must redden"
        );
        assert!(ambient_brightness(&profile, summer) > ambient_brightness(&profile, winter));
    }

    #[test]
    fn underwater_transition_blends_over_two_metres() {
        assert_eq!(underwater_blend(0.0), 0.0, "the surface must not snap");
        assert!(underwater_blend(0.01) < 0.001, "the blend must ease in smoothly");
        assert!((underwater_blend(1.0) - 0.5).abs() < 1e-5);
        assert_eq!(underwater_blend(2.0), 1.0);
        assert_eq!(underwater_blend(50.0), 1.0);
        let mut previous = 0.0f32;
        for step in 0..=100 {
            let depth = step as f32 / 50.0;
            let blend = underwater_blend(depth);
            assert!(blend >= previous, "the blend must be monotonic");
            previous = blend;
        }
        assert_eq!(underwater_visibility_at(150.0, 0.0), 150.0);
        assert!(underwater_visibility_at(150.0, 10.0) < 150.0);
        assert!(underwater_visibility_at(150.0, 10.0) >= 4.0);
    }

    #[test]
    fn underwater_tint_darkens_with_depth() {
        let shallow = underwater_tint(0.0).to_linear();
        let deep = underwater_tint(40.0).to_linear();
        assert!(deep.blue < shallow.blue + 1e-6);
        assert!(deep.red < shallow.red);
        assert!(underwater_visibility(0.0) > underwater_visibility(20.0));
        assert!(underwater_visibility(1000.0) >= 4.0);
    }
}
