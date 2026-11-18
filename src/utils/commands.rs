use bevy::{ecs::system::SystemId, prelude::*};

pub struct CommandRegistryPlugin;

impl Plugin for CommandRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandRegistry>()
            .add_message::<RunCommand>()
            .add_systems(Update, handle_run_commands);
    }
}

#[derive(Message, Debug, Clone)]
pub struct RunCommand(pub String);

pub trait CommandAppExt {
    fn register_command<M>(
        &mut self,
        name: &'static str,
        description: &'static str,
        system: impl IntoSystem<(), (), M> + 'static,
    ) -> &mut Self;
}

impl CommandAppExt for App {
    fn register_command<M>(
        &mut self,
        name: &'static str,
        description: &'static str,
        system: impl IntoSystem<(), (), M> + 'static,
    ) -> &mut Self {
        let system_id = self.register_system(system);
        self.init_resource::<CommandRegistry>();
        self.world_mut()
            .resource_mut::<CommandRegistry>()
            .register(name, description, system_id);
        self
    }
}

#[derive(Clone)]
pub struct CommandEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub system_id: SystemId,
}

#[derive(Resource, Default)]
pub struct CommandRegistry {
    pub commands: Vec<CommandEntry>,
}

impl CommandRegistry {
    pub fn register(
        &mut self,
        name: &'static str,
        description: &'static str,
        system_id: SystemId,
    ) {
        if let Some(existing) = self.commands.iter_mut().find(|c| c.name == name) {
            existing.description = description;
            existing.system_id = system_id;
        } else {
            self.commands.push(CommandEntry {
                name,
                description,
                system_id,
            });
            self.commands.sort_by(|a, b| a.name.cmp(b.name));
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