use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, poll_once};

use crate::terrain::map::TerrainMap;
use crate::terrain::pick_spawn_height;
use crate::terrain::season::SeasonClock;
use crate::utils::SpawnGroundHeight;

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
    fn wake_by_ref(self: &Arc<Self>) {}
}

pub(crate) fn poll_task<T>(task: &mut Task<T>) -> Option<T> {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut cx = Context::from_waker(&waker);
    let mut once = poll_once(task);
    match Pin::new(&mut once).poll(&mut cx) {
        Poll::Ready(completed) => completed,
        Poll::Pending => None,
    }
}

#[derive(Default, Resource)]
pub struct TerrainJob {
    queued: bool,
    task: Option<(u64, Task<TerrainMap>)>,
    requested: u64,
}

pub fn run_terrain_job(
    mut job: ResMut<TerrainJob>,
    mut map: ResMut<TerrainMap>,
    clock: Res<SeasonClock>,
    mut spawn: ResMut<SpawnGroundHeight>,
) {
    if job.queued {
        let superseded = job.task.as_ref().is_some_and(|(id, _)| *id != job.requested);
        if job.task.is_none() || superseded {
            job.queued = false;
            let config = map.config().clone();
            let id = job.requested;
            job.task = Some((
                id,
                AsyncComputeTaskPool::get().spawn(async move { TerrainMap::new(config) }),
            ));
            return;
        }
    }
    let delivered = match job.task.as_mut() {
        Some((_, task)) => poll_task(task),
        None => None,
    };
    if let Some(generated) = delivered {
        *map = generated;
        map.sync_season_state(&clock);
        spawn.0 = pick_spawn_height(&map);
        job.task = None;
    }
}

pub fn queue_regeneration(job: &mut TerrainJob) {
    job.requested += 1;
    job.queued = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::map::TerrainConfig;
    use crate::terrain::season::Season;
    use crate::terrain::seed::WorldSeed;

    fn job_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .insert_resource(TerrainMap::new(TerrainConfig::compact()))
            .init_resource::<TerrainJob>()
            .init_resource::<SeasonClock>()
            .init_resource::<SpawnGroundHeight>()
            .add_systems(Update, run_terrain_job);
        app
    }

    fn wait_for_idle(app: &mut App) {
        for _ in 0..1200 {
            app.update();
            if app.world().resource::<TerrainJob>().task.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the generation task never finished");
    }

    #[test]
    fn a_queued_job_delivers_a_regenerated_map() {
        let mut app = job_app();
        app.world_mut()
            .resource_mut::<TerrainMap>()
            .config_mut()
            .seed = WorldSeed::new(123);
        queue_regeneration(&mut app.world_mut().resource_mut::<TerrainJob>());
        wait_for_idle(&mut app);
        assert_eq!(app.world().resource::<TerrainMap>().seed().0, 123);
        assert!(app.world().resource::<TerrainJob>().task.is_none());
    }

    #[test]
    fn a_request_during_a_running_task_supersedes_it() {
        let mut app = job_app();
        app.world_mut()
            .resource_mut::<TerrainMap>()
            .config_mut()
            .seed = WorldSeed::new(111);
        queue_regeneration(&mut app.world_mut().resource_mut::<TerrainJob>());
        app.update();
        assert!(app.world().resource::<TerrainJob>().task.is_some());
        app.world_mut()
            .resource_mut::<TerrainMap>()
            .config_mut()
            .seed = WorldSeed::new(222);
        queue_regeneration(&mut app.world_mut().resource_mut::<TerrainJob>());
        wait_for_idle(&mut app);
        assert_eq!(
            app.world().resource::<TerrainMap>().seed().0,
            222,
            "the latest request must win over the in-flight task"
        );
        assert!(app.world().resource::<TerrainJob>().task.is_none());
    }

    #[test]
    fn a_delivered_map_adopts_the_current_season() {
        let mut app = job_app();
        {
            let mut clock = app.world_mut().resource_mut::<SeasonClock>();
            clock.set_season(Season::Winter);
        }
        queue_regeneration(&mut app.world_mut().resource_mut::<TerrainJob>());
        wait_for_idle(&mut app);
        let map = app.world().resource::<TerrainMap>();
        let expected = map.sea_levels().continuous_level(
            app.world().resource::<SeasonClock>().cycle_progress(),
        );
        assert!(
            (map.sea_level() - expected).abs() < 1e-4,
            "the delivered map must adopt the winter sea level, got {} want {expected}",
            map.sea_level()
        );
    }
}
