use std::fs;
use std::path::Path;

use bevy::prelude::*;

mod commands_docs;
mod lua_docs;

pub struct DocgenPlugin;

impl Plugin for DocgenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (lua_docs::write, commands_docs::write));
    }
}

pub(super) fn write_doc(file_name: &str, contents: &str) {
    let dir = Path::new("docs/.compiled");
    let result = fs::create_dir_all(dir).and_then(|()| fs::write(dir.join(file_name), contents));
    if let Err(e) = result {
        error!("failed to write docs/.compiled/{file_name}: {e}");
    }
}
