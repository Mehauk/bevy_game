//! The menu stack: which menus are open, in what order.
//!
//! This is the resource other systems (input contexts, cursor locking) read to answer
//! "is any UI open?". It deliberately holds *only* the stack — per-dispatcher caches live
//! with their dispatcher.

use bevy::prelude::*;

/// Request to flip a menu open/closed by name (hotkey or `commands.run`).
#[derive(Message, Debug, Clone)]
pub struct ToggleMenu(pub String);

/// Direct open/close request; `Close(None)` closes the topmost menu.
#[derive(Message, Debug, Clone)]
pub enum MenuCmd {
    Open(String),
    Close(Option<String>),
}

/// Stack of open menus, bottom-most first. The top of the stack is the modal surface.
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

    pub(crate) fn top_entity(&self) -> Option<Entity> {
        self.open_menus.last().map(|(_, entity)| *entity)
    }

    /// Nesting depth, used to layer each newly opened menu above the previous one.
    pub(crate) fn depth(&self) -> usize {
        self.open_menus.len()
    }

    pub(crate) fn open_names(&self) -> impl Iterator<Item = &str> {
        self.open_menus.iter().map(|(name, _)| name.as_str())
    }

    pub(crate) fn push(&mut self, name: String, entity: Entity) {
        self.open_menus.push((name, entity));
    }

    /// Index of the topmost instance of `name`, if open.
    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.open_menus
            .iter()
            .rposition(|(n, _)| n == name)
    }

    pub(crate) fn remove_at(&mut self, pos: usize) -> (String, Entity) {
        self.open_menus.remove(pos)
    }
}