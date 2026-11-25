use bevy::{
    asset::{AssetId, uuid::Uuid},
    mesh::MeshVertexBufferLayoutRef,
    pbr::{
        ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
        StandardMaterial,
    },
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::{Shader, ShaderDefVal, ShaderRef},
};

pub const MAX_RIPPLES: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct RippleWave {
    pub origin: Vec2,
    pub birth: f32,
    pub strength: f32,
    pub frequency: f32,
    pub speed: f32,
    pub decay: f32,
    pub spread: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct RippleSet {
    pub count: u32,
    pub pad0: u32,
    pub pad1: u32,
    pub pad2: u32,
    pub waves: [RippleWave; MAX_RIPPLES],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct WaterParams {
    pub time: f32,
    pub water_height: f32,
    pub foam_width: f32,
    pub foam_strength: f32,
    pub max_depth: f32,
    pub phase_g: f32,
    pub ripple_normal: f32,
    pub ripple_foam: f32,
    pub scattering: Vec3,
    pub absorption: Vec3,
    pub sun_direction: Vec3,
    pub sun_illuminance: Vec3,
    pub ambient_illuminance: Vec3,
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            water_height: 0.0,
            foam_width: 0.8,
            foam_strength: 0.25,
            max_depth: 8.0,
            phase_g: 0.6,
            ripple_normal: 0.55,
            ripple_foam: 0.9,
            scattering: Vec3::new(0.03, 0.045, 0.05),
            absorption: Vec3::new(0.35, 0.12, 0.06),
            sun_direction: Vec3::new(0.707, 0.0, 0.707),
            sun_illuminance: Vec3::splat(3.0),
            ambient_illuminance: Vec3::new(0.15, 0.2, 0.25),
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct WaterMaterialKey {
    pub foam: bool,
    pub optics: bool,
    pub ripples: bool,
    pub override_shader: Option<Uuid>,
}

impl From<&WaterMaterialExt> for WaterMaterialKey {
    fn from(ext: &WaterMaterialExt) -> Self {
        Self {
            foam: ext.foam,
            optics: ext.optics,
            ripples: ext.ripples,
            override_shader: ext.override_shader.as_ref().and_then(|shader| match shader.id() {
                AssetId::Uuid { uuid } => Some(uuid),
                AssetId::Index { .. } => None,
            }),
        }
    }
}

pub fn water_shader_defs(key: &WaterMaterialKey) -> Vec<ShaderDefVal> {
    vec![
        ShaderDefVal::Bool("WATER_FOAM".into(), key.foam),
        ShaderDefVal::Bool("WATER_OPTICS".into(), key.optics),
        ShaderDefVal::Bool("WATER_RIPPLES".into(), key.ripples),
    ]
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(WaterMaterialKey)]
pub struct WaterMaterialExt {
    #[uniform(100)]
    pub params: WaterParams,
    #[uniform(101)]
    pub waves: RippleSet,
    pub foam: bool,
    pub optics: bool,
    pub ripples: bool,
    pub override_shader: Option<Handle<Shader>>,
}

impl Default for WaterMaterialExt {
    fn default() -> Self {
        Self {
            params: default(),
            waves: default(),
            foam: true,
            optics: true,
            ripples: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key_ext() -> WaterMaterialExt {
        WaterMaterialExt {
            optics: false,
            ..default()
        }
    }

    #[test]
    fn key_mirrors_extension_flags() {
        let key = WaterMaterialKey::from(&key_ext());
        assert!(key.foam);
        assert!(!key.optics);
        assert!(key.ripples);
        assert_eq!(key.override_shader, None);
    }

    #[test]
    fn ripple_uniform_matches_the_shader_layout() {
        assert_eq!(RippleWave::min_size().get(), 32);
        assert_eq!(RippleSet::min_size().get(), 16 + 32 * MAX_RIPPLES as u64);
    }

    #[test]
    fn shader_declares_the_same_ripple_capacity() {
        let source = include_str!("shaders/water/ripples.wesl");
        assert!(source.contains(&format!("array<RippleWave, {MAX_RIPPLES}>")));
        assert!(source.contains(&format!("min(ripples.count, {MAX_RIPPLES}u)")));
        assert!(source.contains("@binding(101)"));
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
    fn defs_carry_feature_flags() {
        let defs = water_shader_defs(&WaterMaterialKey::from(&key_ext()));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_FOAM".into(), true)));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_OPTICS".into(), false)));
        assert!(defs.contains(&ShaderDefVal::Bool("WATER_RIPPLES".into(), true)));
    }
}
