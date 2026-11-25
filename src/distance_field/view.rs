use bevy::{pbr::StandardMaterial, prelude::*};
use bevy_shader_kit::{DistanceFieldViewMaterial, DistanceFieldViewUniform, SdfVolume};

use super::{
    DimmedColorRestore, DimmedFog, DistanceFieldDimmedColor, DistanceFieldSource,
    DistanceFieldView, DistanceFieldViewHidden, DistanceFieldVolume, InvisibleMaterial,
};

pub fn animate_distance_field(
    time: Res<Time>,
    sources: Query<(&GlobalTransform, &DistanceFieldSource)>,
    mut view_materials: ResMut<Assets<DistanceFieldViewMaterial>>,
) {
    for (global, source) in sources.iter() {
        if !source.baked {
            continue;
        }
        let view = DistanceFieldViewUniform {
            volume: SdfVolume {
                field_from_world: global.to_matrix().inverse(),
                ..source.volume
            },
            time: time.elapsed_secs(),
            glow_range: source.glow_range,
            clay_alpha: source.clay_alpha,
        };
        if let Some(mut material) = view_materials.get_mut(&source.view_material) {
            material.extension.view = view;
            material.extension.sdf = source.image.clone();
        }
    }
}

pub fn sync_distance_field_view(
    view: Res<DistanceFieldView>,
    mut commands: Commands,
    invisible: Option<Res<InvisibleMaterial>>,
    mut sources: Query<(
        Entity,
        &mut DistanceFieldSource,
        Option<&MeshMaterial3d<StandardMaterial>>,
    )>,
    mut marked: Query<(
        Entity,
        &mut Visibility,
        Has<DistanceFieldVolume>,
        Has<DistanceFieldViewHidden>,
    )>,
    dimmable: Query<(
        Entity,
        &MeshMaterial3d<StandardMaterial>,
        &DistanceFieldDimmedColor,
        Option<&DimmedColorRestore>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fogs: Query<(Entity, &mut DistanceFog, Option<&DimmedFog>)>,
) {
    if !view.is_changed() {
        return;
    }
    for (entity, mut source, material) in sources.iter_mut() {
        if view.active {
            if let (Some(invisible), Some(material)) = (invisible.as_deref(), material) {
                source.hidden_material = Some(material.0.clone());
                commands
                    .entity(entity)
                    .insert(MeshMaterial3d(invisible.0.clone()));
            }
        } else if let Some(handle) = source.hidden_material.take() {
            commands.entity(entity).insert(MeshMaterial3d(handle));
        }
    }
    for (_, mut visibility, is_volume, is_hidden) in marked.iter_mut() {
        if is_volume {
            *visibility = if view.active {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        } else if is_hidden {
            *visibility = if view.active {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
    for (entity, material_handle, dimmed, stored) in dimmable.iter() {
        let Some(mut material) = materials.get_mut(material_handle) else {
            continue;
        };
        if view.active {
            if stored.is_none() {
                commands
                    .entity(entity)
                    .insert(DimmedColorRestore(material.base_color));
            }
            material.base_color = dimmed.0;
        } else if let Some(DimmedColorRestore(original)) = stored {
            material.base_color = *original;
            commands.entity(entity).remove::<DimmedColorRestore>();
        }
    }
    for (entity, mut fog, stored) in fogs.iter_mut() {
        if view.active {
            if stored.is_none() {
                commands
                    .entity(entity)
                    .insert(DimmedFog(fog.color.alpha()));
            }
            fog.color.set_alpha(0.0);
        } else if let Some(DimmedFog(original)) = stored {
            fog.color.set_alpha(*original);
            commands.entity(entity).remove::<DimmedFog>();
        }
    }
}
