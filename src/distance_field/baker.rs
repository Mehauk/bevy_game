use bevy::{light::NotShadowCaster, mesh::VertexAttributeValues, pbr::StandardMaterial, prelude::*};
use bevy_shader_kit::{
    DistanceFieldViewExt, DistanceFieldViewMaterial, DistanceFieldViewUniform, GpuBakeSdfRequest,
    SdfBakeQueue, SdfVolume, uninit_sdf_image,
};

use super::{
    DistanceFieldSource, DistanceFieldUnitCube, DistanceFieldViewHidden, DistanceFieldVolume,
    InvisibleMaterial, MAX_TEXELS,
};

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

#[derive(Clone, Debug, Default)]
pub struct FieldGeometry {
    pub vertices: Vec<Vec3>,
    pub indices: Vec<u32>,
}

impl FieldGeometry {
    fn push_part(&mut self, positions: &[Vec3], indices: &[u32]) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(positions);
        self.indices.extend(indices.iter().map(|index| index + base));
    }

    fn bounds(&self) -> Option<FieldBounds> {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for vertex in &self.vertices {
            min = min.min(*vertex);
            max = max.max(*vertex);
        }
        if min.cmpgt(max).any() {
            return None;
        }
        Some(FieldBounds { min, max })
    }
}

fn mesh_positions(mesh: &Mesh) -> Option<&Vec<[f32; 3]>> {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(positions) => Some(positions),
        _ => None,
    }
}

fn mesh_triangles(mesh: &Mesh) -> Option<Vec<u32>> {
    Some(mesh.indices()?.iter().map(|index| index as u32).collect())
}

fn opaque_material(materials: &Assets<StandardMaterial>, handle: &Handle<StandardMaterial>) -> bool {
    materials
        .get(handle)
        .is_some_and(|m| m.alpha_mode == AlphaMode::Opaque && m.base_color.alpha() >= 1.0)
}

fn collect_field_geometry(
    entity: Entity,
    mesh3d: &Mesh3d,
    own_material: &MeshMaterial3d<StandardMaterial>,
    children: &Query<&Children>,
    transforms: &Query<&Transform>,
    meshed: &Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
    commands: &mut Commands,
) -> Option<(FieldBounds, FieldGeometry)> {
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
    let mut geometry = FieldGeometry::default();
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
        let Some(part_indices) = mesh_triangles(part) else {
            continue;
        };
        let positions: Vec<Vec3> = part_positions
            .iter()
            .map(|p| matrix.transform_point3(Vec3::from(*p)))
            .collect();
        geometry.push_part(&positions, &part_indices);
    }
    let bounds = geometry.bounds()?;
    if geometry.indices.is_empty() {
        return None;
    }
    Some((bounds, geometry))
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
    meshes: Res<Assets<Mesh>>,
    unit_cube: Res<DistanceFieldUnitCube>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut view_materials: ResMut<Assets<DistanceFieldViewMaterial>>,
    mut bake_queue: ResMut<SdfBakeQueue>,
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
    for (entity, mesh3d, own_material, mut source) in sources.iter_mut() {
        if source.baked {
            continue;
        }
        let Some((bounds, geometry)) = collect_field_geometry(
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
        let volume = SdfVolume::from_bounds(
            field_bounds.min,
            field_bounds.max,
            resolution.as_vec3(),
        );
        let image = images.add(uninit_sdf_image(resolution));
        bake_queue.push(GpuBakeSdfRequest {
            image: image.clone(),
            volume,
            vertices: geometry.vertices,
            indices: geometry.indices,
        });
        let span = (field_bounds.max - field_bounds.min).max(Vec3::splat(1e-3));
        let view = DistanceFieldViewUniform {
            volume,
            glow_range,
            clay_alpha: source.clay_alpha,
            ..default()
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
                view,
            },
        });
        let center = (bounds.max + bounds.min) * 0.5;
        let view_entity = commands
            .spawn((
                DistanceFieldVolume,
                ChildOf(entity),
                Visibility::Hidden,
                Transform::from_translation(center).with_scale(span + Vec3::splat(2.0 * glow_range)),
                Mesh3d(unit_cube.0.clone()),
                MeshMaterial3d(view_material.clone()),
                NotShadowCaster,
            ))
            .id();
        source.image = image;
        source.view_material = view_material;
        source.surface_top = bounds.max.y;
        source.glow_range = glow_range;
        source.volume = volume;
        source.view_entity = Some(view_entity);
        source.baked = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureUsages};

    fn cube_at(matrix: Mat4, size: [f32; 3]) -> (Vec<Vec3>, Vec<u32>) {
        let mesh: Mesh = Cuboid::new(size[0], size[1], size[2]).mesh().into();
        let positions = mesh_positions(&mesh)
            .unwrap()
            .iter()
            .map(|p| matrix.transform_point3(Vec3::from(*p)))
            .collect();
        (positions, mesh_triangles(&mesh).unwrap())
    }

    #[test]
    fn merged_geometry_rebases_each_part_to_its_own_vertex_range() {
        let mut geometry = FieldGeometry::default();
        let (first, first_indices) = cube_at(Mat4::IDENTITY, [1.0, 1.0, 1.0]);
        let (second, second_indices) = cube_at(Mat4::from_translation(Vec3::X * 4.0), [1.0, 2.0, 3.0]);
        geometry.push_part(&first, &first_indices);
        geometry.push_part(&second, &second_indices);

        assert_eq!(geometry.vertices.len(), first.len() + second.len());
        let base = first.len() as u32;
        assert_eq!(geometry.indices.len(), first_indices.len() + second_indices.len());
        assert_eq!(&geometry.indices[..first_indices.len()], &first_indices[..]);
        assert_eq!(
            &geometry.indices[first_indices.len()..],
            &second_indices.iter().map(|i| i + base).collect::<Vec<u32>>()[..]
        );
        assert!(
            geometry.indices.iter().all(|i| (*i as usize) < geometry.vertices.len()),
            "every index must address the merged vertex buffer"
        );
    }

    #[test]
    fn bounds_span_every_part() {
        let mut geometry = FieldGeometry::default();
        geometry.push_part(&[Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 2.0)], &[0, 1, 0]);
        geometry.push_part(&[Vec3::new(3.0, -2.0, 5.0)], &[]);
        let bounds = geometry.bounds().unwrap();
        assert_eq!(bounds.min, Vec3::new(-1.0, -2.0, 0.0));
        assert_eq!(bounds.max, Vec3::new(3.0, 1.0, 5.0));
        assert!(FieldGeometry::default().bounds().is_none());
    }

    #[test]
    fn a_single_cube_bounds_its_own_extent() {
        let mesh: Mesh = Cuboid::new(2.0, 4.0, 6.0).mesh().into();
        let geometry = FieldGeometry {
            vertices: mesh_positions(&mesh)
                .unwrap()
                .iter()
                .map(|p| Vec3::from(*p))
                .collect(),
            indices: mesh_triangles(&mesh).unwrap(),
        };
        let bounds = geometry.bounds().unwrap();
        assert!((bounds.max.x - 1.0).abs() < 1e-5);
        assert!((bounds.max.y - 2.0).abs() < 1e-5);
        assert!((bounds.max.z - 3.0).abs() < 1e-5);
        assert_eq!(bounds.min, -bounds.max);
    }

    #[test]
    fn the_bake_target_is_an_uninitialized_storage_texture() {
        let image = uninit_sdf_image(UVec3::new(8, 16, 32));
        assert!(image.data.is_none(), "the field must never be staged through the CPU");
        let descriptor = image.texture_descriptor;
        assert_eq!(descriptor.format, TextureFormat::R32Float);
        assert_eq!(descriptor.dimension, TextureDimension::D3);
        assert_eq!(descriptor.size.width, 8);
        assert_eq!(descriptor.size.height, 16);
        assert_eq!(descriptor.size.depth_or_array_layers, 32);
        assert!(descriptor.usage.contains(TextureUsages::STORAGE_BINDING));
        assert!(descriptor.usage.contains(TextureUsages::TEXTURE_BINDING));
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
