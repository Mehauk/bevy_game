use std::path::{Path, PathBuf};

use bevy::{asset::AssetEvent, prelude::*};
use bevy_mod_scripting::asset::ScriptAsset;

use crate::config::MenuConfig;
use crate::html::HtmlAsset;
use crate::lifecycle::{MenuRoot, reload_menu_at};
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

    let affected: Vec<usize> = manager
        .entries()
        .filter(|(_, _, entity)| {
            q_roots
                .get(*entity)
                .is_ok_and(|root| root.tracks(&modified_scripts, &modified_html))
        })
        .map(|(pos, _, _)| pos)
        .collect();

    if affected.is_empty() {
        return;
    }

    commands.queue(move |world: &mut World| {
        for pos in affected {
            reload_menu_at(world, pos);
        }
        world
            .resource_mut::<MenuManager>()
            .bump_reload_generation();
    });
}
