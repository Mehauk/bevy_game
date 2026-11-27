use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::lod::LodTransforms;
use crate::settings::OceanSettings;

#[derive(Component)]
pub struct OceanRoot;

#[derive(Resource, Clone, Copy, Debug)]
pub struct OceanViewer {
    pub position: Vec3,
}

impl Default for OceanViewer {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
        }
    }
}

const RING_CELLS: u32 = 64;
const HOLE_CELLS: u32 = RING_CELLS / 4;

pub fn build_ocean_mesh(lod_count: usize) -> Mesh {
    let side = RING_CELLS + 1;
    let half_cells = RING_CELLS / 2;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut alphas: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for ring in 0..lod_count.min(8) {
        let step = (1u32 << ring) as f32;
        let base = positions.len() as u32;
        let vertex = |i: u32, j: u32| -> u32 { base + j * side + i };
        let on_border = |i: u32, j: u32| i == 0 || j == 0 || i == RING_CELLS || j == RING_CELLS;
        for j in 0..side {
            for i in 0..side {
                positions.push([
                    (i as f32 - half_cells as f32) * step,
                    0.0,
                    (j as f32 - half_cells as f32) * step,
                ]);
                normals.push([0.0, 1.0, 0.0]);
                let morph = on_border(i, j) && ring + 1 < lod_count.min(8);
                alphas.push(if morph { 1.0 } else { 0.0 });
            }
        }
        let in_hole = |i: u32, j: u32| {
            ring > 0
                && i >= HOLE_CELLS
                && i < RING_CELLS - HOLE_CELLS
                && j >= HOLE_CELLS
                && j < RING_CELLS - HOLE_CELLS
        };
        for j in 0..RING_CELLS {
            for i in 0..RING_CELLS {
                if in_hole(i, j) {
                    continue;
                }
                indices.extend_from_slice(&[
                    vertex(i, j),
                    vertex(i, j + 1),
                    vertex(i + 1, j + 1),
                    vertex(i, j),
                    vertex(i + 1, j + 1),
                    vertex(i + 1, j),
                ]);
            }
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, alphas.iter().map(|a| [*a, 0.0]).collect::<Vec<[f32; 2]>>());
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn update_lod_transforms(
    viewer: Res<OceanViewer>,
    settings: Res<OceanSettings>,
    mut transforms: ResMut<LodTransforms>,
) {
    if !viewer.is_changed() && !settings.is_changed() {
        return;
    }
    *transforms = LodTransforms::compute(&settings, viewer.position);
}

fn sync_ocean_transform(
    transforms: Res<LodTransforms>,
    settings: Res<OceanSettings>,
    mut ocean: Query<&mut Transform, With<OceanRoot>>,
) {
    let Some(lod0) = transforms.transforms.first() else {
        return;
    };
    let center = lod0.origin + Vec3::splat(lod0.extent * 0.5);
    for mut transform in &mut ocean {
        transform.translation = Vec3::new(center.x, settings.water_level, center.z);
    }
}

pub struct OceanPlugin;

impl Plugin for OceanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OceanViewer>()
            .init_resource::<LodTransforms>()
            .add_systems(
                Update,
                (update_lod_transforms, sync_ocean_transform).chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mesh_faces_up() {
        let mesh = build_ocean_mesh(7);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let indices: Vec<u32> = mesh
            .indices()
            .unwrap()
            .iter()
            .map(|index| index as u32)
            .collect();
        for tri in indices.chunks(3) {
            let a = Vec3::from(positions[tri[0] as usize]);
            let b = Vec3::from(positions[tri[1] as usize]);
            let c = Vec3::from(positions[tri[2] as usize]);
            assert!((b - a).cross(c - a).y > 0.0, "every triangle must face up");
        }
    }

    #[test]
    fn morph_alpha_marks_only_outer_borders() {
        let mesh = build_ocean_mesh(7);
        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap() {
            bevy::render::mesh::VertexAttributeValues::Float32x2(values) => values,
            _ => panic!("uv attribute must be float32x2"),
        };
        let side = (RING_CELLS + 1) as usize;
        let ring_base = |ring: u32| (ring as usize) * side * side;
        for ring in 0..6 {
            let base = ring_base(ring);
            let center = (RING_CELLS / 2) as usize;
            assert_eq!(uvs[base + center * side + center][0], 0.0);
            assert_eq!(uvs[base][0], 1.0);
        }
        let last_base = ring_base(6);
        assert_eq!(uvs[last_base][0], 0.0, "the outermost ring must not morph");
    }

    #[test]
    fn lod_transforms_snap_to_the_texel_grid() {
        let settings = OceanSettings::default();
        for step in 0..32 {
            let viewer = Vec3::new(step as f32 * 0.31 - 5.0, 0.0, step as f32 * -0.47 + 3.0);
            let transforms = LodTransforms::compute(&settings, viewer);
            for lod in 0..transforms.count {
                let transform = &transforms.transforms[lod];
                assert_eq!(transform.origin.x % transform.texel_size, 0.0);
                assert_eq!(transform.origin.z % transform.texel_size, 0.0);
            }
        }
    }
}
