use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_shader_kit::CoastField;

use super::grid::HeightGrid;
use super::terrain::Terrain;

#[derive(Resource)]
pub struct TerrainHeightMap {
    pub image: Handle<Image>,
    pub field: CoastField,
    pub revision: u64,
}

fn coast_bytes(ground: &[f32], spill: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(ground.len() * 8);
    for index in 0..ground.len() {
        bytes.extend_from_slice(&ground[index].to_le_bytes());
        bytes.extend_from_slice(&spill[index].to_le_bytes());
    }
    bytes
}

pub fn bake_height_map(terrain: &Terrain, images: &mut Assets<Image>) -> TerrainHeightMap {
    let grid = terrain.grid();
    let resolution = grid.resolution();

    let image = images.add(Image::new(
        Extent3d {
            width: resolution.x,
            height: resolution.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        coast_bytes(grid.heights(), grid.spill()),
        TextureFormat::Rg32Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    TerrainHeightMap {
        image,
        field: coast_field(grid),
        revision: terrain.revision(),
    }
}

fn coast_field(grid: &HeightGrid) -> CoastField {
    CoastField {
        bounds_min: grid.bounds_min(),
        texel: grid.texel(),
        inv_size: grid.inv_size(),
        resolution: grid.resolution().as_vec2(),
    }
}

pub(super) fn sync_height_map(
    terrain: Res<Terrain>,
    maps: Option<ResMut<TerrainHeightMap>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut maps) = maps else {
        return;
    };
    if maps.revision == terrain.revision() {
        return;
    }
    let grid = terrain.grid();
    let Some(mut image) = images.get_mut(&maps.image) else {
        return;
    };
    image.data = Some(coast_bytes(grid.heights(), grid.spill()));
    maps.field = coast_field(grid);
    maps.revision = terrain.revision();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::terrain::{Generator, TerrainConfig};

    fn spill_of(terrain: &Terrain, images: &mut Assets<Image>) -> Vec<f32> {
        let maps = bake_height_map(terrain, images);
        let image = images.get(&maps.image).unwrap();
        let bytes = image.data.as_ref().unwrap();
        let count = (maps.field.resolution.x * maps.field.resolution.y) as usize;
        (0..count)
            .map(|i| f32::from_le_bytes(bytes[i * 8 + 4..i * 8 + 8].try_into().unwrap()))
            .collect()
    }

    #[test]
    fn the_flat_plain_spills_from_the_ocean_rim() {
        let terrain = Terrain::default();
        let mut images = Assets::<Image>::default();
        let spill = spill_of(&terrain, &mut images);
        assert!(spill.iter().all(|level| level.is_finite()));
        assert!(spill.iter().all(|level| *level <= 0.0));
    }

    #[test]
    fn sync_height_map_rewrites_the_image_in_place() {
        let terrain = Terrain::default();
        let mut images = Assets::<Image>::default();
        let maps = bake_height_map(&terrain, &mut images);
        let handle = maps.image.clone();
        let before = images.get(&handle).unwrap().data.clone().unwrap();

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(terrain.clone())
            .insert_resource(images)
            .insert_resource(maps)
            .add_systems(Update, sync_height_map);
        app.update();
        assert_eq!(
            app.world().resource::<TerrainHeightMap>().revision,
            0,
            "an up-to-date map is left untouched"
        );

        {
            let mut terrain = app.world_mut().resource_mut::<Terrain>();
            terrain.set_generator(Generator::Noise {
                seed: 9,
                octaves: 4,
                scale: 0.01,
            });
            terrain.regenerate();
        }
        app.update();

        let maps = app.world().resource::<TerrainHeightMap>();
        assert_eq!(maps.revision, 1);
        assert_eq!(maps.image, handle, "the same handle stays bound");
        let images = app.world().resource::<Assets<Image>>();
        assert_eq!(images.len(), 1, "the map is rewritten, not rebaked");
        let after = images.get(&handle).unwrap().data.clone().unwrap();
        assert_eq!(after.len(), before.len());
        assert_ne!(after, before, "the coast bytes changed in place");
    }

    #[test]
    fn a_noise_world_bakes_finite_spill_at_or_above_the_ground() {
        let terrain = Terrain::new(TerrainConfig {
            generator: Generator::Noise {
                seed: 3,
                octaves: 4,
                scale: 0.008,
            },
            ..default()
        });
        let mut images = Assets::<Image>::default();
        let maps = bake_height_map(&terrain, &mut images);
        let image = images.get(&maps.image).unwrap();
        let bytes = image.data.as_ref().unwrap();
        let count = (maps.field.resolution.x * maps.field.resolution.y) as usize;
        for index in 0..count {
            let ground = f32::from_le_bytes(bytes[index * 8..index * 8 + 4].try_into().unwrap());
            let spill = f32::from_le_bytes(bytes[index * 8 + 4..index * 8 + 8].try_into().unwrap());
            assert!(ground.is_finite() && spill.is_finite());
            assert!(spill >= ground, "spill must never sit below its own ground");
        }
    }
}
