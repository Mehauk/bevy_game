use bevy::prelude::*;
use bevy_shader_kit::{MAX_WATER_FIELDS, SdfVolume, WaterMaterial};

use crate::distance_field::{DistanceFieldSource, PrimaryField};
use crate::terrain::{TerrainSurface, WaterHeightSource, WaterSurface};

pub(super) fn register(app: &mut App) {
    app.add_observer(attach_primary_field)
        .add_systems(Update, sync_water_fields);
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

fn field_params(global: &GlobalTransform, source: &DistanceFieldSource) -> SdfVolume {
    SdfVolume {
        field_from_world: global.to_matrix().inverse(),
        ..source.volume
    }
}

fn chosen_fields(
    sources: &Query<(&GlobalTransform, &DistanceFieldSource, Has<PrimaryField>)>,
    water_height: f32,
) -> Vec<(Handle<Image>, SdfVolume)> {
    let mut chosen: Vec<(Handle<Image>, SdfVolume)> = Vec::with_capacity(MAX_WATER_FIELDS);
    for primary in [true, false] {
        for (global, source, has_primary) in sources.iter() {
            let matches = source.baked
                && source.surface_top >= water_height
                && has_primary == primary
                && chosen.len() < MAX_WATER_FIELDS;
            if matches {
                chosen.push((source.image.clone(), field_params(global, source)));
            }
        }
    }
    chosen
}

fn sync_water_fields(
    time: Res<Time>,
    heights: Query<&Transform, With<WaterHeightSource>>,
    sources: Query<(&GlobalTransform, &DistanceFieldSource, Has<PrimaryField>)>,
    surface: Option<Res<WaterSurface>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let Some(surface) = surface else {
        return;
    };
    let Some(mut material) = materials.get_mut(&surface.material) else {
        return;
    };
    let mut params = material.extension.params;
    params.time = time.elapsed_secs();
    if let Some(transform) = heights.iter().next() {
        params.water_height = transform.translation.y;
    }
    let chosen = chosen_fields(&sources, params.water_height);
    let ext = &mut material.extension;
    let slots = [
        &mut ext.field0,
        &mut ext.field1,
        &mut ext.field2,
        &mut ext.field3,
    ];
    for (index, slot) in slots.into_iter().enumerate() {
        match chosen.get(index) {
            Some((image, field)) => {
                *slot = image.clone();
                params.fields[index] = *field;
            }
            None => {
                *slot = surface.dummy.clone();
                params.fields[index] = SdfVolume::default();
            }
        }
    }
    params.field_count = chosen.len() as u32;
    ext.params = params;
}
