use bevy::prelude::*;

#[derive(Message, Debug, Clone)]
pub struct ToggleMenu(pub String);

#[derive(Message, Debug, Clone)]
pub enum MenuCmd {
    Open(String),
    Close(Option<String>),
}

#[derive(Resource, Default)]
pub struct MenuManager {
    open_menus: Vec<(String, Entity)>,
    reload_generation: u64,
}

impl MenuManager {
    pub fn any_open(&self) -> bool {
        !self.open_menus.is_empty()
    }

    pub fn reload_generation(&self) -> u64 {
        self.reload_generation
    }

    pub(crate) fn bump_reload_generation(&mut self) {
        self.reload_generation = self.reload_generation.wrapping_add(1);
    }

    pub fn top_entity(&self) -> Option<Entity> {
        self.open_menus.last().map(|(_, entity)| *entity)
    }

    pub fn depth(&self) -> usize {
        self.open_menus.len()
    }

    pub fn open_names(&self) -> impl Iterator<Item = &str> {
        self.open_menus.iter().map(|(name, _)| name.as_str())
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.open_menus
            .iter()
            .rposition(|(n, _)| n == name)
    }

    pub fn push(&mut self, name: String, entity: Entity) {
        self.open_menus.push((name, entity));
    }

    pub fn remove_at(&mut self, pos: usize) -> (String, Entity) {
        self.open_menus.remove(pos)
    }
}
