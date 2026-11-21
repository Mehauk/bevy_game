use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, poll_once};

use crate::terrain::map::TerrainMap;

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
    task: Option<Task<TerrainMap>>,
}

pub fn run_terrain_job(mut job: ResMut<TerrainJob>, mut map: ResMut<TerrainMap>) {
    if job.queued && job.task.is_none() {
        job.queued = false;
        let config = map.config().clone();
        job.task = Some(AsyncComputeTaskPool::get().spawn(async move {
            TerrainMap::new(config)
        }));
        return;
    }
    let Some(task) = job.task.as_mut() else {
        return;
    };
    if let Some(generated) = poll_task(task) {
        *map = generated;
        job.task = None;
    }
}

pub fn queue_regeneration(job: &mut TerrainJob) {
    job.queued = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::map::TerrainConfig;
    use crate::terrain::seed::WorldSeed;

    fn job_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .insert_resource(TerrainMap::new(TerrainConfig::compact()))
            .init_resource::<TerrainJob>()
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
    fn a_request_during_a_running_task_waits_its_turn() {
        let mut app = job_app();
        queue_regeneration(&mut app.world_mut().resource_mut::<TerrainJob>());
        app.update();
        assert!(app.world().resource::<TerrainJob>().task.is_some());
        queue_regeneration(&mut app.world_mut().resource_mut::<TerrainJob>());
        wait_for_idle(&mut app);
        app.update();
        assert!(
            app.world().resource::<TerrainJob>().task.is_some(),
            "the deferred request must spawn once the first task lands"
        );
        wait_for_idle(&mut app);
    }
}
