use bevy::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuAssetRoot {
    pub folder: String,
    pub prefix: String,
}

impl MenuAssetRoot {
    pub fn new(folder: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self {
            folder: folder.into(),
            prefix: prefix.into(),
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct MenuConfig {
    pub roots: Vec<MenuAssetRoot>,
    pub base_z_index: i32,
}

impl Default for MenuConfig {
    fn default() -> Self {
        Self {
            roots: vec![MenuAssetRoot::new("assets", "")],
            base_z_index: 100,
        }
    }
}
