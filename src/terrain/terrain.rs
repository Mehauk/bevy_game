use bevy::{ecs::resource::Resource, mesh::{Mesh, Meshable}, shape::Plane3d};


#[derive(Resource)]
pub struct Terrain {
    height: f32,
    width: f32,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            height: 400.0,
            width: 600.0
        }
    }
}

impl Terrain {
    pub fn size(&self) -> (f32, f32) {
        (self.height, self.width)
    }

    pub fn sample_ground_height(&self, x: f32, y:f32) -> f32 {
        0.0
    }
}

impl From<&Terrain> for Mesh {
    fn from(world: &Terrain) -> Self {
        Plane3d::default().mesh().size(world.height, world.width).into()
    }
}