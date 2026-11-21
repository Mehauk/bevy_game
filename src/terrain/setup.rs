use bevy::prelude::*;

use crate::terrain::gpu::{self, TileArrays};
use crate::terrain::material;
use crate::terrain::splat::SurfaceTiles;

pub fn load_surface_tiles(mut tiles: ResMut<SurfaceTiles>) {
    let loaded = SurfaceTiles::load(material::DISK_ROOT);
    if !loaded.is_complete() {
        warn!(
            "terrain splat tiles incomplete ({} missing), falling back to single-layer materials",
            loaded.missing().len()
        );
    }
    *tiles = loaded;
}

pub fn build_terrain_tile_arrays(
    tiles: Res<SurfaceTiles>,
    mut images: ResMut<Assets<Image>>,
    mut arrays: ResMut<TileArrays>,
) {
    if let Some(built) = gpu::build_tile_arrays(&tiles, &mut images) {
        *arrays = built;
    } else {
        arrays.fallback = images.add(gpu::fallback_array_image());
        arrays.fallback_2d = images.add(gpu::fallback_2d_image());
        arrays.ready = false;
    }
}

pub fn validate_terrain_textures(mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    let missing = material::validate_on_disk(material::DISK_ROOT);
    if !missing.is_empty() {
        warn!(
            "terrain PBR textures missing ({} files), falling back to flat colours: {:?}",
            missing.len(),
            &missing[..missing.len().min(4)]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::material::DISK_ROOT;
    use bevy::asset::{AssetApp, AssetPlugin};

    #[test]
    fn the_tile_array_build_validates_the_loaded_state_before_uploading() {
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default()).init_asset::<Image>();
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let incomplete = SurfaceTiles::default();
        assert!(
            gpu::build_tile_arrays(&incomplete, &mut images).is_none(),
            "an incomplete load must not produce tile arrays"
        );
        let complete = SurfaceTiles::load(DISK_ROOT);
        assert!(
            complete.is_complete(),
            "the disk set must be complete: {:?}",
            complete.missing()
        );
        let built = gpu::build_tile_arrays(&complete, &mut images).expect("complete set builds");
        assert!(built.ready);
    }

    #[test]
    fn the_27_tiles_are_decoded_exactly_once_per_layer_slot() {
        let tiles = SurfaceTiles::load(DISK_ROOT);
        assert_eq!(
            tiles.loaded_count(),
            SurfaceTiles::expected_tiles(),
            "every layer/slot pair must be present exactly once"
        );
        assert_eq!(SurfaceTiles::expected_tiles(), 27);
    }
}
