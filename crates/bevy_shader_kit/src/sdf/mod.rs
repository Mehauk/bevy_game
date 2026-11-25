use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

pub mod baker;
pub mod view;

pub use baker::{FieldBounds, bake_distance_fields, sdf_from_triangles};
pub use view::{animate_distance_field, sync_distance_field_view};

pub const MAX_TEXELS: usize = 4_000_000;

#[derive(Resource, Default)]
pub struct DistanceFieldView {
    pub active: bool,
}

impl DistanceFieldView {
    pub fn toggle(&mut self) -> bool {
        self.active = !self.active;
        self.active
    }
}

#[derive(Component, Default)]
pub struct DistanceFieldSource {
    pub texel: Vec3,
    pub water_margin: f32,
    pub surface_top: f32,
    pub clay_alpha: f32,
    pub image: Handle<Image>,
    pub view_material: Handle<DistanceFieldViewMaterial>,
    pub params: DistanceFieldParams,
    pub baked: bool,
    pub volume: Option<Entity>,
    pub hidden_material: Option<Handle<StandardMaterial>>,
}

impl DistanceFieldSource {
    pub fn prop() -> Self {
        Self {
            texel: Vec3::splat(0.1),
            water_margin: 3.0,
            clay_alpha: 1.0,
            ..default()
        }
    }
}

#[derive(Component)]
pub struct PrimaryField;

#[derive(Component)]
pub struct DistanceFieldVolume;

#[derive(Component)]
pub struct DistanceFieldViewHidden;

#[derive(Component)]
pub struct DistanceFieldDimmedColor(pub Color);

#[derive(Component)]
pub struct DimmedColorRestore(pub Color);

#[derive(Component)]
pub struct DimmedFog(pub f32);

#[derive(Resource)]
pub struct InvisibleMaterial(pub Handle<StandardMaterial>);

#[derive(Resource)]
pub struct DistanceFieldUnitCube(pub Handle<Mesh>);

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct DistanceFieldParams {
    pub field_from_world: Mat4,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub inv_bounds_size: Vec3,
    pub resolution: Vec3,
    pub time: f32,
    pub glow_range: f32,
    pub clay_alpha: f32,
}

impl Default for DistanceFieldParams {
    fn default() -> Self {
        Self {
            field_from_world: Mat4::IDENTITY,
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ZERO,
            inv_bounds_size: Vec3::ZERO,
            resolution: Vec3::ZERO,
            time: 0.0,
            glow_range: 0.0,
            clay_alpha: 1.0,
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct DistanceFieldViewExt {
    #[texture(101, dimension = "3d", sample_type = "float", filterable = false)]
    pub sdf: Handle<Image>,
    #[uniform(100)]
    pub params: DistanceFieldParams,
}

impl MaterialExtension for DistanceFieldViewExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/sdf/view_mode.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/sdf/view_mode.wesl".into()
    }
}

pub type DistanceFieldViewMaterial = ExtendedMaterial<StandardMaterial, DistanceFieldViewExt>;
