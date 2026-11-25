use bevy::prelude::*;
use bevy_shader_kit::{SdfVolume, WaterMaterial};

use crate::character::CharacterBody;
use crate::distance_field::DistanceFieldSource;
use crate::terrain::WaterSurface;

pub(super) fn register(app: &mut App) {
    app.add_systems(Update, sync_body_sdf);
}

fn sync_body_sdf(
    surface: Option<Res<WaterSurface>>,
    bodies: Query<(&GlobalTransform, &DistanceFieldSource), With<CharacterBody>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let Some(surface) = surface else {
        return;
    };
    let Some((global, source)) = bodies.iter().find(|(_, source)| source.baked) else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    let extension = &mut material.extension;
    extension.body_foam = true;
    extension.body_volume = SdfVolume {
        field_from_world: global.to_matrix().inverse().into(),
        ..source.volume
    };
    if extension.body_sdf != source.image {
        extension.body_sdf = source.image.clone();
    }
}
