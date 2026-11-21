use bevy::prelude::*;

#[derive(Resource)]
pub struct SpawnGroundHeight(pub f32);

impl Default for SpawnGroundHeight {
    fn default() -> Self {
        Self(0.0)
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct WaterImmersion {
    pub submerged: bool,
    pub depth: f32,
    pub surface_y: f32,
    pub flow: Vec2,
    pub frozen: bool,
}
