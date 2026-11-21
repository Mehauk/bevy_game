use std::sync::Arc;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};

use crate::terrain::chunk::{CHUNK_MESH_STEPS, DEFAULT_CHUNK_SIZE};
use crate::terrain::gpu::{self, WaterRenderMaterial};
use crate::terrain::job::poll_task;
use crate::terrain::map::TerrainMap;
use crate::terrain::params::Salinity;
use crate::terrain::season::{Season, SeasonClock};
use crate::terrain::sky::TerrainAtmosphere;
use crate::terrain::topology::{wrapped_copies, WorldTopology};
use crate::terrain::water::{self, WaterBody};

const TIDE_MARGIN: f32 = 0.8;
const WATER_UPLOAD_BUDGET: usize = 4;

#[derive(Component)]
pub struct WaterSurface;

#[derive(Component)]
pub struct SeaSurface;

#[derive(Resource, Default)]
pub struct WaterJobQueue {
    jobs: Vec<Task<Option<WaterBuild>>>,
}

pub enum WaterBuild {
    Sea {
        mesh: Mesh,
        ice: Option<Collider>,
        frozen: bool,
    },
    Lake {
        anchor: Vec2,
        mesh: Mesh,
        surface: f32,
        salinity: Salinity,
        frozen: bool,
        collider: Option<Collider>,
    },
    River {
        anchor: Vec2,
        mesh: Mesh,
    },
    Cascade {
        offset: Vec2,
        mesh: Mesh,
    },
}

pub fn rebuild_water_surfaces(
    map: Res<TerrainMap>,
    clock: Res<SeasonClock>,
    mut state: Local<Option<(u64, Season)>>,
    mut queue: ResMut<WaterJobQueue>,
    mut commands: Commands,
    existing: Query<Entity, With<WaterSurface>>,
) {
    let key = (map.generation(), clock.season);
    if *state == Some(key) {
        return;
    }
    *state = Some(key);
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    queue.jobs.clear();
    if map.topology() == WorldTopology::Infinite {
        return;
    }

    let shared = Arc::new(map.clone());
    let sea_level = map.sea_level();
    let sea_top = map.sea_levels().max() + TIDE_MARGIN;
    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());

    for chunk in map.chunk_keys(chunk_size) {
        let map = shared.clone();
        queue.jobs.push(AsyncComputeTaskPool::get().spawn(async move {
            let (low, high) = map.chunk_bounds(chunk, chunk_size);
            let centre = (low + high) * 0.5;
            let frozen = map
                .water_surface_at(centre.x, centre.y)
                .map(|sample| sample.frozen)
                .unwrap_or(false);
            let mesh = water::sea_chunk_mesh(
                map.heights(),
                map.ocean_mask(),
                map.water_frozen(),
                sea_level,
                sea_top,
                low,
                high,
                CHUNK_MESH_STEPS,
                false,
            )?;
            let ice = frozen
                .then(|| {
                    water::sea_chunk_mesh(
                        map.heights(),
                        map.ocean_mask(),
                        map.water_frozen(),
                        sea_level,
                        sea_top,
                        low,
                        high,
                        CHUNK_MESH_STEPS,
                        true,
                    )
                    .and_then(|sheet| Collider::trimesh_from_mesh(&sheet))
                })
                .flatten();
            Some(WaterBuild::Sea { mesh, ice, frozen })
        }));
    }

    for (index, lake) in map.lakes().iter().enumerate() {
        if lake.cells.len() < 6 || lake.depth <= 0.05 {
            continue;
        }
        let map = shared.clone();
        let lake = lake.clone();
        let salinity = map
            .lake_salinity()
            .get(index)
            .copied()
            .unwrap_or(Salinity::Freshwater);
        queue.jobs.push(AsyncComputeTaskPool::get().spawn(async move {
            let (fx, fz) = map.heights().coords(lake.cells[0]);
            let focus = map.heights().world_of(fx, fz);
            let frozen = map
                .water_surface_at(focus.x, focus.y)
                .map(|sample| sample.frozen)
                .unwrap_or(false);
            let (anchor, mesh) = water::lake_basin_mesh(&lake, map.heights(), frozen)?;
            let collider = frozen.then(|| Collider::trimesh_from_mesh(&mesh)).flatten();
            Some(WaterBuild::Lake {
                anchor,
                mesh,
                surface: lake.surface,
                salinity,
                frozen,
                collider,
            })
        }));
    }

    for path in map.river_paths() {
        let map = shared.clone();
        let path = path.clone();
        queue.jobs.push(AsyncComputeTaskPool::get().spawn(async move {
            let (anchor, mesh) =
                water::river_ribbon_mesh(&path, map.heights(), map.accumulation_field())?;
            Some(WaterBuild::River { anchor, mesh })
        }));
    }

    let cascade_width = map.heights().spacing() * 1.5;
    for fall in map.waterfalls() {
        let map = shared.clone();
        let fall = fall.clone();
        queue.jobs.push(AsyncComputeTaskPool::get().spawn(async move {
            let mesh = water::cascade_mesh(&fall, map.heights(), cascade_width)?;
            Some(WaterBuild::Cascade {
                offset: Vec2::ZERO,
                mesh,
            })
        }));
    }
}

pub fn drain_water_builds(
    map: Res<TerrainMap>,
    mut queue: ResMut<WaterJobQueue>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WaterRenderMaterial>>,
    mut commands: Commands,
) {
    gpu::set_prevailing_wind(map.prevailing_wind());
    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    let sea_level = map.sea_level();
    let copies = wrapped_copies(map.topology(), map.size());
    let mut applied = 0;
    let mut index = 0;
    while index < queue.jobs.len() && applied < WATER_UPLOAD_BUDGET {
        let completed = poll_task(&mut queue.jobs[index]);
        match completed {
            Some(Some(build)) => {
                applied += 1;
                let _ = queue.jobs.remove(index);
                match build {
                    WaterBuild::Sea { mesh, ice, frozen } => {
                        let body = WaterBody {
                            center: Vec2::ZERO,
                            radius: chunk_size,
                            surface: sea_level,
                            salinity: Salinity::Marine,
                            frozen,
                        };
                        let handle = meshes.add(mesh);
                        let material = materials.add(gpu::water_render_material(
                            water::water_material(&body),
                            water::WaterKind::Sea,
                            frozen,
                            true,
                        ));
                        for offset in &copies {
                            let mut entity = commands.spawn((
                                Name::new("Sea Surface"),
                                WaterSurface,
                                SeaSurface,
                                Transform::from_xyz(offset.x, sea_level, offset.y),
                                Mesh3d(handle.clone()),
                                MeshMaterial3d(material.clone()),
                            ));
                            if let Some(collider) = &ice {
                                entity.insert(collider.clone());
                            }
                        }
                    }
                    WaterBuild::Lake {
                        anchor,
                        mesh,
                        surface,
                        salinity,
                        frozen,
                        collider,
                    } => {
                        let body = WaterBody {
                            center: anchor,
                            radius: 1.0,
                            surface,
                            salinity,
                            frozen,
                        };
                        let handle = meshes.add(mesh);
                        let material = materials.add(gpu::water_render_material(
                            water::water_material(&body),
                            water::WaterKind::Lake,
                            frozen,
                            true,
                        ));
                        for offset in &copies {
                            let mut entity = commands.spawn((
                                Name::new("Lake Surface"),
                                WaterSurface,
                                Transform::from_xyz(anchor.x + offset.x, 0.0, anchor.y + offset.y),
                                Mesh3d(handle.clone()),
                                MeshMaterial3d(material.clone()),
                            ));
                            if let Some(collider) = &collider {
                                entity.insert(collider.clone());
                            }
                        }
                    }
                    WaterBuild::River { anchor, mesh } => {
                        let material = materials.add(gpu::water_render_material(
                            water::river_material(),
                            water::WaterKind::River,
                            false,
                            true,
                        ));
                        let handle = meshes.add(mesh);
                        for offset in &copies {
                            commands.spawn((
                                Name::new("River Ribbon"),
                                WaterSurface,
                                Transform::from_xyz(anchor.x + offset.x, 0.0, anchor.y + offset.y),
                                Mesh3d(handle.clone()),
                                MeshMaterial3d(material.clone()),
                            ));
                        }
                    }
                    WaterBuild::Cascade { offset, mesh } => {
                        let material = materials.add(gpu::water_render_material(
                            water::cascade_material(),
                            water::WaterKind::Waterfall,
                            false,
                            false,
                        ));
                        let handle = meshes.add(mesh);
                        for copy in &copies {
                            commands.spawn((
                                Name::new("Waterfall Cascade"),
                                WaterSurface,
                                Transform::from_xyz(offset.x + copy.x, 0.0, offset.y + copy.y),
                                Mesh3d(handle.clone()),
                                MeshMaterial3d(material.clone()),
                            ));
                        }
                    }
                }
            }
            Some(None) => {
                let _ = queue.jobs.remove(index);
            }
            None => index += 1,
        }
    }
}

pub fn sync_sea_tide(
    map: Res<TerrainMap>,
    mut seas: Query<&mut Transform, With<SeaSurface>>,
) {
    let level = map.sea_level();
    for mut transform in &mut seas {
        transform.translation.y = level;
    }
}

pub fn animate_water_materials(
    time: Res<Time>,
    atmosphere: Res<TerrainAtmosphere>,
    lights: Query<(&DirectionalLight, &GlobalTransform)>,
    mut materials: ResMut<Assets<WaterRenderMaterial>>,
) {
    let (sun_direction, sun_colour, sun_intensity) =
        lights
            .iter()
            .next()
            .map_or((Vec3::new(0.4, 0.7, 0.3).normalize(), [1.0, 0.96, 0.88, 1.0], 1.0),
                |(light, transform)| {
                    (
                        transform.rotation() * Vec3::NEG_Y,
                        light.color.to_linear().to_f32_array(),
                        (light.illuminance / 6000.0).clamp(0.0, 2.0),
                    )
                });
    let zenith = atmosphere.profile.sky_zenith.to_linear().to_f32_array();
    let horizon = atmosphere.profile.sky_horizon.to_linear().to_f32_array();
    for (_, material) in materials.iter_mut() {
        let params = &mut material.extension.params;
        params.time = time.elapsed_secs();
        params.sun_direction = Vec4::new(
            sun_direction.x,
            sun_direction.y,
            sun_direction.z,
            sun_intensity,
        );
        params.sun_color = Vec4::from(sun_colour);
        params.sky_zenith = Vec4::from(zenith);
        params.sky_horizon = Vec4::from(horizon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::chunk::DEFAULT_CHUNK_SIZE;

    #[test]
    fn every_submerged_ocean_vertex_lands_in_a_sea_chunk() {
        let map = TerrainMap::compact();
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        let sea_top = map.sea_levels().max() + TIDE_MARGIN;
        let meshed: Vec<IVec2> = map
            .chunk_keys(chunk_size)
            .into_iter()
            .filter(|&chunk| {
                let (low, high) = map.chunk_bounds(chunk, chunk_size);
                water::sea_chunk_mesh(
                    map.heights(),
                    map.ocean_mask(),
                    map.water_frozen(),
                    map.sea_level(),
                    sea_top,
                    low,
                    high,
                    8,
                    false,
                )
                .is_some()
            })
            .collect();
        assert!(!meshed.is_empty(), "a flooded world must spawn sea chunks");

        let field = map.heights();
        let count = map.chunk_count(chunk_size);
        let half = map.size() * 0.5;
        let mut covered = 0usize;
        for index in 0..field.len() {
            if !map.ocean_mask()[index] || field.get_at(index) > map.sea_level() {
                continue;
            }
            let (x, z) = field.coords(index);
            let point = field.world_of(x, z);
            let key = IVec2::new(
                (((point.x + half) / chunk_size).floor() as i32).clamp(0, count.x - 1),
                (((point.y + half) / chunk_size).floor() as i32).clamp(0, count.y - 1),
            );
            assert!(
                meshed.contains(&key),
                "submerged ocean vertex {index} at {point:?} has no sea chunk"
            );
            covered += 1;
        }
        assert!(covered > 0, "the compact world must have ocean");
    }

    #[test]
    fn wrapped_copies_cover_the_torus_and_nothing_else() {
        use crate::terrain::topology::WorldTopology;
        assert_eq!(wrapped_copies(WorldTopology::Finite, 400.0).len(), 1);
        let torus = wrapped_copies(WorldTopology::FiniteTorus, 400.0);
        assert_eq!(torus.len(), 9);
        assert!(torus.contains(&Vec2::ZERO));
        assert!(torus.contains(&Vec2::new(-400.0, 400.0)));
    }
}
