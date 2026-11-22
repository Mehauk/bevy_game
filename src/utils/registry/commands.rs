use std::collections::VecDeque;

use bevy::{ecs::system::SystemId, prelude::*};

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
    pub args: Vec<String>,
}

impl CommandArgs {
    pub fn get_f32(&self, index: usize) -> Option<f32> {
        self.args.get(index).and_then(|s| s.parse::<f32>().ok())
    }
}

#[derive(Resource, Default)]
pub struct CommandOutput(Option<String>);

impl CommandOutput {
    pub fn set(&mut self, text: impl Into<String>) {
        self.0 = Some(text.into());
    }

    pub fn take(&mut self) -> Option<String> {
        self.0.take()
    }
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub command: String,
    pub output: Option<String>,
}

#[derive(Message, Debug, Clone)]
pub struct RunCommand(pub String);

#[derive(Resource, Default)]
pub struct CommandHistory {
    pub entries: VecDeque<HistoryEntry>,
    pub version: usize,
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
        if self.world().get_resource::<CommandRegistry>().is_none() {
            self.init_resource::<CommandRegistry>();
        }
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

        let mut matches = self.commands.iter().filter(|c| {
            c.name
                .rsplit('>')
                .next()
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(name))
        });
        let found = matches.next()?;
        if matches.next().is_some() {
            warn!(
                "Ambiguous command shorthand '{name}': several commands end with this name; use the full name"
            );
            return None;
        }
        Some(found)
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

    #[test]
    fn ambiguous_shorthand_matches_no_command() {
        let mut world = World::new();
        let id = sys(&mut world);
        let mut reg = CommandRegistry::default();
        reg.register("alpha>toggle".into(), "d", id);
        reg.register("beta>toggle".into(), "d", id);

        assert!(
            reg.find_command("toggle").is_none(),
            "a shorthand that matches two commands must not pick one"
        );
        assert!(reg.find_command("alpha>toggle").is_some());
    }

    #[test]
    fn register_command_initializes_the_registry_lazily() {
        let mut app = App::new();
        app.register_command("test>lazy", "d", || {});
        assert!(app.world().resource::<CommandRegistry>().get("test>lazy").is_some());
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

}
