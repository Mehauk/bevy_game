use bevy::{
    asset::embedded_asset,
    pbr::MaterialPlugin,
    prelude::*,
    shader::load_shader_library,
};

pub mod compose;
pub mod field;
pub mod water;

pub use field::{
    DistanceFieldViewExt, DistanceFieldViewMaterial, DistanceFieldViewUniform, SdfVolume,
};
pub use compose::{
    BlueprintSegment, ComposedShader, DynamicShader, DynamicShaderRegistry, HookMode, ShaderBlueprint,
    ShaderHook, ShaderHookPoint, ShaderModification, compose_wesl, rebuild_dynamic_shaders,
    water_blueprint,
};
pub use water::{
    MAX_WATER_FIELDS, WaterMaterial, WaterMaterialExt, WaterMaterialKey, WaterParams,
    dummy_field_image, water_shader_defs,
};

pub struct ShaderKitPlugin;

impl Plugin for ShaderKitPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/math/aa.wesl");
        load_shader_library!(app, "shaders/math/falloff.wesl");
        load_shader_library!(app, "shaders/noise/hash.wesl");
        load_shader_library!(app, "shaders/noise/value2d.wesl");
        load_shader_library!(app, "shaders/noise/fbm2d.wesl");
        load_shader_library!(app, "shaders/optics/fresnel.wesl");
        load_shader_library!(app, "shaders/optics/refraction.wesl");
        load_shader_library!(app, "shaders/optics/scattering.wesl");
        load_shader_library!(app, "shaders/shading/hemisphere.wesl");
        load_shader_library!(app, "shaders/sdf/texture3d.wesl");
        load_shader_library!(app, "shaders/sdf/volume.wesl");
        load_shader_library!(app, "shaders/sdf/field_array.wesl");
        load_shader_library!(app, "shaders/sdf/contact.wesl");
        load_shader_library!(app, "shaders/sdf/view_bindings.wesl");
        load_shader_library!(app, "shaders/water/types.wesl");
        load_shader_library!(app, "shaders/water/ripples.wesl");
        load_shader_library!(app, "shaders/water/foam.wesl");
        load_shader_library!(app, "shaders/water/optics.wesl");
        embedded_asset!(app, "shaders/sdf/view_mode.wesl");
        embedded_asset!(app, "shaders/water/water.wesl");

        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<DynamicShaderRegistry>()
            .add_systems(Update, rebuild_dynamic_shaders);
    }
}
