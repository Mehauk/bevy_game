use bevy::{pbr::MaterialPlugin, prelude::*};
use bevy_shader_kit::DebugCurtainMaterial;

use crate::utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput, command};
use crate::utils::registry::debug_info::{DebugInfoAppExt, debug_info};
use crate::utils::registry::debug_views::{DebugViewLevel, DebugViewRegistry};

pub struct DebugViewPlugin;

impl Plugin for DebugViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugViewRegistry>()
            .add_plugins(MaterialPlugin::<DebugCurtainMaterial>::default());

        command!(app, "Set, cycle, or list debug view layers", debug_view);
        command!(app, "List debug view layers and their levels", list_debug_views);
        debug_info!(app, "Views", debug_view_debug_info);
    }
}

fn registry_text(registry: &DebugViewRegistry) -> String {
    if registry.layers().is_empty() {
        return "no debug views registered".to_string();
    }
    registry
        .layers()
        .iter()
        .map(|layer| format!("{}: {}", layer.name, layer.level.label()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn debug_view(args: Res<CommandArgs>, mut registry: ResMut<DebugViewRegistry>, mut output: ResMut<CommandOutput>) {
    let lowered: Vec<String> = args.args.iter().map(|arg| arg.to_lowercase()).collect();
    let args: Vec<&str> = lowered.iter().map(String::as_str).collect();
    let text = match args.as_slice() {
        [] | ["list"] => registry_text(&registry),
        ["off"] => {
            registry.all_off();
            "debug views: off".to_string()
        }
        [name] => match registry.cycle(name) {
            Some(level) => format!("{name}: {}", level.label()),
            None => format!("unknown debug view '{name}'; try 'debug_view list'"),
        },
        [name, "cycle"] => match registry.cycle(name) {
            Some(level) => format!("{name}: {}", level.label()),
            None => format!("unknown debug view '{name}'; try 'debug_view list'"),
        },
        [name, level] => match DebugViewLevel::parse(level) {
            Some(level) => match registry.set(name, level) {
                Some(_) => format!("{name}: {}", level.label()),
                None => format!("unknown debug view '{name}'; try 'debug_view list'"),
            },
            None => "usage: debug_view [list | off | <name> [0 | 1 | 2 | cycle]]".to_string(),
        },
        _ => "usage: debug_view [list | off | <name> [0 | 1 | 2 | cycle]]".to_string(),
    };
    output.set(text);
}

fn list_debug_views(registry: Res<DebugViewRegistry>, mut output: ResMut<CommandOutput>) {
    output.set(registry_text(&registry));
}

fn debug_view_debug_info(registry: Res<DebugViewRegistry>) -> Option<String> {
    let layer = registry.active()?;
    Some(format!("{} {}", layer.name, layer.level.label()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::registry::debug_views::DebugViewAppExt;

    fn command_app() -> App {
        let mut app = App::new();
        app.init_resource::<CommandArgs>()
            .init_resource::<CommandOutput>()
            .init_resource::<DebugViewRegistry>();
        app.register_debug_view("terrain>chunks", "d");
        app.register_debug_view("terrain>lod", "d");
        app
    }

    fn run(app: &mut App, args: &[&str]) -> Option<String> {
        *app.world_mut().resource_mut::<CommandArgs>() = CommandArgs {
            args: args.iter().map(|arg| arg.to_string()).collect(),
        };
        let system = app.world_mut().register_system(debug_view);
        app.world_mut().run_system(system).unwrap();
        app.world_mut().resource_mut::<CommandOutput>().take()
    }

    #[test]
    fn a_layer_can_be_set_by_shorthand_and_level() {
        let mut app = command_app();
        assert_eq!(run(&mut app, &["chunks", "1"]).as_deref(), Some("chunks: embedded"));
        assert_eq!(
            app.world().resource::<DebugViewRegistry>().level("terrain>chunks"),
            DebugViewLevel::Embedded
        );
        assert_eq!(run(&mut app, &["lod", "2"]).as_deref(), Some("lod: translucent"));
        assert_eq!(
            app.world().resource::<DebugViewRegistry>().level("terrain>chunks"),
            DebugViewLevel::Off,
            "activating one layer turns the others off"
        );
    }

    #[test]
    fn cycling_walks_through_the_levels() {
        let mut app = command_app();
        assert_eq!(run(&mut app, &["chunks"]).as_deref(), Some("chunks: embedded"));
        assert_eq!(run(&mut app, &["chunks", "cycle"]).as_deref(), Some("chunks: translucent"));
        assert_eq!(run(&mut app, &["chunks"]).as_deref(), Some("chunks: off"));
    }

    #[test]
    fn off_clears_every_layer() {
        let mut app = command_app();
        run(&mut app, &["lod", "1"]);
        assert_eq!(run(&mut app, &["off"]).as_deref(), Some("debug views: off"));
        assert!(app.world().resource::<DebugViewRegistry>().active().is_none());
    }

    #[test]
    fn list_shows_every_layer_with_its_level() {
        let mut app = command_app();
        run(&mut app, &["chunks", "1"]);
        let listing = run(&mut app, &["list"]).unwrap();
        assert!(listing.contains("terrain>chunks: embedded"));
        assert!(listing.contains("terrain>lod: off"));
        assert!(run(&mut app, &[]).unwrap().contains("terrain>lod"));
    }

    #[test]
    fn unknown_names_and_levels_are_reported_not_panicked() {
        let mut app = command_app();
        assert!(run(&mut app, &["nope", "1"]).unwrap().contains("unknown debug view"));
        assert!(run(&mut app, &["chunks", "9"]).unwrap().contains("usage"));
        assert!(run(&mut app, &["a", "b", "c"]).unwrap().contains("usage"));
    }

    #[test]
    fn the_debug_info_line_names_the_active_layer() {
        let mut app = command_app();
        let info = app.world_mut().register_system(debug_view_debug_info);
        assert_eq!(app.world_mut().run_system(info).unwrap(), None);

        run(&mut app, &["lod", "2"]);
        assert_eq!(
            app.world_mut().run_system(info).unwrap().as_deref(),
            Some("terrain>lod translucent")
        );
    }
}
