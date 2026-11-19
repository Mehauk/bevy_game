use std::collections::VecDeque;

use bevy::{ecs::system::SystemId, prelude::*};
use bevy_enhanced_input::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::menus::{MenuAppExt, MenuSpec};
use crate::utils::{script_map, script_str};

/// Hotkey action that toggles the command palette (the console's UI).
#[derive(InputAction)]
#[action_output(bool)]
struct ToggleCommandPalette;

#[macro_export]
macro_rules! command {
    ($app:expr, $desc:expr, $path:path) => {{
        let raw_path = stringify!($path).replace(' ', "");
        let full_name = if let Some(stripped) = raw_path.strip_prefix("crate::") {
            stripped.replace("::", ">")
        } else {
            let mod_path = module_path!()
                .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
                .unwrap_or(module_path!());
            format!("{mod_path}>{raw_path}").replace("::", ">")
        };
        $app.register_command(full_name, $desc, $path)
    }};
}

pub use crate::command;

/// The parameters of the command currently being run.
///
/// Written immediately before each command system runs, so a command can only ever observe
/// its own arguments (see [`handle_run_commands`]).
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandArgs {
    /// Whitespace-separated tokens after the command name.
    pub args: Vec<String>,
}

impl CommandArgs {
    pub fn get_f32(&self, index: usize) -> Option<f32> {
        self.args.get(index).and_then(|s| s.parse::<f32>().ok())
    }
}

pub struct CommandRegistryPlugin;

impl Plugin for CommandRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandRegistry>()
            .init_resource::<CommandArgs>()
            .init_resource::<CommandHistory>()
            .add_message::<RunCommand>()
            .add_systems(Update, handle_run_commands);

        // The palette is the console's front-end, so its declaration lives with the
        // console. Queued, so this plugin needs no ordering against `MenuPlugin`.
        app.register_menu::<ToggleCommandPalette>(MenuSpec {
            id: "command_palette",
            hotkey_label: "Command Palette",
            command: None,
            bindings: vec![Binding::from(KeyCode::Backquote)],
        });

        NamespaceBuilder::in_namespace(app.world_mut(), "commands")
            .register(
                "list",
                |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                    let world = ctx.world()?;
                    world.with_resource(|registry: &CommandRegistry| {
                        let list: Vec<ScriptValue> = registry
                            .commands
                            .iter()
                            .map(|cmd| {
                                script_map([
                                    ("name", script_str(&cmd.name)),
                                    ("description", script_str(cmd.description)),
                                ])
                            })
                            .collect();
                        ScriptValue::List(VecDeque::from(list))
                    })
                },
            )
            .register(
                "run",
                |ctx: FunctionCallContext, name: String| -> Result<(), InteropError> {
                    let world = ctx.world()?;
                    world.with_resource_mut(|mut writer: Mut<Messages<RunCommand>>| {
                        writer.write(RunCommand(name));
                    })
                },
            )
            .register(
                "history",
                |ctx: FunctionCallContext| -> Result<ScriptValue, InteropError> {
                    let world = ctx.world()?;
                    world.with_resource(|history: &CommandHistory| {
                        let list: Vec<ScriptValue> =
                            history.lines.iter().map(|l| script_str(l)).collect();
                        ScriptValue::List(VecDeque::from(list))
                    })
                },
            );
    }
}

#[derive(Message, Debug, Clone)]
pub struct RunCommand(pub String);

/// How many executed command lines the console history keeps.
const HISTORY_LIMIT: usize = 200;

/// Recently executed command lines, oldest first.
///
/// Lives on the world (not in the palette script) so the history survives the menu
/// closing and reopening — the script, and with it all its Lua state, is recreated
/// on every open.
#[derive(Resource, Default)]
pub struct CommandHistory {
    pub lines: VecDeque<String>,
}

pub trait CommandAppExt {
    fn register_command<F, M>(
        &mut self,
        name: impl Into<String>,
        description: &'static str,
        system: F,
    ) -> &mut Self
    where
        F: IntoSystem<(), (), M> + 'static;
}

impl CommandAppExt for App {
    fn register_command<F, M>(
        &mut self,
        name: impl Into<String>,
        description: &'static str,
        system: F,
    ) -> &mut Self
    where
        F: IntoSystem<(), (), M> + 'static,
    {
        let system_id = self.register_system(system);
        self.world_mut()
            .resource_mut::<CommandRegistry>()
            .register(name.into(), description, system_id);
        self
    }
}

#[derive(Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: &'static str,
    pub system_id: SystemId,
}

#[derive(Resource, Default)]
pub struct CommandRegistry {
    pub commands: Vec<CommandEntry>,
}

impl CommandRegistry {
    pub fn register(&mut self, name: String, description: &'static str, system_id: SystemId) {
        if let Some(existing) = self
            .commands
            .iter_mut()
            .find(|c| c.name.eq_ignore_ascii_case(&name))
        {
            existing.name = name;
            existing.description = description;
            existing.system_id = system_id;
        } else {
            self.commands.push(CommandEntry {
                name,
                description,
                system_id,
            });
            self.commands.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }

    pub fn get(&self, name: &str) -> Option<&CommandEntry> {
        self.commands
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn find_command(&self, name: &str) -> Option<&CommandEntry> {
        if let Some(entry) = self.get(name) {
            return Some(entry);
        }

        self.commands.iter().find(|c| {
            if let Some(suffix) = c.name.rsplit('>').next() {
                suffix.eq_ignore_ascii_case(name)
            } else {
                false
            }
        })
    }
}

/// Resolve and run each requested command, in the order requested.
///
/// Each run is queued as a world closure rather than issued with `commands.run_system`,
/// because [`CommandArgs`] is a single shared resource: writing it here and deferring the run
/// would let a second command issued in the same frame overwrite the arguments before the
/// first system had executed. Queuing writes the arguments immediately before *their* run, so
/// a command can only ever observe its own.
fn handle_run_commands(
    mut commands: Commands,
    registry: Res<CommandRegistry>,
    mut history: ResMut<CommandHistory>,
    mut reader: MessageReader<RunCommand>,
) {
    for msg in reader.read() {
        let trimmed = msg.0.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Recorded before resolution so failed and unknown attempts show up in the
        // console history too, exactly as they would in a real console.
        history.lines.push_back(trimmed.to_string());
        if history.lines.len() > HISTORY_LIMIT {
            history.lines.pop_front();
        }

        // Command names never contain whitespace, so the first token is always the name.
        let mut parts = trimmed.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let args: Vec<String> = parts.map(String::from).collect();

        let Some(entry) = registry.find_command(name) else {
            warn!("Unknown command '{}'", trimmed);
            continue;
        };

        let system_id = entry.system_id;
        commands.queue(move |world: &mut World| {
            *world.resource_mut::<CommandArgs>() = CommandArgs { args };
            if let Err(err) = world.run_system(system_id) {
                warn!("Command failed to run: {err}");
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn sys(world: &mut World) -> SystemId {
        world.register_system(|| {})
    }

    #[test]
    fn lookup_ignores_case() {
        let mut world = World::new();
        let id = sys(&mut world);
        let mut reg = CommandRegistry::default();
        reg.register("character>Respawn".into(), "d", id);
        assert!(reg.get("CHARACTER>RESPAWN").is_some());
        assert!(reg.get("character>Respawn").is_some());
        assert!(reg.get("nope").is_none());
    }

    #[test]
    fn suffix_lookup_matches_only_the_last_segment() {
        let mut world = World::new();
        let id = sys(&mut world);
        let mut reg = CommandRegistry::default();
        reg.register("character>respawn_character".into(), "d", id);

        // This is what the palette actually types, so it is the load-bearing behaviour.
        assert!(reg.find_command("respawn_character").is_some());
        assert!(reg.find_command("RESPAWN_CHARACTER").is_some());
        assert!(reg.find_command("character").is_none(), "a namespace segment is not a command");
    }

    #[test]
    fn re_registering_a_name_replaces_it_rather_than_duplicating() {
        let mut world = World::new();
        let first = sys(&mut world);
        let second = sys(&mut world);
        let mut reg = CommandRegistry::default();
        reg.register("dev>toggle".into(), "first", first);
        reg.register("dev>toggle".into(), "second", second);

        assert_eq!(reg.commands.len(), 1, "a re-registered command must not appear twice");
        let entry = reg.get("dev>toggle").unwrap();
        assert_eq!(entry.description, "second");
        assert_eq!(entry.system_id, second);
    }

    #[test]
    fn commands_stay_sorted_by_name() {
        let mut world = World::new();
        let id = sys(&mut world);
        let mut reg = CommandRegistry::default();
        for name in ["zulu", "alpha", "mike"] {
            reg.register(name.into(), "d", id);
        }
        let names: Vec<&str> = reg.commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mike", "zulu"]);
    }

    fn history_app() -> App {
        let mut app = App::new();
        app.init_resource::<CommandRegistry>()
            .init_resource::<CommandArgs>()
            .init_resource::<CommandHistory>()
            .add_message::<RunCommand>()
            .add_systems(Update, handle_run_commands);
        app.register_command("test>ok", "d", || {});
        app
    }

    #[test]
    fn history_records_attempts_and_skips_blank_lines() {
        let mut app = history_app();
        {
            let world = app.world_mut();
            let mut msgs = world.resource_mut::<Messages<RunCommand>>();
            msgs.write(RunCommand("   ".into()));
            msgs.write(RunCommand("ok 1".into()));
            msgs.write(RunCommand("not_a_command".into()));
        }
        app.update();

        let lines: Vec<&str> = app
            .world()
            .resource::<CommandHistory>()
            .lines
            .iter()
            .map(String::as_str)
            .collect();
        // Unknown commands are recorded too — a real console shows what you typed,
        // not just what worked.
        assert_eq!(lines, vec!["ok 1", "not_a_command"]);
    }

    #[test]
    fn history_drops_oldest_entries_beyond_the_cap() {
        let mut app = history_app();
        {
            let world = app.world_mut();
            let mut msgs = world.resource_mut::<Messages<RunCommand>>();
            for i in 0..(HISTORY_LIMIT + 5) {
                msgs.write(RunCommand(format!("ok {i}")));
            }
        }
        app.update();

        let history = app.world().resource::<CommandHistory>();
        assert_eq!(history.lines.len(), HISTORY_LIMIT);
        assert_eq!(history.lines.front().unwrap(), "ok 5", "the five oldest lines are gone");
    }

    #[test]
    fn args_parse_positionally() {
        let args = CommandArgs {
            args: vec!["10".to_string(), "nope".to_string()],
        };
        assert_eq!(args.get_f32(0), Some(10.0));
        assert_eq!(args.get_f32(1), None, "unparseable is None, not a panic");
        assert_eq!(args.get_f32(9), None, "out of range is None");
    }

    fn a_registered_command() {}

    #[test]
    fn command_macro_derives_its_name_from_the_module_path() {
        // The palette shows this name and the user types its last segment, so the derivation
        // is a user-facing contract: moving a command's module renames it. This test is the
        // guard rail for that (a real regression was nearly shipped during the refactor).
        let mut app = App::new();
        app.init_resource::<CommandRegistry>();
        command!(app, "does nothing", a_registered_command);

        let reg = app.world().resource::<CommandRegistry>();
        let name = reg.commands[0].name.as_str();
        assert_eq!(name, "utils>registry>commands>tests>a_registered_command");
        assert_eq!(
            reg.find_command("a_registered_command").map(|c| c.name.as_str()),
            Some(name),
            "the last segment is what the user types"
        );
    }

    #[derive(Resource, Default)]
    struct Seen(Vec<String>);

    #[test]
    fn two_commands_in_one_frame_each_see_their_own_args() {
        // `CommandArgs` is one shared resource, so writing it and *then* deferring the run
        // let the second command clobber the first one's arguments. Each run must observe
        // exactly its own.
        let mut app = App::new();
        app.init_resource::<CommandRegistry>()
            .init_resource::<CommandArgs>()
            .init_resource::<CommandHistory>()
            .init_resource::<Seen>()
            .add_message::<RunCommand>()
            .add_systems(Update, handle_run_commands);

        app.register_command("test>first", "d", |args: Res<CommandArgs>, mut seen: ResMut<Seen>| {
            seen.0.push(format!("first:{}", args.args.join(",")));
        });
        app.register_command(
            "test>second",
            "d",
            |args: Res<CommandArgs>, mut seen: ResMut<Seen>| {
                seen.0.push(format!("second:{}", args.args.join(",")));
            },
        );

        {
            let world = app.world_mut();
            world.resource_mut::<Messages<RunCommand>>().write(RunCommand("first a b".into()));
            world.resource_mut::<Messages<RunCommand>>().write(RunCommand("second c d".into()));
        }
        app.update();

        assert_eq!(app.world().resource::<Seen>().0, vec!["first:a,b", "second:c,d"]);
    }
}
