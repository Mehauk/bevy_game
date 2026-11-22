use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::utils::registry::commands::{
    CommandArgs, CommandHistory, CommandOutput, CommandRegistry, HistoryEntry, RunCommand,
};
use crate::utils::{script_map, script_str};

const HISTORY_LIMIT: usize = 200;

pub struct CommandsPlugin;

impl Plugin for CommandsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandRegistry>()
            .init_resource::<CommandArgs>()
            .init_resource::<CommandOutput>()
            .init_resource::<CommandHistory>()
            .add_message::<RunCommand>()
            .add_systems(Update, handle_run_commands);

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
                        let list: Vec<ScriptValue> = history
                            .entries
                            .iter()
                            .map(|e| {
                                script_map([
                                    ("command", script_str(&e.command)),
                                    ("output", script_str(e.output.as_deref().unwrap_or(""))),
                                ])
                            })
                            .collect();
                        ScriptValue::List(VecDeque::from(list))
                    })
                },
            );
    }
}

fn handle_run_commands(
    mut commands: Commands,
    registry: Res<CommandRegistry>,
    mut reader: MessageReader<RunCommand>,
) {
    for msg in reader.read() {
        let trimmed = msg.0.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let args: Vec<String> = parts.map(String::from).collect();
        let line = trimmed.to_string();

        let Some(entry) = registry.find_command(name) else {
            warn!("Unknown command '{}'", trimmed);
            commands.queue(move |world: &mut World| push_history(world, line, None));
            continue;
        };

        let system_id = entry.system_id;
        commands.queue(move |world: &mut World| {
            *world.resource_mut::<CommandArgs>() = CommandArgs { args };
            *world.resource_mut::<CommandOutput>() = CommandOutput::default();
            let output = match world.run_system(system_id) {
                Ok(()) => world.resource_mut::<CommandOutput>().take(),
                Err(err) => {
                    warn!("Command failed to run: {err}");
                    Some(format!("error: {err}"))
                }
            };
            push_history(world, line, output);
        });
    }
}

fn push_history(world: &mut World, command: String, output: Option<String>) {
    let mut history = world.resource_mut::<CommandHistory>();
    history.entries.push_back(HistoryEntry { command, output });
    if history.entries.len() > HISTORY_LIMIT {
        history.entries.pop_front();
    }
    history.version += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::registry::commands::{CommandAppExt, CommandRegistry};

    fn history_app() -> App {
        let mut app = App::new();
        app.init_resource::<CommandRegistry>()
            .init_resource::<CommandArgs>()
            .init_resource::<CommandOutput>()
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

        let commands: Vec<&str> = app
            .world()
            .resource::<CommandHistory>()
            .entries
            .iter()
            .map(|e| e.command.as_str())
            .collect();
        assert_eq!(commands, vec!["ok 1", "not_a_command"]);
    }

    #[test]
    fn history_attaches_command_output() {
        let mut app = history_app();
        app.register_command(
            "test>echo",
            "d",
            |args: Res<CommandArgs>, mut output: ResMut<CommandOutput>| {
                output.set(args.args.join(" "));
            },
        );
        {
            let world = app.world_mut();
            let mut msgs = world.resource_mut::<Messages<RunCommand>>();
            msgs.write(RunCommand("echo hello world".into()));
            msgs.write(RunCommand("ok".into()));
        }
        app.update();

        let history = app.world().resource::<CommandHistory>();
        assert_eq!(history.entries[0].output.as_deref(), Some("hello world"));
        assert_eq!(
            history.entries[1].output, None,
            "a command that writes nothing has no output"
        );
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
        assert_eq!(history.entries.len(), HISTORY_LIMIT);
        assert_eq!(
            history.entries.front().unwrap().command, "ok 5",
            "the five oldest lines are gone"
        );
    }

    #[derive(Resource, Default)]
    struct Seen(Vec<String>);

    #[test]
    fn two_commands_in_one_frame_each_see_their_own_args() {
        let mut app = App::new();
        app.init_resource::<CommandRegistry>()
            .init_resource::<CommandArgs>()
            .init_resource::<CommandOutput>()
            .init_resource::<CommandHistory>()
            .init_resource::<Seen>()
            .add_message::<RunCommand>()
            .add_systems(Update, handle_run_commands);

        app.register_command(
            "test>first",
            "d",
            |args: Res<CommandArgs>, mut seen: ResMut<Seen>| {
                seen.0.push(format!("first:{}", args.args.join(",")));
            },
        );
        app.register_command(
            "test>second",
            "d",
            |args: Res<CommandArgs>, mut seen: ResMut<Seen>| {
                seen.0.push(format!("second:{}", args.args.join(",")));
            },
        );

        {
            let world = app.world_mut();
            world
                .resource_mut::<Messages<RunCommand>>()
                .write(RunCommand("first a b".into()));
            world
                .resource_mut::<Messages<RunCommand>>()
                .write(RunCommand("second c d".into()));
        }
        app.update();

        assert_eq!(
            app.world().resource::<Seen>().0,
            vec!["first:a,b", "second:c,d"]
        );
    }
}
