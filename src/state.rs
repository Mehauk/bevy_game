use avian3d::prelude::Physics;
use avian3d::schedule::PhysicsTime;
use bevy::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    InGame,
}

#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(GameState = GameState::InGame)]
pub enum PlayState {
    #[default]
    Running,
    Paused,
}

#[derive(Resource, Default)]
pub(crate) struct PauseTracker {
    pub(crate) menu_pauses: u32,
    pub(crate) manual_paused: bool,
    pub(crate) last_reload_gen: u64,
}

impl PauseTracker {
    pub(crate) fn is_paused(&self) -> bool {
        self.menu_pauses > 0 || self.manual_paused
    }

    pub(crate) fn request(&mut self, menu_open: bool) -> bool {
        if menu_open {
            self.menu_pauses += 1;
        } else {
            self.manual_paused = true;
        }
        self.is_paused()
    }

    pub(crate) fn release(&mut self, menu_open: bool) -> bool {
        if menu_open && self.menu_pauses > 0 {
            self.menu_pauses -= 1;
        } else {
            self.manual_paused = false;
            if !menu_open {
                self.menu_pauses = 0;
            }
        }
        !self.is_paused()
    }

    pub(crate) fn sync_menus(&mut self, any_open: bool, reload_gen: u64) -> bool {
        if reload_gen != self.last_reload_gen {
            self.last_reload_gen = reload_gen;
            self.menu_pauses = 0;
        }
        if !any_open {
            self.menu_pauses = 0;
        }
        !any_open && !self.is_paused()
    }
}

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .add_sub_state::<PlayState>()
            .init_resource::<PauseTracker>()
            .add_systems(OnEnter(PlayState::Paused), pause_simulation)
            .add_systems(OnExit(PlayState::Paused), unpause_simulation)
            .add_systems(OnExit(GameState::InGame), clear_pauses);

        NamespaceBuilder::in_namespace(app.world_mut(), "state")
            .register(
                "is_paused",
                |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                    let world = ctx.world()?;
                    world.with_resource(|play_state: &State<PlayState>| {
                        ScriptValue::Bool(*play_state.get() == PlayState::Paused)
                    })
                },
            );
    }
}

fn pause_simulation(
    mut virtual_time: ResMut<Time<Virtual>>,
    physics_time: Option<ResMut<Time<Physics>>>,
) {
    virtual_time.pause();
    if let Some(mut physics) = physics_time {
        physics.pause();
    }
}

fn unpause_simulation(
    mut virtual_time: ResMut<Time<Virtual>>,
    physics_time: Option<ResMut<Time<Physics>>>,
) {
    virtual_time.unpause();
    if let Some(mut physics) = physics_time {
        physics.unpause();
    }
}

fn clear_pauses(mut tracker: ResMut<PauseTracker>) {
    tracker.menu_pauses = 0;
    tracker.manual_paused = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, StatePlugin));
        app.update();
        app
    }

    #[test]
    fn default_state_is_in_game_and_running() {
        let app = test_app();
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::InGame);
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Running);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    }

    #[test]
    fn pause_tracker_reference_counts_menu_requests() {
        let mut t = PauseTracker::default();
        assert!(t.request(true), "first menu pause pauses");
        assert!(t.request(true), "a second menu pause stays paused");
        assert!(!t.release(true), "releasing one of two keeps it paused");
        assert!(t.release(true), "releasing the last resumes");
    }

    #[test]
    fn sync_menus_resets_stale_counts_and_reports_resume() {
        let mut t = PauseTracker::default();
        t.request(true);
        t.request(true);
        assert!(
            !t.sync_menus(true, 0),
            "menus still open keeps the pause and the count"
        );
        assert_eq!(t.menu_pauses, 2);

        assert!(
            t.sync_menus(false, 0),
            "all menus closed with nothing else pausing resumes"
        );
        assert_eq!(t.menu_pauses, 0);

        let mut t = PauseTracker::default();
        t.request(true);
        t.request(false);
        assert!(
            !t.sync_menus(false, 7),
            "a reload clears menu pauses but a manual pause holds"
        );
        assert_eq!(t.menu_pauses, 0);
        assert!(t.manual_paused);
    }

    #[test]
    fn pause_tracker_manual_pause_is_a_single_toggle() {
        let mut t = PauseTracker::default();
        assert!(t.request(false), "manual pause pauses");
        assert!(t.request(false), "a second manual pause stays paused");
        assert!(t.release(false), "manual release resumes regardless of count");
        assert!(!t.is_paused());
    }
}
