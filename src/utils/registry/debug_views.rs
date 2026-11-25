use bevy::prelude::*;

#[macro_export]
macro_rules! debug_view {
    ($app:expr, $name:expr, $desc:expr) => {{
        let mod_path = module_path!()
            .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
            .unwrap_or(module_path!());
        let full_name = format!("{mod_path}>{}", $name).replace("::", ">");
        $app.register_debug_view(full_name, $desc)
    }};
}

pub use crate::debug_view;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DebugViewLevel {
    #[default]
    Off,
    Embedded,
    Translucent,
}

impl DebugViewLevel {
    pub fn index(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Embedded => 1,
            Self::Translucent => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Embedded => "embedded",
            Self::Translucent => "translucent",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "0" | "off" => Some(Self::Off),
            "1" | "embedded" => Some(Self::Embedded),
            "2" | "translucent" => Some(Self::Translucent),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Embedded,
            Self::Embedded => Self::Translucent,
            Self::Translucent => Self::Off,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DebugViewLayer {
    pub name: String,
    pub description: &'static str,
    pub level: DebugViewLevel,
}

#[derive(Resource, Default)]
pub struct DebugViewRegistry {
    layers: Vec<DebugViewLayer>,
}

impl DebugViewRegistry {
    pub fn register(&mut self, name: String, description: &'static str) {
        if let Some(existing) = self
            .layers
            .iter_mut()
            .find(|layer| layer.name.eq_ignore_ascii_case(&name))
        {
            existing.name = name;
            existing.description = description;
            existing.level = DebugViewLevel::Off;
        } else {
            self.layers.push(DebugViewLayer {
                name,
                description,
                level: DebugViewLevel::Off,
            });
            self.layers.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }

    pub fn layers(&self) -> &[DebugViewLayer] {
        &self.layers
    }

    pub fn get(&self, name: &str) -> Option<&DebugViewLayer> {
        self.layers
            .iter()
            .find(|layer| layer.name.eq_ignore_ascii_case(name))
    }

    pub fn find(&self, name: &str) -> Option<&DebugViewLayer> {
        if let Some(layer) = self.get(name) {
            return Some(layer);
        }
        let mut matches = self.layers.iter().filter(|layer| {
            layer
                .name
                .rsplit('>')
                .next()
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(name))
        });
        let found = matches.next()?;
        if matches.next().is_some() {
            warn!(
                "Ambiguous debug view shorthand '{name}': several layers end with this name; use the full name"
            );
            return None;
        }
        Some(found)
    }

    pub fn level(&self, name: &str) -> DebugViewLevel {
        self.get(name).map_or(DebugViewLevel::Off, |layer| layer.level)
    }

    pub fn set(&mut self, name: &str, level: DebugViewLevel) -> Option<DebugViewLevel> {
        let index = self.find(name).map(|found| {
            self.layers
                .iter()
                .position(|layer| layer.name == found.name)
                .expect("the found layer is registered")
        })?;
        if level != DebugViewLevel::Off {
            for (i, layer) in self.layers.iter_mut().enumerate() {
                if i != index {
                    layer.level = DebugViewLevel::Off;
                }
            }
        }
        self.layers[index].level = level;
        Some(level)
    }

    pub fn cycle(&mut self, name: &str) -> Option<DebugViewLevel> {
        let level = self.find(name)?.level.next();
        self.set(name, level)
    }

    pub fn all_off(&mut self) {
        for layer in &mut self.layers {
            layer.level = DebugViewLevel::Off;
        }
    }

    pub fn active(&self) -> Option<&DebugViewLayer> {
        self.layers
            .iter()
            .find(|layer| layer.level != DebugViewLevel::Off)
    }
}

pub trait DebugViewAppExt {
    fn register_debug_view(&mut self, name: impl Into<String>, description: &'static str)
    -> &mut Self;
}

impl DebugViewAppExt for App {
    fn register_debug_view(
        &mut self,
        name: impl Into<String>,
        description: &'static str,
    ) -> &mut Self {
        if self.world().get_resource::<DebugViewRegistry>().is_none() {
            self.init_resource::<DebugViewRegistry>();
        }
        self.world_mut()
            .resource_mut::<DebugViewRegistry>()
            .register(name.into(), description);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(names: &[&str]) -> DebugViewRegistry {
        let mut registry = DebugViewRegistry::default();
        for name in names {
            registry.register((*name).into(), "d");
        }
        registry
    }

    #[test]
    fn layers_stay_sorted_and_replace_instead_of_duplicating() {
        let mut registry = DebugViewRegistry::default();
        registry.register("zulu>view".into(), "first");
        registry.register("alpha>view".into(), "second");
        registry.register("zulu>view".into(), "third");
        let names: Vec<&str> = registry.layers().iter().map(|layer| layer.name.as_str()).collect();
        assert_eq!(names, vec!["alpha>view", "zulu>view"]);
        assert_eq!(registry.get("zulu>view").unwrap().description, "third");
    }

    #[test]
    fn find_matches_the_full_name_or_a_unique_suffix() {
        let registry = registry_with(&["terrain>chunks", "terrain>lod"]);
        assert!(registry.find("terrain>chunks").is_some());
        assert!(registry.find("CHUNKS").is_some());
        assert!(registry.find("terrain").is_none(), "a namespace segment is not a layer");
        assert!(registry.find("missing").is_none());
    }

    #[test]
    fn an_ambiguous_shorthand_matches_no_layer() {
        let registry = registry_with(&["alpha>toggle", "beta>toggle"]);
        assert!(registry.find("toggle").is_none());
        assert!(registry.find("alpha>toggle").is_some());
    }

    #[test]
    fn activating_a_layer_turns_the_others_off() {
        let mut registry = registry_with(&["terrain>chunks", "terrain>lod"]);
        registry.set("chunks", DebugViewLevel::Embedded).unwrap();
        registry.set("lod", DebugViewLevel::Translucent).unwrap();
        assert_eq!(registry.level("terrain>chunks"), DebugViewLevel::Off);
        assert_eq!(registry.level("terrain>lod"), DebugViewLevel::Translucent);
        assert_eq!(registry.active().unwrap().name, "terrain>lod");
    }

    #[test]
    fn turning_one_layer_off_leaves_the_others_alone() {
        let mut registry = registry_with(&["terrain>chunks", "terrain>lod"]);
        registry.set("lod", DebugViewLevel::Embedded).unwrap();
        registry.set("chunks", DebugViewLevel::Off).unwrap();
        assert_eq!(registry.level("terrain>lod"), DebugViewLevel::Embedded);
    }

    #[test]
    fn setting_an_unknown_layer_reports_nothing() {
        let mut registry = registry_with(&["terrain>chunks"]);
        assert!(registry.set("nope", DebugViewLevel::Embedded).is_none());
        assert!(registry.cycle("nope").is_none());
    }

    #[test]
    fn cycling_walks_off_embedded_translucent() {
        let mut registry = registry_with(&["terrain>chunks"]);
        assert_eq!(registry.cycle("chunks"), Some(DebugViewLevel::Embedded));
        assert_eq!(registry.cycle("chunks"), Some(DebugViewLevel::Translucent));
        assert_eq!(registry.cycle("chunks"), Some(DebugViewLevel::Off));
    }

    #[test]
    fn all_off_clears_every_layer() {
        let mut registry = registry_with(&["terrain>chunks", "terrain>lod"]);
        registry.set("lod", DebugViewLevel::Translucent).unwrap();
        registry.all_off();
        assert!(registry.active().is_none());
    }

    #[test]
    fn levels_parse_the_documented_spellings() {
        assert_eq!(DebugViewLevel::parse("0"), Some(DebugViewLevel::Off));
        assert_eq!(DebugViewLevel::parse("1"), Some(DebugViewLevel::Embedded));
        assert_eq!(DebugViewLevel::parse("2"), Some(DebugViewLevel::Translucent));
        assert_eq!(DebugViewLevel::parse("embedded"), Some(DebugViewLevel::Embedded));
        assert_eq!(DebugViewLevel::parse("translucent"), Some(DebugViewLevel::Translucent));
        assert_eq!(DebugViewLevel::parse("9"), None);
        assert_eq!(DebugViewLevel::Embedded.index(), 1);
        assert_eq!(DebugViewLevel::Translucent.index(), 2);
    }

    #[test]
    fn register_debug_view_initializes_the_registry_lazily() {
        let mut app = App::new();
        app.register_debug_view("test>lazy", "d");
        assert!(app.world().resource::<DebugViewRegistry>().get("test>lazy").is_some());
    }

    #[test]
    fn the_macro_derives_the_layer_name_from_the_module_path() {
        let mut app = App::new();
        app.init_resource::<DebugViewRegistry>();
        debug_view!(app, "sample", "does nothing");

        let registry = app.world().resource::<DebugViewRegistry>();
        let name = registry.layers()[0].name.as_str();
        assert_eq!(name, "utils>registry>debug_views>tests>sample");
        assert!(registry.find("sample").is_some(), "the last segment is what the user types");
    }
}
