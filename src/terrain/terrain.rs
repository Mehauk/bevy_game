use bevy::{
    ecs::resource::Resource, mesh::{Mesh, Meshable, VertexAttributeValues}, shape::Plane3d,
};

#[derive(Resource)]
pub struct Terrain {
    width: f32,   
    height: f32,      
    subdivisions: u32,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            height: 400.0,
            width: 600.0,
            subdivisions: 1000,
        }
    }
}

impl Terrain {
    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn sample_ground_height(&self, x: f32, z: f32) -> f32 {
        let half_inner_x = self.width / 2.0 - 100.0;
        let half_inner_z = self.height / 2.0 - 100.0;

        let over_x = (x.abs() - half_inner_x).max(0.0);
        let over_z = (z.abs() - half_inner_z).max(0.0);

        let dist_outside = (over_x * over_x + over_z * over_z).sqrt();

        let t = (dist_outside / 30.0).clamp(0.0, 1.0);

        let smooth_t = t * t * (3.0 - 2.0 * t);

        -30.0 * smooth_t
    }
}

impl From<&Terrain> for Mesh {
    fn from(terrain: &Terrain) -> Self {
        let total_x = terrain.width;
        let total_z = terrain.height;

        let mut mesh: Mesh = Plane3d::default()
            .mesh()
            .size(total_x, total_z)
            .subdivisions(terrain.subdivisions)
            .into();

        if let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {
            for pos in positions.iter_mut() {
                pos[1] = terrain.sample_ground_height(pos[0], pos[2]);
            }
        }

        mesh.compute_normals();
        mesh
    }
}
