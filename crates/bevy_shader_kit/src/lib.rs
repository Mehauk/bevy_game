use bevy::{
    asset::embedded_asset,
    pbr::MaterialPlugin,
    prelude::*,
    shader::load_shader_library,
};

pub mod bake;
pub mod compose;
pub mod debug;
pub mod field;
pub mod post_process;
pub mod underwater;
pub mod water;
pub mod wet;

pub use bake::{GpuBakeSdfRequest, GpuSdfBakePlugin, SdfBakeQueue, uninit_sdf_image};
pub use debug::{
    DebugCurtainExt, DebugCurtainMaterial, DebugCurtainParams, MAX_SURFACE_DEBUG_CELLS, SurfaceDebug,
    region_color,
};
pub use post_process::{PostProcessEffect, PostProcessPlugin};
pub use underwater::UnderwaterPostProcess;
pub use wet::{WetSandExt, WetSandMaterial, Wetness};
pub use field::{
    DistanceFieldViewExt, DistanceFieldViewMaterial, DistanceFieldViewUniform, SdfVolume,
};
pub use compose::{
    BlueprintSegment, ComposedShader, DynamicShader, DynamicShaderRegistry, HookMode, ShaderBlueprint,
    ShaderHook, ShaderHookPoint, ShaderModification, compose_wesl, rebuild_dynamic_shaders,
    water_blueprint,
};
pub use water::{
    MAX_RIPPLES, MAX_SWELLS, REFRACTION_STRENGTH, RippleSet, RippleWave, ShoreContext,
    SHORE_ZONE, SHOAL_DEPTH, SwellSet, SwellWave, WaterCaustics, WaterDye, WaterDetail,
    WaterMaterial, WaterMaterialExt, WaterMaterialKey, WaterOptics, WaterParams, shore_factor,
    shore_horiz_damp, shore_vert_damp, water_shader_defs,
};

pub struct ShaderKitPlugin;

impl Plugin for ShaderKitPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/math/aa.wesl");
        load_shader_library!(app, "shaders/math/falloff.wesl");
        load_shader_library!(app, "shaders/math/normal.wesl");
        load_shader_library!(app, "shaders/noise/hash.wesl");
        load_shader_library!(app, "shaders/noise/value2d.wesl");
        load_shader_library!(app, "shaders/noise/fbm2d.wesl");
        load_shader_library!(app, "shaders/optics/fresnel.wesl");
        load_shader_library!(app, "shaders/optics/refraction.wesl");
        load_shader_library!(app, "shaders/optics/scattering.wesl");
        load_shader_library!(app, "shaders/shading/hemisphere.wesl");
        load_shader_library!(app, "shaders/sdf/texture3d.wesl");
        load_shader_library!(app, "shaders/sdf/volume.wesl");
        load_shader_library!(app, "shaders/sdf/view_bindings.wesl");
        load_shader_library!(app, "shaders/water/optics_types.wesl");
        load_shader_library!(app, "shaders/water/types.wesl");
        load_shader_library!(app, "shaders/water/foam.wesl");
        load_shader_library!(app, "shaders/water/ripples.wesl");
        load_shader_library!(app, "shaders/water/optics.wesl");
        load_shader_library!(app, "shaders/water/swell_types.wesl");
        load_shader_library!(app, "shaders/water/swells.wesl");
        load_shader_library!(app, "shaders/water/body.wesl");
        load_shader_library!(app, "shaders/water/detail.wesl");
        load_shader_library!(app, "shaders/water/dye.wesl");
        load_shader_library!(app, "shaders/water/caustics.wesl");
        load_shader_library!(app, "shaders/water/refract.wesl");
        load_shader_library!(app, "shaders/post_process/bindings.wesl");
        load_shader_library!(app, "shaders/debug/region_color.wesl");
        load_shader_library!(app, "shaders/debug/surface_debug.wesl");
        embedded_asset!(app, "shaders/sdf/view_mode.wesl");
        embedded_asset!(app, "shaders/sdf/bake_compute.wesl");
        embedded_asset!(app, "shaders/water/water.wesl");
        embedded_asset!(app, "shaders/water/water_vertex.wesl");
        embedded_asset!(app, "shaders/wet/wet_sand.wesl");
        embedded_asset!(app, "shaders/debug/curtain.wesl");
        embedded_asset!(app, "shaders/post_process/underwater.wesl");

        app.add_plugins(GpuSdfBakePlugin)
            .add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_plugins(PostProcessPlugin::<UnderwaterPostProcess>::default())
            .init_resource::<DynamicShaderRegistry>()
            .add_systems(Update, rebuild_dynamic_shaders);
    }
}
