use bevy::prelude::*;

#[cfg(test)]
use crate::terrain::params::{ParamLevel, SlopeMagnitude, Substrate};
use crate::terrain::region::Region;

#[cfg(test)]
pub const TEXTURE_ROOT: &str = "textures/terrain";
pub const DISK_ROOT: &str = "assets/textures/terrain";
#[cfg(test)]
pub const TEXTURE_RESOLUTION: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceLayer {
    Sand,
    Dune,
    Silt,
    Gravel,
    Rock,
    Moss,
    Soil,
    Volcanic,
    Snow,
}

impl SurfaceLayer {
    pub const ALL: [SurfaceLayer; 9] = [
        SurfaceLayer::Sand,
        SurfaceLayer::Dune,
        SurfaceLayer::Silt,
        SurfaceLayer::Gravel,
        SurfaceLayer::Rock,
        SurfaceLayer::Moss,
        SurfaceLayer::Soil,
        SurfaceLayer::Volcanic,
        SurfaceLayer::Snow,
    ];

    pub fn dir(self) -> &'static str {
        match self {
            SurfaceLayer::Sand => "sand",
            SurfaceLayer::Dune => "dune",
            SurfaceLayer::Silt => "silt",
            SurfaceLayer::Gravel => "gravel",
            SurfaceLayer::Rock => "rock",
            SurfaceLayer::Moss => "moss",
            SurfaceLayer::Soil => "soil",
            SurfaceLayer::Volcanic => "volcanic",
            SurfaceLayer::Snow => "snow",
        }
    }

    #[cfg(test)]
    pub fn albedo(self) -> String {
        format!("{}/{}/{}_albedo.jpg", TEXTURE_ROOT, self.dir(), self.dir())
    }

    #[cfg(test)]
    pub fn normal(self) -> String {
        format!("{}/{}/{}_normal.png", TEXTURE_ROOT, self.dir(), self.dir())
    }

    #[cfg(test)]
    pub fn orm(self) -> String {
        format!("{}/{}/{}_orm.png", TEXTURE_ROOT, self.dir(), self.dir())
    }

    #[cfg(test)]
    pub fn required_paths() -> Vec<String> {
        SurfaceLayer::ALL
            .iter()
            .flat_map(|layer| [layer.albedo(), layer.normal(), layer.orm()])
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct RegionMaterialProfile {
    pub region: Region,
    pub base: SurfaceLayer,
    pub cliff: SurfaceLayer,
    pub tint: Color,
    pub roughness: f32,
    pub reflectance: f32,
}

const fn profile(
    region: Region,
    base: SurfaceLayer,
    cliff: SurfaceLayer,
    tint: [u8; 3],
    roughness: f32,
    reflectance: f32,
) -> RegionMaterialProfile {
    RegionMaterialProfile {
        region,
        base,
        cliff,
        tint: Color::srgb(
            tint[0] as f32 / 255.0,
            tint[1] as f32 / 255.0,
            tint[2] as f32 / 255.0,
        ),
        roughness,
        reflectance,
    }
}

pub static REGION_MATERIAL_TABLE: [RegionMaterialProfile; 36] = [
    profile(Region::AbyssalTrench, SurfaceLayer::Silt, SurfaceLayer::Rock, [90, 104, 122], 0.55, 0.3),
    profile(Region::DeepOceanBasin, SurfaceLayer::Silt, SurfaceLayer::Rock, [104, 118, 132], 0.6, 0.25),
    profile(Region::HydrothermalVentField, SurfaceLayer::Volcanic, SurfaceLayer::Rock, [70, 62, 60], 0.7, 0.24),
    profile(Region::KelpForest, SurfaceLayer::Gravel, SurfaceLayer::Rock, [86, 118, 96], 0.66, 0.2),
    profile(Region::CoralReef, SurfaceLayer::Sand, SurfaceLayer::Gravel, [188, 206, 190], 0.42, 0.35),
    profile(Region::SeagrassMeadow, SurfaceLayer::Silt, SurfaceLayer::Gravel, [96, 126, 110], 0.68, 0.2),
    profile(Region::BarrenSeabed, SurfaceLayer::Gravel, SurfaceLayer::Rock, [150, 156, 146], 0.68, 0.2),
    profile(Region::SeaIce, SurfaceLayer::Snow, SurfaceLayer::Rock, [222, 232, 242], 0.36, 0.44),
    profile(Region::MangroveSwamp, SurfaceLayer::Silt, SurfaceLayer::Soil, [88, 112, 82], 0.8, 0.14),
    profile(Region::TidalFlat, SurfaceLayer::Silt, SurfaceLayer::Sand, [146, 138, 118], 0.7, 0.2),
    profile(Region::SandyBeachDunes, SurfaceLayer::Dune, SurfaceLayer::Sand, [214, 196, 156], 0.8, 0.14),
    profile(Region::RockyCoast, SurfaceLayer::Rock, SurfaceLayer::Rock, [132, 128, 120], 0.82, 0.12),
    profile(Region::RiverDelta, SurfaceLayer::Silt, SurfaceLayer::Sand, [122, 126, 100], 0.84, 0.12),
    profile(Region::FrozenShore, SurfaceLayer::Snow, SurfaceLayer::Rock, [196, 206, 216], 0.52, 0.36),
    profile(Region::TropicalRainforest, SurfaceLayer::Soil, SurfaceLayer::Rock, [72, 104, 60], 0.9, 0.08),
    profile(Region::TropicalMonsoonForest, SurfaceLayer::Soil, SurfaceLayer::Rock, [80, 112, 66], 0.89, 0.08),
    profile(Region::TropicalDeciduousForest, SurfaceLayer::Soil, SurfaceLayer::Rock, [94, 120, 72], 0.88, 0.08),
    profile(Region::Savanna, SurfaceLayer::Soil, SurfaceLayer::Gravel, [168, 152, 96], 0.86, 0.1),
    profile(Region::DesertBarren, SurfaceLayer::Dune, SurfaceLayer::Gravel, [212, 176, 122], 0.82, 0.12),
    profile(Region::WetlandMarsh, SurfaceLayer::Silt, SurfaceLayer::Soil, [92, 106, 84], 0.9, 0.1),
    profile(Region::TemperateGrassland, SurfaceLayer::Soil, SurfaceLayer::Rock, [112, 138, 78], 0.86, 0.1),
    profile(Region::TemperateForest, SurfaceLayer::Soil, SurfaceLayer::Rock, [86, 110, 68], 0.88, 0.08),
    profile(Region::MediterraneanScrubland, SurfaceLayer::Soil, SurfaceLayer::Rock, [130, 138, 92], 0.84, 0.1),
    profile(Region::Taiga, SurfaceLayer::Moss, SurfaceLayer::Rock, [78, 98, 76], 0.88, 0.08),
    profile(Region::ArcticTundra, SurfaceLayer::Snow, SurfaceLayer::Moss, [176, 184, 190], 0.72, 0.2),
    profile(Region::PlateauMesa, SurfaceLayer::Gravel, SurfaceLayer::Rock, [166, 150, 128], 0.8, 0.14),
    profile(Region::BadlandsCanyonlands, SurfaceLayer::Silt, SurfaceLayer::Rock, [176, 128, 96], 0.86, 0.08),
    profile(Region::HighlandSteppeDesert, SurfaceLayer::Gravel, SurfaceLayer::Rock, [164, 160, 150], 0.8, 0.14),
    profile(Region::MontaneForest, SurfaceLayer::Moss, SurfaceLayer::Rock, [82, 102, 74], 0.88, 0.08),
    profile(Region::HighlandMoorland, SurfaceLayer::Moss, SurfaceLayer::Silt, [96, 104, 84], 0.9, 0.08),
    profile(Region::KarstHills, SurfaceLayer::Gravel, SurfaceLayer::Rock, [158, 156, 144], 0.78, 0.16),
    profile(Region::SubalpineForest, SurfaceLayer::Moss, SurfaceLayer::Rock, [74, 94, 72], 0.89, 0.08),
    profile(Region::AlpineMeadow, SurfaceLayer::Moss, SurfaceLayer::Rock, [116, 140, 96], 0.86, 0.1),
    profile(Region::ScreeTalus, SurfaceLayer::Gravel, SurfaceLayer::Rock, [138, 134, 126], 0.84, 0.12),
    profile(Region::GlacierIcefield, SurfaceLayer::Snow, SurfaceLayer::Rock, [226, 234, 244], 0.34, 0.46),
    profile(Region::VolcanicCaldera, SurfaceLayer::Volcanic, SurfaceLayer::Rock, [96, 84, 80], 0.74, 0.2),
];

pub fn material_profile(region: Region) -> &'static RegionMaterialProfile {
    REGION_MATERIAL_TABLE
        .iter()
        .find(|entry| entry.region == region)
        .unwrap_or(&REGION_MATERIAL_TABLE[0])
}

#[cfg(test)]
pub fn cliff_layer_for(substrate: Substrate, slope: SlopeMagnitude) -> SurfaceLayer {
    if slope.index() >= SlopeMagnitude::Steep.index() {
        return match substrate {
            Substrate::Sand | Substrate::SiltClay => SurfaceLayer::Silt,
            Substrate::LimestoneKarst => SurfaceLayer::Gravel,
            Substrate::RockCrystalline => SurfaceLayer::Rock,
            Substrate::Volcanic => SurfaceLayer::Volcanic,
        };
    }
    match substrate {
        Substrate::Sand => SurfaceLayer::Sand,
        Substrate::SiltClay => SurfaceLayer::Silt,
        Substrate::LimestoneKarst => SurfaceLayer::Gravel,
        Substrate::RockCrystalline => SurfaceLayer::Rock,
        Substrate::Volcanic => SurfaceLayer::Volcanic,
    }
}

pub fn required_disk_paths(root: &str) -> Vec<String> {
    SurfaceLayer::ALL
        .iter()
        .flat_map(|layer| {
            [
                format!("{root}/{dir}/{dir}_albedo.jpg", dir = layer.dir()),
                format!("{root}/{dir}/{dir}_normal.png", dir = layer.dir()),
                format!("{root}/{dir}/{dir}_orm.png", dir = layer.dir()),
            ]
        })
        .collect()
}

pub fn validate_on_disk(root: &str) -> Vec<String> {
    required_disk_paths(root)
        .into_iter()
        .filter(|path| !std::path::Path::new(path).exists())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub enum DebugMaterialMode {
    Textured,
    FlatColor,
    UvGrid,
}

impl DebugMaterialMode {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "textured" => Some(DebugMaterialMode::Textured),
            "flat_color" | "flat" => Some(DebugMaterialMode::FlatColor),
            "uv_grid" | "uv" => Some(DebugMaterialMode::UvGrid),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DebugMaterialMode::Textured => "textured",
            DebugMaterialMode::FlatColor => "flat_color",
            DebugMaterialMode::UvGrid => "uv_grid",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::region::REGION_TABLE;

    #[test]
    fn every_region_has_a_material_profile() {
        for definition in REGION_TABLE.iter() {
            let profile = material_profile(definition.region);
            assert_eq!(profile.region, definition.region);
            assert!(SurfaceLayer::ALL.contains(&profile.base));
            assert!(SurfaceLayer::ALL.contains(&profile.cliff));
            assert!(profile.roughness > 0.0 && profile.roughness <= 1.0);
        }
    }

    #[test]
    fn material_table_covers_all_regions_exactly_once() {
        assert_eq!(REGION_MATERIAL_TABLE.len(), REGION_TABLE.len());
        for definition in REGION_TABLE.iter() {
            let hits = REGION_MATERIAL_TABLE
                .iter()
                .filter(|entry| entry.region == definition.region)
                .count();
            assert_eq!(hits, 1, "{:?} must appear exactly once", definition.region);
        }
    }

    #[test]
    fn layer_paths_are_unique_and_well_formed() {
        let paths = SurfaceLayer::required_paths();
        assert_eq!(paths.len(), SurfaceLayer::ALL.len() * 3);
        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(unique.len(), paths.len(), "texture paths must not collide");
        for path in &paths {
            assert!(path.starts_with(TEXTURE_ROOT));
            assert!(path.ends_with(".jpg") || path.ends_with(".png"));
        }
    }

    #[test]
    fn cliff_layer_hardens_with_slope() {
        assert_eq!(
            cliff_layer_for(Substrate::RockCrystalline, SlopeMagnitude::Sheer),
            SurfaceLayer::Rock
        );
        assert_eq!(
            cliff_layer_for(Substrate::Volcanic, SlopeMagnitude::Steep),
            SurfaceLayer::Volcanic
        );
        assert_eq!(
            cliff_layer_for(Substrate::Sand, SlopeMagnitude::Flat),
            SurfaceLayer::Sand
        );
    }

    #[test]
    fn downloaded_texture_sets_are_all_on_disk() {
        let missing = validate_on_disk(DISK_ROOT);
        assert!(missing.is_empty(), "missing PBR textures: {missing:?}");
    }

    #[test]
    fn texture_sets_are_uniformly_sized_and_decodable() {
        let paths = required_disk_paths(DISK_ROOT);
        assert_eq!(paths.len(), SurfaceLayer::ALL.len() * 3);
        for path in &paths {
            let opened = image::open(path).unwrap_or_else(|error| panic!("{path}: {error}"));
            let width = opened.width();
            let height = opened.height();
            assert_eq!(width, height, "{path} must be square");
            assert!(width.is_power_of_two(), "{path} must be power-of-two");
            assert_eq!(width, TEXTURE_RESOLUTION, "{path} must be normalized");
            let colour = opened.color();
            assert!(
                colour.channel_count() >= 3,
                "{path} must carry at least three channels"
            );
            assert!(
                colour.bits_per_pixel() >= 24,
                "{path} must be at least 8-bit per channel"
            );
            assert_eq!(opened.to_rgb8().width(), TEXTURE_RESOLUTION);
            assert_eq!(opened.to_rgb8().height(), TEXTURE_RESOLUTION);
        }
    }

    #[test]
    fn albedo_slots_are_opaque_and_normal_slots_carry_linear_data() {
        for layer in SurfaceLayer::ALL {
            let albedo = image::open(format!("{DISK_ROOT}/{d}/{d}_albedo.jpg", d = layer.dir()))
                .expect("albedo must decode");
            assert!(
                !albedo.color().has_alpha(),
                "{} albedo must be opaque sRGB",
                layer.dir()
            );
            for slot in ["normal", "orm"] {
                let linear =
                    image::open(format!("{DISK_ROOT}/{d}/{d}_{slot}.png", d = layer.dir()))
                        .expect("linear map must decode");
                assert!(
                    linear.color().channel_count() >= 3,
                    "{} {slot} must pack its channels in RGB",
                    layer.dir()
                );
            }
        }
    }

    #[test]
    fn every_profile_slot_resolves_to_a_complete_pbr_set() {
        for entry in REGION_MATERIAL_TABLE.iter() {
            for layer in [entry.base, entry.cliff] {
                let set = [
                    format!("{DISK_ROOT}/{d}/{d}_albedo.jpg", d = layer.dir()),
                    format!("{DISK_ROOT}/{d}/{d}_normal.png", d = layer.dir()),
                    format!("{DISK_ROOT}/{d}/{d}_orm.png", d = layer.dir()),
                ];
                for path in set {
                    assert!(
                        std::path::Path::new(&path).exists(),
                        "{:?} slot {:?} has no texture on disk: {path}",
                        entry.region,
                        layer
                    );
                }
            }
        }
    }

    #[test]
    fn texture_manifest_records_license_per_layer() {
        let raw = std::fs::read_to_string(format!("{DISK_ROOT}/manifest.json"))
            .expect("texture manifest must exist");
        assert!(raw.contains("CC0 1.0"), "manifest must record the license");
        assert!(
            raw.contains("polyhaven.com/a/"),
            "manifest must record the upstream source"
        );
        for layer in SurfaceLayer::ALL {
            assert!(
                raw.contains(layer.dir()),
                "manifest must record layer {}",
                layer.dir()
            );
        }
    }
}
