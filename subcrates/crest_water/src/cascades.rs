use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::{
        Extract, ExtractSchedule, RenderApp,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    },
};

use crate::settings::OceanSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CascadeData {
    SeaFloorDepth,
    AnimatedWaves,
    AnimatedDerivs,
    DynamicWaves,
    DynamicPrev,
    Flow,
    Foam,
    FoamPrev,
    Shadow,
    ClipSurface,
    SpectrumInit,
    SpectrumHeight,
    SpectrumDispX,
    SpectrumDispZ,
    FftDisp,
    FftDerivs,
}

pub const ALL_CASCADE_DATA: [CascadeData; 16] = [
    CascadeData::SeaFloorDepth,
    CascadeData::AnimatedWaves,
    CascadeData::AnimatedDerivs,
    CascadeData::DynamicWaves,
    CascadeData::DynamicPrev,
    CascadeData::Flow,
    CascadeData::Foam,
    CascadeData::FoamPrev,
    CascadeData::Shadow,
    CascadeData::ClipSurface,
    CascadeData::SpectrumInit,
    CascadeData::SpectrumHeight,
    CascadeData::SpectrumDispX,
    CascadeData::SpectrumDispZ,
    CascadeData::FftDisp,
    CascadeData::FftDerivs,
];

impl CascadeData {
    pub fn format(self) -> TextureFormat {
        match self {
            CascadeData::SeaFloorDepth => TextureFormat::R16Float,
            CascadeData::DynamicWaves | CascadeData::DynamicPrev => TextureFormat::Rg32Float,
            CascadeData::AnimatedWaves | CascadeData::SpectrumInit | CascadeData::FftDisp => {
                TextureFormat::Rgba16Float
            }
            CascadeData::AnimatedDerivs
            | CascadeData::Flow
            | CascadeData::SpectrumHeight
            | CascadeData::SpectrumDispX
            | CascadeData::SpectrumDispZ
            | CascadeData::FftDerivs => TextureFormat::Rg32Float,
            CascadeData::Foam | CascadeData::FoamPrev => TextureFormat::R16Float,
            CascadeData::Shadow | CascadeData::ClipSurface => TextureFormat::R8Unorm,
        }
    }

    pub fn bytes_per_pixel(self) -> usize {
        match self {
            CascadeData::SeaFloorDepth | CascadeData::Foam | CascadeData::FoamPrev => 2,
            CascadeData::DynamicWaves | CascadeData::DynamicPrev => 8,
            CascadeData::AnimatedWaves | CascadeData::SpectrumInit | CascadeData::FftDisp => 8,
            CascadeData::AnimatedDerivs
            | CascadeData::Flow
            | CascadeData::SpectrumHeight
            | CascadeData::SpectrumDispX
            | CascadeData::SpectrumDispZ
            | CascadeData::FftDerivs => 8,
            CascadeData::Shadow | CascadeData::ClipSurface => 1,
        }
    }
}

#[derive(Resource, Default)]
pub struct Cascades {
    pub images: Vec<(CascadeData, Handle<Image>)>,
    pub resolution: usize,
    pub lod_count: usize,
    pub generation: u64,
}

impl Cascades {
    pub fn image(&self, data: CascadeData) -> Option<&Handle<Image>> {
        self.images
            .iter()
            .find(|(kind, _)| *kind == data)
            .map(|(_, handle)| handle)
    }
}

#[derive(Resource, Default)]
pub struct ExtractedCascades {
    pub images: Vec<(CascadeData, Handle<Image>)>,
    pub resolution: u32,
    pub lod_count: u32,
    pub generation: u64,
}

impl ExtractedCascades {
    pub fn image(&self, data: CascadeData) -> Option<&Handle<Image>> {
        self.images
            .iter()
            .find(|(kind, _)| *kind == data)
            .map(|(_, handle)| handle)
    }
}

#[derive(Resource, Default, Clone)]
pub struct QuerySlices {
    pub animated: Handle<Image>,
    pub flow: Handle<Image>,
    pub depth: Handle<Image>,
}

fn cascade_image(resolution: usize, lod_count: usize, data: CascadeData) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: resolution as u32,
            height: resolution as u32,
            depth_or_array_layers: lod_count as u32,
        },
        TextureDimension::D2,
        vec![0u8; resolution * resolution * lod_count * data.bytes_per_pixel()],
        data.format(),
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING
        | TextureUsages::COPY_SRC
        | TextureUsages::COPY_DST
        | TextureUsages::TEXTURE_BINDING;
    image
}

pub const QUERY_SLICE_SIZE: usize = 16;

fn query_image(format: TextureFormat, bytes_per_pixel: usize) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: QUERY_SLICE_SIZE as u32,
            height: QUERY_SLICE_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0u8; QUERY_SLICE_SIZE * QUERY_SLICE_SIZE * bytes_per_pixel],
        format,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.texture_descriptor.usage |=
        TextureUsages::COPY_SRC | TextureUsages::COPY_DST | TextureUsages::STORAGE_BINDING;
    image
}

fn build_cascades(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    settings: Res<OceanSettings>,
    existing: Option<Res<Cascades>>,
) {
    if let Some(existing) = existing.as_ref()
        && existing.resolution == settings.lod_resolution
        && existing.lod_count == settings.lod_count
    {
        return;
    }
    let generation = existing.map(|existing| existing.generation + 1).unwrap_or(1);
    let images_handles: Vec<(CascadeData, Handle<Image>)> = ALL_CASCADE_DATA
        .iter()
        .map(|data| {
            (
                *data,
                images.add(cascade_image(
                    settings.lod_resolution,
                    settings.lod_count,
                    *data,
                )),
            )
        })
        .collect();
    commands.insert_resource(Cascades {
        images: images_handles,
        resolution: settings.lod_resolution,
        lod_count: settings.lod_count,
        generation,
    });
}

fn build_query_slices(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(QuerySlices {
        animated: images.add(query_image(TextureFormat::Rgba32Float, 16)),
        flow: images.add(query_image(TextureFormat::Rgba32Float, 16)),
        depth: images.add(query_image(TextureFormat::Rgba32Float, 16)),
    });
}

fn extract_query_slices(
    slices: Extract<Res<QuerySlices>>,
    mut extracted: ResMut<QuerySlices>,
) {
    if slices.animated != extracted.animated
        || slices.flow != extracted.flow
        || slices.depth != extracted.depth
    {
        *extracted = slices.clone();
    }
}

fn extract_cascades(
    cascades: Extract<Res<Cascades>>,
    mut extracted: ResMut<ExtractedCascades>,
) {
    if extracted.generation != cascades.generation {
        extracted.images = cascades.images.clone();
        extracted.resolution = cascades.resolution as u32;
        extracted.lod_count = cascades.lod_count as u32;
        extracted.generation = cascades.generation;
    }
}

pub struct CascadesPlugin;

impl Plugin for CascadesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (build_cascades, build_query_slices).chain())
            .add_systems(Update, build_cascades);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedCascades>()
            .init_resource::<QuerySlices>()
            .add_systems(ExtractSchedule, (extract_cascades, extract_query_slices));
    }
}
