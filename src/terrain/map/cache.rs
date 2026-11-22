use std::path::PathBuf;

use bevy::prelude::Vec2;

use crate::terrain::field::ScalarField;
use crate::terrain::hydrology::Waterfall;
use crate::terrain::season::{SeaLevelSeason, Season};
use crate::terrain::topology::WorldTopology;

use super::{TerrainConfig, TerrainMap};

const CACHE_MAGIC: [u8; 4] = *b"BTW1";
const CACHE_VERSION: u32 = 1;
const TOPOLOGY_LABEL_WIDTH: usize = 16;

#[derive(Clone, Debug)]
pub enum TerrainSource {
    Generated(TerrainConfig),
    StaticAsset(StaticHeightmap),
}

#[derive(Clone, Debug)]
pub struct StaticHeightmap {
    pub path: PathBuf,
    pub size: f32,
    pub relief: f32,
    pub shoreline_level: f32,
    pub topology: WorldTopology,
    pub seed: u64,
}

impl Default for StaticHeightmap {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            size: TerrainConfig::default().size,
            relief: 180.0,
            shoreline_level: 0.45,
            topology: WorldTopology::default(),
            seed: 0,
        }
    }
}

impl StaticHeightmap {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }

    pub fn load_field(&self) -> Result<ScalarField, HeightmapError> {
        let image = image::open(&self.path).map_err(|reason| HeightmapError::Unreadable {
            path: self.path.clone(),
            reason: reason.to_string(),
        })?;
        let grey = image.to_luma16();
        let width = grey.width() as usize;
        let height = grey.height() as usize;
        if width != height {
            return Err(HeightmapError::NotSquare {
                path: self.path.clone(),
                width,
                height,
            });
        }
        if width < 2 {
            return Err(HeightmapError::TooSmall {
                path: self.path.clone(),
                width,
            });
        }
        let sea = SeaLevelSeason::default().level_at(Season::Summer);
        let mut field = ScalarField::new(width, self.size, self.topology);
        for z in 0..height {
            for x in 0..width {
                let raw = grey.get_pixel(x as u32, z as u32).0[0] as f32 / 65535.0;
                field.set(x, z, (raw - self.shoreline_level) * self.relief + sea);
            }
        }
        Ok(field)
    }

    pub fn load_map(&self) -> Result<TerrainMap, HeightmapError> {
        TerrainMap::from_static_heightmap(self)
    }
}

#[derive(Clone, Debug)]
pub enum HeightmapError {
    Unreadable { path: PathBuf, reason: String },
    NotSquare { path: PathBuf, width: usize, height: usize },
    TooSmall { path: PathBuf, width: usize },
    Unsupported { detail: String },
}

impl std::fmt::Display for HeightmapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeightmapError::Unreadable { path, reason } => {
                write!(formatter, "{} is not a readable heightmap: {reason}", path.display())
            }
            HeightmapError::NotSquare { path, width, height } => write!(
                formatter,
                "{} is {width}x{height}; a heightmap must be square",
                path.display()
            ),
            HeightmapError::TooSmall { path, width } => {
                write!(formatter, "{} is only {width} texels wide", path.display())
            }
            HeightmapError::Unsupported { detail } => write!(formatter, "{detail}"),
        }
    }
}

impl std::error::Error for HeightmapError {}

#[derive(Clone, Debug)]
pub struct TerrainCache {
    pub dir: PathBuf,
}

impl TerrainCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn key_for(config: &TerrainConfig) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in format!("v{CACHE_VERSION}:{config:?}").as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }

    pub fn path_for(&self, config: &TerrainConfig) -> PathBuf {
        self.dir
            .join(format!("{:016x}.terrain", Self::key_for(config)))
    }

    pub fn load(&self, config: &TerrainConfig) -> Option<CachedWorld> {
        let bytes = std::fs::read(self.path_for(config)).ok()?;
        CachedWorld::decode(&bytes)
    }

    pub fn store(&self, config: &TerrainConfig, map: &TerrainMap) {
        let world = CachedWorld {
            dim: map.heights.dim as u32,
            size: map.heights.size,
            topology: map.heights.topology,
            heights: map.base_heights.values.clone(),
            precipitation: map.precipitation.values.clone(),
            waterfalls: map.waterfalls.iter().map(CachedWaterfall::from).collect(),
        };
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let _ = std::fs::write(self.path_for(config), world.encode());
    }
}

#[derive(Clone, Debug)]
pub struct CachedWorld {
    pub dim: u32,
    pub size: f32,
    pub topology: WorldTopology,
    pub heights: Vec<f32>,
    pub precipitation: Vec<f32>,
    pub waterfalls: Vec<CachedWaterfall>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CachedWaterfall {
    pub index: u32,
    pub next: u32,
    pub drop: f32,
    pub lip: [f32; 2],
    pub plunge: [f32; 2],
    pub pool_depth: f32,
    pub pool_radius: f32,
}

impl From<&Waterfall> for CachedWaterfall {
    fn from(fall: &Waterfall) -> Self {
        Self {
            index: fall.index as u32,
            next: fall.next as u32,
            drop: fall.drop,
            lip: [fall.lip.x, fall.lip.y],
            plunge: [fall.plunge.x, fall.plunge.y],
            pool_depth: fall.pool_depth,
            pool_radius: fall.pool_radius,
        }
    }
}

impl CachedWorld {
    pub(super) fn to_waterfalls(&self, len: usize) -> Vec<Waterfall> {
        self.waterfalls
            .iter()
            .filter(|fall| fall.index < len as u32 && fall.next < len as u32)
            .map(|fall| Waterfall {
                index: fall.index as usize,
                next: fall.next as usize,
                drop: fall.drop,
                lip: Vec2::new(fall.lip[0], fall.lip[1]),
                plunge: Vec2::new(fall.plunge[0], fall.plunge[1]),
                pool_depth: fall.pool_depth,
                pool_radius: fall.pool_radius,
            })
            .collect()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        fn put_u32(out: &mut Vec<u8>, value: u32) {
            out.extend_from_slice(&value.to_le_bytes());
        }
        fn put_f32(out: &mut Vec<u8>, value: f32) {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&CACHE_MAGIC);
        put_u32(&mut out, CACHE_VERSION);
        put_u32(&mut out, self.dim);
        put_f32(&mut out, self.size);
        let label = self.topology.label();
        let mut slot = [0u8; TOPOLOGY_LABEL_WIDTH];
        slot[..label.len().min(TOPOLOGY_LABEL_WIDTH)].copy_from_slice(&label.as_bytes()[..label.len().min(TOPOLOGY_LABEL_WIDTH)]);
        out.extend_from_slice(&slot);
        put_u32(&mut out, self.heights.len() as u32);
        for value in &self.heights {
            put_f32(&mut out, *value);
        }
        put_u32(&mut out, self.precipitation.len() as u32);
        for value in &self.precipitation {
            put_f32(&mut out, *value);
        }
        put_u32(&mut out, self.waterfalls.len() as u32);
        for fall in &self.waterfalls {
            put_u32(&mut out, fall.index);
            put_u32(&mut out, fall.next);
            put_f32(&mut out, fall.drop);
            put_f32(&mut out, fall.lip[0]);
            put_f32(&mut out, fall.lip[1]);
            put_f32(&mut out, fall.plunge[0]);
            put_f32(&mut out, fall.plunge[1]);
            put_f32(&mut out, fall.pool_depth);
            put_f32(&mut out, fall.pool_radius);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<CachedWorld> {
        struct Reader<'a> {
            bytes: &'a [u8],
            cursor: usize,
        }
        impl Reader<'_> {
            fn take(&mut self, count: usize) -> Option<&[u8]> {
                let end = self.cursor.checked_add(count)?;
                if end > self.bytes.len() {
                    return None;
                }
                let slice = &self.bytes[self.cursor..end];
                self.cursor = end;
                Some(slice)
            }
            fn u32(&mut self) -> Option<u32> {
                Some(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
            }
            fn f32(&mut self) -> Option<f32> {
                Some(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
            }
            fn f32s(&mut self, count: usize) -> Option<Vec<f32>> {
                let mut out = Vec::with_capacity(count);
                for _ in 0..count {
                    out.push(self.f32()?);
                }
                Some(out)
            }
        }
        let mut reader = Reader { bytes, cursor: 0 };
        if reader.take(4)? != CACHE_MAGIC {
            return None;
        }
        if reader.u32()? != CACHE_VERSION {
            return None;
        }
        let dim = reader.u32()?;
        let size = reader.f32()?;
        let label = reader.take(TOPOLOGY_LABEL_WIDTH)?;
        let end = label.iter().position(|byte| *byte == 0).unwrap_or(label.len());
        let topology = WorldTopology::parse(std::str::from_utf8(&label[..end]).ok()?)?;
        let heights_len = reader.u32()? as usize;
        let heights = reader.f32s(heights_len)?;
        let precipitation_len = reader.u32()? as usize;
        let precipitation = reader.f32s(precipitation_len)?;
        let fall_count = reader.u32()? as usize;
        let mut waterfalls = Vec::with_capacity(fall_count);
        for _ in 0..fall_count {
            waterfalls.push(CachedWaterfall {
                index: reader.u32()?,
                next: reader.u32()?,
                drop: reader.f32()?,
                lip: [reader.f32()?, reader.f32()?],
                plunge: [reader.f32()?, reader.f32()?],
                pool_depth: reader.f32()?,
                pool_radius: reader.f32()?,
            });
        }
        Some(CachedWorld {
            dim,
            size,
            topology,
            heights,
            precipitation,
            waterfalls,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_world_bytes_round_trip_exactly() {
        let world = CachedWorld {
            dim: 4,
            size: 12.5,
            topology: WorldTopology::FiniteTorus,
            heights: (0..16).map(|i| i as f32 * 0.5 - 3.25).collect(),
            precipitation: vec![0.25; 16],
            waterfalls: vec![CachedWaterfall {
                index: 3,
                next: 7,
                drop: 4.5,
                lip: [1.0, -2.0],
                plunge: [3.0, -4.0],
                pool_depth: 2.5,
                pool_radius: 3.5,
            }],
        };
        let decoded = CachedWorld::decode(&world.encode()).expect("round trip");
        assert_eq!(decoded.dim, world.dim);
        assert_eq!(decoded.size, world.size);
        assert_eq!(decoded.topology, world.topology);
        assert_eq!(decoded.heights, world.heights);
        assert_eq!(decoded.precipitation, world.precipitation);
        assert_eq!(decoded.waterfalls.len(), 1);
        assert_eq!(decoded.waterfalls[0].next, 7);
        assert_eq!(decoded.waterfalls[0].plunge, [3.0, -4.0]);

        assert!(CachedWorld::decode(&[]).is_none());
        assert!(CachedWorld::decode(b"junk").is_none());
        let mut truncated = world.encode();
        truncated.truncate(truncated.len() - 4);
        assert!(CachedWorld::decode(&truncated).is_none());
    }

    fn heightmap_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("bevy_game_heightmap_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn heightmap_path(tag: &str, name: &str) -> PathBuf {
        heightmap_dir(tag).join(name)
    }

    fn ramp_texel(x: u32, z: u32, width: u32, height: u32) -> f32 {
        (((x + z) as f32 / (width + height) as f32 * 255.0) as u8) as f32 / 255.0
    }

    fn write_heightmap(tag: &str, name: &str, width: u32, height: u32) -> PathBuf {
        let path = heightmap_path(tag, name);
        let mut image = image::GrayImage::new(width, height);
        for z in 0..height {
            for x in 0..width {
                let value = (ramp_texel(x, z, width, height) * 255.0) as u8;
                image.put_pixel(x, z, image::Luma([value]));
            }
        }
        image.save(&path).expect("write the png heightmap");
        path
    }

    #[test]
    fn a_static_heightmap_becomes_a_world() {
        let path = write_heightmap("ramp", "ramp.png", 32, 32);
        let asset = StaticHeightmap {
            size: 120.0,
            relief: 60.0,
            shoreline_level: 0.5,
            ..StaticHeightmap::new(&path)
        };
        let map = TerrainMap::from_source(&TerrainSource::StaticAsset(asset.clone()))
            .unwrap_or_else(|error| panic!("the ramp heightmap must load: {error}"));
        let again =
            asset.load_map().unwrap_or_else(|error| panic!("loading twice works: {error}"));
        assert_eq!(map.checksum(), again.checksum(), "loading is deterministic");

        assert_eq!(map.heights().dim, 32);
        assert!((map.size() - 120.0).abs() < 1e-3);
        assert!(!map.cells().is_empty(), "the parameter layer still runs");
        assert!(!map.hardness_field().values.is_empty());

        let sea = map.sea_levels().level_at(Season::Summer);
        let low = map.heights().world_of(0, 0);
        let high = map.heights().world_of(31, 31);
        let texel_at = |x: u32, z: u32| (ramp_texel(x, z, 32, 32) - 0.5) * 60.0 + sea;
        assert!(
            (map.height_at(low.x, low.y) - texel_at(0, 0)).abs() < 1e-3,
            "the darkest texel sits half a relief below the shoreline"
        );
        assert!(
            (map.height_at(high.x, high.y) - texel_at(31, 31)).abs() < 1e-3,
            "the brightest texel sits above the shoreline"
        );
        assert!(map.is_underwater(low.x, low.y));
        assert!(!map.is_underwater(high.x, high.y));
        assert!(map.slope_at(high.x, high.y) > 0.0, "a ramp must have slope");
        let info = map.surface_info_at(low.x, low.y);
        assert!(info.is_submerged(), "the dark corner is flooded");
        let _ = std::fs::remove_dir_all(heightmap_dir("ramp"));
    }

    #[test]
    fn heightmap_failures_are_reported() {
        let missing = StaticHeightmap::new(heightmap_path("failures", "absent.png"));
        assert!(matches!(
            missing.load_field(),
            Err(HeightmapError::Unreadable { .. })
        ));
        assert!(matches!(
            TerrainMap::from_source(&TerrainSource::StaticAsset(missing)),
            Err(HeightmapError::Unreadable { .. })
        ));

        let wide = write_heightmap("failures", "wide.png", 16, 8);
        let asset = StaticHeightmap::new(&wide);
        let error = asset.load_field().expect_err("a non-square map must fail");
        assert!(matches!(error, HeightmapError::NotSquare { .. }));
        assert!(!error.to_string().is_empty());
        assert!(matches!(
            TerrainMap::from_source(&TerrainSource::StaticAsset(asset)),
            Err(HeightmapError::NotSquare { .. })
        ));

        let junk = heightmap_path("failures", "junk.png");
        std::fs::write(&junk, b"this is not a png").unwrap();
        assert!(matches!(
            StaticHeightmap::new(junk).load_field(),
            Err(HeightmapError::Unreadable { .. })
        ));
        let _ = std::fs::remove_dir_all(heightmap_dir("failures"));
    }
}
