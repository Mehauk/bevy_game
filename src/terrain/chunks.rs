use std::collections::{BTreeMap, HashMap};

use avian3d::prelude::*;
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use bevy_shader_kit::{WetSandExt, WetSandMaterial};

use super::terrain::{StitchFlags, Terrain, build_chunk_collider, build_chunk_mesh};

const UNBUILT: u8 = u8::MAX;
const NEIGHBOUR_OFFSETS: [IVec2; 4] = [
    IVec2::new(0, 1),
    IVec2::new(0, -1),
    IVec2::new(1, 0),
    IVec2::new(-1, 0),
];

#[derive(Resource, Default)]
pub struct TerrainViewer {
    pub position: Vec3,
}

#[derive(Component)]
pub struct TerrainChunk {
    pub coord: IVec2,
    pub lod: u8,
    pub flags: StitchFlags,
}

impl TerrainChunk {
    pub fn built_lod(&self) -> Option<u8> {
        (self.lod != UNBUILT).then_some(self.lod)
    }
}

#[derive(Component)]
pub(super) struct ChunkMeshTask {
    task: Task<Mesh>,
    lod: u8,
    flags: StitchFlags,
}

#[derive(Resource)]
pub struct TerrainSurfaceMaterial {
    pub material: Handle<WetSandMaterial>,
}

fn viewer_distance(viewer: &TerrainViewer, point: Vec3) -> f32 {
    let delta = viewer.position - point;
    Vec2::new(delta.x, delta.z).length()
}

pub(super) fn spawn_terrain_chunks(
    mut commands: Commands,
    mut materials: ResMut<Assets<WetSandMaterial>>,
    terrain: Res<Terrain>,
    viewer: Res<TerrainViewer>,
) {
    let material = materials.add(WetSandMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.28, 0.42, 0.20),
            perceptual_roughness: 0.92,
            reflectance: 0.15,
            ..default()
        },
        extension: WetSandExt::default(),
    });
    commands.insert_resource(TerrainSurfaceMaterial {
        material: material.clone(),
    });

    let sampler = *terrain;
    let radius = terrain.config().collider_radius;
    for coord in sampler.chunk_coords() {
        let center = sampler.chunk_center(coord);
        let position = Vec3::new(center.x, 0.0, center.y);
        let mut entity = commands.spawn((
            Name::new(format!("Terrain Chunk {} {}", coord.x, coord.y)),
            TerrainChunk {
                coord,
                lod: UNBUILT,
                flags: StitchFlags::NONE,
            },
            Transform::from_xyz(center.x, 0.0, center.y),
            RigidBody::Static,
            Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
        ));
        if viewer_distance(&viewer, position) <= radius {
            entity.insert(build_chunk_collider(&sampler, coord));
        }
    }
}

pub(super) fn update_chunk_lods(
    mut commands: Commands,
    terrain: Res<Terrain>,
    viewer: Res<TerrainViewer>,
    chunks: Query<(Entity, &TerrainChunk, Option<&ChunkMeshTask>)>,
) {
    let sampler = *terrain;
    let mut desired: HashMap<IVec2, u8> = HashMap::new();
    for (_, chunk, _) in chunks.iter() {
        let center = sampler.chunk_center(chunk.coord);
        let distance = viewer_distance(&viewer, Vec3::new(center.x, 0.0, center.y));
        let lod = if chunk.lod == UNBUILT {
            sampler.raw_lod(distance)
        } else {
            sampler.lod_for_distance(distance, chunk.lod)
        };
        desired.insert(chunk.coord, lod);
    }
    relax_lods(&mut desired);

    for (entity, chunk, pending) in &chunks {
        let Some(&lod) = desired.get(&chunk.coord) else {
            continue;
        };
        let flags = stitch_flags(&desired, chunk.coord, lod);
        if chunk.lod == lod && chunk.flags == flags {
            continue;
        }
        if pending.is_some_and(|task| task.lod == lod && task.flags == flags) {
            continue;
        }
        let coord = chunk.coord;
        let task = AsyncComputeTaskPool::get()
            .spawn(async move { build_chunk_mesh(&sampler, coord, lod, flags) });
        commands.entity(entity).insert(ChunkMeshTask {
            task,
            lod,
            flags,
        });
    }
}

fn relax_lods(desired: &mut HashMap<IVec2, u8>) {
    let coords: Vec<IVec2> = desired.keys().copied().collect();
    loop {
        let mut changed = false;
        for coord in &coords {
            let lod = desired[coord];
            for offset in NEIGHBOUR_OFFSETS {
                if let Some(neighbor) = desired.get_mut(&(*coord + offset)) {
                    if *neighbor > lod + 1 {
                        *neighbor = lod + 1;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            return;
        }
    }
}

fn stitch_flags(desired: &HashMap<IVec2, u8>, coord: IVec2, lod: u8) -> StitchFlags {
    let coarser = |offset: IVec2| {
        desired
            .get(&(coord + offset))
            .is_some_and(|neighbor| *neighbor > lod)
    };
    let mut flags = StitchFlags::NONE;
    if coarser(IVec2::new(0, 1)) {
        flags = flags.with_north();
    }
    if coarser(IVec2::new(0, -1)) {
        flags = flags.with_south();
    }
    if coarser(IVec2::new(1, 0)) {
        flags = flags.with_east();
    }
    if coarser(IVec2::new(-1, 0)) {
        flags = flags.with_west();
    }
    flags
}

pub(super) fn sync_chunk_colliders(
    terrain: Res<Terrain>,
    viewer: Res<TerrainViewer>,
    mut commands: Commands,
    chunks: Query<(Entity, &TerrainChunk, &Transform, Has<Collider>)>,
) {
    let radius = terrain.config().collider_radius;
    let detach_beyond = radius + terrain.chunk_extent().max_element();
    for (entity, chunk, transform, has_collider) in &chunks {
        let distance = viewer_distance(&viewer, transform.translation);
        if !has_collider && distance <= radius {
            commands.entity(entity).insert(build_chunk_collider(&terrain, chunk.coord));
        } else if has_collider && distance > detach_beyond {
            commands.entity(entity).remove::<Collider>();
        }
    }
}

pub(super) fn attach_finished_chunk_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    surface: Res<TerrainSurfaceMaterial>,
    mut pending: Query<(Entity, &mut ChunkMeshTask, &mut TerrainChunk)>,
) {
    for (entity, mut pending, mut chunk) in &mut pending {
        let Some(mesh) = finished_mesh(&mut pending.task) else {
            continue;
        };
        chunk.lod = pending.lod;
        chunk.flags = pending.flags;
        commands.entity(entity).remove::<ChunkMeshTask>().insert((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surface.material.clone()),
        ));
    }
}

fn finished_mesh(task: &mut Task<Mesh>) -> Option<Mesh> {
    block_on(poll_once(task))
}

pub(super) fn chunk_debug_info(
    chunks: Query<&TerrainChunk>,
    pending: Query<&TerrainChunk, With<ChunkMeshTask>>,
) -> Option<String> {
    let total = chunks.iter().count();
    if total == 0 {
        return None;
    }
    let waiting = pending.iter().count();
    if waiting == 0 {
        let mut levels: BTreeMap<u8, usize> = BTreeMap::new();
        for chunk in chunks.iter() {
            if chunk.lod != UNBUILT {
                *levels.entry(chunk.lod).or_default() += 1;
            }
        }
        let spread: Vec<String> = levels
            .iter()
            .map(|(lod, count)| format!("L{lod}:{count}"))
            .collect();
        return Some(format!("{total}/{total} built {}", spread.join(" ")));
    }
    let coords: Vec<String> = pending
        .iter()
        .take(3)
        .map(|chunk| format!("{}:{}", chunk.coord.x, chunk.coord.y))
        .collect();
    Some(format!(
        "{}/{} building {}",
        total - waiting,
        total,
        coords.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::terrain::TerrainConfig;

    fn count<T: Component>(world: &World) -> usize {
        world
            .iter_entities()
            .filter(|entity| entity.get::<T>().is_some())
            .count()
    }

    fn lod_app() -> App {
        let terrain = Terrain(TerrainConfig {
            world: Vec2::splat(300.0),
            ..default()
        });
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(terrain)
            .init_resource::<TerrainViewer>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WetSandMaterial>>()
            .add_systems(Startup, spawn_terrain_chunks)
            .add_systems(
                Update,
                (update_chunk_lods, sync_chunk_colliders, attach_finished_chunk_meshes).chain(),
            );
        app
    }

    fn settle(app: &mut App) {
        for _ in 0..200 {
            app.update();
            if count::<Mesh3d>(app.world()) == 25 && count::<ChunkMeshTask>(app.world()) == 0 {
                break;
            }
        }
    }

    fn chunk_lod(app: &App, coord: IVec2) -> u8 {
        app.world()
            .iter_entities()
            .filter_map(|entity| entity.get::<TerrainChunk>())
            .find(|chunk| chunk.coord == coord)
            .expect("chunk exists")
            .lod
    }

    #[test]
    fn every_chunk_gets_a_collider_only_near_the_viewer() {
        let mut app = lod_app();
        app.update();

        assert_eq!(count::<TerrainChunk>(app.world()), 25);
        assert_eq!(
            count::<Collider>(app.world()),
            13,
            "the viewer at the origin keeps colliders within two chunks"
        );
        assert_eq!(
            count::<Mesh3d>(app.world()) + count::<ChunkMeshTask>(app.world()),
            25,
            "every chunk has either a finished mesh or a pending build, never neither"
        );
        assert!(app.world().contains_resource::<TerrainSurfaceMaterial>());
    }

    #[test]
    fn chunk_meshes_arrive_from_the_task_pool() {
        let mut app = lod_app();
        app.update();
        settle(&mut app);

        assert_eq!(count::<Mesh3d>(app.world()), 25);
        assert_eq!(count::<ChunkMeshTask>(app.world()), 0);
        assert_eq!(count::<MeshMaterial3d<WetSandMaterial>>(app.world()), 25);
    }

    #[test]
    fn distant_chunks_build_coarser_than_near_chunks() {
        let mut app = lod_app();
        app.update();
        settle(&mut app);

        assert_eq!(chunk_lod(&app, IVec2::new(2, 2)), 0);
        assert_eq!(chunk_lod(&app, IVec2::new(0, 0)), 2);
        for chunk in app
            .world()
            .iter_entities()
            .filter_map(|entity| entity.get::<TerrainChunk>())
        {
            assert!(chunk.lod <= 3);
        }
    }

    #[test]
    fn neighbouring_chunks_never_differ_by_more_than_one_lod() {
        let mut app = lod_app();
        app.update();
        settle(&mut app);

        let lods: HashMap<IVec2, u8> = app
            .world()
            .iter_entities()
            .filter_map(|entity| entity.get::<TerrainChunk>())
            .map(|chunk| (chunk.coord, chunk.lod))
            .collect();
        for (coord, lod) in &lods {
            for offset in NEIGHBOUR_OFFSETS {
                if let Some(neighbor) = lods.get(&(*coord + offset)) {
                    assert!(
                        (*lod as i32 - *neighbor as i32).abs() <= 1,
                        "chunk {coord} at lod {lod} touches {offset} at lod {neighbor}"
                    );
                }
            }
        }
    }

    #[test]
    fn colliders_follow_the_viewer() {
        let mut app = lod_app();
        app.update();

        let corner = IVec2::new(4, 4);
        app.world_mut()
            .resource_mut::<TerrainViewer>()
            .position = Vec3::new(150.0, 1.0, 150.0);
        for _ in 0..4 {
            app.update();
        }

        let has_collider = |app: &App, coord: IVec2| -> bool {
            app.world()
                .iter_entities()
                .filter_map(|entity| {
                    let chunk = entity.get::<TerrainChunk>()?;
                    chunk.coord.eq(&coord).then(|| entity.get::<Collider>())?
                })
                .count()
                > 0
        };
        assert!(has_collider(&app, corner), "the far corner gained a collider");
        assert!(
            !has_collider(&app, IVec2::new(2, 2)),
            "the origin chunk lost its collider"
        );
    }
}
