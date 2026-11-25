use core::any::type_name;
use core::marker::PhantomData;

use bevy::{
    asset::{AssetId, uuid::Uuid},
    core_pipeline::{Core3d, Core3dSystems, FullscreenShader, prepass::ViewPrepassTextures},
    ecs::error::BevyError,
    prelude::*,
    render::{
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        prelude::Msaa,
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedRenderPipelineId, Canonical, ColorTargetState, ColorWrites, FragmentState,
            Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
            RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
            ShaderType, Specializer, SpecializerKey, TextureFormat, TextureSampleType, Variants,
            binding_types::{
                sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer,
            },
            encase::internal::WriteInto,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::{ExtractedView, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
        Render, RenderApp, RenderStartup, RenderSystems,
    },
    shader::{Shader, ShaderDefVal, ShaderRef},
};

pub trait PostProcessEffect:
    Component
    + ExtractComponent<RenderApp>
    + Clone
    + Copy
    + ShaderType
    + WriteInto
    + Default
{
    fn fragment_shader() -> ShaderRef;

    fn stage() -> Core3dSystems {
        Core3dSystems::EarlyPostProcess
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn override_shader(&self) -> Option<Handle<Shader>> {
        None
    }
}

pub struct PostProcessPlugin<T: PostProcessEffect> {
    _marker: PhantomData<T>,
}

impl<T: PostProcessEffect> Default for PostProcessPlugin<T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T: PostProcessEffect> Plugin for PostProcessPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<T>::default(),
            UniformComponentPlugin::<T>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(RenderStartup, init_post_process_pipeline::<T>)
            .add_systems(
                Render,
                prepare_post_process_pipelines::<T>.in_set(RenderSystems::Prepare),
            )
            .add_systems(Core3d, post_process_pass::<T>.in_set(T::stage()));
    }
}

#[derive(Resource)]
struct PostProcessPipeline<T: PostProcessEffect> {
    layouts: [BindGroupLayoutDescriptor; 2],
    sampler: Sampler,
    variants: Variants<RenderPipeline, PostProcessSpecializer>,
    _marker: PhantomData<T>,
}

fn settings_layout<T: ShaderType>(
    label: &'static str,
    multisampled: bool,
) -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        label,
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                if multisampled {
                    texture_depth_2d_multisampled()
                } else {
                    texture_depth_2d()
                },
                uniform_buffer::<ViewUniform>(true),
                uniform_buffer::<T>(true),
            ),
        ),
    )
}

struct PostProcessSpecializer {
    layouts: [BindGroupLayoutDescriptor; 2],
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, SpecializerKey)]
struct PostProcessPipelineKey {
    target_format: TextureFormat,
    multisampled: bool,
    override_shader: Option<Uuid>,
}

impl Specializer<RenderPipeline> for PostProcessSpecializer {
    type Key = PostProcessPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        descriptor: &mut RenderPipelineDescriptor,
    ) -> Result<Canonical<Self::Key>, BevyError> {
        descriptor.layout = vec![self.layouts[key.multisampled as usize].clone()];
        let fragment = descriptor.fragment_mut()?;
        fragment
            .shader_defs
            .push(ShaderDefVal::Bool("MULTISAMPLED".into(), key.multisampled));
        if let Some(uuid) = key.override_shader {
            fragment.shader = Handle::from(uuid);
        }
        fragment.set_target(
            0,
            ColorTargetState {
                format: key.target_format,
                blend: None,
                write_mask: ColorWrites::ALL,
            },
        );
        Ok(key)
    }
}

fn init_post_process_pipeline<T: PostProcessEffect>(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
) {
    let layouts = [
        settings_layout::<T>("post process bind group layout", false),
        settings_layout::<T>("post process multisampled bind group layout", true),
    ];
    let sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let shader = match T::fragment_shader() {
        ShaderRef::Default => {
            unimplemented!("PostProcessEffect::fragment_shader() must not return ShaderRef::Default")
        }
        ShaderRef::Handle(handle) => handle,
        ShaderRef::Path(path) => asset_server.load(path),
    };

    let descriptor = RenderPipelineDescriptor {
        label: Some(format!("post_process_pipeline<{}>", type_name::<T>()).into()),
        layout: vec![layouts[0].clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader,
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    };

    commands.insert_resource(PostProcessPipeline::<T> {
        layouts: layouts.clone(),
        sampler,
        variants: Variants::new(PostProcessSpecializer { layouts }, descriptor),
        _marker: PhantomData,
    });
}

#[derive(Component)]
struct PostProcessPipelineId<T: PostProcessEffect> {
    id: CachedRenderPipelineId,
    multisampled: bool,
    _marker: PhantomData<T>,
}

fn prepare_post_process_pipelines<T: PostProcessEffect>(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<PostProcessPipeline<T>>,
    views: Query<(Entity, &ExtractedView, &Msaa, &T)>,
) -> Result<(), BevyError> {
    for (entity, view, msaa, effect) in &views {
        let Some(override_shader) = specialized_override(effect) else {
            commands.entity(entity).remove::<PostProcessPipelineId<T>>();
            continue;
        };
        let multisampled = msaa.samples() > 1;
        let id = pipeline.variants.specialize(
            &pipeline_cache,
            PostProcessPipelineKey {
                target_format: view.target_format,
                multisampled,
                override_shader,
            },
        )?;
        commands.entity(entity).insert(PostProcessPipelineId::<T> {
            id,
            multisampled,
            _marker: PhantomData,
        });
    }
    Ok(())
}

fn specialized_override<T: PostProcessEffect>(effect: &T) -> Option<Option<Uuid>> {
    if !effect.is_enabled() {
        return None;
    }
    Some(
        effect
            .override_shader()
            .and_then(|shader| match shader.id() {
                AssetId::Uuid { uuid } => Some(uuid),
                AssetId::Index { .. } => None,
            }),
    )
}

fn post_process_pass<T: PostProcessEffect>(
    view: ViewQuery<(
        &ViewTarget,
        &ViewUniformOffset,
        &DynamicUniformIndex<T>,
        &PostProcessPipelineId<T>,
        Option<&ViewPrepassTextures>,
    )>,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Res<PostProcessPipeline<T>>,
    view_uniforms: Res<ViewUniforms>,
    settings_uniforms: Res<ComponentUniforms<T>>,
    mut ctx: RenderContext,
    mut label: Local<String>,
) {
    let (view_target, view_uniform_offset, settings_index, pipeline_id, prepass_textures) =
        view.into_inner();

    let Some(depth_view) = prepass_textures.and_then(|textures| textures.depth_only_view()) else {
        return;
    };
    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline_id.id) else {
        return;
    };
    if view_uniforms.uniforms.binding().is_none()
        || settings_uniforms.uniforms().binding().is_none()
    {
        return;
    }

    let post_process = view_target.post_process_write();
    let bind_group = ctx.render_device().create_bind_group(
        Some("post process bind group"),
        &pipeline_cache.get_bind_group_layout(&pipeline.layouts[pipeline_id.multisampled as usize]),
        &BindGroupEntries::sequential((
            post_process.source,
            &pipeline.sampler,
            depth_view,
            &view_uniforms.uniforms,
            settings_uniforms.uniforms(),
        )),
    );

    if label.is_empty() {
        *label = format!("post_process_pass<{}>", type_name::<T>());
    }
    let pass_descriptor = RenderPassDescriptor {
        label: Some(label.as_str()),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post_process.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };

    let mut render_pass = ctx.command_encoder().begin_render_pass(&pass_descriptor);
    render_pass.set_pipeline(render_pipeline);
    render_pass.set_bind_group(
        0,
        &bind_group,
        &[view_uniform_offset.offset, settings_index.index()],
    );
    render_pass.draw(0..3, 0..1);
}
