mod camera;
mod character;
mod commands;
mod debug_views;
mod dev;
mod distance_field;
mod docgen;
mod input;
mod integration;
mod profiler;
mod ripples;
mod screenshot;
mod state;
mod terrain;
mod utils;

use avian3d::prelude::*;
use bevy::{
    asset::io::AssetSourceBuilder,
    dev_tools::fps_overlay::FpsOverlayPlugin,
    prelude::*,
};
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_mod_scripting::BMSPlugin;
use bevy_script_menus::{MenuAssetRoot, MenuConfig, MenuPlugin};
use bevy_shader_kit::ShaderKitPlugin;

use camera::CameraPlugin;
use character::CharacterPlugin;
use commands::CommandsPlugin;
use debug_views::DebugViewPlugin;
use dev::DevUtilsPlugin;
use distance_field::DistanceFieldPlugin;
use docgen::DocgenPlugin;
use input::InputPlugin;
use integration::IntegrationPlugin;
use profiler::ProfilerPlugin;
use ripples::RipplePlugin;
use screenshot::ScreenshotPlugin;
use state::StatePlugin;
use terrain::TerrainPlugin;

fn main() {
    let _ = std::fs::create_dir_all("mods");

    App::new()
        .register_asset_source("mods", AssetSourceBuilder::platform_default("mods", None))
        .insert_resource(MenuConfig {
            roots: vec![
                MenuAssetRoot::new("mods", "mods://"),
                MenuAssetRoot::new("assets", ""),
            ],
            ..default()
        })
        .add_plugins((
            DefaultPlugins,
            BrpExtrasPlugin::default(),
            PhysicsPlugins::default(),
            BMSPlugin,
            FpsOverlayPlugin::default(),
            CommandsPlugin,
            InputPlugin,
            MenuPlugin,
        ))
        .add_plugins((
            StatePlugin,
            ShaderKitPlugin,
            DevUtilsPlugin,
            ProfilerPlugin,
            CameraPlugin,
            CharacterPlugin,
            DebugViewPlugin,
            DistanceFieldPlugin,
            TerrainPlugin,
            RipplePlugin,
            ScreenshotPlugin,
            DocgenPlugin,
            IntegrationPlugin,
        ))
        .run();
}