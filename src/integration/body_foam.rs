use bevy::prelude::*;
use bevy_shader_kit::{SdfVolume, WaterMaterial};

use world_water::WaterSurface;

use crate::character::CharacterBody;
use crate::distance_field::DistanceFieldSource;

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
    extension.body_volume = SdfVolume {
        field_from_world: global.to_matrix().inverse().into(),
        ..source.volume
    };
    if extension.body_sdf != source.image {
        extension.body_sdf = source.image.clone();
        extension.body_foam = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::pbr::StandardMaterial;
    use bevy_shader_kit::WaterMaterialExt;

    #[test]
    fn the_console_toggle_survives_the_body_sync() {
        let mut water_materials = Assets::<WaterMaterial>::default();
        let handle = water_materials.add(WaterMaterial {
            base: StandardMaterial::default(),
            extension: WaterMaterialExt {
                body_foam: false,
                ..default()
            },
        });
        let mut images = Assets::<Image>::default();
        let sdf = images.add(Image::default());

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(WaterSurface {
                material: handle.clone(),
            })
            .insert_resource(water_materials)
            .insert_resource(images)
            .add_systems(Update, sync_body_sdf);
        app.world_mut().spawn((
            CharacterBody,
            GlobalTransform::default(),
            DistanceFieldSource {
                baked: true,
                image: sdf.clone(),
                ..default()
            },
        ));
        app.update();

        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
        assert_eq!(material.extension.body_sdf, sdf);
        assert!(
            material.extension.body_foam,
            "the first bake enables body foam"
        );

        app.world_mut()
            .resource_mut::<Assets<WaterMaterial>>()
            .get_mut(&handle)
            .unwrap()
            .extension
            .body_foam = false;
        app.update();

        let material = app
            .world()
            .resource::<Assets<WaterMaterial>>()
            .get(&handle)
            .unwrap();
        assert!(
            !material.extension.body_foam,
            "sync_body_sdf must not re-enable body foam while the sdf is unchanged"
        );
    }
}
