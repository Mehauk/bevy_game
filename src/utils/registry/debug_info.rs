use bevy::ecs::system::SystemId;
use bevy::prelude::*;

#[macro_export]
macro_rules! debug_info {
    ($app:expr, $label:expr, $path:path) => {{
        let category = module_path!()
            .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
            .unwrap_or(module_path!());
        $app.register_debug_info(category, $label, $path)
    }};
}

pub use crate::debug_info;

#[derive(Clone)]
pub struct DebugInfoEntry {
    pub category: &'static str,
    pub label: &'static str,
    pub system_id: SystemId<(), Option<String>>,
}

#[derive(Resource, Default)]
pub struct DebugInfoRegistry {
    pub entries: Vec<DebugInfoEntry>,
}

impl DebugInfoRegistry {
    pub fn register(
        &mut self,
        category: &'static str,
        label: &'static str,
        system_id: SystemId<(), Option<String>>,
    ) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.category == category && e.label == label)
        {
            existing.system_id = system_id;
        } else {
            self.entries.push(DebugInfoEntry {
                category,
                label,
                system_id,
            });
            self.entries
                .sort_by(|a, b| (a.category, a.label).cmp(&(b.category, b.label)));
        }
    }
}

pub trait DebugInfoAppExt {
    fn register_debug_info<F, M>(
        &mut self,
        category: &'static str,
        label: &'static str,
        system: F,
    ) -> &mut Self
    where
        F: IntoSystem<(), Option<String>, M> + 'static;
}

impl DebugInfoAppExt for App {
    fn register_debug_info<F, M>(
        &mut self,
        category: &'static str,
        label: &'static str,
        system: F,
    ) -> &mut Self
    where
        F: IntoSystem<(), Option<String>, M> + 'static,
    {
        if self.world().get_resource::<DebugInfoRegistry>().is_none() {
            self.init_resource::<DebugInfoRegistry>();
        }
        let system_id = self.register_system(system);
        self.world_mut()
            .resource_mut::<DebugInfoRegistry>()
            .register(category, label, system_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(world: &mut World) -> SystemId<(), Option<String>> {
        world.register_system(|| Some("value".to_string()))
    }

    #[test]
    fn entries_stay_sorted_by_category_then_label() {
        let mut world = World::new();
        let id = sys(&mut world);
        let mut reg = DebugInfoRegistry::default();
        reg.register("movement", "Speed", id);
        reg.register("camera", "Yaw", id);
        reg.register("movement", "State", id);
        let keys: Vec<(&str, &str)> = reg
            .entries
            .iter()
            .map(|e| (e.category, e.label))
            .collect();
        assert_eq!(
            keys,
            vec![("camera", "Yaw"), ("movement", "Speed"), ("movement", "State")]
        );
    }

    #[test]
    fn re_registering_a_category_and_label_replaces_the_system() {
        let mut world = World::new();
        let first = sys(&mut world);
        let second = sys(&mut world);
        let mut reg = DebugInfoRegistry::default();
        reg.register("movement", "State", first);
        reg.register("movement", "State", second);
        assert_eq!(reg.entries.len(), 1);
        assert_eq!(reg.entries[0].system_id, second);
    }

    fn a_registered_system() -> Option<String> {
        None
    }

    #[test]
    fn debug_info_macro_derives_its_category_from_the_module_path() {
        let mut app = App::new();
        debug_info!(app, "Label", a_registered_system);

        let reg = app.world().resource::<DebugInfoRegistry>();
        assert_eq!(reg.entries.len(), 1);
        assert_eq!(reg.entries[0].category, "utils::registry::debug_info::tests");
        assert_eq!(reg.entries[0].label, "Label");
    }
}
