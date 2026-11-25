use bevy::prelude::*;

use crate::distance_field::{DistanceFieldSource, PrimaryField};
use crate::terrain::TerrainSurface;

pub(super) fn register(app: &mut App) {
    app.add_observer(attach_primary_field);
}

fn attach_primary_field(trigger: On<Add<TerrainSurface>>, mut commands: Commands) {
    commands.entity(trigger.entity).insert((
        PrimaryField,
        DistanceFieldSource {
            clay_alpha: 0.35,
            water_margin: 0.0,
            ..DistanceFieldSource::prop()
        },
    ));
}
