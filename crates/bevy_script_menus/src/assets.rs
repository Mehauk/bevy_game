use std::path::{Path, PathBuf};

use bevy::{asset::AssetEvent, prelude::*};
use bevy_mod_scripting::asset::ScriptAsset;

use crate::config::MenuConfig;
use crate::html::HtmlAsset;
use crate::lifecycle::{MenuRoot, close_top_menu, open_menu_in_world};
use crate::state::MenuManager;

pub(crate) fn resolve_menu_asset(config: &MenuConfig, relative_path: &str) -> Option<String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .ok()
        .map(PathBuf::from);
    for root in &config.roots {
        let candidates = [PathBuf::from(&root.folder), Path::new("..").join(&root.folder)]
            .into_iter()
            .chain(manifest_dir.as_ref().map(|m| m.join(&root.folder)));
        for base in candidates {
            if base.join(relative_path).is_file() {
                return Some(format!("{}{relative_path}", root.prefix));
            }
        }
    }
    None
}

pub(crate) fn reload_menus_on_asset_change(
    mut script_events: MessageReader<AssetEvent<ScriptAsset>>,
    mut html_events: MessageReader<AssetEvent<HtmlAsset>>,
    q_roots: Query<&MenuRoot>,
    manager: Res<MenuManager>,
    mut commands: Commands,
) {
    let modified_scripts: Vec<AssetId<ScriptAsset>> = script_events
        .read()
        .filter_map(|e| if let AssetEvent::Modified { id } = e { Some(*id) } else { None })
        .collect();
    let modified_html: Vec<AssetId<HtmlAsset>> = html_events
        .read()
        .filter_map(|e| if let AssetEvent::Modified { id } = e { Some(*id) } else { None })
        .collect();

    if modified_scripts.is_empty() && modified_html.is_empty() {
        return;
    }

    let modified = q_roots.iter().any(|r| {
        r.script
            .as_ref()
            .is_some_and(|h| modified_scripts.contains(&h.id()))
            || r.html
                .as_ref()
                .is_some_and(|h| modified_html.contains(&h.id()))
    });

    if modified && manager.any_open() {
        commands.queue(|world: &mut World| {
            let names: Vec<String> = world
                .resource::<MenuManager>()
                .open_names()
                .map(str::to_string)
                .collect();
            while close_top_menu(world) {}
            world
                .resource_mut::<MenuManager>()
                .bump_reload_generation();
            for name in names {
                open_menu_in_world(world, &name);
            }
        });
    }
}
