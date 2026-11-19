use std::{borrow::Cow, collections::VecDeque};

use bevy::{ecs::system::SystemId, platform::collections::HashMap, prelude::*};
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

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

pub struct CommandRegistryPlugin;

impl Plugin for CommandRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandRegistry>()
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
                                let mut map = HashMap::default();
                                map.insert(
                                    "name".to_string(),
                                    ScriptValue::String(Cow::Owned(cmd.name.clone())),
                                );
                                map.insert(
                                    "description".to_string(),
                                    ScriptValue::String(Cow::Borrowed(cmd.description)),
                                );
                                ScriptValue::Map(map)
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
        self.init_resource::<CommandRegistry>();
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
}

fn handle_run_commands(
    mut commands: Commands,
    registry: Res<CommandRegistry>,
    mut reader: MessageReader<RunCommand>,
) {
    for msg in reader.read() {
        if let Some(entry) = registry.get(&msg.0) {
            commands.run_system(entry.system_id);
        } else {
            warn!("Unknown command '{}'", msg.0);
        }
    }
}