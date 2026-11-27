use bevy::prelude::*;
use crest_water::OceanViewer;

use crate::camera::MainCamera;
use crate::terrain::TerrainViewer;

pub(super) fn register(app: &mut App) {
    app.add_systems(Update, sync_terrain_viewer);
}

fn sync_terrain_viewer(
    cameras: Query<&GlobalTransform, With<MainCamera>>,
    mut viewer: ResMut<TerrainViewer>,
    mut ocean_viewer: ResMut<OceanViewer>,
) {
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    let position = camera.translation();
    if viewer.position != position {
        viewer.position = position;
    }
    if ocean_viewer.position != position {
        ocean_viewer.position = position;
    }
}
