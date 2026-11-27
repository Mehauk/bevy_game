use std::collections::HashMap;

use bevy::{
    prelude::*,
    render::{
        render_resource::{
            BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
            BindingResource, BindingType, Buffer, BufferBindingType, BufferDescriptor,
            BufferUsages, CachedComputePipelineId, CommandEncoderDescriptor, ComputePassDescriptor,
            ComputePipeline, ComputePipelineDescriptor, PipelineCache, ShaderStages, ShaderType,
            StorageTextureAccess, TextureFormat, TextureSampleType, TextureViewDimension,
        },
        renderer::{RenderDevice, RenderQueue},
    },
    shader::Shader,
};

use crate::lod::LodTransform;

pub const MAX_GERSTNER: usize = 16;
pub const MAX_EMITTERS: usize = 32;
pub const MAX_SPLINES: usize = 8;
pub const MAX_SPLINE_POINTS: usize = 64;
pub const MAX_QUERY_POINTS: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct LodUniforms {
    pub lods: [LodTransform; 8],
    pub lod_count: u32,
    pub resolution: u32,
    pub time: f32,
    pub water_level: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType)]
pub struct SpectrumUniforms {
    pub size: u32,
    pub lod_count: u32,
    pub wind_speed: f32,
    pub gravity: f32,
    pub loop_period: f32,
    pub turbulence: f32,
    pub chop: f32,
    pub fft_world: f32,
    pub wind_direction: Vec2,
    pub power_log: [Vec4; 14],
}

impl Default for SpectrumUniforms {
    fn default() -> Self {
        Self {
            size: 64,
            lod_count: 7,
            wind_speed: 4.0,
            gravity: 9.81,
            loop_period: 32.0,
            turbulence: 0.4,
            chop: 1.6,
            fft_world: 4.0,
            wind_direction: Vec2::X,
            power_log: [Vec4::ZERO; 14],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct FftUniforms {
    pub size: u32,
    pub axis: u32,
    pub inverse: u32,
    pub pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct GerstnerUniform {
    pub wavelength: f32,
    pub amplitude: f32,
    pub speed: f32,
    pub steepness: f32,
    pub direction: Vec2,
    pub phase: f32,
    pub chop: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct BakeUniforms {
    pub size: u32,
    pub lod_count: u32,
    pub fft_world: f32,
    pub pad1: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct CombineUniforms {
    pub attenuation_in_shallows: f32,
    pub shallows_max_depth: f32,
    pub gerstner_count: u32,
    pub base_texel_size: f32,
    pub dynamic_gain: f32,
    pub average_wavelength: f32,
    pub fft_world: f32,
    pub pad2: f32,
    pub gerstner: [GerstnerUniform; MAX_GERSTNER],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct SpectrumUpdateUniforms {
    pub time: f32,
    pub chop: f32,
    pub gravity: f32,
    pub loop_period: f32,
    pub size: u32,
    pub lod_count: u32,
    pub fft_world: f32,
    pub pad1: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct DynWavesUniforms {
    pub dt: f32,
    pub wave_speed: f32,
    pub damping: f32,
    pub resolution: u32,
    pub emitter_count: u32,
    pub lod_count: u32,
    pub base_texel_size: f32,
    pub pad: f32,
    pub shifts: [Vec4; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct FoamUniforms {
    pub dt: f32,
    pub decay: f32,
    pub wave_folding_threshold: f32,
    pub shoreline_depth: f32,
    pub shoreline_width: f32,
    pub wave_gain: f32,
    pub shoreline_gain: f32,
    pub dynamic_gain: f32,
    pub resolution: u32,
    pub lod_count: u32,
    pub spline_count: u32,
    pub pad: f32,
    pub shifts: [Vec4; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct FlowUniforms {
    pub spline_count: u32,
    pub resolution: u32,
    pub lod_count: u32,
    pub pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct DepthUniforms {
    pub grid_origin: Vec2,
    pub grid_texel: f32,
    pub grid_size: u32,
    pub water_level: f32,
    pub resolution: u32,
    pub lod_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct FillUniforms {
    pub value: f32,
    pub resolution: u32,
    pub lod_count: u32,
    pub carve_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct CopySliceUniforms {
    pub lod: u32,
    pub resolution: u32,
    pub slice_size: u32,
    pub pad0: u32,
    pub slice_origin: Vec2,
    pub lod_origin: Vec2,
    pub texel: f32,
    pub extent: f32,
    pub pad1: f32,
    pub pad2: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct WaterQueryUniforms {
    pub lods: [LodTransform; 8],
    pub count: u32,
    pub resolution: u32,
    pub lod_count: u32,
    pub pad: u32,
}

pub fn uniform_entry(binding: u32, buffer: &Buffer) -> (u32, BindingResource<'_>) {
    (binding, buffer.as_entire_binding())
}

pub fn uniform_layout_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub fn storage_layout_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub fn sample_2d_layout_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: false },
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub fn sample_array_layout_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: false },
            view_dimension: TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

pub fn storage_texture_layout_entry(
    binding: u32,
    format: TextureFormat,
    array: bool,
) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::StorageTexture {
            access: StorageTextureAccess::WriteOnly,
            format,
            view_dimension: if array {
                TextureViewDimension::D2Array
            } else {
                TextureViewDimension::D2
            },
        },
        count: None,
    }
}

pub struct ComputePassSpec {
    pub label: &'static str,
    pub entry: &'static str,
    pub shader: Handle<Shader>,
    pub layout: BindGroupLayoutDescriptor,
}

#[derive(Resource, Default)]
pub struct CascadePipelines {
    pub queued: HashMap<&'static str, CachedComputePipelineId>,
}

pub fn pipeline_for<'a>(
    cache: &'a PipelineCache,
    spec: &ComputePassSpec,
    pipelines: &mut CascadePipelines,
) -> Option<&'a ComputePipeline> {
    if !pipelines.queued.contains_key(spec.label) {
        let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(spec.label.into()),
            layout: vec![spec.layout.clone()],
            immediate_size: 0,
            shader: spec.shader.clone(),
            shader_defs: vec![],
            entry_point: Some(spec.entry.into()),
            zero_initialize_workgroup_memory: false,
            constants: vec![],
        });
        pipelines.queued.insert(spec.label, id);
    }
    pipelines
        .queued
        .get(spec.label)
        .and_then(|id| cache.get_compute_pipeline(*id))
}

pub fn write_storage(
    device: &RenderDevice,
    queue: &RenderQueue,
    label: &str,
    bytes: &[u8],
) -> Buffer {
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(label.into()),
        size: bytes.len().max(16) as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, bytes);
    buffer
}

pub fn write_uniform<T: ShaderType + bevy::render::render_resource::encase::internal::WriteInto>(
    device: &RenderDevice,
    queue: &RenderQueue,
    label: &str,
    value: &T,
) -> Buffer {
    use bevy::render::render_resource::encase::UniformBuffer;
    let mut buffer = UniformBuffer::new(Vec::new());
    buffer.write(value).expect("uniform encodes");
    let bytes = buffer.into_inner();
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(label.into()),
        size: bytes.len() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, &bytes);
    buffer
}

pub fn dispatch(
    device: &RenderDevice,
    queue: &RenderQueue,
    pipeline: &ComputePipeline,
    layout: &BindGroupLayout,
    entries: &[(u32, BindingResource)],
    groups: [u32; 3],
    label: &str,
) {
    let entries: Vec<BindGroupEntry> = entries
        .iter()
        .map(|(binding, resource)| BindGroupEntry {
            binding: *binding,
            resource: resource.clone(),
        })
        .collect();
    let bind_group = device.create_bind_group(label, layout, &entries);
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some(label.into()),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(label.into()),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
    }
    queue.submit([encoder.finish()]);
}
