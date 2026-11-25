use std::collections::BTreeMap;

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

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugInfoSystems {
    Sync,
    Collect,
    Render,
}

#[derive(Resource, Default)]
pub struct DebugInfoEnabled(pub bool);

fn debug_info_enabled(enabled: Res<DebugInfoEnabled>) -> bool {
    enabled.0
}

#[derive(Clone)]
struct DebugInfoWrite {
    category: &'static str,
    label: &'static str,
    value: Option<String>,
}

#[derive(Resource, Default)]
pub struct DebugInfoSnapshot {
    entries: BTreeMap<(&'static str, &'static str), String>,
    dirty: bool,
}

impl DebugInfoSnapshot {
    fn record(&mut self, category: &'static str, label: &'static str, value: Option<String>) {
        match value {
            Some(value) => {
                if self.entries.get(&(category, label)).map(String::as_str) != Some(value.as_str())
                {
                    self.entries.insert((category, label), value);
                    self.dirty = true;
                }
            }
            None => {
                if self.entries.remove(&(category, label)).is_some() {
                    self.dirty = true;
                }
            }
        }
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn render(&self) -> String {
        let mut text = String::new();
        let mut shown: Option<&str> = None;
        for ((category, label), value) in &self.entries {
            if shown != Some(category) {
                if shown.is_some() {
                    text.push('\n');
                }
                text.push_str(&format!("[{category}]\n"));
                shown = Some(category);
            }
            text.push_str(&format!("  {label}: {value}\n"));
        }
        text
    }
}

fn record_debug_info(In(write): In<DebugInfoWrite>, mut snapshot: ResMut<DebugInfoSnapshot>) {
    snapshot.record(write.category, write.label, write.value);
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
        if self.world().get_resource::<DebugInfoSnapshot>().is_none() {
            self.init_resource::<DebugInfoSnapshot>();
        }
        if self.world().get_resource::<DebugInfoEnabled>().is_none() {
            self.init_resource::<DebugInfoEnabled>();
        }
        self.add_systems(
            Update,
            IntoSystem::into_system(system)
                .map(move |value| DebugInfoWrite {
                    category,
                    label,
                    value,
                })
                .pipe(record_debug_info)
                .distributive_run_if(debug_info_enabled)
                .in_set(DebugInfoSystems::Collect),
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speed() -> Option<String> {
        Some("6.0 m/s".to_string())
    }

    fn state() -> Option<String> {
        Some("walking".to_string())
    }

    fn yaw() -> Option<String> {
        Some("12.0 deg".to_string())
    }

    fn absent() -> Option<String> {
        None
    }

    #[test]
    fn entries_group_by_category_and_sort_by_label() {
        let mut snapshot = DebugInfoSnapshot::default();
        snapshot.record("movement", "Speed", speed());
        snapshot.record("camera", "Yaw", yaw());
        snapshot.record("movement", "State", state());
        assert_eq!(
            snapshot.render(),
            "[camera]\n  Yaw: 12.0 deg\n\n[movement]\n  Speed: 6.0 m/s\n  State: walking\n"
        );
    }

    #[test]
    fn re_recording_a_label_replaces_its_value() {
        let mut snapshot = DebugInfoSnapshot::default();
        snapshot.record("movement", "Speed", speed());
        snapshot.record("movement", "Speed", Some("12.0 m/s".to_string()));
        assert_eq!(snapshot.render(), "[movement]\n  Speed: 12.0 m/s\n");
    }

    #[test]
    fn a_missing_producer_drops_its_line() {
        let mut snapshot = DebugInfoSnapshot::default();
        snapshot.record("movement", "Speed", speed());
        snapshot.record("movement", "Speed", None);
        assert_eq!(snapshot.render(), "");
    }

    #[test]
    fn dirty_reports_only_real_changes() {
        let mut snapshot = DebugInfoSnapshot::default();
        assert!(!snapshot.take_dirty());
        snapshot.record("movement", "Speed", speed());
        assert!(snapshot.take_dirty());
        snapshot.record("movement", "Speed", speed());
        assert!(!snapshot.take_dirty(), "an unchanged value is not a change");
        snapshot.record("movement", "Speed", None);
        assert!(snapshot.take_dirty());
        snapshot.record("movement", "Speed", None);
        assert!(!snapshot.take_dirty());
    }

    fn a_registered_system() -> Option<String> {
        Some("value".to_string())
    }

    fn an_app_with_one_producer() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        debug_info!(app, "Label", a_registered_system);
        app
    }

    #[test]
    fn debug_info_macro_derives_its_category_from_the_module_path() {
        let mut app = an_app_with_one_producer();
        app.world_mut().resource_mut::<DebugInfoEnabled>().0 = true;
        app.update();

        assert_eq!(
            app.world().resource::<DebugInfoSnapshot>().render(),
            "[utils::registry::debug_info::tests]\n  Label: value\n"
        );
    }

    #[test]
    fn producers_stay_asleep_until_the_snapshot_is_enabled() {
        let mut app = an_app_with_one_producer();
        app.update();

        assert_eq!(app.world().resource::<DebugInfoSnapshot>().render(), "");
    }

    #[test]
    fn a_producer_with_nothing_to_say_occupies_no_space() {
        let mut app = an_app_with_one_producer();
        debug_info!(app, "Absent", absent);
        app.world_mut().resource_mut::<DebugInfoEnabled>().0 = true;
        app.update();

        assert_eq!(
            app.world().resource::<DebugInfoSnapshot>().render(),
            "[utils::registry::debug_info::tests]\n  Label: value\n"
        );
    }
}
