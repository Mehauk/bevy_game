use std::collections::HashSet;

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_asset::RenderAssets,
        render_resource::{
            BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
            BindingResource, BindingType, Buffer, BufferBindingType, BufferDescriptor,
            BufferUsages, CachedComputePipelineId, CommandEncoderDescriptor, ComputePassDescriptor,
            ComputePipeline, ComputePipelineDescriptor, Extent3d, PipelineCache, ShaderStages,
            ShaderType, StorageTextureAccess, TextureDimension, TextureFormat, TextureUsages,
            TextureViewDimension, encase::UniformBuffer,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
    shader::Shader,
};

use crate::field::SdfVolume;

pub const SDF_BAKE_SHADER_PATH: &str = "embedded://bevy_shader_kit/shaders/sdf/bake_compute.wesl";

const WORKGROUP: u32 = 4;
const REQUEST_LIFETIME: u64 = 3;
const LABEL: &str = "sdf_bake";

#[derive(Clone, Debug)]
pub struct GpuBakeSdfRequest {
    pub image: Handle<Image>,
    pub volume: SdfVolume,
    pub vertices: Vec<Vec3>,
    pub indices: Vec<u32>,
}

#[derive(Clone)]
struct QueuedRequest {
    id: u64,
    frame: u64,
    request: GpuBakeSdfRequest,
}

#[derive(Resource, Default)]
pub struct SdfBakeQueue {
    next_id: u64,
    frame: u64,
    pending: Vec<QueuedRequest>,
}

impl SdfBakeQueue {
    pub fn push(&mut self, request: GpuBakeSdfRequest) {
        self.next_id += 1;
        self.pending.push(QueuedRequest {
            id: self.next_id,
            frame: self.frame,
            request,
        });
    }
}

#[derive(Resource)]
struct SdfBakeShader(Handle<Shader>);

#[derive(Resource, Default)]
struct BakeJobs {
    shader: Option<Handle<Shader>>,
    queued: Vec<QueuedRequest>,
    seen: HashSet<u64>,
}

#[derive(Resource, Default)]
struct BakeState {
    pipeline: Option<CachedComputePipelineId>,
}

pub struct GpuSdfBakePlugin;

impl Plugin for GpuSdfBakePlugin {
    fn build(&self, app: &mut App) {
        let shader = app
            .world()
            .resource::<AssetServer>()
            .load(SDF_BAKE_SHADER_PATH);
        app.insert_resource(SdfBakeShader(shader))
            .init_resource::<SdfBakeQueue>()
            .add_systems(PreUpdate, retire_bake_requests);
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<BakeJobs>()
            .init_resource::<BakeState>()
            .add_systems(ExtractSchedule, extract_bake_jobs)
            .add_systems(Render, gpu_bake_sdf.in_set(RenderSystems::PrepareResources));
    }
}

fn retire_bake_requests(mut queue: ResMut<SdfBakeQueue>) {
    queue.frame += 1;
    let frame = queue.frame;
    queue
        .pending
        .retain(|queued| frame.saturating_sub(queued.frame) < REQUEST_LIFETIME);
}

fn extract_bake_jobs(
    queue: Extract<Res<SdfBakeQueue>>,
    shader: Extract<Res<SdfBakeShader>>,
    mut jobs: ResMut<BakeJobs>,
) {
    jobs.shader = Some(shader.0.clone());
    for queued in &queue.pending {
        if jobs.seen.insert(queued.id) {
            jobs.queued.push(queued.clone());
        }
    }
}

fn layout_descriptor() -> BindGroupLayoutDescriptor {
    let storage = |binding: u32| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    BindGroupLayoutDescriptor {
        label: LABEL.into(),
        entries: vec![
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::WriteOnly,
                    format: TextureFormat::R32Float,
                    view_dimension: TextureViewDimension::D3,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: SdfVolume::min_size().into(),
                },
                count: None,
            },
            storage(2),
            storage(3),
        ],
    }
}

fn pipeline_descriptor(shader: Handle<Shader>) -> ComputePipelineDescriptor {
    ComputePipelineDescriptor {
        label: Some(LABEL.into()),
        layout: vec![layout_descriptor()],
        immediate_size: 0,
        shader,
        shader_defs: vec![],
        entry_point: Some("bake_sdf".into()),
        zero_initialize_workgroup_memory: false,
        constants: vec![],
    }
}

fn f32_bytes(values: &[[f32; 4]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 16);
    for value in values {
        for component in value {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
    bytes
}

fn u32_bytes(values: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn volume_bytes(volume: &SdfVolume) -> Vec<u8> {
    let mut buffer = UniformBuffer::new(Vec::new());
    buffer
        .write(volume)
        .expect("the volume always encodes into its uniform buffer");
    buffer.into_inner()
}

fn storage_buffer(
    device: &RenderDevice,
    queue: &RenderQueue,
    label: &'static str,
    bytes: &[u8],
) -> Buffer {
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(label).into(),
        size: bytes.len() as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, bytes);
    buffer
}

fn bake_one(
    device: &RenderDevice,
    queue: &RenderQueue,
    pipeline: &ComputePipeline,
    layout: &BindGroupLayout,
    target: &GpuImage,
    request: &GpuBakeSdfRequest,
) {
    if request.vertices.is_empty() || request.indices.is_empty() {
        return;
    }

    let vertices: Vec<[f32; 4]> = request
        .vertices
        .iter()
        .map(|v| [v.x, v.y, v.z, 0.0])
        .collect();
    let vertex_bytes = f32_bytes(&vertices);
    let index_bytes = u32_bytes(&request.indices);
    let volume_bytes = volume_bytes(&request.volume);

    let uniform = device.create_buffer(&BufferDescriptor {
        label: Some("sdf_bake_volume").into(),
        size: volume_bytes.len() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform, 0, &volume_bytes);

    let vertex_buffer = storage_buffer(device, queue, "sdf_bake_vertices", &vertex_bytes);
    let index_buffer = storage_buffer(device, queue, "sdf_bake_indices", &index_bytes);

    let entries = [
        BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&target.texture_view),
        },
        BindGroupEntry {
            binding: 1,
            resource: uniform.as_entire_binding(),
        },
        BindGroupEntry {
            binding: 2,
            resource: vertex_buffer.as_entire_binding(),
        },
        BindGroupEntry {
            binding: 3,
            resource: index_buffer.as_entire_binding(),
        },
    ];
    let bind_group = device.create_bind_group(LABEL, layout, &entries);

    let resolution = request.volume.resolution;
    let workgroups = |axis: f32| (axis as u32).div_ceil(WORKGROUP).max(1);

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some(LABEL).into(),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(LABEL).into(),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            workgroups(resolution.x),
            workgroups(resolution.y),
            workgroups(resolution.z),
        );
    }
    queue.submit([encoder.finish()]);
}

fn gpu_bake_sdf(
    mut jobs: ResMut<BakeJobs>,
    images: Res<RenderAssets<GpuImage>>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut state: ResMut<BakeState>,
) {
    if jobs.queued.is_empty() {
        return;
    }

    if state.pipeline.is_none() {
        let Some(shader) = jobs.shader.clone() else {
            return;
        };
        state.pipeline = Some(pipeline_cache.queue_compute_pipeline(pipeline_descriptor(shader)));
    }

    let Some(pipeline) = state
        .pipeline
        .and_then(|id| pipeline_cache.get_compute_pipeline(id))
    else {
        return;
    };
    let layout = pipeline_cache.get_bind_group_layout(&layout_descriptor());

    jobs.queued.retain_mut(|queued| {
        let Some(target) = images.get(&queued.request.image) else {
            return true;
        };
        bake_one(&device, &queue, pipeline, &layout, target, &queued.request);
        false
    });
}

pub fn uninit_sdf_image(resolution: UVec3) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: resolution.x,
            height: resolution.y,
            depth_or_array_layers: resolution.z,
        },
        TextureDimension::D3,
        TextureFormat::R32Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING;
    image
}
