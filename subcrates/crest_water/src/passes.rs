use std::collections::HashMap;

use bevy::{
    prelude::*,
    render::{
        Extract, ExtractSchedule, RenderApp,
        render_asset::RenderAssets,
        render_resource::{
            BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource,
            Buffer, BufferBinding, BufferDescriptor, BufferUsages, PipelineCache,
            ShaderType, TextureFormat, TextureView, encase::UniformBuffer,
        },
        renderer::{RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue},
        texture::GpuImage,
    },
    shader::Shader,
};

use crate::cascades::{
    CascadeData, CascadeData::*, ExtractedCascades, QUERY_SLICE_SIZE, QueryPointTextures,
    QuerySlices,
};
use crate::compute::{
    BakeUniforms, CascadePipelines, CombineUniforms, ComputePassSpec, CopySliceUniforms,
    DepthUniforms, DynWavesUniforms, FftUniforms, FillUniforms, FlowUniforms, FoamUniforms,
    GerstnerUniform, LodUniforms, MAX_GERSTNER, MAX_QUERY_POINTS, SpectrumUniforms,
    SpectrumUpdateUniforms, WaterQueryUniforms, dispatch, pipeline_for, sample_2d_layout_entry,
    sample_array_layout_entry, storage_layout_entry, storage_texture_layout_entry, uniform_entry,
    uniform_layout_entry,
};
use crate::inputs::{ExtractedInputs, WaterQueryPoint, f32_bytes};
use crate::lod::LodTransforms;
use crate::reference;
use crate::settings::{OceanSettings, average_wavelength};

const DYN_GAIN: f32 = 1.0;
const FOAM_DECAY: f32 = 0.8;
const FOAM_FOLDING: f32 = 0.55;
const FOAM_WAVE_GAIN: f32 = 1.0;
const FOAM_SHORELINE_DEPTH: f32 = 0.65;
const FOAM_SHORELINE_GAIN: f32 = 2.0;
const FOAM_DYNAMIC_GAIN: f32 = 0.0;

#[derive(Resource, Clone)]
pub struct CascadeShaders {
    pub seafloor_depth: Handle<Shader>,
    pub spectrum_init: Handle<Shader>,
    pub spectrum_update: Handle<Shader>,
    pub fft: Handle<Shader>,
    pub fft_bake: Handle<Shader>,
    pub shape_combine: Handle<Shader>,
    pub dynwaves: Handle<Shader>,
    pub flow: Handle<Shader>,
    pub foam: Handle<Shader>,
    pub fill: Handle<Shader>,
    pub copy_slice: Handle<Shader>,
    pub copy_foam: Handle<Shader>,
    pub water_query: Handle<Shader>,
}

#[derive(Resource, Default)]
pub struct ExtractedFrame {
    pub settings: OceanSettings,
    pub settings_changed: bool,
    pub lods: LodTransforms,
    pub time: f32,
    pub dt: f32,
    pub query_point: Option<Vec3>,
    pub frame: u64,
}

fn extract_frame(
    mut out: ResMut<ExtractedFrame>,
    settings: Extract<Res<OceanSettings>>,
    lods: Extract<Res<LodTransforms>>,
    time: Extract<Res<Time>>,
    query: Extract<Res<WaterQueryPoint>>,
) {
    out.settings_changed = out.settings != **settings;
    out.settings = settings.clone();
    out.lods = lods.clone();
    out.time = time.elapsed_secs();
    out.dt = time.delta_secs().clamp(0.0, 1.0 / 20.0);
    out.query_point = query.position;
    out.frame += 1;
}

#[derive(Resource, Default)]
pub struct CascadeBuffers {
    pub layouts: HashMap<&'static str, BindGroupLayout>,
    pub lod: Option<Buffer>,
    pub spectrum: Option<Buffer>,
    pub spectrum_update: Option<Buffer>,
    pub bake: Option<Buffer>,
    pub fft: Option<Buffer>,
    pub combine: Option<Buffer>,
    pub depth: Option<Buffer>,
    pub dynwaves: Option<Buffer>,
    pub foam: Option<Buffer>,
    pub flow: Option<Buffer>,
    pub fill: Option<Buffer>,
    pub clip_fill: Option<Buffer>,
    pub copy_slice: Option<Buffer>,
    pub water_query: Option<Buffer>,
    pub query_points: Option<Buffer>,
    pub emitters: Option<Buffer>,
    pub emitter_velocities: Option<Buffer>,
    pub flow_headers: Option<Buffer>,
    pub foam_headers: Option<Buffer>,
    pub spline_points: Option<Buffer>,
    pub carves: Option<Buffer>,
    pub prev_origins: [Vec2; 8],
    pub prev_origins_set: bool,
    pub spectrum_ready: bool,
    pub generation: u64,
}

impl CascadeBuffers {
    fn layout(
        &mut self,
        device: &RenderDevice,
        label: &'static str,
        entries: &[BindGroupLayoutEntry],
    ) -> BindGroupLayout {
        if let Some(layout) = self.layouts.get(label) {
            return layout.clone();
        }
        let layout = device.create_bind_group_layout(label, entries);
        self.layouts.insert(label, layout.clone());
        layout
    }
}

fn refresh_uniform<T: ShaderType + bevy::render::render_resource::encase::internal::WriteInto>(
    device: &RenderDevice,
    queue: &RenderQueue,
    slot: &mut Option<Buffer>,
    value: &T,
) {
    let mut encoded = UniformBuffer::new(Vec::new());
    encoded.write(value).expect("uniform encodes");
    let bytes = encoded.into_inner();
    let buffer = slot.get_or_insert_with(|| {
        device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes.len() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    });
    queue.write_buffer(buffer, 0, &bytes);
}

fn write_uniform<T: ShaderType + bevy::render::render_resource::encase::internal::WriteInto>(
    queue: &RenderQueue,
    buffer: &Buffer,
    value: &T,
) {
    let mut encoded = UniformBuffer::new(Vec::new());
    encoded.write(value).expect("uniform encodes");
    queue.write_buffer(buffer, 0, &encoded.into_inner());
}

fn refresh_storage(
    device: &RenderDevice,
    queue: &RenderQueue,
    slot: &mut Option<Buffer>,
    bytes: &[u8],
) {
    let size = bytes.len().max(16) as u64;
    let stale = slot
        .as_ref()
        .map(|buffer| buffer.size() < size)
        .unwrap_or(true);
    if stale {
        *slot = Some(device.create_buffer(&BufferDescriptor {
            label: None,
            size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    queue.write_buffer(slot.as_ref().expect("storage exists"), 0, bytes);
}

fn storage_binding(binding: u32, buffer: &Buffer) -> (u32, BindingResource<'_>) {
    (
        binding,
        BindingResource::Buffer(BufferBinding {
            buffer,
            offset: 0,
            size: None,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn run_cascades(
    frame: Res<ExtractedFrame>,
    cascades: Res<ExtractedCascades>,
    inputs: Res<ExtractedInputs>,
    slices: Res<QuerySlices>,
    point_textures: Res<QueryPointTextures>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cache: Res<PipelineCache>,
    shaders: Res<CascadeShaders>,
    mut pipelines: ResMut<CascadePipelines>,
    mut buffers: ResMut<CascadeBuffers>,
) {
    if cascades.lod_count == 0 {
        return;
    }
    if buffers.generation != cascades.generation {
        *buffers = CascadeBuffers::default();
        buffers.generation = cascades.generation;
    }

    let settings = &frame.settings;
    let res = cascades.resolution;
    let lods = cascades.lod_count.min(8);
    let wg = res.div_ceil(8);
    let size = res;

    let view = |data: CascadeData| -> Option<TextureView> {
        cascades
            .image(data)
            .and_then(|handle| images.get(handle))
            .map(|image| image.texture_view.clone())
    };
    let slice_view = |handle: &Handle<Image>| {
        images
            .get(handle)
            .map(|image| image.texture_view.clone())
    };

    let (
        depth,
        waves,
        derivs,
        dyn_now,
        dyn_prev,
        flow_tex,
        foam,
        foam_prev,
        shadow,
        clip,
        spectrum_init,
        spectrum_height,
        spectrum_disp_x,
        spectrum_disp_z,
        fft_disp,
        fft_derivs,
        slice_animated,
        slice_flow,
        slice_depth,
    ) = match (
        view(SeaFloorDepth),
        view(AnimatedWaves),
        view(AnimatedDerivs),
        view(DynamicWaves),
        view(DynamicPrev),
        view(Flow),
        view(Foam),
        view(FoamPrev),
        view(Shadow),
        view(ClipSurface),
        view(SpectrumInit),
        view(SpectrumHeight),
        view(SpectrumDispX),
        view(SpectrumDispZ),
        view(FftDisp),
        view(FftDerivs),
        slice_view(&slices.animated),
        slice_view(&slices.flow),
        slice_view(&slices.depth),
    ) {
        (
            Some(depth),
            Some(waves),
            Some(derivs),
            Some(dyn_now),
            Some(dyn_prev),
            Some(flow_tex),
            Some(foam),
            Some(foam_prev),
            Some(shadow),
            Some(clip),
            Some(spectrum_init),
            Some(spectrum_height),
            Some(spectrum_disp_x),
            Some(spectrum_disp_z),
            Some(fft_disp),
            Some(fft_derivs),
            Some(slice_animated),
            Some(slice_flow),
            Some(slice_depth),
        ) => (
            depth,
            waves,
            derivs,
            dyn_now,
            dyn_prev,
            flow_tex,
            foam,
            foam_prev,
            shadow,
            clip,
            spectrum_init,
            spectrum_height,
            spectrum_disp_x,
            spectrum_disp_z,
            fft_disp,
            fft_derivs,
            slice_animated,
            slice_flow,
            slice_depth,
        ),
        _ => return,
    };
    let grid = images
        .get(&inputs.grid.image)
        .map(|image| image.texture_view.clone())
        .unwrap_or_else(|| slice_animated.clone());
    let query_animated = slice_view(&point_textures.animated);
    let query_flow = slice_view(&point_textures.flow);
    let query_depth = slice_view(&point_textures.depth);

    let mut shifts = [Vec4::ZERO; 8];
    if buffers.prev_origins_set {
        for lod in 0..lods as usize {
            let transform = frame.lods.transforms[lod];
            let shift =
                ((transform.origin.xz() - buffers.prev_origins[lod]) / transform.texel_size)
                    .round();
            shifts[lod] = Vec4::new(shift.x, shift.y, 0.0, 0.0);
        }
    }
    for lod in 0..lods as usize {
        buffers.prev_origins[lod] = frame.lods.transforms[lod].origin.xz();
    }
    buffers.prev_origins_set = true;

    let lod_uniform = LodUniforms {
        lods: frame.lods.transforms,
        lod_count: lods,
        resolution: res,
        time: frame.time,
        water_level: settings.water_level,
    };
    let depth_uniform = DepthUniforms {
        grid_origin: inputs.grid.origin,
        grid_texel: inputs.grid.texel,
        grid_size: inputs.grid.size,
        water_level: settings.water_level,
        resolution: res,
        lod_count: lods,
    };
    let spectrum_uniform = SpectrumUniforms {
        size,
        lod_count: lods,
        wind_speed: settings.waves.wind_speed,
        gravity: settings.waves.gravity,
        loop_period: settings.waves.loop_period,
        turbulence: settings.waves.turbulence,
        chop: settings.waves.chop,
        fft_world: settings.cascade_extent(0),
        wind_direction: settings.waves.wind_direction,
        power_log: std::array::from_fn(|i| Vec4::splat(reference::spectrum::POWER_LOG[i])),
    };
    let spectrum_update_uniform = SpectrumUpdateUniforms {
        time: frame.time,
        chop: settings.waves.chop,
        gravity: settings.waves.gravity,
        loop_period: settings.waves.loop_period,
        size,
        lod_count: lods,
        fft_world: settings.cascade_extent(0),
        pad1: 0,
    };
    let bake_uniform = BakeUniforms {
        size,
        lod_count: lods,
        fft_world: settings.cascade_extent(0),
        pad1: 0,
    };
    let mut gerstner = [GerstnerUniform::default(); MAX_GERSTNER];
    let gerstner_count = settings.waves.gerstner_count.min(MAX_GERSTNER as u32) as usize;
    for index in 0..gerstner_count {
        gerstner[index] = GerstnerUniform {
            wavelength: settings.waves.gerstner_wavelengths[index],
            amplitude: settings.waves.gerstner_amplitudes[index],
            speed: settings.waves.gerstner_speeds[index],
            steepness: settings.waves.gerstner_steepness,
            direction: settings.waves.gerstner_directions[index],
            phase: settings.waves.gerstner_phases[index],
            chop: settings.waves.gerstner_chop,
        };
    }
    let combine_uniform = CombineUniforms {
        attenuation_in_shallows: settings.waves.attenuation_in_shallows,
        shallows_max_depth: settings.waves.shallows_max_depth,
        gerstner_count: gerstner_count as u32,
        base_texel_size: settings.base_texel_size,
        dynamic_gain: DYN_GAIN,
        average_wavelength: average_wavelength(settings),
        fft_world: settings.cascade_extent(0),
        pad2: 0.0,
        gerstner,
    };
    let dyn_uniform = DynWavesUniforms {
        dt: frame.dt,
        wave_speed: settings.dyn_wave_speed,
        damping: settings.dyn_damping,
        resolution: res,
        emitter_count: inputs.emitters.len() as u32,
        lod_count: lods,
        base_texel_size: settings.base_texel_size,
        pad: 0.0,
        shifts,
    };
    let foam_uniform = FoamUniforms {
        dt: frame.dt,
        decay: FOAM_DECAY,
        wave_folding_threshold: FOAM_FOLDING,
        shoreline_depth: FOAM_SHORELINE_DEPTH,
        shoreline_width: 1.0,
        wave_gain: FOAM_WAVE_GAIN,
        shoreline_gain: FOAM_SHORELINE_GAIN,
        dynamic_gain: FOAM_DYNAMIC_GAIN,
        resolution: res,
        lod_count: lods,
        spline_count: inputs.foam_spline_headers.len() as u32,
        pad: 0.0,
        shifts,
    };
    let flow_uniform = FlowUniforms {
        spline_count: inputs.flow_spline_headers.len() as u32,
        resolution: res,
        lod_count: lods,
        pad: 0.0,
    };
    let fill_uniform = FillUniforms {
        value: 1.0,
        resolution: res,
        lod_count: lods,
        carve_count: 0,
    };
    let clip_uniform = FillUniforms {
        carve_count: inputs.carves.len() as u32,
        ..fill_uniform
    };

    refresh_uniform(&device, &queue, &mut buffers.lod, &lod_uniform);
    refresh_uniform(&device, &queue, &mut buffers.depth, &depth_uniform);
    refresh_uniform(&device, &queue, &mut buffers.spectrum, &spectrum_uniform);
    refresh_uniform(
        &device,
        &queue,
        &mut buffers.spectrum_update,
        &spectrum_update_uniform,
    );
    refresh_uniform(&device, &queue, &mut buffers.bake, &bake_uniform);
    refresh_uniform(
        &device,
        &queue,
        &mut buffers.fft,
        &FftUniforms {
            size,
            axis: 0,
            inverse: 1,
            pad: 0,
        },
    );
    refresh_uniform(&device, &queue, &mut buffers.combine, &combine_uniform);
    refresh_uniform(&device, &queue, &mut buffers.dynwaves, &dyn_uniform);
    refresh_uniform(&device, &queue, &mut buffers.foam, &foam_uniform);
    refresh_uniform(&device, &queue, &mut buffers.flow, &flow_uniform);
    refresh_uniform(&device, &queue, &mut buffers.fill, &fill_uniform);
    refresh_uniform(&device, &queue, &mut buffers.clip_fill, &clip_uniform);
    refresh_uniform(
        &device,
        &queue,
        &mut buffers.copy_slice,
        &CopySliceUniforms::default(),
    );
    refresh_uniform(
        &device,
        &queue,
        &mut buffers.water_query,
        &WaterQueryUniforms {
            lods: frame.lods.transforms,
            count: inputs.query_points.len() as u32,
            resolution: res,
            lod_count: lods,
            pad: 0,
        },
    );
    refresh_storage(
        &device,
        &queue,
        &mut buffers.emitters,
        &f32_bytes(&inputs.emitters),
    );
    refresh_storage(
        &device,
        &queue,
        &mut buffers.emitter_velocities,
        &f32_bytes(&inputs.emitter_velocities),
    );
    refresh_storage(
        &device,
        &queue,
        &mut buffers.flow_headers,
        &f32_bytes(&inputs.flow_spline_headers),
    );
    refresh_storage(
        &device,
        &queue,
        &mut buffers.foam_headers,
        &f32_bytes(&inputs.foam_spline_headers),
    );
    refresh_storage(
        &device,
        &queue,
        &mut buffers.spline_points,
        &f32_bytes(&inputs.spline_points),
    );
    refresh_storage(
        &device,
        &queue,
        &mut buffers.carves,
        &f32_bytes(&inputs.carves),
    );
    refresh_storage(
        &device,
        &queue,
        &mut buffers.query_points,
        &f32_bytes(&inputs.query_points),
    );

    let lod_buf = buffers.lod.clone().expect("uniforms exist");
    let depth_buf = buffers.depth.clone().expect("uniforms exist");
    let spectrum_buf = buffers.spectrum.clone().expect("uniforms exist");
    let spectrum_update_buf = buffers.spectrum_update.clone().expect("uniforms exist");
    let bake_buf = buffers.bake.clone().expect("uniforms exist");
    let fft_buf = buffers.fft.clone().expect("uniforms exist");
    let combine_buf = buffers.combine.clone().expect("uniforms exist");
    let dyn_buf = buffers.dynwaves.clone().expect("uniforms exist");
    let foam_buf = buffers.foam.clone().expect("uniforms exist");
    let flow_buf = buffers.flow.clone().expect("uniforms exist");
    let fill_buf = buffers.fill.clone().expect("uniforms exist");
    let clip_fill_buf = buffers.clip_fill.clone().expect("uniforms exist");
    let copy_slice_buf = buffers.copy_slice.clone().expect("uniforms exist");
    let water_query_buf = buffers.water_query.clone().expect("uniforms exist");
    let query_points_buf = buffers.query_points.clone().expect("storages exist");
    let emitters_buf = buffers.emitters.clone().expect("storages exist");
    let velocities_buf = buffers.emitter_velocities.clone().expect("storages exist");
    let flow_headers_buf = buffers.flow_headers.clone().expect("storages exist");
    let foam_headers_buf = buffers.foam_headers.clone().expect("storages exist");
    let points_buf = buffers.spline_points.clone().expect("storages exist");
    let carves_buf = buffers.carves.clone().expect("storages exist");

    let need_spectrum = frame.settings_changed || !buffers.spectrum_ready;

    macro_rules! pass {
        ($label:expr, $entry:expr, $shader:expr, $layout:expr, $entries:expr, $groups:expr) => {{
            let layout_entries: Vec<BindGroupLayoutEntry> = $layout;
            let bind_layout = buffers.layout(&device, $label, &layout_entries);
            let spec = ComputePassSpec {
                label: $label,
                entry: $entry,
                shader: $shader.clone(),
                layout: BindGroupLayoutDescriptor {
                    label: $label.into(),
                    entries: layout_entries,
                },
            };
            if let Some(pipeline) = pipeline_for(&cache, &spec, &mut pipelines) {
                dispatch(
                    &device,
                    &queue,
                    pipeline,
                    &bind_layout,
                    $entries,
                    $groups,
                    $label,
                );
                true
            } else {
                false
            }
        }};
    }

    pass!(
        "crest.seafloor_depth",
        "seafloor_depth",
        &shaders.seafloor_depth,
        vec![
            sample_2d_layout_entry(0),
            storage_texture_layout_entry(1, SeaFloorDepth.format(), true),
            uniform_layout_entry(2),
            uniform_layout_entry(3),
        ],
        &[
            (0, BindingResource::TextureView(&grid)),
            (1, BindingResource::TextureView(&depth)),
            uniform_entry(2, &depth_buf),
            uniform_entry(3, &lod_buf),
        ],
        [wg, wg, lods]
    );

    let spectrum_init_ran = if need_spectrum {
        pass!(
            "crest.spectrum_init",
            "spectrum_init",
            &shaders.spectrum_init,
            vec![
                storage_texture_layout_entry(0, SpectrumInit.format(), true),
                uniform_layout_entry(1),
            ],
            &[
                (0, BindingResource::TextureView(&spectrum_init)),
                uniform_entry(1, &spectrum_buf),
            ],
            [wg, wg, lods]
        )
    } else {
        false
    };

    pass!(
        "crest.spectrum_update",
        "spectrum_update",
        &shaders.spectrum_update,
        vec![
            sample_array_layout_entry(0),
            storage_texture_layout_entry(1, SpectrumHeight.format(), true),
            storage_texture_layout_entry(2, SpectrumDispX.format(), true),
            storage_texture_layout_entry(3, SpectrumDispZ.format(), true),
            uniform_layout_entry(4),
        ],
        &[
            (0, BindingResource::TextureView(&spectrum_init)),
            (1, BindingResource::TextureView(&spectrum_height)),
            (2, BindingResource::TextureView(&spectrum_disp_x)),
            (3, BindingResource::TextureView(&spectrum_disp_z)),
            uniform_entry(4, &spectrum_update_buf),
        ],
        [wg, wg, lods]
    );

    for source in [&spectrum_height, &spectrum_disp_x, &spectrum_disp_z] {
        for axis in [0u32, 1u32] {
            write_uniform(
                &queue,
                &fft_buf,
                &FftUniforms {
                    size,
                    axis,
                    inverse: 1,
                    pad: 0,
                },
            );
            let (src, dst) = if axis == 0 {
                (source, &fft_derivs)
            } else {
                (&fft_derivs, source)
            };
            pass!(
                "crest.fft",
                "fft",
                &shaders.fft,
                vec![
                    sample_array_layout_entry(0),
                    storage_texture_layout_entry(1, FftDerivs.format(), true),
                    uniform_layout_entry(2),
                ],
                &[
                    (0, BindingResource::TextureView(src)),
                    (1, BindingResource::TextureView(dst)),
                    uniform_entry(2, &fft_buf),
                ],
                [lods, size, 1]
            );
        }
    }

    pass!(
        "crest.fft_bake",
        "fft_bake",
        &shaders.fft_bake,
        vec![
            sample_array_layout_entry(0),
            sample_array_layout_entry(1),
            sample_array_layout_entry(2),
            storage_texture_layout_entry(3, FftDisp.format(), true),
            storage_texture_layout_entry(4, FftDerivs.format(), true),
            uniform_layout_entry(5),
        ],
        &[
            (0, BindingResource::TextureView(&spectrum_height)),
            (1, BindingResource::TextureView(&spectrum_disp_x)),
            (2, BindingResource::TextureView(&spectrum_disp_z)),
            (3, BindingResource::TextureView(&fft_disp)),
            (4, BindingResource::TextureView(&fft_derivs)),
            uniform_entry(5, &bake_buf),
        ],
        [wg, wg, lods]
    );

    pass!(
        "crest.shape_combine",
        "shape_combine",
        &shaders.shape_combine,
        vec![
            sample_array_layout_entry(0),
            sample_array_layout_entry(1),
            sample_array_layout_entry(2),
            sample_array_layout_entry(3),
            storage_texture_layout_entry(4, AnimatedWaves.format(), true),
            storage_texture_layout_entry(5, AnimatedDerivs.format(), true),
            uniform_layout_entry(6),
            uniform_layout_entry(7),
        ],
        &[
            (0, BindingResource::TextureView(&fft_disp)),
            (1, BindingResource::TextureView(&fft_derivs)),
            (2, BindingResource::TextureView(&depth)),
            (3, BindingResource::TextureView(&dyn_now)),
            (4, BindingResource::TextureView(&waves)),
            (5, BindingResource::TextureView(&derivs)),
            uniform_entry(6, &lod_buf),
            uniform_entry(7, &combine_buf),
        ],
        [wg, wg, lods]
    );

    if settings.dynamic_waves_enabled {
        let dyn_layout = vec![
            sample_array_layout_entry(0),
            storage_texture_layout_entry(1, DynamicWaves.format(), true),
            uniform_layout_entry(2),
            storage_layout_entry(3),
            storage_layout_entry(4),
            uniform_layout_entry(5),
        ];
        pass!(
            "crest.dynwaves_update",
            "dynwaves_update",
            &shaders.dynwaves,
            dyn_layout.clone(),
            &[
                (0, BindingResource::TextureView(&dyn_now)),
                (1, BindingResource::TextureView(&dyn_prev)),
                uniform_entry(2, &dyn_buf),
                storage_binding(3, &emitters_buf),
                storage_binding(4, &velocities_buf),
                uniform_entry(5, &lod_buf),
            ],
            [wg, wg, lods]
        );
        pass!(
            "crest.dynwaves_splat",
            "dynwaves_splat",
            &shaders.dynwaves,
            dyn_layout,
            &[
                (0, BindingResource::TextureView(&dyn_prev)),
                (1, BindingResource::TextureView(&dyn_now)),
                uniform_entry(2, &dyn_buf),
                storage_binding(3, &emitters_buf),
                storage_binding(4, &velocities_buf),
                uniform_entry(5, &lod_buf),
            ],
            [wg, wg, lods]
        );
    }

    if settings.flow_enabled {
        pass!(
            "crest.flow",
            "flow",
            &shaders.flow,
            vec![
                storage_texture_layout_entry(0, Flow.format(), true),
                uniform_layout_entry(1),
                storage_layout_entry(2),
                storage_layout_entry(3),
                uniform_layout_entry(4),
            ],
            &[
                (0, BindingResource::TextureView(&flow_tex)),
                uniform_entry(1, &flow_buf),
                storage_binding(2, &flow_headers_buf),
                storage_binding(3, &points_buf),
                uniform_entry(4, &lod_buf),
            ],
            [wg, wg, lods]
        );
    }

    if settings.foam_enabled {
        pass!(
            "crest.foam",
        "foam",
        &shaders.foam,
        vec![
            sample_array_layout_entry(0),
            sample_array_layout_entry(1),
            sample_array_layout_entry(2),
            sample_array_layout_entry(3),
            storage_texture_layout_entry(4, Foam.format(), true),
            uniform_layout_entry(5),
            storage_layout_entry(6),
            storage_layout_entry(7),
            uniform_layout_entry(8),
        ],
        &[
            (0, BindingResource::TextureView(&foam_prev)),
            (1, BindingResource::TextureView(&waves)),
            (2, BindingResource::TextureView(&depth)),
            (3, BindingResource::TextureView(&dyn_now)),
            (4, BindingResource::TextureView(&foam)),
            uniform_entry(5, &foam_buf),
            storage_binding(6, &foam_headers_buf),
            storage_binding(7, &points_buf),
            uniform_entry(8, &lod_buf),
        ],
        [wg, wg, lods]
    );
        pass!(
            "crest.copy_foam",
            "copy_foam",
            &shaders.copy_foam,
            vec![
                sample_array_layout_entry(0),
                storage_texture_layout_entry(1, FoamPrev.format(), true),
                uniform_layout_entry(2),
            ],
            &[
                (0, BindingResource::TextureView(&foam)),
                (1, BindingResource::TextureView(&foam_prev)),
                uniform_entry(2, &fill_buf),
            ],
            [wg, wg, lods]
        );
    }

    let fill_layout = vec![
        storage_texture_layout_entry(0, Shadow.format(), true),
        uniform_layout_entry(1),
        storage_layout_entry(2),
        uniform_layout_entry(3),
    ];
    pass!(
        "crest.fill_shadow",
        "fill",
        &shaders.fill,
        fill_layout.clone(),
        &[
            (0, BindingResource::TextureView(&shadow)),
            uniform_entry(1, &fill_buf),
            storage_binding(2, &carves_buf),
            uniform_entry(3, &lod_buf),
        ],
        [wg, wg, lods]
    );
    pass!(
        "crest.fill_clip",
        "fill",
        &shaders.fill,
        fill_layout,
        &[
            (0, BindingResource::TextureView(&clip)),
            uniform_entry(1, &clip_fill_buf),
            storage_binding(2, &carves_buf),
            uniform_entry(3, &lod_buf),
        ],
        [wg, wg, lods]
    );

    if let Some(point) = frame.query_point {
        let transform = frame.lods.transforms[0];
        let slice = QUERY_SLICE_SIZE as u32;
        write_uniform(
            &queue,
            &copy_slice_buf,
            &CopySliceUniforms {
                lod: 0,
                resolution: res,
                slice_size: slice,
                pad0: 0,
                slice_origin: point.xz()
                    - Vec2::splat(slice as f32 * transform.texel_size * 0.5),
                lod_origin: transform.origin.xz(),
                texel: transform.texel_size,
                extent: transform.extent,
                pad1: 0.0,
                pad2: 0.0,
            },
        );
        let copy_layout = vec![
            sample_array_layout_entry(0),
            storage_texture_layout_entry(1, TextureFormat::Rgba32Float, false),
            uniform_layout_entry(2),
        ];
        for (source, target) in [
            (&waves, &slice_animated),
            (&flow_tex, &slice_flow),
            (&depth, &slice_depth),
        ] {
            pass!(
                "crest.copy_slice",
                "copy_slice",
                &shaders.copy_slice,
                copy_layout.clone(),
                &[
                    (0, BindingResource::TextureView(source)),
                    (1, BindingResource::TextureView(target)),
                    uniform_entry(2, &copy_slice_buf),
                ],
                [slice / 8, slice / 8, 1]
            );
        }
    }

    if need_spectrum && spectrum_init_ran {
        buffers.spectrum_ready = true;
    }

    if !inputs.query_points.is_empty() {
        if let (Some(qa), Some(qf), Some(qd)) =
            (query_animated.as_ref(), query_flow.as_ref(), query_depth.as_ref())
        {
            pass!(
                "crest.water_query",
                "water_query",
                &shaders.water_query,
                vec![
                    sample_array_layout_entry(0),
                    sample_array_layout_entry(1),
                    sample_array_layout_entry(2),
                    sample_array_layout_entry(3),
                    storage_texture_layout_entry(4, TextureFormat::Rgba32Float, false),
                    storage_texture_layout_entry(5, TextureFormat::Rgba32Float, false),
                    storage_texture_layout_entry(6, TextureFormat::Rgba32Float, false),
                    uniform_layout_entry(7),
                    storage_layout_entry(8),
                ],
                &[
                    (0, BindingResource::TextureView(&waves)),
                    (1, BindingResource::TextureView(&flow_tex)),
                    (2, BindingResource::TextureView(&depth)),
                    (3, BindingResource::TextureView(&derivs)),
                    (4, BindingResource::TextureView(qa)),
                    (5, BindingResource::TextureView(qf)),
                    (6, BindingResource::TextureView(qd)),
                    uniform_entry(7, &water_query_buf),
                    storage_binding(8, &query_points_buf),
                ],
                [MAX_QUERY_POINTS as u32 / 8, 1, 1]
            );
        }
    }
}

pub struct PassesPlugin;

impl Plugin for PassesPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedFrame>()
            .init_resource::<CascadePipelines>()
            .init_resource::<CascadeBuffers>()
            .add_systems(ExtractSchedule, extract_frame)
            .add_systems(RenderGraph, run_cascades.in_set(RenderGraphSystems::Begin));
    }
}
