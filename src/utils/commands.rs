use std::{borrow::Cow, collections::VecDeque};

use bevy::{ecs::system::SystemId, prelude::*};
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::utils::script_map;

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

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandArgs {
    pub raw: String,
    pub command: String,
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
                                    (
                                        "name",
                                        ScriptValue::String(Cow::Owned(cmd.name.clone())),
                                    ),
                                    (
                                        "description",
                                        ScriptValue::String(Cow::Borrowed(cmd.description)),
                                    ),
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
            );
    }
}

#[derive(Message, Debug, Clone)]
pub struct RunCommand(pub String);

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

fn handle_run_commands(
    mut commands: Commands,
    registry: Res<CommandRegistry>,
    mut command_args: ResMut<CommandArgs>,
    mut reader: MessageReader<RunCommand>,
) {
    for msg in reader.read() {
        let trimmed = msg.0.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Command names never contain whitespace, so the first token is always the name.
        let mut parts = trimmed.split_whitespace();
        let Some(first) = parts.next() else { continue };
        let args: Vec<String> = parts.map(String::from).collect();
        let raw_args = trimmed[first.len()..].trim().to_string();

        if let Some(entry) = registry.find_command(first) {
            command_args.raw = raw_args;
            command_args.command = entry.name.clone();
            command_args.args = args;
            commands.run_system(entry.system_id);
        } else {
            warn!("Unknown command '{}'", trimmed);
        }
    }
}