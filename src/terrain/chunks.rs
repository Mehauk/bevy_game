use avian3d::prelude::*;
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use bevy_shader_kit::{WetSandExt, WetSandMaterial, Wetness};

use super::terrain::{Terrain, build_chunk_collider, build_chunk_mesh};

#[derive(Component)]
pub struct TerrainChunk {
    pub coord: IVec2,
}

#[derive(Component)]
pub(super) struct ChunkMeshTask {
    task: Task<Mesh>,
}

#[derive(Resource)]
pub struct TerrainSurfaceMaterial {
    pub material: Handle<WetSandMaterial>,
}

pub(super) fn spawn_terrain_chunks(
    mut commands: Commands,
    mut materials: ResMut<Assets<WetSandMaterial>>,
    terrain: Res<Terrain>,
) {
    let material = materials.add(WetSandMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.28, 0.42, 0.20),
            perceptual_roughness: 0.92,
            reflectance: 0.15,
            ..default()
        },
        extension: WetSandExt {
            wetness: Wetness::default(),
        },
    });
    commands.insert_resource(TerrainSurfaceMaterial {
        material: material.clone(),
    });

    let sampler = *terrain;
    for coord in sampler.chunk_coords() {
        let center = sampler.chunk_center(coord);
        let task =
            AsyncComputeTaskPool::get().spawn(async move { build_chunk_mesh(&sampler, coord) });
        commands.spawn((
            Name::new(format!("Terrain Chunk {} {}", coord.x, coord.y)),
            TerrainChunk { coord },
            Transform::from_xyz(center.x, 0.0, center.y),
            RigidBody::Static,
            Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
            build_chunk_collider(&sampler, coord),
            ChunkMeshTask { task },
        ));
    }
}

pub(super) fn attach_finished_chunk_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    surface: Res<TerrainSurfaceMaterial>,
    mut pending: Query<(Entity, &mut ChunkMeshTask)>,
) {
    for (entity, mut pending) in &mut pending {
        let Some(mesh) = finished_mesh(&mut pending.task) else {
            continue;
        };
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
        return Some(format!("{total}/{total} built"));
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

    fn count<T: Component>(world: &World) -> usize {
        world
            .iter_entities()
            .filter(|entity| entity.get::<T>().is_some())
            .count()
    }

    #[test]
    fn every_chunk_gets_its_collider_at_startup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Terrain>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WetSandMaterial>>()
            .add_systems(Startup, spawn_terrain_chunks);
        app.update();

        assert_eq!(count::<TerrainChunk>(app.world()), 25);
        assert_eq!(count::<Collider>(app.world()), 25);
        assert_eq!(count::<Mesh3d>(app.world()), 0);
        assert!(app.world().contains_resource::<TerrainSurfaceMaterial>());
    }

    #[test]
    fn chunk_meshes_arrive_from_the_task_pool() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Terrain>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WetSandMaterial>>()
            .add_systems(Startup, spawn_terrain_chunks)
            .add_systems(Update, attach_finished_chunk_meshes);
        app.update();

        for _ in 0..200 {
            app.update();
            if count::<Mesh3d>(app.world()) == 25 {
                break;
            }
        }
        assert_eq!(count::<Mesh3d>(app.world()), 25);
        assert_eq!(count::<ChunkMeshTask>(app.world()), 0);
        assert_eq!(count::<MeshMaterial3d<WetSandMaterial>>(app.world()), 25);
    }
}
