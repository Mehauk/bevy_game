//! Locating menu assets and hot-reloading them.
//!
//! Menus live in `assets/menus/<id>/` and can be overridden per-file by placing a file at
//! `mods/menus/<id>/`, which is what makes a mod able to restyle or replace a single menu
//! without touching the game's own assets.

use std::path::{Path, PathBuf};

use bevy::{asset::AssetEvent, prelude::*};
use bevy_mod_scripting::asset::ScriptAsset;

use crate::menus::lifecycle::{MenuRoot, close_top_menu, open_menu_in_world};
use crate::menus::state::MenuManager;

/// Resolve `relative_path` against the mod override first, then the game assets.
///
/// Returns the on-disk path plus the `AssetServer`-loadable path (`mods://…` when the
/// override wins, plain otherwise).
pub(crate) fn resolve_menu_file(relative_path: &str) -> Option<(PathBuf, String)> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok().map(PathBuf::from);
    for (folder, prefix) in [("mods", "mods://"), ("assets", "")] {
        let candidates = [PathBuf::from(folder), Path::new("..").join(folder)]
            .into_iter()
            .chain(manifest_dir.as_ref().map(|m| m.join(folder)));
        for base in candidates {
            let full = base.join(relative_path);
            if full.is_file() {
                return Some((full, format!("{prefix}{relative_path}")));
            }
        }
    }
    None
}

/// HTML is read from disk (not via the `AssetServer`), so it has no change events; this
/// timer polls its mtime instead. Scripts *do* use the `AssetServer` and arrive as events.
#[derive(Resource)]
pub(crate) struct HtmlHotReloadState {
    timer: Timer,
}

impl Default for HtmlHotReloadState {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.25, TimerMode::Repeating),
        }
    }
}

/// Rebuild every open menu when its HTML or script changes on disk.
pub(crate) fn reload_menus_on_asset_change(
    time: Res<Time>,
    mut hot: ResMut<HtmlHotReloadState>,
    mut script_events: MessageReader<AssetEvent<ScriptAsset>>,
    q_roots: Query<&MenuRoot>,
    manager: Res<MenuManager>,
    mut commands: Commands,
) {
    let modified_scripts: Vec<AssetId<ScriptAsset>> = script_events
        .read()
        .filter_map(|e| if let AssetEvent::Modified { id } = e { Some(*id) } else { None })
        .collect();

    let script_modified = q_roots.iter().any(|r| {
        r.script
            .as_ref()
            .is_some_and(|h| modified_scripts.contains(&h.id()))
    });

    let html_modified = hot.timer.tick(time.delta()).just_finished()
        && q_roots.iter().any(|r| {
            std::fs::metadata(&r.html_path)
                .ok()
                .and_then(|m| m.modified().ok())
                != r.html_modified
        });

    if (script_modified || html_modified) && manager.any_open() {
        commands.queue(|world: &mut World| {
            let names: Vec<String> = world
                .resource::<MenuManager>()
                .open_names()
                .map(str::to_string)
                .collect();
            while close_top_menu(world) {}
            for name in names {
                open_menu_in_world(world, &name);
            }
        });
    }
}
