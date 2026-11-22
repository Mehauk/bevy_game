use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDataOrder, TextureDimension, TextureFormat,
};
use image::{ImageBuffer, RgbaImage};

use crate::terrain::chunk::clamped_chunk_bounds;
use crate::terrain::map::TerrainMap;
use crate::terrain::material::{SurfaceLayer, material_profile, tint_multiplier};
use crate::terrain::region::Region;

pub const SPLAT_RESOLUTION: usize = 128;
pub const TILE_RESOLUTION: u32 = 256;
pub const TEXTURE_TILE_METRES: f32 = 24.0;

const CLIFF_START: f32 = 0.55;
const CLIFF_FULL: f32 = 1.20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TileSlot {
    Albedo,
    Normal,
    Orm,
}

impl TileSlot {
    pub const ALL: [TileSlot; 3] = [TileSlot::Albedo, TileSlot::Normal, TileSlot::Orm];

    pub fn suffix(self) -> &'static str {
        match self {
            TileSlot::Albedo => "albedo",
            TileSlot::Normal => "normal",
            TileSlot::Orm => "orm",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            TileSlot::Albedo => "jpg",
            TileSlot::Normal | TileSlot::Orm => "png",
        }
    }
}

pub fn tile_disk_path(root: &str, layer: SurfaceLayer, slot: TileSlot) -> String {
    format!(
        "{root}/{dir}/{dir}_{name}.{ext}",
        dir = layer.dir(),
        name = slot.suffix(),
        ext = slot.extension()
    )
}

#[derive(Default, Resource)]
pub struct SurfaceTiles {
    tiles: HashMap<(SurfaceLayer, TileSlot), RgbaImage>,
    missing: Vec<String>,
}

impl SurfaceTiles {
    pub fn load(root: &str) -> Self {
        let mut tiles = HashMap::new();
        let mut missing = Vec::new();
        for layer in SurfaceLayer::ALL {
            for slot in TileSlot::ALL {
                let path = tile_disk_path(root, layer, slot);
                match read_tile(&path, slot) {
                    Some(image) => {
                        tiles.insert((layer, slot), image);
                    }
                    None => missing.push(path),
                }
            }
        }
        Self { tiles, missing }
    }

    pub fn expected_tiles() -> usize {
        SurfaceLayer::ALL.len() * TileSlot::ALL.len()
    }

    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.tiles.len() == Self::expected_tiles()
    }

    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    #[cfg(test)]
    pub fn loaded_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn get(&self, layer: SurfaceLayer, slot: TileSlot) -> Option<&RgbaImage> {
        self.tiles.get(&(layer, slot))
    }

    fn sample(&self, layer: SurfaceLayer, slot: TileSlot, u: f32, v: f32) -> Option<[f32; 3]> {
        let image = self.get(layer, slot)?;
        let width = image.width();
        let height = image.height();
        let px = wrap_texel(u, width);
        let py = wrap_texel(v, height);
        let pixel = image.get_pixel(px, py).0;
        Some(match slot {
            TileSlot::Albedo => [
                srgb_to_linear(pixel[0] as f32 / 255.0),
                srgb_to_linear(pixel[1] as f32 / 255.0),
                srgb_to_linear(pixel[2] as f32 / 255.0),
            ],
            _ => [
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ],
        })
    }
}

fn read_tile(path: &str, slot: TileSlot) -> Option<RgbaImage> {
    let raw = image::open(path).ok()?.to_rgba8();
    Some(fit_tile(raw, slot))
}

fn fit_tile(raw: RgbaImage, slot: TileSlot) -> RgbaImage {
    let fitted = if raw.width() == TILE_RESOLUTION && raw.height() == TILE_RESOLUTION {
        raw
    } else {
        image::imageops::resize(
            &raw,
            TILE_RESOLUTION,
            TILE_RESOLUTION,
            image::imageops::FilterType::Triangle,
        )
    };
    match slot {
        TileSlot::Normal => renormalize_normals(fitted),
        _ => fitted,
    }
}

fn renormalize_normals(mut image: RgbaImage) -> RgbaImage {
    for pixel in image.pixels_mut() {
        let mut vector = [
            pixel[0] as f32 / 127.5 - 1.0,
            pixel[1] as f32 / 127.5 - 1.0,
            pixel[2] as f32 / 127.5 - 1.0,
        ];
        let length =
            (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
        if length > f32::EPSILON {
            for channel in vector.iter_mut() {
                *channel /= length;
            }
        } else {
            vector = [0.0, 0.0, 1.0];
        }
        for channel in 0..3 {
            pixel[channel] = ((vector[channel] * 0.5 + 0.5) * 255.0).round() as u8;
        }
    }
    image
}

fn wrap_texel(coordinate: f32, extent: u32) -> u32 {
    let scaled = (coordinate * extent as f32).rem_euclid(extent as f32);
    (scaled as u32).min(extent.saturating_sub(1))
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn cliff_share(slope: f32) -> f32 {
    smoothstep(CLIFF_START, CLIFF_FULL, slope)
}

pub struct SplatMaps {
    pub albedo: RgbaImage,
    #[allow(dead_code)]
    pub normal: RgbaImage,
    #[allow(dead_code)]
    pub orm: RgbaImage,
    #[allow(dead_code)]
    pub cell_data: RgbaImage,
}

#[cfg(test)]
impl SplatMaps {
    pub fn resolution(&self) -> u32 {
        self.albedo.width()
    }
}

#[derive(Clone)]
struct LayerMix(Vec<(SurfaceLayer, f32)>);

impl LayerMix {
    fn new() -> Self {
        LayerMix(Vec::new())
    }

    fn add(&mut self, layer: SurfaceLayer, weight: f32) {
        if weight <= 0.0 {
            return;
        }
        if let Some(entry) = self.0.iter_mut().find(|(known, _)| *known == layer) {
            entry.1 += weight;
            return;
        }
        self.0.push((layer, weight));
    }

    fn normalize(&mut self) {
        let total: f32 = self.0.iter().map(|(_, weight)| *weight).sum();
        if total <= f32::EPSILON {
            self.0.clear();
            return;
        }
        for entry in self.0.iter_mut() {
            entry.1 /= total;
        }
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    fn dominant(&self) -> Option<SurfaceLayer> {
        self.0
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(layer, _)| *layer)
    }

    fn sample(&self, tiles: &SurfaceTiles, slot: TileSlot, u: f32, v: f32) -> [f32; 3] {
        let mut acc = [0.0f32; 3];
        for (layer, weight) in &self.0 {
            let Some(pixel) = tiles.sample(*layer, slot, u, v) else {
                continue;
            };
            for channel in 0..3 {
                acc[channel] += pixel[channel] * weight;
            }
        }
        acc
    }
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn encode_srgb(values: [f32; 3]) -> [f32; 3] {
    values.map(linear_to_srgb)
}

fn pack(values: [f32; 3]) -> image::Rgba<u8> {
    image::Rgba([byte(values[0]), byte(values[1]), byte(values[2]), 255])
}

fn pack3(steepness: f32, weight: f32, cliff: f32, flooded: bool) -> image::Rgba<u8> {
    image::Rgba([
        byte(steepness),
        byte(weight),
        byte(cliff),
        if flooded { 255 } else { 0 },
    ])
}

fn encode_normal(mut acc: [f32; 3]) -> [f32; 3] {
    for channel in acc.iter_mut() {
        *channel = *channel * 2.0 - 1.0;
    }
    let length = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt();
    if length > f32::EPSILON {
        for channel in acc.iter_mut() {
            *channel /= length;
        }
    } else {
        acc = [0.0, 0.0, 1.0];
    }
    acc.map(|channel| channel * 0.5 + 0.5)
}

struct TexelPlan {
    mix: LayerMix,
    cliff_layer: Option<SurfaceLayer>,
    steepness: f32,
    dominant_weight: f32,
    cliff: f32,
    flooded: bool,
    tint: [f32; 3],
    roughness: f32,
}

fn plan_core(map: &TerrainMap, point: Vec2) -> TexelPlan {
    let height = map.height_at(point.x, point.y);
    let slope = map.slope_at(point.x, point.y);
    let cliff = cliff_share(slope);
    let weights = map.cell_weights_at(point.x, point.y);
    let dominant_params = weights
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .and_then(|(id, _)| map.cell(*id))
        .map(|cell| &cell.parameters);

    let flooded = map.flood_zone().contains(height);
    let shore = if flooded {
        dominant_params.and_then(|parameters| {
            crate::terrain::season::shore_override_with_hardness(
                map.season_clock().season,
                map.flood_zone(),
                height,
                parameters.salinity,
                parameters.precipitation,
                map.sea_level(),
                map.hardness_at(point.x, point.y),
            )
        })
    } else {
        None
    };
    let snow = dominant_params
        .map(|parameters| {
            crate::terrain::season::snow_coverage_with_slope(
                map.season_clock(),
                parameters.temperature,
                height,
                map.snowline(),
                slope,
            )
        })
        .unwrap_or(0.0);

    let mut mix = LayerMix::new();
    let mut highest: Option<(f32, Region)> = None;
    let mut dominant_weight = 0.0f32;
    let mut tint_acc = [0f32; 3];
    let mut roughness_acc = 0f32;
    let mut tint_weight = 0f32;
    for (id, weight) in &weights {
        dominant_weight = dominant_weight.max(*weight);
        let Some(region) = map.region_of_cell(*id) else {
            continue;
        };
        let profile = material_profile(region);
        let share = weight * (1.0 - cliff);
        mix.add(shore.unwrap_or(profile.base), share * (1.0 - snow));
        let multiplier = tint_multiplier(profile);
        for channel in 0..3 {
            tint_acc[channel] += multiplier[channel] * share;
        }
        roughness_acc += profile.roughness * share;
        tint_weight += share;
        let elevation = map
            .cell(*id)
            .map(|cell| cell.macro_elevation + cell.base_height)
            .unwrap_or(0.0);
        if highest.is_none_or(|(best, _)| elevation > best) {
            highest = Some((elevation, region));
        }
    }
    if snow > 0.0 {
        mix.add(SurfaceLayer::Snow, (1.0 - cliff) * snow);
    }
    let cliff_layer = if cliff > 0.0 {
        highest.map(|(_, region)| material_profile(region).cliff)
    } else {
        None
    };
    if cliff > 0.0 {
        if let Some((_, region)) = highest {
            let profile = material_profile(region);
            let multiplier = tint_multiplier(profile);
            for channel in 0..3 {
                tint_acc[channel] += multiplier[channel] * cliff;
            }
            roughness_acc += profile.roughness * cliff;
            tint_weight += cliff;
        }
    }
    let blended = tint_weight > f32::EPSILON;

    TexelPlan {
        mix,
        cliff_layer,
        steepness: (slope / 1.6).clamp(0.0, 1.0),
        dominant_weight,
        cliff,
        flooded,
        tint: if blended {
            [
                tint_acc[0] / tint_weight,
                tint_acc[1] / tint_weight,
                tint_acc[2] / tint_weight,
            ]
        } else {
            [1.0, 1.0, 1.0]
        },
        roughness: if blended {
            roughness_acc / tint_weight
        } else {
            0.0
        },
    }
}

fn plan_texel(map: &TerrainMap, point: Vec2) -> TexelPlan {
    let mut plan = plan_core(map, point);
    if plan.cliff > 0.0 {
        if let Some(layer) = plan.cliff_layer {
            plan.mix.add(layer, plan.cliff);
        }
    }
    plan.mix.normalize();
    plan
}

pub fn bake_chunk(
    map: &TerrainMap,
    tiles: &SurfaceTiles,
    chunk: IVec2,
    chunk_size: f32,
    resolution: usize,
) -> Option<SplatMaps> {
    if !tiles.is_complete() || resolution == 0 {
        return None;
    }
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let span = high - low;
    let edge = resolution as u32;

    let mut albedo = ImageBuffer::from_pixel(edge, edge, image::Rgba([0, 0, 0, 255]));
    let mut normal = ImageBuffer::from_pixel(edge, edge, image::Rgba([128, 128, 255, 255]));
    let mut orm = ImageBuffer::from_pixel(edge, edge, image::Rgba([0, 0, 0, 255]));
    let mut cell_data = ImageBuffer::from_pixel(edge, edge, image::Rgba([0, 0, 0, 255]));

    for iz in 0..resolution {
        for ix in 0..resolution {
            let point = Vec2::new(
                low.x + (ix as f32 + 0.5) / resolution as f32 * span.x,
                low.y + (iz as f32 + 0.5) / resolution as f32 * span.y,
            );
            let plan = plan_texel(map, point);
            if plan.mix.is_empty() {
                continue;
            }
            let u = point.x / TEXTURE_TILE_METRES;
            let v = point.y / TEXTURE_TILE_METRES;

            let albedo_pixel = encode_srgb(plan.mix.sample(tiles, TileSlot::Albedo, u, v));
            let normal_pixel = encode_normal(plan.mix.sample(tiles, TileSlot::Normal, u, v));
            let orm_pixel = plan.mix.sample(tiles, TileSlot::Orm, u, v);

            let px = ix as u32;
            let py = edge - 1 - iz as u32;
            albedo.put_pixel(px, py, pack(albedo_pixel));
            normal.put_pixel(px, py, pack(normal_pixel));
            orm.put_pixel(px, py, pack(orm_pixel));
            cell_data.put_pixel(
                px,
                py,
                pack3(plan.steepness, plan.dominant_weight, plan.cliff, plan.flooded),
            );
        }
    }

    Some(SplatMaps {
        albedo,
        normal,
        orm,
        cell_data,
    })
}

pub struct ControlMaps {
    pub weights: RgbaImage,
    pub indices: RgbaImage,
    pub cell_data: RgbaImage,
    pub cliff_meta: RgbaImage,
    pub tint: RgbaImage,
}

#[derive(Clone)]
pub struct ControlHandles {
    pub weights: Handle<Image>,
    pub indices: Handle<Image>,
    pub cell_data: Handle<Image>,
    pub cliff_meta: Handle<Image>,
    pub tint: Handle<Image>,
}

pub fn bake_control_chunk(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    resolution: usize,
) -> ControlMaps {
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let span = high - low;
    let edge = resolution as u32;

    let mut weights = ImageBuffer::from_pixel(edge, edge, image::Rgba([0, 0, 0, 0]));
    let mut indices = ImageBuffer::from_pixel(edge, edge, image::Rgba([0, 0, 0, 0]));
    let mut cell_data = ImageBuffer::from_pixel(edge, edge, image::Rgba([0, 0, 0, 255]));
    let mut cliff_meta = ImageBuffer::from_pixel(edge, edge, image::Rgba([0, 0, 0, 255]));
    let mut tint = ImageBuffer::from_pixel(edge, edge, image::Rgba([255, 255, 255, 0]));

    for iz in 0..resolution {
        for ix in 0..resolution {
            let point = Vec2::new(
                low.x + (ix as f32 + 0.5) / resolution as f32 * span.x,
                low.y + (iz as f32 + 0.5) / resolution as f32 * span.y,
            );
            let plan = plan_core(map, point);
            let mut mix = plan.mix;
            mix.normalize();
            let mut entries = mix.0;
            entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let total: f32 = entries.iter().take(4).map(|(_, weight)| *weight).sum();
            let mut weight_bytes = [0u8; 4];
            let mut index_bytes = [0u8; 4];
            for (slot, (layer, weight)) in entries.into_iter().take(4).enumerate() {
                weight_bytes[slot] =
                    byte(if total > f32::EPSILON { weight / total } else { 0.0 });
                index_bytes[slot] = layer as u8 + 1;
            }
            let px = ix as u32;
            let py = iz as u32;
            weights.put_pixel(px, py, image::Rgba(weight_bytes));
            indices.put_pixel(px, py, image::Rgba(index_bytes));
            cell_data.put_pixel(
                px,
                py,
                pack3(plan.steepness, plan.dominant_weight, plan.cliff, plan.flooded),
            );
            let meta = plan
                .cliff_layer
                .map(|layer| layer as u8 + 1)
                .unwrap_or(0);
            cliff_meta.put_pixel(px, py, image::Rgba([meta, 0, 0, 255]));
            tint.put_pixel(
                px,
                py,
                image::Rgba([
                    byte(plan.tint[0]),
                    byte(plan.tint[1]),
                    byte(plan.tint[2]),
                    byte(plan.roughness),
                ]),
            );
        }
    }

    ControlMaps {
        weights,
        indices,
        cell_data,
        cliff_meta,
        tint,
    }
}

pub(crate) fn mip_size(size: u32, mip: u32) -> u32 {
    (size >> mip).max(1)
}

pub(crate) fn mip_level_count(width: u32, height: u32) -> u32 {
    width.min(height).max(1).ilog2() + 1
}

fn mip_chain(
    image: RgbaImage,
    format: TextureFormat,
    descriptor: ImageSamplerDescriptor,
    filter: image::imageops::FilterType,
    renormalize: bool,
) -> Image {
    let width = image.width();
    let height = image.height();
    let levels = mip_level_count(width, height);
    let mut texel_bytes: Vec<u8> = Vec::new();
    let mut current = image;
    for mip in 0..levels {
        texel_bytes.extend_from_slice(current.as_raw());
        if mip + 1 < levels {
            let mut resized = image::imageops::resize(
                &current,
                mip_size(width, mip + 1),
                mip_size(height, mip + 1),
                filter,
            );
            if renormalize {
                resized = renormalize_normals(resized);
            }
            current = resized;
        }
    }
    let mut built = Image::new_uninit(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::default(),
    );
    built.texture_descriptor.mip_level_count = levels;
    built.data_order = TextureDataOrder::MipMajor;
    built.sampler = ImageSampler::Descriptor(descriptor);
    built.data = Some(texel_bytes);
    built
}

fn control_texture(image: RgbaImage, linear: bool) -> Image {
    let mut descriptor = if linear {
        ImageSamplerDescriptor::linear()
    } else {
        ImageSamplerDescriptor::nearest()
    };
    descriptor.address_mode_u = ImageAddressMode::ClampToEdge;
    descriptor.address_mode_v = ImageAddressMode::ClampToEdge;
    let filter = if linear {
        image::imageops::FilterType::Triangle
    } else {
        image::imageops::FilterType::Nearest
    };
    mip_chain(
        image,
        TextureFormat::Rgba8Unorm,
        descriptor,
        filter,
        false,
    )
}

pub fn upload_control(maps: &ControlMaps, images: &mut Assets<Image>) -> ControlHandles {
    ControlHandles {
        weights: images.add(control_texture(maps.weights.clone(), true)),
        indices: images.add(control_texture(maps.indices.clone(), false)),
        cell_data: images.add(control_texture(maps.cell_data.clone(), true)),
        cliff_meta: images.add(control_texture(maps.cliff_meta.clone(), false)),
        tint: images.add(control_texture(maps.tint.clone(), true)),
    }
}

#[cfg(test)]
fn wrapped(image: RgbaImage, format: TextureFormat, renormalize: bool) -> Image {
    let mut descriptor = ImageSamplerDescriptor::linear();
    descriptor.address_mode_u = ImageAddressMode::ClampToEdge;
    descriptor.address_mode_v = ImageAddressMode::ClampToEdge;
    mip_chain(
        image,
        format,
        descriptor,
        image::imageops::FilterType::Triangle,
        renormalize,
    )
}

#[cfg(test)]
pub fn albedo_texture(maps: &SplatMaps) -> Image {
    wrapped(maps.albedo.clone(), TextureFormat::Rgba8UnormSrgb, false)
}

#[cfg(test)]
pub fn normal_texture(maps: &SplatMaps) -> Image {
    wrapped(maps.normal.clone(), TextureFormat::Rgba8Unorm, true)
}

#[cfg(test)]
pub fn orm_texture(maps: &SplatMaps) -> Image {
    wrapped(maps.orm.clone(), TextureFormat::Rgba8Unorm, false)
}

#[cfg(test)]
pub fn cell_data_texture(maps: &SplatMaps) -> Image {
    wrapped(maps.cell_data.clone(), TextureFormat::Rgba8Unorm, false)
}

#[cfg(test)]
pub struct SplatHandles {
    pub albedo: Handle<Image>,
    pub normal: Handle<Image>,
    pub orm: Handle<Image>,
    pub cell_data: Handle<Image>,
}

#[cfg(test)]
pub fn upload(maps: &SplatMaps, images: &mut Assets<Image>) -> SplatHandles {
    SplatHandles {
        albedo: images.add(albedo_texture(maps)),
        normal: images.add(normal_texture(maps)),
        orm: images.add(orm_texture(maps)),
        cell_data: images.add(cell_data_texture(maps)),
    }
}

#[cfg(test)]
pub fn splat_material(handles: &SplatHandles) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(handles.albedo.clone()),
        normal_map_texture: Some(handles.normal.clone()),
        metallic_roughness_texture: Some(handles.orm.clone()),
        perceptual_roughness: 1.0,
        reflectance: 0.2,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::map::TerrainConfig;
    use crate::terrain::material::{RegionMaterialProfile, DISK_ROOT};
    use crate::terrain::season::Season;
    use bevy::image::ImageFilterMode;

    fn tiles() -> SurfaceTiles {
        SurfaceTiles::load(DISK_ROOT)
    }

    fn map() -> TerrainMap {
        TerrainMap::new(TerrainConfig::compact())
    }

    #[test]
    fn every_pbr_tile_loads_and_is_normalized() {
        let tiles = tiles();
        assert!(
            tiles.is_complete(),
            "missing tiles: {:?}",
            tiles.missing()
        );
        assert_eq!(tiles.loaded_count(), SurfaceLayer::ALL.len() * TileSlot::ALL.len());
        for layer in SurfaceLayer::ALL {
            for slot in TileSlot::ALL {
                let image = tiles.get(layer, slot).expect("tile present");
                assert_eq!(image.width(), TILE_RESOLUTION);
                assert_eq!(image.height(), TILE_RESOLUTION);
            }
        }
    }

    #[test]
    fn tile_paths_are_distinct_per_slot() {
        let paths: Vec<String> = SurfaceLayer::ALL
            .iter()
            .flat_map(|layer| TileSlot::ALL.map(|slot| tile_disk_path("root", *layer, slot)))
            .collect();
        let unique: std::collections::HashSet<&String> = paths.iter().collect();
        assert_eq!(unique.len(), paths.len());
    }

    #[test]
    fn wrap_texel_repeats_outside_the_unit_range() {
        assert_eq!(wrap_texel(0.0, 256), 0);
        assert_eq!(wrap_texel(1.0, 256), 0);
        assert_eq!(wrap_texel(-0.001, 256), 255);
        assert_eq!(wrap_texel(0.5, 256), 128);
        assert_eq!(wrap_texel(2.75, 256), 192);
    }

    #[test]
    fn srgb_transfer_round_trips() {
        for value in [0.0, 0.02, 0.2, 0.5, 1.0] {
            let round = linear_to_srgb(srgb_to_linear(value));
            assert!((round - value).abs() < 1e-3, "{value} round trip");
        }
        assert!(srgb_to_linear(0.5) < 0.5, "srgb decode must darken mid grey");
    }

    #[test]
    fn cliff_share_ramps_monotonically_with_slope() {
        assert_eq!(cliff_share(0.0), 0.0);
        assert_eq!(cliff_share(CLIFF_FULL + 1.0), 1.0);
        let mut previous = -1.0;
        for step in 0..12 {
            let slope = step as f32 * 0.2;
            let share = cliff_share(slope);
            assert!(share >= previous, "cliff share must not decrease");
            previous = share;
        }
    }

    #[test]
    fn layer_mix_normalizes_to_unit_weight() {
        let mut mix = LayerMix::new();
        mix.add(SurfaceLayer::Rock, 3.0);
        mix.add(SurfaceLayer::Rock, 1.0);
        mix.add(SurfaceLayer::Snow, 4.0);
        mix.add(SurfaceLayer::Sand, -1.0);
        mix.normalize();
        let total: f32 = mix.0.iter().map(|(_, weight)| *weight).sum();
        assert!((total - 1.0).abs() < 1e-5);
        assert_eq!(mix.0.len(), 2, "negative weights are dropped");
        assert_eq!(mix.dominant(), Some(SurfaceLayer::Snow));
    }

    #[test]
    fn baking_produces_aligned_maps() {
        let tiles = tiles();
        let map = map();
        let chunk = IVec2::ZERO;
        let chunk_size = 120.0f32.min(map.size());
        let maps = bake_chunk(&map, &tiles, chunk, chunk_size, 16)
            .expect("bake must succeed with complete tiles");
        assert_eq!(maps.resolution(), 16);
        assert_eq!(maps.normal.dimensions(), maps.albedo.dimensions());
        assert_eq!(maps.orm.dimensions(), maps.albedo.dimensions());
        assert_eq!(maps.cell_data.dimensions(), maps.albedo.dimensions());
    }

    #[test]
    fn baking_refuses_incomplete_tile_sets() {
        let map = map();
        assert!(bake_chunk(&map, &SurfaceTiles::default(), IVec2::ZERO, 60.0, 8).is_none());
    }

    #[test]
    fn cell_data_encodes_steepness_and_flood_state() {
        let tiles = tiles();
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let maps = bake_chunk(&map, &tiles, IVec2::ZERO, chunk_size, 16).unwrap();
        let mut steep = false;
        let mut flooded = false;
        for pixel in maps.cell_data.pixels() {
            if pixel[0] > 0 {
                steep = true;
            }
            if pixel[3] == 255 {
                flooded = true;
            }
            if pixel[0] == 0 {
                assert_eq!(pixel[2], 0, "flat ground must not carry a cliff share");
            }
        }
        assert!(steep, "some texel must register slope");
        assert!(flooded, "a compact world must have a seasonal flood zone");
    }

    #[test]
    fn baked_albedo_actually_varies() {
        let tiles = tiles();
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let maps = bake_chunk(&map, &tiles, IVec2::ZERO, chunk_size, 16).unwrap();
        let first = maps.albedo.get_pixel(0, 0).0;
        let varies = maps
            .albedo
            .pixels()
            .any(|pixel| pixel.0 != first);
        assert!(varies, "a flat splat means the tiles were never sampled");
    }

    #[test]
    fn textures_are_clamp_sampled_and_correctly_encoded() {
        let tiles = tiles();
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let maps = bake_chunk(&map, &tiles, IVec2::ZERO, chunk_size, 8).unwrap();

        let albedo = albedo_texture(&maps);
        assert_eq!(
            albedo.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
        let normal = normal_texture(&maps);
        assert_eq!(normal.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        for built in [albedo, normal, orm_texture(&maps), cell_data_texture(&maps)] {
            assert_eq!(built.width(), 8);
            assert_eq!(built.height(), 8);
            let descriptor = match &built.sampler {
                ImageSampler::Descriptor(descriptor) => descriptor.clone(),
                ImageSampler::Default => panic!("splat textures must carry an explicit sampler"),
            };
            assert_eq!(descriptor.address_mode_u, ImageAddressMode::ClampToEdge);
            assert_eq!(descriptor.address_mode_v, ImageAddressMode::ClampToEdge);
            assert_eq!(descriptor.mag_filter, ImageFilterMode::Linear);
        }
    }

    #[test]
    fn control_maps_encode_top_four_layers() {
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let controls = bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        assert_eq!(controls.weights.dimensions(), controls.indices.dimensions());
        assert_eq!(controls.cell_data.dimensions(), controls.indices.dimensions());
        let mut any_layer = false;
        for pixel in controls.indices.pixels() {
            for channel in 0..4 {
                if pixel.0[channel] > 0 {
                    any_layer = true;
                }
            }
        }
        assert!(any_layer, "some texel must resolve a surface layer");
        for (weight, index) in controls.weights.pixels().zip(controls.indices.pixels()) {
            for channel in 0..4 {
                if index.0[channel] == 0 {
                    assert_eq!(weight.0[channel], 0, "empty slots must carry no weight");
                } else {
                    assert!(
                        (index.0[channel] as usize) <= SurfaceLayer::ALL.len(),
                        "layer indices are stored one-based"
                    );
                }
            }
        }
    }

    #[test]
    fn control_weights_renormalize_to_one() {
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let controls = bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        let mut checked = 0;
        for pixel in controls.weights.pixels() {
            let total: u32 = pixel.0.iter().map(|value| *value as u32).sum();
            if total > 0 {
                assert!(
                    (total as i64 - 255).abs() <= 4,
                    "top-four weights must renormalize: {total}"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "the compact world must resolve layers");
    }

    fn shared_layer_pair(
        map: &TerrainMap,
        differs: impl Fn(&RegionMaterialProfile, &RegionMaterialProfile) -> bool,
        score: impl Fn(&RegionMaterialProfile, &RegionMaterialProfile) -> f32,
    ) -> Option<(Vec2, Vec2)> {
        let mut best: Option<(f32, Vec2, Vec2)> = None;
        for a in map.cells() {
            let Some(region_a) = map.region_of_cell(a.id) else {
                continue;
            };
            let profile_a = material_profile(region_a);
            for b in map.cells() {
                let Some(region_b) = map.region_of_cell(b.id) else {
                    continue;
                };
                let profile_b = material_profile(region_b);
                if region_a == region_b || profile_a.base != profile_b.base || !differs(profile_a, profile_b) {
                    continue;
                }
                let strength = score(profile_a, profile_b);
                if best.is_none_or(|(strongest, _, _)| strength > strongest) {
                    best = Some((strength, a.center, b.center));
                }
            }
        }
        best.map(|(_, a, b)| (a, b))
    }

    fn tint_drift(a: [f32; 3], b: [f32; 3]) -> f32 {
        (a[0] - b[0])
            .abs()
            .max((a[1] - b[1]).abs())
            .max((a[2] - b[2]).abs())
    }

    #[test]
    fn control_bake_tint_separates_regions_sharing_a_layer() {
        let map = TerrainMap::new(TerrainConfig {
            cell_grid_dim: 6,
            ..TerrainConfig::compact()
        });
        let (a, b) = shared_layer_pair(
            &map,
            |x, y| tint_multiplier(x) != tint_multiplier(y),
            |x, y| tint_drift(tint_multiplier(x), tint_multiplier(y)),
        )
        .expect("the compact world must contain two regions sharing a base layer with distinct tints");
        let plan_a = plan_core(&map, a);
        let plan_b = plan_core(&map, b);
        assert!(
            tint_drift(plan_a.tint, plan_b.tint) > 1.0 / 255.0,
            "regions sharing a SurfaceLayer must bake distinct tints: {:?} vs {:?}",
            plan_a.tint,
            plan_b.tint
        );
        let chunk_size = 120.0f32.min(map.size());
        let controls = bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        assert_eq!(controls.tint.dimensions(), controls.indices.dimensions());
        let distinct: std::collections::HashSet<[u8; 4]> =
            controls.tint.pixels().map(|pixel| pixel.0).collect();
        assert!(
            distinct.len() > 1,
            "the tint control map must vary across the chunk"
        );
    }

    #[test]
    fn control_bake_roughness_separates_regions_sharing_a_layer() {
        let map = TerrainMap::new(TerrainConfig {
            cell_grid_dim: 6,
            ..TerrainConfig::compact()
        });
        let (a, b) = shared_layer_pair(
            &map,
            |x, y| x.roughness != y.roughness,
            |x, y| (x.roughness - y.roughness).abs(),
        )
        .expect("the compact world must contain two regions sharing a base layer with distinct roughness");
        let plan_a = plan_core(&map, a);
        let plan_b = plan_core(&map, b);
        assert!(
            (plan_a.roughness - plan_b.roughness).abs() > 0.01,
            "regions sharing a SurfaceLayer must bake distinct roughness: {} vs {}",
            plan_a.roughness,
            plan_b.roughness
        );
        let chunk_size = 120.0f32.min(map.size());
        let controls = bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        let alphas: std::collections::HashSet<u8> =
            controls.tint.pixels().map(|pixel| pixel.0[3]).collect();
        assert!(
            alphas.len() > 1,
            "the tint alpha channel must carry the blended region roughness"
        );
    }

    #[test]
    fn control_cell_data_matches_the_cpu_bake() {
        let tiles = tiles();
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let baked =
            bake_chunk(&map, &tiles, IVec2::ZERO, chunk_size, 16).unwrap();
        let controls = bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        for iz in 0..16u32 {
            for ix in 0..16u32 {
                let old = baked.cell_data.get_pixel(ix, 15 - iz).0;
                let fresh = controls.cell_data.get_pixel(ix, iz).0;
                assert_eq!(old, fresh, "cell_data must agree at {ix},{iz}");
            }
        }
    }

    #[test]
    fn control_cliff_meta_names_a_layer_on_steep_ground() {
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let controls = bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        assert!(
            controls.cliff_meta.pixels().any(|pixel| pixel.0[0] > 0),
            "a compact world with cliffs must name cliff layers"
        );
    }

    #[test]
    fn shore_override_changes_the_bake_in_the_flood_zone() {
        let tiles = tiles();
        let mut map = map();
        let chunk_size = 120.0f32.min(map.size());
        let summer = bake_chunk(&map, &tiles, IVec2::ZERO, chunk_size, 16).unwrap();
        map.set_season(Season::Winter);
        let winter = bake_chunk(&map, &tiles, IVec2::ZERO, chunk_size, 16).unwrap();
        assert_ne!(
            summer.orm.to_vec(),
            winter.orm.to_vec(),
            "seasonal shore overrides must reach the baked material"
        );
    }

    #[test]
    fn cliff_layer_follows_absolute_elevation_not_base_jitter() {
        let mut map = map();
        let jitter = map.config().cell_base_height_range;
        let mut pair: Option<(usize, usize, f32)> = None;
        for cell in map.cells() {
            let Some(cliff_a) = map.region_of_cell(cell.id).map(|r| material_profile(r).cliff) else {
                continue;
            };
            for &neighbour in map.cell_neighbours(cell.id) {
                let Some(other) = map.cell(neighbour) else {
                    continue;
                };
                let Some(cliff_b) = map.region_of_cell(neighbour).map(|r| material_profile(r).cliff) else {
                    continue;
                };
                if cliff_a == cliff_b {
                    continue;
                }
                let delta = other.macro_elevation - cell.macro_elevation;
                if delta > 2.0 * jitter && pair.is_none_or(|(_, _, best)| delta > best) {
                    pair = Some((cell.id, neighbour, delta));
                }
            }
        }
        let (low_id, alpine_id, _) =
            pair.expect("the compact world must have adjacent cells further apart than the jitter range");
        assert!(map.set_cell_base_height(low_id, jitter));
        assert!(map.set_cell_base_height(alpine_id, -jitter));

        let low_center = map.cell(low_id).unwrap().center;
        let alpine_center = map.cell(alpine_id).unwrap().center;
        let mut found = false;
        for step in 0..=24 {
            let t = step as f32 / 24.0;
            let point = low_center.lerp(alpine_center, t);
            let plan = plan_core(&map, point);
            if plan.cliff <= 0.0 {
                continue;
            }
            let mut elevation: Option<(f32, usize)> = None;
            let mut jitter_winner: Option<(f32, usize)> = None;
            for (id, _) in map.cell_weights_at(point.x, point.y) {
                let Some(cell) = map.cell(id) else {
                    continue;
                };
                let absolute = cell.macro_elevation + cell.base_height;
                if elevation.is_none_or(|(best, _)| absolute > best) {
                    elevation = Some((absolute, id));
                }
                if jitter_winner.is_none_or(|(best, _)| cell.base_height > best) {
                    jitter_winner = Some((cell.base_height, id));
                }
            }
            let (Some((_, elevation_id)), Some((_, jitter_id))) = (elevation, jitter_winner) else {
                continue;
            };
            if elevation_id == jitter_id {
                continue;
            }
            let elevation_cliff = material_profile(map.region_of_cell(elevation_id).unwrap()).cliff;
            let jitter_cliff = material_profile(map.region_of_cell(jitter_id).unwrap()).cliff;
            if elevation_cliff == jitter_cliff {
                continue;
            }
            assert_eq!(
                plan.cliff_layer,
                Some(elevation_cliff),
                "cliff texture at {point:?} must follow absolute elevation, not jitter"
            );
            found = true;
            break;
        }
        assert!(
            found,
            "the forced elevation step must produce cliff ground where jitter and absolute elevation disagree"
        );
    }

    #[test]
    fn resized_normal_tiles_are_renormalized_to_unit_vectors() {
        let mut source = ImageBuffer::from_pixel(64, 64, image::Rgba([128, 128, 128, 200]));
        for (x, y, pixel) in source.enumerate_pixels_mut() {
            let nx = (x as f32 / 63.0) * 1.2 - 0.6;
            let ny = (y as f32 / 63.0) * 1.2 - 0.6;
            let nz = (1.0 - nx * nx - ny * ny).max(0.04).sqrt();
            *pixel = image::Rgba([
                byte(nx * 0.5 + 0.5),
                byte(ny * 0.5 + 0.5),
                byte(nz * 0.5 + 0.5),
                200,
            ]);
        }
        let fitted = fit_tile(source, TileSlot::Normal);
        assert_eq!(fitted.width(), TILE_RESOLUTION);
        assert_eq!(fitted.height(), TILE_RESOLUTION);
        for pixel in fitted.pixels() {
            assert_eq!(pixel[3], 200, "alpha must be left alone by renormalization");
            let vector = [
                pixel[0] as f32 / 127.5 - 1.0,
                pixel[1] as f32 / 127.5 - 1.0,
                pixel[2] as f32 / 127.5 - 1.0,
            ];
            let length =
                (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
            assert!(
                (length - 1.0).abs() < 0.02,
                "resized normal texel {vector:?} has length {length}"
            );
        }
    }

    #[test]
    fn control_textures_carry_a_full_mip_chain() {
        let map = map();
        let chunk_size = 120.0f32.min(map.size());
        let controls = bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        let levels = mip_level_count(16, 16);
        assert_eq!(levels, 5);
        let expected_bytes: usize = (0..levels)
            .map(|mip| {
                let edge = (16u32 >> mip).max(1) as usize;
                edge * edge * 4
            })
            .sum();
        for built in [
            control_texture(controls.weights.clone(), true),
            control_texture(controls.indices.clone(), false),
            control_texture(controls.cell_data.clone(), true),
            control_texture(controls.cliff_meta.clone(), false),
        ] {
            assert_eq!(built.texture_descriptor.mip_level_count, levels);
            assert_eq!(built.data_order, TextureDataOrder::MipMajor);
            assert_eq!(built.data.expect("mip data").len(), expected_bytes);
        }
    }

    #[test]
    fn normal_texture_mips_are_renormalized() {
        let mut normal = ImageBuffer::from_pixel(8, 8, image::Rgba([128, 128, 255, 255]));
        for (x, y, pixel) in normal.enumerate_pixels_mut() {
            let nx = (x as f32 / 7.0) * 1.0 - 0.5;
            let ny = (y as f32 / 7.0) * 1.0 - 0.5;
            let nz = (1.0 - nx * nx - ny * ny).max(0.04).sqrt();
            *pixel = image::Rgba([
                byte(nx * 0.5 + 0.5),
                byte(ny * 0.5 + 0.5),
                byte(nz * 0.5 + 0.5),
                255,
            ]);
        }
        let flat = ImageBuffer::from_pixel(8, 8, image::Rgba([0, 0, 0, 255]));
        let maps = SplatMaps {
            albedo: flat.clone(),
            normal,
            orm: flat.clone(),
            cell_data: flat,
        };
        let built = normal_texture(&maps);
        let levels = built.texture_descriptor.mip_level_count;
        assert!(levels > 1, "the normal texture must carry a mip chain");
        let data = built.data.expect("mip data");
        let mut offset = 0usize;
        for mip in 0..levels {
            let edge = (8u32 >> mip).max(1);
            for texel in 0..(edge * edge) {
                let pixel = &data[offset + (texel * 4) as usize..];
                let vector = [
                    pixel[0] as f32 / 127.5 - 1.0,
                    pixel[1] as f32 / 127.5 - 1.0,
                    pixel[2] as f32 / 127.5 - 1.0,
                ];
                let length =
                    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
                assert!(
                    (length - 1.0).abs() < 0.02,
                    "mip {mip} texel {texel} {vector:?} has length {length}"
                );
            }
            offset += (edge * edge * 4) as usize;
        }
        assert_eq!(offset, data.len());
    }
}