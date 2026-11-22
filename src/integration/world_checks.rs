#[cfg(test)]
mod tests {
    use std::time::Duration;

    use avian3d::prelude::*;
    use bevy::asset::AssetApp;
    use bevy::prelude::*;
    use bevy::time::TimeUpdateStrategy;

    use crate::character::{Character, GroundState, character_ground_check};
    use crate::terrain::chunk::{CHUNK_MESH_STEPS, DEFAULT_CHUNK_SIZE, chunk_heightfield};
    use crate::terrain::{TerrainConfig, TerrainMap};
    use crate::terrain::topology::{WorldTopology, wrapped_copies};

    use super::super::world_wrap::wrap_character_to_world;

    #[derive(Resource)]
    struct ProbeRequest {
        x: f32,
        z: f32,
        top: f32,
        exclude: Option<Entity>,
    }

    #[derive(Resource, Default)]
    struct ProbeResult(Option<f32>);

    fn run_probe(
        spatial: SpatialQuery,
        request: Res<ProbeRequest>,
        mut result: ResMut<ProbeResult>,
    ) {
        let probe = Collider::sphere(0.35);
        let config = ShapeCastConfig::from_max_distance(request.top + 200.0);
        let filter = match request.exclude {
            Some(entity) => SpatialQueryFilter::from_excluded_entities([entity]),
            None => SpatialQueryFilter::default(),
        };
        result.0 = spatial
            .cast_shape(
                &probe,
                Vec3::new(request.x, request.top, request.z),
                Quat::IDENTITY,
                Dir3::NEG_Y,
                &config,
                &filter,
            )
            .map(|hit| hit.point1.y);
    }

    fn walk_config(topology: WorldTopology) -> TerrainConfig {
        TerrainConfig {
            size: 400.0,
            subdivisions: 64,
            cell_grid_dim: 6,
            topology,
            ..TerrainConfig::compact()
        }
    }

    fn physics_app(map: TerrainMap) -> App {
        let mut app = App::new();
        app.add_plugins((
                MinimalPlugins,
                bevy::asset::AssetPlugin::default(),
                PhysicsPlugins::default(),
            ))
            .init_asset::<Mesh>()
            .add_message::<bevy::asset::AssetEvent<Mesh>>()
            .insert_resource(map)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                1.0 / 60.0,
            )))
            .init_resource::<ProbeResult>()
            .insert_resource(ProbeRequest {
                x: 0.0,
                z: 0.0,
                top: 0.0,
                exclude: None,
            })
            .add_systems(Update, run_probe);
        app.finish();
        app.cleanup();
        app
    }

    fn spawn_ground(app: &mut App) {
        let map = app.world().resource::<TerrainMap>().clone();
        let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
        for key in map.chunk_keys(chunk_size) {
            let (low, high) = map.chunk_bounds(key, chunk_size);
            let centre = (low + high) * 0.5;
            let collider = chunk_heightfield(&map, key, chunk_size, CHUNK_MESH_STEPS);
            for offset in wrapped_copies(map.topology(), map.size()) {
                app.world_mut().spawn((
                    RigidBody::Static,
                    Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
                    Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
                    Transform::from_xyz(centre.x + offset.x, 0.0, centre.y + offset.y),
                    collider.clone(),
                ));
            }
        }
    }

    fn probe(app: &mut App, x: f32, z: f32, top: f32) -> Option<f32> {
        *app.world_mut().resource_mut::<ProbeRequest>() = ProbeRequest {
            x,
            z,
            top,
            exclude: None,
        };
        app.update();
        app.world().resource::<ProbeResult>().0
    }

    #[test]
    fn the_spawn_floor_is_dry_in_every_finite_topology() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let map = TerrainMap::new(TerrainConfig::compact().with_topology(topology));
            let ground = map.height_at(0.0, 0.0);
            assert!(
                ground >= map.sea_level() - 0.05,
                "{topology:?}: spawn ground {ground} sits under sea level {}",
                map.sea_level()
            );
            let picked = crate::terrain::pick_spawn_height(&map);
            assert!(picked.is_finite(), "{topology:?}: picked spawn must be finite");
            assert!(
                picked >= map.sea_level(),
                "{topology:?}: picked spawn {picked} must be dry (sea level {})",
                map.sea_level()
            );
            assert!(
                picked >= map.heights().min(),
                "{topology:?}: picked spawn {picked} must sit above the seabed {}",
                map.heights().min()
            );
        }
    }

    #[test]
    fn a_character_falls_when_there_is_no_ground_beneath() {
        let map = TerrainMap::new(walk_config(WorldTopology::Finite));
        let mut app = physics_app(map.clone());
        let entity = spawn_walker(&mut app, 0.0, 0.0);
        app.add_systems(
            FixedUpdate,
            (character_ground_check, drive_walker).chain(),
        );
        let start_y = app.world().get::<Transform>(entity).unwrap().translation.y;
        let mut grounded = 0;
        for _ in 0..60 {
            app.update();
            if app.world().get::<GroundState>(entity).unwrap().is_grounded {
                grounded += 1;
            }
        }
        let end_y = app.world().get::<Transform>(entity).unwrap().translation.y;
        assert_eq!(grounded, 0, "with no collider beneath, the walker is never grounded");
        assert!(
            end_y < start_y - 1.0,
            "the walker must fall through the missing terrain ({start_y} -> {end_y})"
        );
    }

    #[test]
    fn a_character_below_the_world_floor_is_respawned() {
        let map = TerrainMap::new(walk_config(WorldTopology::Finite));
        let mut app = physics_app(map.clone());
        app.insert_resource(crate::utils::SpawnGroundHeight(0.0));
        let entity = app
            .world_mut()
            .spawn((
                Character::default(),
                GroundState::default(),
                RigidBody::Dynamic,
                Collider::capsule(0.4, 1.0),
                LinearVelocity::ZERO,
                GravityScale(9.81),
                Transform::from_xyz(0.0, map.heights().min() - 200.0, 0.0),
            ))
            .id();
        app.add_systems(FixedFirst, super::super::bridges::character_kill_plane);
        app.update();
        app.update();
        let y = app.world().get::<Transform>(entity).unwrap().translation.y;
        assert!(
            y > map.heights().min(),
            "the kill plane must respawn the character above the floor, got {y}"
        );
    }

    #[test]
    fn a_camera_probe_never_clips_through_chunk_seams() {
        for topology in [WorldTopology::Finite, WorldTopology::FiniteTorus] {
            let map = TerrainMap::new(TerrainConfig::compact().with_topology(topology));
            let mut app = physics_app(map.clone());
            spawn_ground(&mut app);
            for _ in 0..4 {
                app.update();
            }
            let size = map.size();
            let top = map.heights().max() + 60.0;
            let mut checked = 0;
            for i in 0..9 {
                let x = -size * 0.5 + size * (i + 1) as f32 / 10.0;
                for z in [-size * 0.3, 0.0, size * 0.3] {
                    let hit = probe(&mut app, x, z, top)
                        .unwrap_or_else(|| panic!("{topology:?}: probe at ({x}, {z}) fell through"));
                    let expected = map.height_at(x, z);
                    assert!(
                        (hit - expected).abs() < 2.0,
                        "{topology:?}: probe at ({x}, {z}) hit {hit} but the map says {expected}"
                    );
                    checked += 1;
                }
            }
            assert!(checked == 27);
        }
    }

    #[test]
    fn a_camera_probe_sees_the_same_world_across_the_torus_seam() {
        let map = TerrainMap::new(TerrainConfig::compact().with_topology(WorldTopology::FiniteTorus));
        let mut app = physics_app(map.clone());
        spawn_ground(&mut app);
        for _ in 0..4 {
            app.update();
        }
        let size = map.size();
        let top = map.heights().max() + 60.0;
        let mut water_checked = 0;
        for i in 0..9 {
            let x = -size * 0.5 + size * (i + 1) as f32 / 10.0;
            for z in [-size * 0.3, 0.0, size * 0.3] {
                let here = probe(&mut app, x, z, top).expect("canonical probe must hit");
                let copy = probe(&mut app, x + size, z, top)
                    .unwrap_or_else(|| panic!("wrapped probe at ({}, {z}) fell through", x + size));
                assert!(
                    (here - copy).abs() < 1e-3,
                    "torus seam discontinuity at ({x}, {z}): {here} vs {copy}"
                );
                if let Some(sample) = map.water_surface_at(x, z)
                    && !sample.frozen
                    && sample.surface_y > map.height_at(x, z) + 0.5
                {
                    assert!(
                        here <= sample.surface_y + 0.4,
                        "the ground probe at ({x}, {z}) hit {here}, above the open water surface {}",
                        sample.surface_y
                    );
                    water_checked += 1;
                }
            }
        }
        assert!(water_checked > 0, "the compact torus must have open water to probe");
    }

    fn drive_walker(mut walkers: Query<(&mut LinearVelocity, &GroundState)>) {
        for (mut velocity, ground) in &mut walkers {
            velocity.x = 4.0;
            if ground.is_grounded {
                let gap = ground.ground_distance;
                if gap > 0.005 {
                    velocity.y = -((gap - 0.005) * 10.0).min(4.0);
                } else if velocity.y < 0.0 {
                    velocity.y = 0.0;
                }
            }
        }
    }

    fn spawn_walker(app: &mut App, x: f32, z: f32) -> Entity {
        let map = app.world().resource::<TerrainMap>().clone();
        let ground = map.height_at(x, z);
        let entity = app
            .world_mut()
            .spawn((
                Character::default(),
                GroundState::default(),
                RigidBody::Dynamic,
                Collider::capsule(0.4, 1.0),
                LockedAxes::ROTATION_LOCKED,
                Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
                Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
                LinearVelocity::ZERO,
                GravityScale(9.81),
                Transform::from_xyz(x, ground + 1.8, z),
            ))
            .id();
        app.world_mut().entity_mut(entity).insert(
            ShapeCaster::new(
                Collider::sphere(0.35),
                Vec3::new(0.0, -0.5, 0.0),
                Quat::IDENTITY,
                Dir3::NEG_Y,
            )
            .with_max_distance(0.5)
            .with_query_filter(SpatialQueryFilter::from_excluded_entities([entity])),
        );
        entity
    }

    fn dry_lane(map: &TerrainMap, x: f32, reach: f32) -> f32 {
        let size = map.size();
        let mut best: Option<(f32, f32)> = None;
        let mut z = -size * 0.45;
        while z <= size * 0.45 {
            let mut worst_slope = 0.0f32;
            let mut lowest = f32::MAX;
            let mut dry = true;
            let mut s = x - reach;
            while s <= x + reach {
                let h = map.height_at(s, z);
                if h <= map.sea_level() + 0.2 {
                    dry = false;
                    break;
                }
                lowest = lowest.min(h);
                worst_slope = worst_slope.max(map.slope_at(s, z));
                s += 1.0;
            }
            if dry {
                let score = lowest * 0.1 - worst_slope * 40.0;
                if best.is_none_or(|(previous, _)| score > previous) {
                    best = Some((score, z));
                }
            }
            z += 1.0;
        }
        best.unwrap_or_else(|| panic!("no dry gentle lane across x = {x} in this world"))
            .1
    }

    #[test]
    fn walking_across_an_internal_chunk_boundary_keeps_ground_contact() {
        let map = TerrainMap::new(walk_config(WorldTopology::Finite));
        let mut app = physics_app(map.clone());
        spawn_ground(&mut app);
        let boundary = 0.0;
        let z = dry_lane(&map, boundary, 8.0);
        let entity = spawn_walker(&mut app, boundary - 8.0, z);
        app.add_systems(
            FixedUpdate,
            (character_ground_check, drive_walker).chain(),
        );
        let mut grounded = 0;
        let mut measured = 0;
        let mut worst_penetration = 0.0f32;
        let mut worst_sink = 0.0f32;
        for step in 0..300 {
            let before = app.world().get::<Transform>(entity).unwrap().translation;
            *app.world_mut().resource_mut::<ProbeRequest>() = ProbeRequest {
                x: before.x,
                z: before.z,
                top: before.y + 2.0,
                exclude: Some(entity),
            };
            app.update();
            if step < 40 {
                continue;
            }
            measured += 1;
            let state = app.world().get::<GroundState>(entity).unwrap();
            if state.is_grounded {
                grounded += 1;
            }
            let transform = app.world().get::<Transform>(entity).unwrap();
            let expected = map.height_at(transform.translation.x, transform.translation.z);
            worst_sink = worst_sink.max(expected - (transform.translation.y - 1.4));
            if let Some(surface) = app.world().resource::<ProbeResult>().0 {
                worst_penetration =
                    worst_penetration.max(surface - (transform.translation.y - 0.9));
            }
        }
        let final_x = app.world().get::<Transform>(entity).unwrap().translation.x;
        assert!(
            final_x > boundary + 4.0,
            "walker never crossed the chunk boundary (ended at {final_x})"
        );
        assert!(
            grounded as f32 / measured as f32 > 0.9,
            "walker lost ground contact on {}% of frames",
            100 - 100 * grounded / measured
        );
        assert!(
            worst_penetration < 0.5,
            "walker sank {worst_penetration} m into the collider surface"
        );
        assert!(
            worst_sink < 1.5,
            "the collider floor diverged from the map by {worst_sink} m"
        );
    }

    #[test]
    fn walking_over_the_torus_seam_wraps_and_keeps_ground_contact() {
        let map = TerrainMap::new(TerrainConfig::compact().with_topology(WorldTopology::FiniteTorus));
        let mut app = physics_app(map.clone());
        spawn_ground(&mut app);
        let size = map.size();
        let seam = size * 0.5;
        let z = dry_lane(&map, seam, 8.0);
        let entity = spawn_walker(&mut app, seam - 8.0, z);
        app.add_systems(
            FixedFirst,
            wrap_character_to_world
                .after(TransformEasingSystems::Reset)
                .before(TransformEasingSystems::UpdateStart),
        );
        app.add_systems(
            FixedUpdate,
            (character_ground_check, drive_walker).chain(),
        );
        let mut grounded = 0;
        let mut measured = 0;
        let mut worst_penetration = 0.0f32;
        let mut worst_sink = 0.0f32;
        for step in 0..300 {
            let before = app.world().get::<Transform>(entity).unwrap().translation;
            *app.world_mut().resource_mut::<ProbeRequest>() = ProbeRequest {
                x: before.x,
                z: before.z,
                top: before.y + 2.0,
                exclude: Some(entity),
            };
            app.update();
            if step < 40 {
                continue;
            }
            measured += 1;
            let state = app.world().get::<GroundState>(entity).unwrap();
            if state.is_grounded {
                grounded += 1;
            }
            let transform = app.world().get::<Transform>(entity).unwrap();
            assert!(
                transform.translation.x >= -seam - 0.25 && transform.translation.x < seam + 0.25,
                "character escaped the wrapped world at x = {}",
                transform.translation.x
            );
            let expected = map.height_at(transform.translation.x, transform.translation.z);
            worst_sink = worst_sink.max(expected - (transform.translation.y - 1.4));
            if let Some(surface) = app.world().resource::<ProbeResult>().0 {
                worst_penetration =
                    worst_penetration.max(surface - (transform.translation.y - 0.9));
            }
        }
        let final_x = app.world().get::<Transform>(entity).unwrap().translation.x;
        assert!(
            final_x < 0.0,
            "character never crossed the torus seam (ended at {final_x})"
        );
        assert!(
            grounded as f32 / measured as f32 > 0.9,
            "walker lost ground contact on {}% of frames",
            100 - 100 * grounded / measured
        );
        assert!(
            worst_penetration < 0.5,
            "walker sank {worst_penetration} m into the collider surface at the seam"
        );
        assert!(
            worst_sink < 1.5,
            "the wrapped collider floor diverged from the map by {worst_sink} m"
        );
    }

    #[test]
    fn easing_does_not_smear_the_character_across_the_torus_seam() {
        let map = TerrainMap::new(TerrainConfig::compact().with_topology(WorldTopology::FiniteTorus));
        let mut app = physics_app(map.clone());
        spawn_ground(&mut app);
        let size = map.size();
        let seam = size * 0.5;
        let z = dry_lane(&map, seam, 8.0);
        let entity = spawn_walker(&mut app, seam - 8.0, z);
        app.world_mut()
            .entity_mut(entity)
            .insert(TransformInterpolation);
        app.add_systems(
            FixedFirst,
            wrap_character_to_world
                .after(TransformEasingSystems::Reset)
                .before(TransformEasingSystems::UpdateStart),
        );
        app.add_systems(
            FixedUpdate,
            (character_ground_check, drive_walker).chain(),
        );
        let mut crossed = false;
        for _ in 0..300 {
            app.update();
            let transform = app.world().get::<Transform>(entity).unwrap().translation;
            let position = app.world().get::<Position>(entity).unwrap().0;
            assert!(
                transform.x >= -seam - 0.25 && transform.x <= seam + 0.25,
                "the eased transform escaped the wrapped world at x = {}",
                transform.x
            );
            assert!(
                (transform.x - position.x).abs() < 4.0,
                "the eased transform lagged physics by {:.1} m, a seam smear",
                (transform.x - position.x).abs()
            );
            if transform.x < 0.0 {
                crossed = true;
            }
        }
        assert!(crossed, "the walker must cross the seam for this test to mean anything");
    }
}
