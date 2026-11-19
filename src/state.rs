//! Application and gameplay state machines.
//!
//! [`GameState`] models top-level game phases. [`PlayState`] is a sub-state scoped to
//! [`GameState::InGame`] via Bevy's `SubStates` derive, so the pause state only exists
//! during gameplay and cleanly transitions simulation time when entering and exiting.

use avian3d::{prelude::Physics, schedule::PhysicsTime};
use bevy::prelude::*;

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

/// Tracks whether a pause was caused by opening the command palette so that closing
/// the palette resumes playback without overriding other pause sources.
#[derive(Resource, Default)]
struct PalettePause(bool);

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .add_sub_state::<PlayState>()
            .init_resource::<PalettePause>()
            .add_systems(OnEnter(PlayState::Paused), pause_simulation)
            .add_systems(OnExit(PlayState::Paused), unpause_simulation)
            .add_systems(OnExit(GameState::InGame), clear_palette_pause)
            .add_systems(
                PostUpdate,
                sync_pause_with_command_palette
                    .run_if(in_state(GameState::InGame).and_then(resource_exists::<MenuManager>)),
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
    mut palette_pause: ResMut<PalettePause>,
) {
    virtual_time.unpause();
    if let Some(mut physics) = physics_time {
        physics.unpause();
    }
    palette_pause.0 = false;
}

fn clear_palette_pause(mut palette_pause: ResMut<PalettePause>) {
    palette_pause.0 = false;
}

fn sync_pause_with_command_palette(
    menu_manager: Res<MenuManager>,
    play_state: Res<State<PlayState>>,
    mut next_play_state: ResMut<NextState<PlayState>>,
    mut palette_pause: ResMut<PalettePause>,
    mut prev_open: Local<Option<bool>>,
) {
    let open = menu_manager.is_open("command_palette");
    if *prev_open == Some(open) {
        return;
    }
    *prev_open = Some(open);

    if open {
        if *play_state.get() == PlayState::Running {
            palette_pause.0 = true;
            next_play_state.set(PlayState::Paused);
        }
    } else if palette_pause.0 {
        palette_pause.0 = false;
        next_play_state.set(PlayState::Running);
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
    fn command_palette_pauses_and_resumes_play_state() {
        let mut app = test_app();

        // Non-palette menu does not trigger pause.
        app.world_mut()
            .resource_mut::<MenuManager>()
            .push("keyboard".into(), Entity::from_raw_u32(1).unwrap());
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Running);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());

        // Opening command_palette triggers pause.
        app.world_mut()
            .resource_mut::<MenuManager>()
            .push("command_palette".into(), Entity::from_raw_u32(2).unwrap());
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Paused);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());

        // Closing command_palette unpauses.
        let mut mgr = app.world_mut().resource_mut::<MenuManager>();
        let pos = mgr.index_of("command_palette").unwrap();
        mgr.remove_at(pos);
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Running);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    }
}