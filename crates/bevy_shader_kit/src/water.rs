use bevy::{
    asset::{AssetId, RenderAssetUsages, uuid::Uuid},
    mesh::MeshVertexBufferLayoutRef,
    pbr::{
        ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
        StandardMaterial,
    },
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat,
    },
    shader::{Shader, ShaderDefVal, ShaderRef},
};

use crate::field::SdfVolume;

pub const MAX_WATER_FIELDS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct WaterParams {
    pub time: f32,
    pub water_height: f32,
    pub ripple_range: f32,
    pub ripple_frequency: f32,
    pub ripple_speed: f32,
    pub ripple_strength: f32,
    pub foam_width: f32,
    pub foam_strength: f32,
    pub field_count: u32,
    pub max_depth: f32,
    pub phase_g: f32,
    pub pad: f32,
    pub scattering: Vec3,
    pub absorption: Vec3,
    pub sun_direction: Vec3,
    pub sun_illuminance: Vec3,
    pub ambient_illuminance: Vec3,
    pub fields: [SdfVolume; MAX_WATER_FIELDS],
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            water_height: 0.0,
            ripple_range: 6.0,
            ripple_frequency: 3.0,
            ripple_speed: 2.5,
            ripple_strength: 0.55,
            foam_width: 0.8,
            foam_strength: 0.25,
            field_count: 0,
            max_depth: 8.0,
            phase_g: 0.6,
            pad: 0.0,
            scattering: Vec3::new(0.03, 0.045, 0.05),
            absorption: Vec3::new(0.35, 0.12, 0.06),
            sun_direction: Vec3::new(0.707, 0.0, 0.707),
            sun_illuminance: Vec3::splat(3.0),
            ambient_illuminance: Vec3::new(0.15, 0.2, 0.25),
            fields: [SdfVolume::default(); MAX_WATER_FIELDS],
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct WaterMaterialKey {
    pub ripples: bool,
    pub foam: bool,
    pub optics: bool,
    pub depth_steps: u32,
    pub override_shader: Option<Uuid>,
}

impl From<&WaterMaterialExt> for WaterMaterialKey {
    fn from(ext: &WaterMaterialExt) -> Self {
        Self {
            ripples: ext.ripples,
            foam: ext.foam,
            optics: ext.optics,
            depth_steps: ext.depth_steps,
            override_shader: ext.override_shader.as_ref().and_then(|shader| match shader.id() {
                AssetId::Uuid { uuid } => Some(uuid),
                AssetId::Index { .. } => None,
            }),
        }
    }
}

pub fn water_shader_defs(key: &WaterMaterialKey) -> Vec<ShaderDefVal> {
    vec![
        ShaderDefVal::Bool("WATER_RIPPLES".into(), key.ripples),
        ShaderDefVal::Bool("WATER_FOAM".into(), key.foam),
        ShaderDefVal::Bool("WATER_OPTICS".into(), key.optics),
        ShaderDefVal::UInt("WATER_DEPTH_STEPS".into(), key.depth_steps),
    ]
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(WaterMaterialKey)]
pub struct WaterMaterialExt {
    #[uniform(100)]
    pub params: WaterParams,
    #[texture(101, dimension = "3d", sample_type = "float", filterable = false)]
    pub field0: Handle<Image>,
    #[texture(102, dimension = "3d", sample_type = "float", filterable = false)]
    pub field1: Handle<Image>,
    #[texture(103, dimension = "3d", sample_type = "float", filterable = false)]
    pub field2: Handle<Image>,
    #[texture(104, dimension = "3d", sample_type = "float", filterable = false)]
    pub field3: Handle<Image>,
    pub ripples: bool,
    pub foam: bool,
    pub optics: bool,
    pub depth_steps: u32,
    pub override_shader: Option<Handle<Shader>>,
}

impl Default for WaterMaterialExt {
    fn default() -> Self {
        Self {
            params: default(),
            field0: default(),
            field1: default(),
            field2: default(),
            field3: default(),
            ripples: true,
            foam: true,
            optics: true,
            depth_steps: 12,
            override_shader: None,
        }
    }
}

impl MaterialExtension for WaterMaterialExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/water/water.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/water/water.wesl".into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.extend(water_shader_defs(&key.bind_group_data));
            if let Some(uuid) = key.bind_group_data.override_shader {
                fragment.shader = Handle::from(uuid);
            }
        }
        Ok(())
    }
}

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterMaterialExt>;

pub fn dummy_field_image(images: &mut Assets<Image>) -> Handle<Image> {
    images.add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D3,
        1.0e6f32.to_le_bytes().to_vec(),
        TextureFormat::R32Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_ext() -> WaterMaterialExt {
        WaterMaterialExt {
            depth_steps: 8,
            optics: false,
            ..default()
        }
    }

    #[test]
    fn key_mirrors_extension_flags() {
        let key = WaterMaterialKey::from(&key_ext());
        assert!(key.ripples && key.foam && !key.optics);
        assert_eq!(key.depth_steps, 8);
        assert_eq!(key.override_shader, None);
    }

    #[test]
    fn key_tracks_override_shader_identity() {
        let uuid = Uuid::new_v4();
        let ext = WaterMaterialExt {
            override_shader: Some(Handle::from(uuid)),
            ..default()
        };
        let key = WaterMaterialKey::from(&ext);
        assert_eq!(key.override_shader, Some(uuid));
    }

    #[test]
    fn defs_carry_flags_and_step_count() {
        let defs = water_shader_defs(&WaterMaterialKey::from(&key_ext()));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_RIPPLES".into(), true)));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_OPTICS".into(), false)));
        assert!(defs.contains(&ShaderDefVal::UInt("WATER_DEPTH_STEPS".into(), 8)));
    }
}
