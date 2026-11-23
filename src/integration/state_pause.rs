use bevy::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
};
use bevy_script_menus::MenuManager;

use crate::state::{GameState, PauseTracker, PlayState};

pub(super) fn register(app: &mut App) {
    app.add_systems(
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
                let should_pause = world
                    .with_resource_mut(|mut tracker: Mut<PauseTracker>| tracker.request(menu_open))?;
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
                let should_resume = world
                    .with_resource_mut(|mut tracker: Mut<PauseTracker>| tracker.release(menu_open))?;
                if should_resume {
                    world.with_resource_mut(|mut next_state: Mut<NextState<PlayState>>| {
                        next_state.set(PlayState::Running);
                    })?;
                }
                Ok(())
            },
        );
}

fn sync_menu_pauses(
    menu_manager: Res<MenuManager>,
    mut tracker: ResMut<PauseTracker>,
    play_state: Res<State<PlayState>>,
    mut next_play_state: ResMut<NextState<PlayState>>,
) {
    let should_resume =
        tracker.sync_menus(menu_manager.any_open(), menu_manager.reload_generation());
    if should_resume && *play_state.get() == PlayState::Paused {
        next_play_state.set(PlayState::Running);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StatePlugin;
    use bevy::state::app::StatesPlugin;

    fn pause_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, StatePlugin))
            .init_resource::<MenuManager>()
            .add_systems(
                PostUpdate,
                sync_menu_pauses
                    .run_if(in_state(GameState::InGame))
                    .run_if(resource_exists::<MenuManager>),
            );
        app.update();
        app
    }

    #[test]
    fn stacked_pausing_menus_stay_paused_until_last_closes() {
        let mut app = pause_test_app();

        let menu_a = app.world_mut().spawn_empty().id();
        let menu_b = app.world_mut().spawn_empty().id();
        {
            let mut mgr = app.world_mut().resource_mut::<MenuManager>();
            mgr.push("menu_a".into(), menu_a);
            mgr.push("menu_b".into(), menu_b);
        }

        app.world_mut().resource_mut::<PauseTracker>().menu_pauses = 2;
        app.world_mut()
            .resource_mut::<NextState<PlayState>>()
            .set(PlayState::Paused);
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Paused);

        {
            let mut mgr = app.world_mut().resource_mut::<MenuManager>();
            let pos = mgr.index_of("menu_b").unwrap();
            mgr.remove_at(pos);
        }
        app.world_mut().resource_mut::<PauseTracker>().menu_pauses -= 1;
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Paused);

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
        let mut app = pause_test_app();

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
