use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat},
    shader::ShaderRef,
};

use super::distance_field::{DistanceFieldSource, DistanceFieldViewHidden, PrimaryField};
use super::terrain::Terrain;

pub const MAX_WATER_FIELDS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct WaterFieldParams {
    pub field_from_world: Mat4,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub inv_bounds_size: Vec3,
    pub resolution: Vec3,
}

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
    pub fields: [WaterFieldParams; MAX_WATER_FIELDS],
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            water_height: 0.0,
            ripple_range: 6.0,
            ripple_frequency: 3.0,
            ripple_speed: 2.5,
            ripple_strength: 0.3,
            foam_width: 0.8,
            foam_strength: 0.4,
            field_count: 0,
            max_depth: 8.0,
            phase_g: 0.6,
            pad: 0.0,
            scattering: Vec3::new(0.03, 0.045, 0.05),
            absorption: Vec3::new(0.35, 0.12, 0.06),
            sun_direction: Vec3::new(0.707, 0.0, 0.707),
            sun_illuminance: Vec3::splat(3.0),
            ambient_illuminance: Vec3::new(0.15, 0.2, 0.25),
            fields: [WaterFieldParams::default(); MAX_WATER_FIELDS],
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
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
}

impl MaterialExtension for WaterMaterialExt {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/water.wesl".into()
    }
}

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterMaterialExt>;

#[derive(Component)]
pub struct Sea;

#[derive(Resource)]
pub struct WaterSurface {
    pub material: Handle<WaterMaterial>,
    pub dummy: Handle<Image>,
}

pub fn register(app: &mut App) {
    app.add_plugins(MaterialPlugin::<WaterMaterial>::default());
}

fn dummy_field_image(images: &mut Assets<Image>) -> Handle<Image> {
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

pub fn setup_water_surface(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
    world: Res<Terrain>,
) {
    let dummy = dummy_field_image(&mut images);
    let material = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.6, 0.75, 0.72),
            alpha_mode: AlphaMode::Opaque,
            specular_transmission: 1.0,
            ior: 1.333,
            perceptual_roughness: 0.04,
            reflectance: 0.35,
            thickness: 0.1,
            attenuation_distance: 2.5,
            attenuation_color: Color::srgb(0.2, 0.7, 0.5),
            ..default()
        },
        extension: WaterMaterialExt {
            params: default(),
            field0: dummy.clone(),
            field1: dummy.clone(),
            field2: dummy.clone(),
            field3: dummy.clone(),
        },
    });
    commands.insert_resource(WaterSurface {
        material: material.clone(),
        dummy: dummy.clone(),
    });
    commands.spawn((
        Name::new("Sea"),
        Sea,
        Transform::from_xyz(0.0, -0.1, 0.0),
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(world.size().0, world.size().1),
            ),
        ),
        MeshMaterial3d(material),
        NotShadowCaster,
        DistanceFieldViewHidden,
    ));
}

fn field_params(global: &GlobalTransform, source: &DistanceFieldSource) -> WaterFieldParams {
    WaterFieldParams {
        field_from_world: global.to_matrix().inverse(),
        bounds_min: source.params.bounds_min,
        bounds_max: source.params.bounds_max,
        inv_bounds_size: source.params.inv_bounds_size,
        resolution: source.params.resolution,
    }
}

pub fn sync_water_fields(
    time: Res<Time>,
    sea: Query<&Transform, With<Sea>>,
    sources: Query<(&GlobalTransform, &DistanceFieldSource, Has<PrimaryField>)>,
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let Some(surface) = surface else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    let mut params = material.extension.params;
    params.time = time.elapsed_secs();
    if let Ok(transform) = sea.single() {
        params.water_height = transform.translation.y;
    }
    let mut chosen: Vec<(Handle<Image>, WaterFieldParams)> = Vec::with_capacity(MAX_WATER_FIELDS);
    for (global, source, primary) in sources.iter() {
        if source.baked && primary && chosen.len() < MAX_WATER_FIELDS {
            chosen.push((source.image.clone(), field_params(global, source)));
        }
    }
    for (global, source, primary) in sources.iter() {
        if source.baked && !primary && chosen.len() < MAX_WATER_FIELDS {
            chosen.push((source.image.clone(), field_params(global, source)));
        }
    }
    let ext = &mut material.extension;
    let slots = [
        &mut ext.field0,
        &mut ext.field1,
        &mut ext.field2,
        &mut ext.field3,
    ];
    for (index, slot) in slots.into_iter().enumerate() {
        match chosen.get(index) {
            Some((image, field)) => {
                *slot = image.clone();
                params.fields[index] = *field;
            }
            None => {
                *slot = surface.dummy.clone();
                params.fields[index] = WaterFieldParams::default();
            }
        }
    }
    params.field_count = chosen.len() as u32;
    ext.params = params;
}
