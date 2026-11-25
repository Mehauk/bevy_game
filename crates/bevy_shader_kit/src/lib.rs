use bevy::{
    asset::embedded_asset,
    pbr::MaterialPlugin,
    prelude::*,
    shader::load_shader_library,
};

pub mod compose;
pub mod sdf;
pub mod water;

pub use compose::{
    BlueprintSegment, ComposedShader, DynamicShader, DynamicShaderRegistry, HookMode, ShaderBlueprint,
    ShaderHook, ShaderHookPoint, ShaderModification, compose_wesl, rebuild_dynamic_shaders,
    water_blueprint,
};
pub use sdf::{
    DistanceFieldDimmedColor, DistanceFieldParams, DistanceFieldSource, DistanceFieldUnitCube,
    DistanceFieldView, DistanceFieldViewExt, DistanceFieldViewHidden, DistanceFieldViewMaterial,
    DistanceFieldVolume, DimmedColorRestore, DimmedFog, FieldBounds, InvisibleMaterial,
    MAX_TEXELS, PrimaryField, animate_distance_field, bake_distance_fields, sdf_from_triangles,
    sync_distance_field_view,
};
pub use water::{
    MAX_WATER_FIELDS, WaterFieldParams, WaterHeightSource, WaterMaterial, WaterMaterialExt,
    WaterMaterialKey, WaterParams, WaterSurface, dummy_field_image, sync_water_fields,
    water_shader_defs,
};

fn setup_unit_cube(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(DistanceFieldUnitCube(meshes.add(Cuboid::default())));
}

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

        app.add_plugins(MaterialPlugin::<DistanceFieldViewMaterial>::default())
            .add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<DistanceFieldView>()
            .init_resource::<DynamicShaderRegistry>()
            .add_systems(Startup, setup_unit_cube)
            .add_systems(
                Update,
                (
                    bake_distance_fields,
                    animate_distance_field,
                    sync_distance_field_view,
                    sync_water_fields,
                    rebuild_dynamic_shaders,
                ),
            );
    }
}
