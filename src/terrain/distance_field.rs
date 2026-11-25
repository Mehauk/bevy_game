use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::VertexAttributeValues,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, StandardMaterial},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat},
    shader::ShaderRef,
};

use crate::utils::registry::commands::CommandOutput;
use super::atmosphere::SkyBox;

pub const MAX_TEXELS: usize = 4_000_000;
const SKYBOX_VIEW_COLOR: Color = Color::srgb(0.015, 0.015, 0.018);

#[derive(Resource, Default)]
pub struct DistanceFieldView {
    pub active: bool,
}

#[derive(Component, Default)]
pub struct DistanceFieldSource {
    pub texel: Vec3,
    pub water_margin: f32,
    pub surface_top: f32,
    pub clay_alpha: f32,
    pub image: Handle<Image>,
    pub view_material: Handle<ExtendedMaterial<StandardMaterial, DistanceFieldViewExt>>,
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
pub struct DimmedSkybox(pub Color);

#[derive(Component)]
pub struct DimmedFog(pub f32);

#[derive(Resource)]
pub struct InvisibleMaterial(pub Handle<StandardMaterial>);

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
        "shaders/distance_field_view_mode.wesl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/distance_field_view_mode.wesl".into()
    }
}

pub type DistanceFieldViewMaterial = ExtendedMaterial<StandardMaterial, DistanceFieldViewExt>;

pub fn register(app: &mut App) {
    app.add_plugins(MaterialPlugin::<DistanceFieldViewMaterial>::default());
    app.init_resource::<DistanceFieldView>();
}

fn opaque_material(materials: &Assets<StandardMaterial>, handle: &Handle<StandardMaterial>) -> bool {
    materials
        .get(handle)
        .is_some_and(|m| m.alpha_mode == AlphaMode::Opaque && m.base_color.alpha() >= 1.0)
}

struct PartGeometry {
    positions: Vec<[f32; 3]>,
    indices: Vec<usize>,
}

fn collect_opaque_parts(
    entity: Entity,
    mesh3d: &Mesh3d,
    own_material: &MeshMaterial3d<StandardMaterial>,
    children: &Query<&Children>,
    transforms: &Query<&Transform>,
    meshed: &Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
    commands: &mut Commands,
) -> Option<(FieldBounds, Vec<PartGeometry>)> {
    let mut handles: Vec<(Handle<Mesh>, Mat4, Handle<StandardMaterial>)> =
        vec![(mesh3d.0.clone(), Mat4::IDENTITY, own_material.0.clone())];
    let mut stack: Vec<(Entity, Mat4)> = vec![(entity, Mat4::IDENTITY)];
    while let Some((current, parent_to_source)) = stack.pop() {
        let Ok(kids) = children.get(current) else {
            continue;
        };
        for kid in kids.iter() {
            let local = transforms
                .get(kid)
                .map(|t| t.to_matrix())
                .unwrap_or_default();
            let to_source = parent_to_source * local;
            if let Ok((kid_mesh, kid_material)) = meshed.get(kid) {
                handles.push((kid_mesh.0.clone(), to_source, kid_material.0.clone()));
                commands.entity(kid).insert(DistanceFieldViewHidden);
            }
            stack.push((kid, to_source));
        }
    }
    let mut parts: Vec<PartGeometry> = Vec::new();
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for (handle, matrix, material) in handles {
        if !opaque_material(materials, &material) {
            continue;
        }
        let Some(part) = meshes.get(&handle) else {
            continue;
        };
        let Some(part_positions) = mesh_positions(part) else {
            continue;
        };
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(part_positions.len());
        for p in part_positions {
            let q = matrix.transform_point3(Vec3::from(*p));
            min = min.min(q);
            max = max.max(q);
            positions.push(q.to_array());
        }
        let Some(part_indices) = part.indices() else {
            continue;
        };
        parts.push(PartGeometry {
            positions,
            indices: part_indices.iter().collect(),
        });
    }
    if parts.is_empty() || min.cmpgt(max).any() {
        return None;
    }
    Some((FieldBounds { min, max }, parts))
}

pub fn bake_distance_fields(
    mut commands: Commands,
    mut sources: Query<(
        Entity,
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &mut DistanceFieldSource,
    )>,
    children: Query<&Children>,
    transforms: Query<&Transform>,
    meshed: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut view_materials: ResMut<Assets<DistanceFieldViewMaterial>>,
    invisible: Option<Res<InvisibleMaterial>>,
) {
    if invisible.is_none() {
        commands.insert_resource(InvisibleMaterial(materials.add(StandardMaterial {
            base_color: Color::NONE,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })));
    }
    if sources.iter().all(|(_, _, _, source)| source.baked) {
        return;
    }
    let volume_mesh = meshes.add(Mesh::from(Cuboid::default()));
    for (entity, mesh3d, own_material, mut source) in sources.iter_mut() {
        if source.baked {
            continue;
        }
        let Some((bounds, parts)) = collect_opaque_parts(
            entity,
            mesh3d,
            own_material,
            &children,
            &transforms,
            &meshed,
            &meshes,
            &materials,
            &mut commands,
        ) else {
            continue;
        };
        let base_resolution = bounds.resolution(source.texel);
        let voxel = (bounds.max - bounds.min) / base_resolution.as_vec3();
        let glow_range = (voxel.max_element() * 4.0).clamp(0.3, 1.0);
        let border = voxel.max(Vec3::splat(glow_range)) + Vec3::splat(source.water_margin);
        let field_bounds = FieldBounds {
            min: bounds.min - border,
            max: bounds.max + border,
        };
        let resolution = field_bounds.resolution(source.texel);
        let mut data = vec![f32::MAX; (resolution.x * resolution.y * resolution.z) as usize];
        for part in &parts {
            let field =
                sdf_from_triangles(&field_bounds, resolution, &part.positions, &part.indices);
            for (out, value) in data.iter_mut().zip(&field) {
                *out = out.min(*value);
            }
        }
        let image = images.add(Image::new(
            Extent3d {
                width: resolution.x,
                height: resolution.y,
                depth_or_array_layers: resolution.z,
            },
            TextureDimension::D3,
            f32_bytes(&data),
            TextureFormat::R32Float,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        ));
        let span = (field_bounds.max - field_bounds.min).max(Vec3::splat(1e-3));
        let params = DistanceFieldParams {
            field_from_world: Mat4::IDENTITY,
            bounds_min: field_bounds.min,
            bounds_max: field_bounds.max,
            inv_bounds_size: Vec3::new(1.0 / span.x, 1.0 / span.y, 1.0 / span.z),
            resolution: resolution.as_vec3(),
            time: 0.0,
            glow_range,
            clay_alpha: source.clay_alpha,
        };
        let view_material = view_materials.add(DistanceFieldViewMaterial {
            base: StandardMaterial {
                base_color: Color::BLACK,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            },
            extension: DistanceFieldViewExt {
                sdf: image.clone(),
                params,
            },
        });
        let center = (bounds.max + bounds.min) * 0.5;
        let volume = commands
            .spawn((
                DistanceFieldVolume,
                ChildOf(entity),
                Visibility::Hidden,
                Transform::from_translation(center).with_scale(span + Vec3::splat(2.0 * glow_range)),
                Mesh3d(volume_mesh.clone()),
                MeshMaterial3d(view_material.clone()),
                NotShadowCaster,
            ))
            .id();
        source.image = image;
        source.view_material = view_material;
        source.surface_top = bounds.max.y;
        source.params = params;
        source.volume = Some(volume);
        source.baked = true;
    }
}

pub fn animate_distance_field(
    time: Res<Time>,
    sources: Query<(&GlobalTransform, &DistanceFieldSource)>,
    mut view_materials: ResMut<Assets<DistanceFieldViewMaterial>>,
) {
    for (global, source) in sources.iter() {
        if !source.baked {
            continue;
        }
        let params = DistanceFieldParams {
            field_from_world: global.to_matrix().inverse(),
            time: time.elapsed_secs(),
            ..source.params
        };
        if let Some(mut material) = view_materials.get_mut(&source.view_material) {
            material.extension.params = params;
            material.extension.sdf = source.image.clone();
        }
    }
}

pub fn sync_distance_field_view(
    view: Res<DistanceFieldView>,
    mut commands: Commands,
    invisible: Option<Res<InvisibleMaterial>>,
    mut sources: Query<(
        Entity,
        &mut DistanceFieldSource,
        Option<&MeshMaterial3d<StandardMaterial>>,
    )>,
    mut marked: Query<(
        Entity,
        &mut Visibility,
        Has<DistanceFieldVolume>,
        Has<DistanceFieldViewHidden>,
    )>,
    skyboxes: Query<
        (
            Entity,
            &MeshMaterial3d<StandardMaterial>,
            Option<&DimmedSkybox>,
        ),
        With<SkyBox>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fogs: Query<(Entity, &mut DistanceFog, Option<&DimmedFog>)>,
) {
    if !view.is_changed() {
        return;
    }
    for (entity, mut source, material) in sources.iter_mut() {
        if view.active {
            if let (Some(invisible), Some(material)) = (invisible.as_deref(), material) {
                source.hidden_material = Some(material.0.clone());
                commands
                    .entity(entity)
                    .insert(MeshMaterial3d(invisible.0.clone()));
            }
        } else if let Some(handle) = source.hidden_material.take() {
            commands.entity(entity).insert(MeshMaterial3d(handle));
        }
    }
    for (_, mut visibility, is_volume, is_hidden) in marked.iter_mut() {
        if is_volume {
            *visibility = if view.active {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        } else if is_hidden {
            *visibility = if view.active {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
    for (entity, material_handle, dimmed) in skyboxes.iter() {
        let Some(mut material) = materials.get_mut(material_handle) else {
            continue;
        };
        if view.active {
            if dimmed.is_none() {
                commands
                    .entity(entity)
                    .insert(DimmedSkybox(material.base_color));
            }
            material.base_color = SKYBOX_VIEW_COLOR;
        } else if let Some(DimmedSkybox(original)) = dimmed {
            material.base_color = *original;
            commands.entity(entity).remove::<DimmedSkybox>();
        }
    }
    for (entity, mut fog, stored) in fogs.iter_mut() {
        if view.active {
            if stored.is_none() {
                commands
                    .entity(entity)
                    .insert(DimmedFog(fog.color.alpha()));
            }
            fog.color.set_alpha(0.0);
        } else if let Some(DimmedFog(original)) = stored {
            fog.color.set_alpha(*original);
            commands.entity(entity).remove::<DimmedFog>();
        }
    }
}

pub fn toggle_distance_field(
    mut view: ResMut<DistanceFieldView>,
    mut output: ResMut<CommandOutput>,
) {
    view.active = !view.active;
    output.set(if view.active {
        "Distance field view: on"
    } else {
        "Distance field view: off"
    });
}

pub fn distance_field_debug_info(
    view: Res<DistanceFieldView>,
    primary: Query<&DistanceFieldSource, With<PrimaryField>>,
) -> Option<String> {
    let source = primary.iter().next()?;
    let r = source.params.resolution;
    Some(format!(
        "{} {}x{}x{}",
        if view.active { "on" } else { "off" },
        r.x as i32,
        r.y as i32,
        r.z as i32,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl FieldBounds {
    pub fn resolution(&self, texel: Vec3) -> UVec3 {
        let span = self.max - self.min;
        let raw = UVec3::new(
            (span.x / texel.x).ceil().max(2.0) as u32,
            (span.y / texel.y).ceil().max(2.0) as u32,
            (span.z / texel.z).ceil().max(2.0) as u32,
        );
        let total = raw.x as usize * raw.y as usize * raw.z as usize;
        if total <= MAX_TEXELS {
            return raw;
        }
        let shrink = (MAX_TEXELS as f32 / total as f32).cbrt();
        UVec3::new(
            ((raw.x as f32 * shrink) as u32).max(2),
            ((raw.y as f32 * shrink) as u32).max(2),
            ((raw.z as f32 * shrink) as u32).max(2),
        )
    }
}

fn mesh_positions(mesh: &Mesh) -> Option<&Vec<[f32; 3]>> {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(positions) => Some(positions),
        _ => None,
    }
}

pub fn sdf_from_triangles(
    bounds: &FieldBounds,
    resolution: UVec3,
    positions: &[[f32; 3]],
    indices: &[usize],
) -> Vec<f32> {
    let min = bounds.min;
    let max = bounds.max;
    let grid = mesh_to_sdf::Grid::from_bounding_box(
        &[min.x, min.y, min.z],
        &[max.x, max.y, max.z],
        [
            resolution.x as usize,
            resolution.y as usize,
            resolution.z as usize,
        ],
    );
    let indices: Vec<u32> = indices.iter().map(|i| *i as u32).collect();
    let generated = mesh_to_sdf::generate_grid_sdf(
        positions,
        mesh_to_sdf::Topology::TriangleList(Some(&indices)),
        &grid,
        mesh_to_sdf::SignMethod::Raycast,
    );
    let (rx, ry, rz) = (
        resolution.x as usize,
        resolution.y as usize,
        resolution.z as usize,
    );
    let mut data = vec![0.0f32; generated.len()];
    for z in 0..rz {
        for y in 0..ry {
            for x in 0..rx {
                data[x + y * rx + z * rx * ry] = generated[x * ry * rz + y * rz + z];
            }
        }
    }
    data
}

fn f32_bytes(data: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triangle_sdf_signs_a_cube_correctly() {
        let mesh: Mesh = Cuboid::new(1.0, 1.0, 1.0).mesh().into();
        let positions = mesh_positions(&mesh).unwrap();
        let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
        let bounds = FieldBounds {
            min: Vec3::splat(-1.5),
            max: Vec3::splat(1.5),
        };
        let res = UVec3::new(6, 6, 6);
        let data = sdf_from_triangles(&bounds, res, positions, &indices);
        let sample = |ix: u32, iy: u32, iz: u32| {
            data[(iz * res.x * res.y + iy * res.x + ix) as usize]
        };
        let inside = sample(2, 3, 2);
        assert!(inside < 0.0, "texel inside the cube should be negative, got {inside}");
        let outside = sample(5, 5, 5);
        assert!(outside > 0.0, "texel outside the cube should be positive, got {outside}");
    }

    #[test]
    fn prop_parts_union_field_is_sane() {
        let part = |mesh: &Mesh, matrix: Mat4| -> PartGeometry {
            PartGeometry {
                positions: mesh_positions(mesh)
                    .unwrap()
                    .iter()
                    .map(|p| matrix.transform_point3(Vec3::from(*p)).to_array())
                    .collect(),
                indices: mesh.indices().unwrap().iter().collect(),
            }
        };
        let parts = vec![
            part(&Mesh::from(Capsule3d::new(0.4, 1.0)), Mat4::IDENTITY),
            part(
                &Mesh::from(Cuboid::new(0.45, 0.18, 0.2)),
                Mat4::from_translation(Vec3::new(0.0, 0.45, -0.35)),
            ),
        ];
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for geometry in &parts {
            for p in &geometry.positions {
                let q = Vec3::from(*p);
                min = min.min(q);
                max = max.max(q);
            }
        }
        let bounds = FieldBounds { min, max };
        let texel = Vec3::splat(0.1);
        let base_resolution = bounds.resolution(texel);
        let voxel = (bounds.max - bounds.min) / base_resolution.as_vec3();
        let glow_range = (voxel.max_element() * 4.0).clamp(0.3, 1.0);
        let border = voxel.max(Vec3::splat(glow_range));
        let field_bounds = FieldBounds {
            min: bounds.min - border,
            max: bounds.max + border,
        };
        let resolution = field_bounds.resolution(texel);
        let mut data = vec![f32::MAX; (resolution.x * resolution.y * resolution.z) as usize];
        for geometry in &parts {
            let field =
                sdf_from_triangles(&field_bounds, resolution, &geometry.positions, &geometry.indices);
            for (out, value) in data.iter_mut().zip(&field) {
                *out = out.min(*value);
            }
        }
        let span = field_bounds.max - field_bounds.min;
        let texel_size = span / resolution.as_vec3();
        let sample = |field: &[f32], ix: u32, iy: u32, iz: u32| {
            field[(iz * resolution.x * resolution.y + iy * resolution.x + ix) as usize]
        };
        let at = |field: &[f32], p: Vec3| {
            let i = ((p - field_bounds.min) / texel_size - Vec3::splat(0.5))
                .round()
                .max(Vec3::ZERO)
                .min(resolution.as_vec3() - Vec3::splat(1.0));
            sample(field, i.x as u32, i.y as u32, i.z as u32)
        };
        assert!(data.iter().all(|v| v.is_finite()), "field has non-finite values");
        assert!(
            data.iter().all(|v| *v < 1.0e5),
            "field has unfilled texels: max is {}",
            data.iter().cloned().fold(f32::MIN, f32::max)
        );
        assert!(at(&data, Vec3::ZERO) < 0.0, "capsule center should be inside");
        assert!(at(&data, Vec3::new(0.0, 0.45, -0.35)) < 0.0, "visor center should be inside");
        assert!(at(&data, field_bounds.min + Vec3::splat(0.01)) > 0.0, "corner should be outside");
        for iz in 0..resolution.z {
            for iy in 0..resolution.y {
                for ix in 0..resolution.x - 1 {
                    let jump = (sample(&data, ix + 1, iy, iz) - sample(&data, ix, iy, iz)).abs();
                    assert!(
                        jump <= texel_size.x * 2.0,
                        "x-adjacent texels ({ix},{iy},{iz}) jump by {jump}, texel {}",
                        texel_size.x
                    );
                }
            }
        }
    }

    #[test]
    fn resolution_stays_within_the_texel_budget() {
        let bounds = FieldBounds {
            min: Vec3::splat(-10_000.0),
            max: Vec3::splat(10_000.0),
        };
        let res = bounds.resolution(Vec3::splat(1.0));
        let total = res.x as usize * res.y as usize * res.z as usize;
        assert!(total <= MAX_TEXELS, "{res:?} exceeds the budget");
    }

    #[test]
    fn resolution_is_at_least_two_per_axis() {
        let bounds = FieldBounds {
            min: Vec3::splat(-0.1),
            max: Vec3::splat(0.1),
        };
        let res = bounds.resolution(Vec3::splat(10.0));
        assert!(res.x >= 2 && res.y >= 2 && res.z >= 2);
    }
}
