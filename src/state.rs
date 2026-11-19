//! Application and gameplay state machines.
//!
//! [`GameState`] models top-level game phases. [`PlayState`] is a sub-state scoped to
//! [`GameState::InGame`] via Bevy's `SubStates` derive. Pausing the game switches
//! [`PlayState`] between `Running` and `Paused`, automatically halting or resuming
//! virtual time and Avian physics simulation.
//!
//! Also registers the `state` Lua namespace so scripts can pause or resume gameplay.

use avian3d::prelude::Physics;
use avian3d::schedule::PhysicsTime;
use bevy::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::menus::MenuManager;

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

/// Reference-counted pause state separating menu-initiated pauses from manual pauses.
#[derive(Resource, Default)]
struct PauseTracker {
    menu_pauses: u32,
    manual_paused: bool,
    last_reload_gen: u64,
}

impl PauseTracker {
    fn is_paused(&self) -> bool {
        self.menu_pauses > 0 || self.manual_paused
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
            .add_systems(OnExit(GameState::InGame), clear_pauses)
            .add_systems(
                PostUpdate,
                sync_menu_pauses
                    .run_if(in_state(GameState::InGame))
                    .run_if(resource_exists::<MenuManager>),
            );

        NamespaceBuilder::in_namespace(app.world_mut(), "state")
            .register(
                "pause",
                |ctx: FunctionCallContext| -> Result<(), InteropError> {
                    let world = ctx.world()?;
                    let menu_open = world
                        .with_resource(|mgr: &MenuManager| mgr.any_open())
                        .unwrap_or(false);
                    let should_pause = world.with_resource_mut(|mut tracker: Mut<PauseTracker>| {
                        if menu_open {
                            tracker.menu_pauses += 1;
                        } else {
                            tracker.manual_paused = true;
                        }
                        tracker.is_paused()
                    })?;
                    if should_pause {
                        world.with_resource_mut(|mut next_state: Mut<NextState<PlayState>>| {
                            next_state.set(PlayState::Paused);
                        })?;
                    }
                    Ok(())
                },
            )
            .register(
                "resume",
                |ctx: FunctionCallContext| -> Result<(), InteropError> {
                    let world = ctx.world()?;
                    let menu_open = world
                        .with_resource(|mgr: &MenuManager| mgr.any_open())
                        .unwrap_or(false);
                    let should_resume = world.with_resource_mut(|mut tracker: Mut<PauseTracker>| {
                        if menu_open && tracker.menu_pauses > 0 {
                            tracker.menu_pauses -= 1;
                        } else {
                            tracker.manual_paused = false;
                            if !menu_open {
                                tracker.menu_pauses = 0;
                            }
                        }
                        !tracker.is_paused()
                    })?;
                    if should_resume {
                        world.with_resource_mut(|mut next_state: Mut<NextState<PlayState>>| {
                            next_state.set(PlayState::Running);
                        })?;
                    }
                    Ok(())
                },
            )
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

/// Resets menu pause counts on hot-reload or once all menus have closed.
fn sync_menu_pauses(
    menu_manager: Res<MenuManager>,
    mut tracker: ResMut<PauseTracker>,
    play_state: Res<State<PlayState>>,
    mut next_play_state: ResMut<NextState<PlayState>>,
) {
    let reload_gen = menu_manager.reload_generation();
    if reload_gen != tracker.last_reload_gen {
        tracker.last_reload_gen = reload_gen;
        tracker.menu_pauses = 0;
    }

    if !menu_manager.any_open() && tracker.menu_pauses > 0 {
        tracker.menu_pauses = 0;
        if !tracker.is_paused() && *play_state.get() == PlayState::Paused {
            next_play_state.set(PlayState::Running);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, StatePlugin))
            .init_resource::<MenuManager>();
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
    fn stacked_pausing_menus_stay_paused_until_last_closes() {
        let mut app = test_app();

        let menu_a = app.world_mut().spawn_empty().id();
        let menu_b = app.world_mut().spawn_empty().id();
        {
            let mut mgr = app.world_mut().resource_mut::<MenuManager>();
            mgr.push("menu_a".into(), menu_a);
            mgr.push("menu_b".into(), menu_b);
        }

        // Simulate two menus calling state.pause().
        app.world_mut().resource_mut::<PauseTracker>().menu_pauses = 2;
        app.world_mut()
            .resource_mut::<NextState<PlayState>>()
            .set(PlayState::Paused);
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Paused);

        // Close one menu (topmost or bottommost); one menu remains open with menu_pauses == 1.
        {
            let mut mgr = app.world_mut().resource_mut::<MenuManager>();
            let pos = mgr.index_of("menu_b").unwrap();
            mgr.remove_at(pos);
        }
        app.world_mut().resource_mut::<PauseTracker>().menu_pauses -= 1;
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Paused);

        // Close remaining menu.
        {
            let mut mgr = app.world_mut().resource_mut::<MenuManager>();
            let pos = mgr.index_of("menu_a").unwrap();
            mgr.remove_at(pos);
        }
        app.world_mut().resource_mut::<PauseTracker>().menu_pauses -= 1;
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Running);
    }

    #[test]
    fn closing_all_menus_without_resume_auto_resumes() {
        let mut app = test_app();

        let menu = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<MenuManager>()
            .push("command_palette".into(), menu);
        app.world_mut().resource_mut::<PauseTracker>().menu_pauses = 1;
        app.world_mut()
            .resource_mut::<NextState<PlayState>>()
            .set(PlayState::Paused);
        app.update();
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused());

        let mut mgr = app.world_mut().resource_mut::<MenuManager>();
        let pos = mgr.index_of("command_palette").unwrap();
        mgr.remove_at(pos);
        app.update();
        app.update();

        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Running);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    }
}