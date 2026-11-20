use bevy::prelude::*;

#[derive(Resource)]
pub struct SpawnGroundHeight(pub f32);

impl Default for SpawnGroundHeight {
    fn default() -> Self {
        Self(0.0)
    }
}
