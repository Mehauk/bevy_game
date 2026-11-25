use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

use crate::debug::SurfaceDebug;

#[repr(C)]
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct Wetness {
    pub height: f32,
    pub feather: f32,
    pub darkening: f32,
    pub wet_roughness: f32,
}

impl Default for Wetness {
    fn default() -> Self {
        Self {
            height: f32::MIN,
            feather: 0.3,
            darkening: 0.5,
            wet_roughness: 0.12,
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct WetSandExt {
    #[uniform(100)]
    pub wetness: Wetness,
    #[uniform(101)]
    pub debug: SurfaceDebug,
}

impl MaterialExtension for WetSandExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/wet/wet_sand.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://bevy_shader_kit/shaders/wet/wet_sand.wesl".into()
    }
}

pub type WetSandMaterial = ExtendedMaterial<StandardMaterial, WetSandExt>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_wetness_dries_out_the_whole_world() {
        assert!(Wetness::default().height < -1.0e30);
    }
}
