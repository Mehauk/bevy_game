use std::sync::atomic::{AtomicU64, Ordering};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TextureDataOrder, TextureDimension, TextureFormat,
    TextureViewDescriptor, TextureViewDimension,
};
use bevy::shader::ShaderRef;
use image::RgbaImage;

use crate::terrain::atmosphere::{self, RegionAtmosphereProfile};
use crate::terrain::chunk::clamped_chunk_bounds;
use crate::terrain::map::TerrainMap;
use crate::terrain::material::{RegionMaterialProfile, SurfaceLayer};
use crate::terrain::season::{self, Season, SeasonClock};
use crate::terrain::splat::{self, ControlHandles, SurfaceTiles, TileSlot, mip_level_count, mip_size};
use crate::terrain::water::{WaterKind, FOAM_DEPTH};

pub const TERRAIN_SHADER_PATH: &str = "shaders/terrain_material.wesl";
pub const WATER_SHADER_PATH: &str = "shaders/water_material.wesl";
pub const SKY_SHADER_PATH: &str = "shaders/sky_material.wesl";

#[cfg(test)]
pub const TERRAIN_SHADER_BINDINGS: [u32; 15] =
    [31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45];
#[cfg(test)]
pub const WATER_SHADER_BINDINGS: [u32; 1] = [31];
#[cfg(test)]
pub const SKY_SHADER_BINDINGS: [u32; 1] = [31];

pub const DEBUG_MODE_TEXTURED: u32 = 0;
pub const DEBUG_MODE_FLAT: u32 = 1;
pub const DEBUG_MODE_UV_GRID: u32 = 2;
pub const DEBUG_MODE_VERTEX_COLOUR: u32 = 3;

pub const NEAR_TILE_METRES: f32 = 14.0;
pub const FAR_TILE_METRES: f32 = 52.0;
pub const MACRO_TINT_METRES: f32 = 96.0;
pub const SNOWLINE_METRES: f32 = season::DEFAULT_SNOWLINE_METRES;
pub const SNOW_SLOPE_LIMIT: f32 = 0.25;
pub const WATER_ABSORPTION_RGB: Vec3 = Vec3::new(0.28, 0.14, 0.12);
pub const WATER_MURKINESS: f32 = 1.0;
pub const CAUSTIC_STRENGTH_STILL: f32 = 1.0;
pub const CAUSTIC_STRENGTH_FLOWING: f32 = 0.4;
pub const DEFAULT_PREVAILING_WIND: Vec2 = Vec2::new(0.7, 0.4);
pub const WIND_SCROLL_MIN: f32 = 0.15;
pub const WIND_SCROLL_MAX: f32 = 2.0;
pub const WAVE_SCALE: f32 = 0.25;
pub const WAVE_AMPLITUDE: f32 = 1.5;
pub const FRESNEL_BASE: f32 = 0.04;
pub const ICE_ROUGHNESS: f32 = 0.35;
#[cfg(test)]
pub const TILE_MIP_LEVELS: u32 = 9;
pub const MAX_ANISOTROPY: u16 = 8;

pub type TerrainRenderMaterial = ExtendedMaterial<StandardMaterial, TerrainSurface>;
pub type WaterRenderMaterial = ExtendedMaterial<StandardMaterial, WaterSurface>;
pub type SkyRenderMaterial = ExtendedMaterial<StandardMaterial, SkySurface>;

#[repr(C)]
#[derive(Clone, Copy, Default, ShaderType, Reflect, Debug)]
pub struct SkySurfaceUniform {
    pub sun_direction: Vec4,
    pub sun_color: Vec4,
    pub zenith: Vec4,
    pub horizon: Vec4,
    pub ground: Vec4,
    pub scattering: Vec2,
    pub snow: f32,
    pub exposure_gain: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, ShaderType, Reflect, Debug)]
pub struct TerrainSurfaceUniform {
    pub tint: Vec4,
    pub season_tint: Vec4,
    pub chunk_low: Vec2,
    pub chunk_size: f32,
    pub control_resolution: f32,
    pub sea_level: f32,
    pub flood_low: f32,
    pub flood_high: f32,
    pub snow_height: f32,
    pub snow_slope_limit: f32,
    pub snow_amount: f32,
    pub tile_scale: f32,
    pub far_scale: f32,
    pub macro_scale: f32,
    pub roughness: f32,
    pub reflectance: f32,
    pub wetness_gain: f32,
    pub debug_mode: u32,
    pub layer_count: u32,
    pub cliff_layer: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, ShaderType, Reflect, Debug)]
pub struct WaterSurfaceUniform {
    pub shallow_color: Vec4,
    pub deep_color: Vec4,
    pub sky_zenith: Vec4,
    pub sky_horizon: Vec4,
    pub sun_color: Vec4,
    pub sun_direction: Vec4,
    pub wind: Vec4,
    pub absorption: Vec4,
    pub time: f32,
    pub wave_scale: f32,
    pub wave_amplitude: f32,
    pub foam_depth: f32,
    pub foam_scale: f32,
    pub frozen_blend: f32,
    pub ice_roughness: f32,
    pub fresnel_strength: f32,
    pub kind: u32,
    pub use_vertex_data: u32,
    pub caustics: f32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainSurface {
    #[uniform(31, TerrainSurfaceUniform)]
    pub params: TerrainSurfaceUniform,
    #[texture(32, dimension = "2d_array", sample_type = "float", filterable = true)]
    #[sampler(33)]
    pub albedo_array: Handle<Image>,
    #[texture(34, dimension = "2d_array", sample_type = "float", filterable = true)]
    #[sampler(35)]
    pub normal_array: Handle<Image>,
    #[texture(36, dimension = "2d_array", sample_type = "float", filterable = true)]
    #[sampler(37)]
    pub orm_array: Handle<Image>,
    #[texture(38, sample_type = "float", filterable = true)]
    #[sampler(39)]
    pub splat_weights: Handle<Image>,
    #[texture(40, sample_type = "float", filterable = true)]
    pub splat_indices: Handle<Image>,
    #[texture(41, sample_type = "float", filterable = true)]
    #[sampler(42)]
    pub cell_data: Handle<Image>,
    #[texture(43, sample_type = "float", filterable = true)]
    pub cliff_meta: Handle<Image>,
    #[texture(44, sample_type = "float", filterable = true)]
    #[sampler(45)]
    pub region_tint: Handle<Image>,
}

impl MaterialExtension for TerrainSurface {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(TERRAIN_SHADER_PATH.into())
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterSurface {
    #[uniform(31, WaterSurfaceUniform)]
    pub params: WaterSurfaceUniform,
}

impl MaterialExtension for WaterSurface {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(WATER_SHADER_PATH.into())
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct SkySurface {
    #[uniform(31, SkySurfaceUniform)]
    pub params: SkySurfaceUniform,
}

impl MaterialExtension for SkySurface {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(SKY_SHADER_PATH.into())
    }
}

static PREVAILING_WIND: AtomicU64 = AtomicU64::new(pack_vec2(DEFAULT_PREVAILING_WIND));

const fn pack_vec2(wind: Vec2) -> u64 {
    ((wind.x.to_bits() as u64) << 32) | wind.y.to_bits() as u64
}

fn unpack_vec2(packed: u64) -> Vec2 {
    Vec2::new(
        f32::from_bits((packed >> 32) as u32),
        f32::from_bits(packed as u32),
    )
}

pub fn set_prevailing_wind(wind: Vec2) {
    PREVAILING_WIND.store(pack_vec2(wind), Ordering::Relaxed);
}

pub fn prevailing_wind() -> Vec2 {
    unpack_vec2(PREVAILING_WIND.load(Ordering::Relaxed))
}

pub fn wind_scroll(wind: Vec2) -> Vec4 {
    let length = wind.length();
    if !length.is_finite() || length < 1e-5 {
        return Vec4::new(1.0, 0.0, WIND_SCROLL_MIN, 0.0);
    }
    Vec4::new(
        wind.x / length,
        wind.y / length,
        length.clamp(WIND_SCROLL_MIN, WIND_SCROLL_MAX),
        0.0,
    )
}

pub fn kind_index(kind: WaterKind) -> u32 {
    match kind {
        WaterKind::Sea => 0,
        WaterKind::Lake => 1,
        WaterKind::River => 2,
        WaterKind::Waterfall => 3,
    }
}

pub fn caustic_strength(kind: WaterKind) -> f32 {
    match kind {
        WaterKind::Sea | WaterKind::Lake => CAUSTIC_STRENGTH_STILL,
        WaterKind::River => CAUSTIC_STRENGTH_FLOWING,
        WaterKind::Waterfall => 0.0,
    }
}

#[derive(Resource, Default, Clone, Debug)]
pub struct TileArrays {
    pub albedo: Handle<Image>,
    pub normal: Handle<Image>,
    pub orm: Handle<Image>,
    pub fallback: Handle<Image>,
    pub fallback_2d: Handle<Image>,
    pub ready: bool,
}

pub fn tile_sampler_descriptor() -> ImageSamplerDescriptor {
    let mut descriptor = ImageSamplerDescriptor::linear();
    descriptor.address_mode_u = ImageAddressMode::Repeat;
    descriptor.address_mode_v = ImageAddressMode::Repeat;
    descriptor.set_anisotropic_filter(MAX_ANISOTROPY);
    descriptor
}

fn layer_stack(tiles: &SurfaceTiles, slot: TileSlot) -> Vec<RgbaImage> {
    SurfaceLayer::ALL
        .iter()
        .filter_map(|layer| tiles.get(*layer, slot).cloned())
        .collect()
}

pub fn array_image(layers: &[RgbaImage], format: TextureFormat) -> Image {
    let width = layers[0].width();
    let height = layers[0].height();
    let levels = mip_level_count(width, height);
    let mut texel_bytes: Vec<u8> = Vec::new();
    for mip in 0..levels {
        let mip_width = mip_size(width, mip);
        let mip_height = mip_size(height, mip);
        for layer in layers {
            let resized;
            let texels = if mip == 0 {
                layer
            } else {
                resized = image::imageops::resize(
                    layer,
                    mip_width,
                    mip_height,
                    image::imageops::FilterType::Triangle,
                );
                &resized
            };
            texel_bytes.extend_from_slice(texels.as_raw());
        }
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width,
            height,
            depth_or_array_layers: layers.len() as u32,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.mip_level_count = levels;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image.data_order = TextureDataOrder::MipMajor;
    image.sampler = ImageSampler::Descriptor(tile_sampler_descriptor());
    image.data = Some(texel_bytes);
    image
}

pub fn fallback_array_image() -> Image {
    let layers = SurfaceLayer::ALL.len();
    let mut image = Image::new_uninit(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: layers as u32,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image.data = Some(vec![128; layers * 4]);
    image.sampler = ImageSampler::Descriptor(tile_sampler_descriptor());
    image
}

pub fn fallback_2d_image() -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2),
        ..default()
    });
    image.data = Some(vec![0, 0, 0, 255]);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

pub fn build_tile_arrays(tiles: &SurfaceTiles, images: &mut Assets<Image>) -> Option<TileArrays> {
    if !tiles.is_complete() {
        return None;
    }
    let fallback = images.add(fallback_array_image());
    let fallback_2d = images.add(fallback_2d_image());
    let albedo = images.add(array_image(
        &layer_stack(tiles, TileSlot::Albedo),
        TextureFormat::Rgba8UnormSrgb,
    ));
    let normal = images.add(array_image(
        &layer_stack(tiles, TileSlot::Normal),
        TextureFormat::Rgba8Unorm,
    ));
    let orm = images.add(array_image(
        &layer_stack(tiles, TileSlot::Orm),
        TextureFormat::Rgba8Unorm,
    ));
    Some(TileArrays {
        albedo,
        normal,
        orm,
        fallback,
        fallback_2d,
        ready: true,
    })
}

pub fn season_tint(season: Season) -> [f32; 4] {
    match season {
        Season::Winter => [0.88, 0.92, 1.0, 1.0],
        Season::Spring => [0.95, 1.0, 0.95, 1.0],
        Season::Summer => [1.0, 1.0, 1.0, 1.0],
        Season::Fall => [1.0, 0.93, 0.82, 1.0],
    }
}

pub fn season_tint_blend(clock: &SeasonClock) -> [f32; 4] {
    let current = season_tint(clock.season);
    let t = clock.blend_to_next();
    if t <= 0.0 {
        return current;
    }
    let next = season_tint(clock.season.next());
    let mut blended = current;
    for channel in 0..4 {
        blended[channel] = current[channel] + (next[channel] - current[channel]) * t;
    }
    blended
}

pub fn sky_uniform(
    profile: &RegionAtmosphereProfile,
    sun_direction: Vec3,
    sun_colour: Color,
    snow: f32,
) -> SkySurfaceUniform {
    let elevation = sun_direction.y;
    let intensity = atmosphere::daylight_factor(elevation);
    let brightened = profile
        .sky_zenith
        .mix(&Color::srgb(0.90, 0.95, 1.00), snow * 0.4);
    let haze = (1.0 - profile.visibility / 220.0).clamp(0.1, 1.0);
    SkySurfaceUniform {
        sun_direction: sun_direction.extend(intensity),
        sun_color: Vec4::from(sun_colour.to_linear().to_f32_array()),
        zenith: Vec4::from(brightened.to_linear().to_f32_array()),
        horizon: Vec4::from(profile.sky_horizon.to_linear().to_f32_array()),
        ground: Vec4::from(profile.fog_colour.to_linear().to_f32_array()),
        scattering: Vec2::new(0.76, haze),
        snow: snow.clamp(0.0, 1.0),
        exposure_gain: 0.75 + 0.35 * profile.ambient_boost,
    }
}

pub fn sky_render_material(params: SkySurfaceUniform) -> SkyRenderMaterial {
    SkyRenderMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            fog_enabled: false,
            cull_mode: None,
            alpha_mode: AlphaMode::Opaque,
            ..default()
        },
        extension: SkySurface { params },
    }
}

pub fn terrain_uniform(
    map: &TerrainMap,
    chunk: IVec2,
    chunk_size: f32,
    profile: &RegionMaterialProfile,
    clock: &SeasonClock,
) -> TerrainSurfaceUniform {
    let (low, high) = clamped_chunk_bounds(map, chunk, chunk_size);
    let span = (high - low).max(Vec2::splat(1.0));
    let centre = (low + high) * 0.5;
    let snow_amount = map.snow_coverage_at(centre.x, centre.y);
    TerrainSurfaceUniform {
        tint: Vec4::from(
            season::ground_tint(profile.tint, snow_amount)
                .to_linear()
                .to_f32_array(),
        ),
        season_tint: Vec4::from(season_tint_blend(clock)),
        chunk_low: low,
        chunk_size: span.x,
        control_resolution: splat::SPLAT_RESOLUTION as f32,
        sea_level: map.sea_level(),
        flood_low: map.flood_zone().low,
        flood_high: map.flood_zone().high,
        snow_height: season::snowline_for(clock, SNOWLINE_METRES),
        snow_slope_limit: SNOW_SLOPE_LIMIT,
        snow_amount,
        tile_scale: NEAR_TILE_METRES,
        far_scale: FAR_TILE_METRES,
        macro_scale: MACRO_TINT_METRES,
        roughness: profile.roughness,
        reflectance: profile.reflectance,
        wetness_gain: 1.0,
        debug_mode: DEBUG_MODE_TEXTURED,
        layer_count: SurfaceLayer::ALL.len() as u32,
        cliff_layer: profile.cliff as u32 + 1,
    }
}

pub fn terrain_render_material(
    profile: &RegionMaterialProfile,
    arrays: &TileArrays,
    control: Option<&ControlHandles>,
    params: TerrainSurfaceUniform,
) -> TerrainRenderMaterial {
    let (weights, indices, cell_data, cliff_meta, region_tint) = match control {
        Some(handles) => (
            handles.weights.clone(),
            handles.indices.clone(),
            handles.cell_data.clone(),
            handles.cliff_meta.clone(),
            handles.tint.clone(),
        ),
        None => (
            arrays.fallback_2d.clone(),
            arrays.fallback_2d.clone(),
            arrays.fallback_2d.clone(),
            arrays.fallback_2d.clone(),
            arrays.fallback_2d.clone(),
        ),
    };
    let albedo_array = if arrays.ready {
        arrays.albedo.clone()
    } else {
        arrays.fallback.clone()
    };
    let normal_array = if arrays.ready {
        arrays.normal.clone()
    } else {
        arrays.fallback.clone()
    };
    let orm_array = if arrays.ready {
        arrays.orm.clone()
    } else {
        arrays.fallback.clone()
    };
    TerrainRenderMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: profile.roughness,
            reflectance: profile.reflectance,
            ..default()
        },
        extension: TerrainSurface {
            params,
            albedo_array,
            normal_array,
            orm_array,
            splat_weights: weights,
            splat_indices: indices,
            cell_data,
            cliff_meta,
            region_tint,
        },
    }
}

pub fn water_render_material(
    base: StandardMaterial,
    kind: WaterKind,
    frozen: bool,
    use_vertex_data: bool,
) -> WaterRenderMaterial {
    let mut base = base;
    base.cull_mode = None;
    let shallow = base.base_color.to_linear().to_f32_array();
    let deep = [
        shallow[0] * 0.25,
        shallow[1] * 0.30,
        shallow[2] * 0.35,
        1.0,
    ];
    let sun = Vec3::new(0.4, 0.7, 0.3).normalize();
    WaterRenderMaterial {
        base,
        extension: WaterSurface {
            params: WaterSurfaceUniform {
                shallow_color: Vec4::from(shallow),
                deep_color: Vec4::from(deep),
                sky_zenith: Vec4::new(0.35, 0.55, 0.85, 1.0),
                sky_horizon: Vec4::new(0.65, 0.75, 0.88, 1.0),
                sun_color: Vec4::new(1.0, 0.96, 0.88, 1.0),
                sun_direction: sun.extend(1.0),
                wind: wind_scroll(prevailing_wind()),
                absorption: Vec4::new(
                    WATER_ABSORPTION_RGB.x,
                    WATER_ABSORPTION_RGB.y,
                    WATER_ABSORPTION_RGB.z,
                    WATER_MURKINESS,
                ),
                time: 0.0,
                wave_scale: WAVE_SCALE,
                wave_amplitude: WAVE_AMPLITUDE,
                foam_depth: FOAM_DEPTH,
                foam_scale: 1.0,
                frozen_blend: if frozen { 1.0 } else { 0.0 },
                ice_roughness: ICE_ROUGHNESS,
                fresnel_strength: FRESNEL_BASE,
                kind: kind_index(kind),
                use_vertex_data: u32::from(use_vertex_data),
                caustics: caustic_strength(kind),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;

    use super::*;
    use crate::terrain::map::TerrainConfig;
    use crate::terrain::material::DISK_ROOT;
    use crate::terrain::region::Region;
    use bevy::image::ImageFilterMode;
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::pbr::PbrPlugin;
    use bevy::shader::{Shader, ShaderLoader};

    fn image_app() -> App {
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default()).init_asset::<Image>();
        app
    }

    fn shader_text(path: &str) -> String {
        let full = format!("assets/{path}");
        std::fs::read_to_string(&full).unwrap_or_else(|error| panic!("{full}: {error}"))
    }

    #[test]
    fn terrain_shader_declares_every_binding_the_rust_side_uses() {
        let text = shader_text(TERRAIN_SHADER_PATH);
        assert!(text.contains("@fragment"), "terrain shader has no fragment entry point");
        assert!(text.contains("fn fragment("), "terrain shader fragment entry missing");
        assert!(
            text.contains("pbr_input_from_standard_material"),
            "terrain shader must compose the base PBR input"
        );
        assert!(text.contains("apply_pbr_lighting"), "terrain shader must light through bevy_pbr");
        assert!(
            text.contains("main_pass_post_lighting_processing"),
            "terrain shader must run fog and tonemap post-processing"
        );
        for binding in TERRAIN_SHADER_BINDINGS {
            let needle = format!("@binding({binding})");
            assert!(text.contains(&needle), "terrain shader missing {needle}");
        }
    }

    #[test]
    fn sky_shader_declares_every_binding_the_rust_side_uses() {
        let text = shader_text(SKY_SHADER_PATH);
        assert!(text.contains("@fragment"), "sky shader has no fragment entry point");
        assert!(text.contains("fn fragment("), "sky shader fragment entry missing");
        assert!(
            text.contains("main_pass_post_lighting_processing"),
            "sky shader must run tonemap post-processing"
        );
        assert!(text.contains("rayleigh"), "sky shader must do rayleigh-style scattering");
        assert!(text.contains("mie"), "sky shader must do mie-style scattering");
        for binding in SKY_SHADER_BINDINGS {
            let needle = format!("@binding({binding})");
            assert!(text.contains(&needle), "sky shader missing {needle}");
        }
    }

    #[test]
    fn water_shader_declares_every_binding_the_rust_side_uses() {
        let text = shader_text(WATER_SHADER_PATH);
        assert!(text.contains("@fragment"), "water shader has no fragment entry point");
        assert!(text.contains("fn fragment("), "water shader fragment entry missing");
        assert!(
            text.contains("main_pass_post_lighting_processing"),
            "water shader must run fog and tonemap post-processing"
        );
        for binding in WATER_SHADER_BINDINGS {
            let needle = format!("@binding({binding})");
            assert!(text.contains(&needle), "water shader missing {needle}");
        }
    }

    fn shader_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(bevy::log::LogPlugin {
                filter: "error".to_string(),
                ..default()
            })
            .add_plugins(AssetPlugin::default())
            .add_plugins(bevy::gltf::GltfPlugin::default())
            .init_asset::<Shader>()
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset_loader::<ShaderLoader>()
            .add_plugins(PbrPlugin::default());
        app
    }

    fn loaded_shader(app: &mut App, path: &str) -> Shader {
        let server = app.world().resource::<AssetServer>().clone();
        let handle = server.load::<Shader>(bevy::asset::AssetPath::from(path.to_string()));
        for _ in 0..600 {
            app.update();
            if app.world().resource::<Assets<Shader>>().contains(&handle) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.world()
            .resource::<Assets<Shader>>()
            .get(&handle)
            .cloned()
            .unwrap_or_else(|| panic!("{path} never landed in Assets<Shader>"))
    }

    #[test]
    fn shader_assets_register_with_the_asset_store() {
        let mut app = shader_app();
        let terrain = loaded_shader(&mut app, TERRAIN_SHADER_PATH);
        let water = loaded_shader(&mut app, WATER_SHADER_PATH);
        let sky = loaded_shader(&mut app, SKY_SHADER_PATH);
        assert!(
            terrain.source.as_str().contains("fn fragment("),
            "the loaded terrain shader carries no fragment entry point"
        );
        assert!(
            water.source.as_str().contains("fn fragment("),
            "the loaded water shader carries no fragment entry point"
        );
        assert!(
            sky.source.as_str().contains("fn fragment("),
            "the loaded sky shader carries no fragment entry point"
        );
        for (name, shader) in [("terrain", &terrain), ("water", &water), ("sky", &sky)] {
            let imports = format!("{:?}", shader.imports);
            assert!(
                !shader.imports.is_empty(),
                "{name} shader imports did not parse, so its wesl source is malformed"
            );
            for package in ["pbr_fragment", "pbr_functions", "forward_io"] {
                assert!(
                    imports.contains(package),
                    "{name} shader imports {imports} never resolved {package}"
                );
            }
        }
        let terrain_text = terrain.source.as_str();
        for token in [
            "stochastic_offsets",
            "sample_albedo_st",
            "textureSampleLevel",
            "region_tint",
            "REGION_ROUGHNESS_MIX",
        ] {
            assert!(
                terrain_text.contains(token),
                "the loaded terrain shader lost {token}"
            );
        }
        let water_text = water.source.as_str();
        for token in [
            "wind: vec4<f32>",
            "absorption: vec4<f32>",
            "flow_envelope",
            "standing_gradient",
            "running_gradient",
            "caustic_field",
            "caustic_layer",
        ] {
            assert!(
                water_text.contains(token),
                "the loaded water shader lost {token}"
            );
        }
    }

    #[test]
    fn terrain_debug_views_stay_exact_under_stochastic_tiling() {
        let text = shader_text(TERRAIN_SHADER_PATH);
        let fragment_at = text.find("fn fragment(").expect("terrain fragment entry");
        let first_stochastic_sample = text[fragment_at..]
            .find("sample_albedo_st(")
            .map(|at| fragment_at + at)
            .expect("stochastic sampling vanished from the terrain fragment");
        for marker in [
            "terrain.debug_mode == 1u",
            "terrain.debug_mode == 2u",
            "terrain.debug_mode == 3u",
        ] {
            let at = text.find(marker).unwrap_or_else(|| panic!("lost {marker}"));
            assert!(
                at < first_stochastic_sample,
                "the {marker} branch must return before any stochastic sampling"
            );
        }
        assert!(
            text.contains("uv_grid_colour(vertex_output.uv)"),
            "the uv-grid debug view must read the raw mesh uv, never a stochastic one"
        );
        let region_sample = text
            .find("textureSampleLevel(region_tint")
            .expect("the region tint sample vanished from the terrain fragment");
        for marker in [
            "terrain.debug_mode == 1u",
            "terrain.debug_mode == 2u",
            "terrain.debug_mode == 3u",
        ] {
            let at = text.find(marker).expect(marker);
            assert!(
                at < region_sample,
                "the {marker} branch must return before any region tint sampling"
            );
        }
        let tint_apply = text
            .find("select(vec3<f32>(1.0), region.rgb, tint_ok)")
            .expect("the region tint modulation vanished from the terrain fragment");
        assert!(
            tint_apply > region_sample,
            "the region tint must modulate the splatted albedo, not the debug branches"
        );
        let water = shader_text(WATER_SHADER_PATH);
        assert!(
            !water.contains("debug_mode") && !water.contains("stochastic"),
            "the water shader must not grow debug or stochastic paths that could leak into debug views"
        );
    }

    #[test]
    fn tile_arrays_pack_every_layer_with_mips() {
        let tiles = SurfaceTiles::load(DISK_ROOT);
        let mut app = image_app();
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let arrays = build_tile_arrays(&tiles, &mut images).expect("tiles on disk are complete");
        assert!(arrays.ready);
        for handle in [&arrays.albedo, &arrays.normal, &arrays.orm] {
            let image = images.get(handle).expect("array uploaded");
            assert_eq!(image.width(), splat::TILE_RESOLUTION);
            assert_eq!(image.height(), splat::TILE_RESOLUTION);
            assert_eq!(
                image.texture_descriptor.size.depth_or_array_layers,
                SurfaceLayer::ALL.len() as u32
            );
            assert_eq!(image.texture_descriptor.mip_level_count, TILE_MIP_LEVELS);
            assert_eq!(image.data_order, TextureDataOrder::MipMajor);
            let expected = mip_array_bytes(splat::TILE_RESOLUTION, SurfaceLayer::ALL.len());
            assert_eq!(image.data.as_ref().map(|data| data.len()), Some(expected));
            match &image.sampler {
                ImageSampler::Descriptor(descriptor) => {
                    assert_eq!(descriptor.anisotropy_clamp, MAX_ANISOTROPY);
                    assert_eq!(descriptor.address_mode_u, ImageAddressMode::Repeat);
                }
                ImageSampler::Default => panic!("tile arrays need an explicit sampler"),
            }
        }
    }

    fn mip_array_bytes(resolution: u32, layers: usize) -> usize {
        let mut total = 0usize;
        for mip in 0..resolution.ilog2() + 1 {
            let width = (resolution >> mip).max(1);
            let height = (resolution >> mip).max(1);
            total += width as usize * height as usize * 4 * layers;
        }
        total
    }

    fn clock_for(season: Season) -> SeasonClock {
        SeasonClock {
            season,
            progress: 0.0,
            running: false,
            ..Default::default()
        }
    }

    #[test]
    fn terrain_uniform_tracks_map_and_season() {
        let mut map = TerrainMap::new(TerrainConfig::compact());
        let profile = crate::terrain::material::material_profile(Region::TemperateGrassland);
        let summer_clock = clock_for(Season::Summer);
        let winter_clock = clock_for(Season::Winter);
        map.sync_season_state(&summer_clock);
        let summer = terrain_uniform(&map, IVec2::ZERO, 120.0, profile, &summer_clock);
        let summer_sea = map.sea_level();
        let summer_flood_high = map.flood_zone().high;
        map.sync_season_state(&winter_clock);
        let winter = terrain_uniform(&map, IVec2::ZERO, 120.0, profile, &winter_clock);
        let (low, high) = map.chunk_bounds(IVec2::ZERO, 120.0);
        assert_eq!(summer.chunk_low, low);
        assert!((summer.chunk_size - (high - low).x).abs() < 1e-5);
        assert!((summer.sea_level - summer_sea).abs() < 1e-5);
        assert!((summer.flood_high - summer_flood_high).abs() < 1e-5);
        assert!((winter.sea_level - map.sea_level()).abs() < 1e-5);
        assert!(summer.snow_amount < winter.snow_amount, "winter must carry more snow");
        assert!(summer.season_tint != winter.season_tint);
        assert_eq!(summer.layer_count, SurfaceLayer::ALL.len() as u32);
        assert_eq!(summer.cliff_layer, profile.cliff as u32 + 1);
    }

    #[test]
    fn terrain_uniform_tracks_the_interpolated_world_state() {
        let mut map = TerrainMap::new(TerrainConfig::compact());
        let profile = crate::terrain::material::material_profile(Region::TemperateGrassland);
        let winter_clock = clock_for(Season::Winter);
        let summer_clock = clock_for(Season::Summer);

        map.sync_season_state(&winter_clock);
        let winter = terrain_uniform(&map, IVec2::ZERO, 120.0, profile, &winter_clock);
        map.sync_season_state(&summer_clock);
        let summer = terrain_uniform(&map, IVec2::ZERO, 120.0, profile, &summer_clock);

        assert!(
            winter.sea_level < summer.sea_level,
            "the uniform sea level must follow the interpolated tide, not a placeholder"
        );
        assert!(winter.snow_height < summer.snow_height, "the snowline must track the season");
        assert!(winter.snow_amount > summer.snow_amount);
        assert!(winter.season_tint != summer.season_tint);

        let mut thawing = clock_for(Season::Spring);
        thawing.progress = 0.95;
        let tint = season_tint_blend(&thawing);
        let spring = season_tint(Season::Spring);
        let summer_tint = season_tint(Season::Summer);
        for channel in 0..3 {
            assert!(
                tint[channel] >= summer_tint[channel].min(spring[channel]) - 1e-5
                    && tint[channel] <= summer_tint[channel].max(spring[channel]) + 1e-5,
                "the blended tint must sit between the two seasons"
            );
        }
        assert!(tint != spring, "the season tail must already be blending");
    }

    #[test]
    fn sky_uniform_follows_the_atmosphere_and_sun() {
        let profile = *atmosphere::atmosphere_profile(Region::TemperateGrassland);
        let high = Vec3::new(0.0, 0.9, 0.4).normalize();
        let low = Vec3::new(0.0, 0.1, 0.9).normalize();
        let noon = sky_uniform(&profile, high, Color::WHITE, 0.0);
        let dusk = sky_uniform(&profile, low, Color::WHITE, 0.0);
        assert!(noon.sun_direction.w > dusk.sun_direction.w);
        assert!((noon.sun_direction.y - 0.9 / (0.9f32 * 0.9_f32 + 0.4 * 0.4).sqrt()).abs() < 1e-4);
        assert_eq!(noon.snow, 0.0);
        let snowy = sky_uniform(&profile, high, Color::WHITE, 1.0);
        assert!(snowy.zenith != noon.zenith, "snow must pale the zenith");
        assert!(noon.exposure_gain > 0.0);
    }

    #[test]
    fn water_uniforms_track_kind_and_frozen_state() {
        let base = StandardMaterial {
            base_color: Color::srgba(0.1, 0.3, 0.4, 0.76),
            alpha_mode: AlphaMode::Blend,
            ..default()
        };
        let sea = water_render_material(base.clone(), WaterKind::Sea, false, true);
        assert_eq!(sea.extension.params.kind, 0);
        assert_eq!(sea.extension.params.foam_depth, FOAM_DEPTH);
        assert_eq!(sea.extension.params.frozen_blend, 0.0);
        assert!(sea.base.cull_mode.is_none(), "water must render double-sided");
        let wind = sea.extension.params.wind;
        assert!(
            (wind.xy().length() - 1.0).abs() < 1e-4,
            "the wind scroll direction must be a unit vector"
        );
        assert!(
            wind.z >= WIND_SCROLL_MIN && wind.z <= WIND_SCROLL_MAX,
            "the wind speed must stay inside the scroll band"
        );
        let absorb = sea.extension.params.absorption;
        assert!(
            absorb.x > absorb.y && absorb.y >= absorb.z,
            "red must absorb fastest for per-channel beer-lambert"
        );
        assert!(absorb.w > 0.0, "murkiness must be positive");
        let frozen = water_render_material(base.clone(), WaterKind::Lake, true, true);
        assert_eq!(frozen.extension.params.frozen_blend, 1.0);
        assert_eq!(frozen.extension.params.kind, 1);
        let fall = water_render_material(base, WaterKind::Waterfall, false, false);
        assert_eq!(fall.extension.params.kind, 3);
        assert_eq!(fall.extension.params.use_vertex_data, 0);
        assert!(
            sea.extension.params.caustics > 0.0,
            "still water must carry caustics"
        );
        assert_eq!(frozen.extension.params.caustics, CAUSTIC_STRENGTH_STILL);
        assert_eq!(
            fall.extension.params.caustics, 0.0,
            "aeration on a waterfall must not bake caustics"
        );
    }

    #[test]
    fn caustic_strength_follows_water_kind() {
        assert_eq!(caustic_strength(WaterKind::Sea), CAUSTIC_STRENGTH_STILL);
        assert_eq!(caustic_strength(WaterKind::Lake), CAUSTIC_STRENGTH_STILL);
        assert_eq!(caustic_strength(WaterKind::River), CAUSTIC_STRENGTH_FLOWING);
        assert_eq!(caustic_strength(WaterKind::Waterfall), 0.0);
        assert!(CAUSTIC_STRENGTH_FLOWING < CAUSTIC_STRENGTH_STILL);
    }

    #[test]
    fn prevailing_wind_flows_into_the_water_uniform() {
        let before = prevailing_wind();
        set_prevailing_wind(Vec2::new(0.0, 1.5));
        let sea = water_render_material(StandardMaterial::default(), WaterKind::Sea, false, true);
        let wind = sea.extension.params.wind;
        assert!(wind.x.abs() < 1e-6);
        assert!((wind.y - 1.0).abs() < 1e-6, "the scroll must follow the set wind");
        assert!((wind.z - 1.5).abs() < 1e-6, "the scroll speed is the wind strength");
        set_prevailing_wind(before);

        let calm = wind_scroll(Vec2::ZERO);
        assert!(calm.z >= WIND_SCROLL_MIN, "calm air still gets a gentle default scroll");
        let gale = wind_scroll(Vec2::new(50.0, 0.0));
        assert!((gale.x - 1.0).abs() < 1e-6);
        assert!((gale.z - WIND_SCROLL_MAX).abs() < 1e-6, "gales are clamped");
    }

    fn wgsl_struct_fields(text: &str, name: &str) -> Vec<(String, String)> {
        let header = format!("struct {name}");
        let start = text
            .find(&header)
            .unwrap_or_else(|| panic!("shader has no {name}"));
        let open = text[start..].find('{').expect("struct open brace") + start;
        let close = text[start..].find('}').expect("struct close brace") + start;
        text[open + 1..close]
            .lines()
            .map(|line| line.trim().trim_end_matches(',').to_string())
            .filter(|line| !line.is_empty())
            .map(|line| {
                let mut parts = line.splitn(2, ':');
                let field = parts.next().expect("field name").trim().to_string();
                let ty = parts.next().expect("field type").trim().to_string();
                (field, ty)
            })
            .collect()
    }

    fn wgsl_size_align(ty: &str) -> (usize, usize) {
        match ty {
            "f32" | "u32" | "i32" => (4, 4),
            "vec2<f32>" => (8, 8),
            "vec3<f32>" | "vec4<f32>" => (16, 16),
            other => panic!("unhandled wgsl type {other}"),
        }
    }

    fn wgsl_layout(fields: &[(String, String)]) -> (Vec<(String, usize, usize)>, usize) {
        let mut offset = 0usize;
        let mut max_align = 1usize;
        let mut placed = Vec::new();
        for (name, ty) in fields {
            let (size, align) = wgsl_size_align(ty);
            max_align = max_align.max(align);
            offset = offset.next_multiple_of(align);
            placed.push((name.clone(), offset, size));
            offset += size;
        }
        (placed, offset.next_multiple_of(max_align))
    }

    #[test]
    fn water_uniform_layout_matches_the_shader() {
        let text = shader_text(WATER_SHADER_PATH);
        let fields = wgsl_struct_fields(&text, "WaterSurface");
        let rust: Vec<(&str, usize, usize)> = vec![
            ("shallow_color", offset_of!(WaterSurfaceUniform, shallow_color), 16),
            ("deep_color", offset_of!(WaterSurfaceUniform, deep_color), 16),
            ("sky_zenith", offset_of!(WaterSurfaceUniform, sky_zenith), 16),
            ("sky_horizon", offset_of!(WaterSurfaceUniform, sky_horizon), 16),
            ("sun_color", offset_of!(WaterSurfaceUniform, sun_color), 16),
            ("sun_direction", offset_of!(WaterSurfaceUniform, sun_direction), 16),
            ("wind", offset_of!(WaterSurfaceUniform, wind), 16),
            ("absorption", offset_of!(WaterSurfaceUniform, absorption), 16),
            ("time", offset_of!(WaterSurfaceUniform, time), 4),
            ("wave_scale", offset_of!(WaterSurfaceUniform, wave_scale), 4),
            ("wave_amplitude", offset_of!(WaterSurfaceUniform, wave_amplitude), 4),
            ("foam_depth", offset_of!(WaterSurfaceUniform, foam_depth), 4),
            ("foam_scale", offset_of!(WaterSurfaceUniform, foam_scale), 4),
            ("frozen_blend", offset_of!(WaterSurfaceUniform, frozen_blend), 4),
            ("ice_roughness", offset_of!(WaterSurfaceUniform, ice_roughness), 4),
            ("fresnel_strength", offset_of!(WaterSurfaceUniform, fresnel_strength), 4),
            ("kind", offset_of!(WaterSurfaceUniform, kind), 4),
            ("use_vertex_data", offset_of!(WaterSurfaceUniform, use_vertex_data), 4),
            ("caustics", offset_of!(WaterSurfaceUniform, caustics), 4),
        ];
        let (placed, total) = wgsl_layout(&fields);
        assert_eq!(fields.len(), rust.len(), "water struct field count drifted");
        for (name, rust_offset, rust_size) in &rust {
            let found = placed
                .iter()
                .find(|(field, _, _)| field == name)
                .unwrap_or_else(|| panic!("WaterSurface has no wgsl field {name}"));
            assert_eq!(found.1, *rust_offset, "offset of {name}");
            assert_eq!(found.2, *rust_size, "size of {name}");
        }
        assert_eq!(
            WaterSurfaceUniform::min_size().get() as usize,
            total,
            "the shader struct and the rust ShaderType disagree on total size"
        );
        assert_eq!(std::mem::size_of::<WaterSurfaceUniform>(), total);
    }

    #[test]
    fn terrain_uniform_layout_matches_the_shader() {
        let text = shader_text(TERRAIN_SHADER_PATH);
        let fields = wgsl_struct_fields(&text, "TerrainSurface");
        let rust: Vec<(&str, usize, usize)> = vec![
            ("tint", offset_of!(TerrainSurfaceUniform, tint), 16),
            ("season_tint", offset_of!(TerrainSurfaceUniform, season_tint), 16),
            ("chunk_low", offset_of!(TerrainSurfaceUniform, chunk_low), 8),
            ("chunk_size", offset_of!(TerrainSurfaceUniform, chunk_size), 4),
            ("control_resolution", offset_of!(TerrainSurfaceUniform, control_resolution), 4),
            ("sea_level", offset_of!(TerrainSurfaceUniform, sea_level), 4),
            ("flood_low", offset_of!(TerrainSurfaceUniform, flood_low), 4),
            ("flood_high", offset_of!(TerrainSurfaceUniform, flood_high), 4),
            ("snow_height", offset_of!(TerrainSurfaceUniform, snow_height), 4),
            ("snow_slope_limit", offset_of!(TerrainSurfaceUniform, snow_slope_limit), 4),
            ("snow_amount", offset_of!(TerrainSurfaceUniform, snow_amount), 4),
            ("tile_scale", offset_of!(TerrainSurfaceUniform, tile_scale), 4),
            ("far_scale", offset_of!(TerrainSurfaceUniform, far_scale), 4),
            ("macro_scale", offset_of!(TerrainSurfaceUniform, macro_scale), 4),
            ("roughness", offset_of!(TerrainSurfaceUniform, roughness), 4),
            ("reflectance", offset_of!(TerrainSurfaceUniform, reflectance), 4),
            ("wetness_gain", offset_of!(TerrainSurfaceUniform, wetness_gain), 4),
            ("debug_mode", offset_of!(TerrainSurfaceUniform, debug_mode), 4),
            ("layer_count", offset_of!(TerrainSurfaceUniform, layer_count), 4),
            ("cliff_layer", offset_of!(TerrainSurfaceUniform, cliff_layer), 4),
        ];
        let (placed, total) = wgsl_layout(&fields);
        assert_eq!(fields.len(), rust.len(), "terrain struct field count drifted");
        for (name, rust_offset, rust_size) in &rust {
            let found = placed
                .iter()
                .find(|(field, _, _)| field == name)
                .unwrap_or_else(|| panic!("TerrainSurface has no wgsl field {name}"));
            assert_eq!(found.1, *rust_offset, "offset of {name}");
            assert_eq!(found.2, *rust_size, "size of {name}");
        }
        assert_eq!(TerrainSurfaceUniform::min_size().get() as usize, total);
        assert_eq!(std::mem::size_of::<TerrainSurfaceUniform>(), total);
    }

    #[test]
    fn terrain_render_material_falls_back_without_control_maps() {
        let mut app = image_app();
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let arrays = TileArrays {
            fallback: images.add(fallback_array_image()),
            ..default()
        };
        let profile = crate::terrain::material::material_profile(Region::TemperateGrassland);
        let built = terrain_render_material(profile, &arrays, None, TerrainSurfaceUniform::default());
        assert_eq!(built.extension.splat_weights, arrays.fallback_2d);
        assert_eq!(built.extension.region_tint, arrays.fallback_2d);
        assert_eq!(built.extension.albedo_array, arrays.fallback);
        assert_eq!(built.base.base_color, Color::WHITE);
    }

    #[test]
    fn control_tint_texture_uploads_linear_and_clamped() {
        let map = TerrainMap::new(TerrainConfig::compact());
        let chunk_size = 120.0f32.min(map.size());
        let controls = splat::bake_control_chunk(&map, IVec2::ZERO, chunk_size, 16);
        let mut app = image_app();
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let handles = splat::upload_control(&controls, &mut images);
        let image = images.get(&handles.tint).expect("tint texture uploaded");
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        match &image.sampler {
            ImageSampler::Descriptor(descriptor) => {
                assert_eq!(descriptor.address_mode_u, ImageAddressMode::ClampToEdge);
                assert_eq!(descriptor.address_mode_v, ImageAddressMode::ClampToEdge);
                assert_eq!(descriptor.mag_filter, ImageFilterMode::Linear);
            }
            ImageSampler::Default => panic!("the tint control map needs an explicit sampler"),
        }
        assert!(image.texture_descriptor.mip_level_count > 1);
    }
}
