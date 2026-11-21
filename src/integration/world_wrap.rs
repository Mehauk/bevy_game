use avian3d::prelude::TransformEasingSystems;
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::character::Character;
use crate::state::PlayState;
use crate::terrain::TerrainMap;
use crate::terrain::stream::ChunkFocus;

pub(super) fn register(app: &mut App) {
    app.add_systems(
        FixedFirst,
        wrap_character_to_world
            .after(TransformEasingSystems::Reset)
            .before(TransformEasingSystems::UpdateStart)
            .run_if(in_state(PlayState::Running)),
    )
    .add_systems(
        Update,
        sync_chunk_focus.run_if(in_state(PlayState::Running)),
    );
}

pub(crate) fn wrap_character_to_world(
    map: Res<TerrainMap>,
    mut characters: Query<&mut Transform, With<Character>>,
) {
    if !map.topology().wraps() {
        return;
    }
    let size = map.size();
    for mut transform in &mut characters {
        let wrapped = map
            .topology()
            .wrap_xz(Vec2::new(transform.translation.x, transform.translation.z), size);
        transform.translation.x = wrapped.x;
        transform.translation.z = wrapped.y;
    }
}

fn sync_chunk_focus(
    cameras: Query<&Transform, With<MainCamera>>,
    characters: Query<&Transform, With<Character>>,
    mut focus: ResMut<ChunkFocus>,
) {
    if let Ok(camera) = cameras.single() {
        focus.0 = camera.translation;
    } else if let Ok(character) = characters.single() {
        focus.0 = character.translation;
    }
}
