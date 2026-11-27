use bevy::{
    asset::embedded_asset,
    prelude::*,
    render::RenderApp,
    shader::{Shader, load_shader_library},
};

use buoyancy::BuoyancyPlugin;
use cascades::CascadesPlugin;
use inputs::InputsPlugin;
use interaction::InteractionPlugin;
use material::OceanMaterialPlugin;
use ocean::OceanPlugin;
use passes::{CascadeShaders, PassesPlugin};
use query::QueryPlugin;
use splines::SplinesPlugin;
use underwater::UnderwaterPlugin;

pub mod buoyancy;
pub mod cascades;
pub mod compute;
pub mod inputs;
pub mod interaction;
pub mod lod;
pub mod material;
pub mod ocean;
pub mod passes;
pub mod query;
pub mod reference;
pub mod settings;
pub mod splines;
pub mod underwater;

pub use cascades::{CascadeData, Cascades};
pub use inputs::{
    ClipCarve, DynWaveEmitter, FlowSpline, FoamSpline, SeaFloorHeightGrid, WaterQueryPoint,
};
pub use lod::LodTransforms;
pub use ocean::{OceanRoot, OceanViewer};
pub use passes::ExtractedFrame;
pub use query::{WaterSample, WaterSampler};
pub use settings::{OceanSettings, WaveSettings};

pub struct CrestWaterPlugin;

fn shader(asset_server: &AssetServer, name: &'static str) -> Handle<Shader> {
    asset_server.load(format!("embedded://crest_water/shaders/{name}.wesl"))
}

impl Plugin for CrestWaterPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/crest_math.wesl");
        embedded_asset!(app, "shaders/seafloor_depth.wesl");
        embedded_asset!(app, "shaders/spectrum_init.wesl");
        embedded_asset!(app, "shaders/spectrum_update.wesl");
        embedded_asset!(app, "shaders/fft.wesl");
        embedded_asset!(app, "shaders/fft_bake.wesl");
        embedded_asset!(app, "shaders/shape_combine.wesl");
        embedded_asset!(app, "shaders/dynwaves.wesl");
        embedded_asset!(app, "shaders/flow.wesl");
        embedded_asset!(app, "shaders/foam.wesl");
        embedded_asset!(app, "shaders/fill.wesl");
        embedded_asset!(app, "shaders/copy_slice.wesl");
        embedded_asset!(app, "shaders/copy_foam.wesl");

        let asset_server = app.world().resource::<AssetServer>().clone();
        let shaders = CascadeShaders {
            seafloor_depth: shader(&asset_server, "seafloor_depth"),
            spectrum_init: shader(&asset_server, "spectrum_init"),
            spectrum_update: shader(&asset_server, "spectrum_update"),
            fft: shader(&asset_server, "fft"),
            fft_bake: shader(&asset_server, "fft_bake"),
            shape_combine: shader(&asset_server, "shape_combine"),
            dynwaves: shader(&asset_server, "dynwaves"),
            flow: shader(&asset_server, "flow"),
            foam: shader(&asset_server, "foam"),
            fill: shader(&asset_server, "fill"),
            copy_slice: shader(&asset_server, "copy_slice"),
            copy_foam: shader(&asset_server, "copy_foam"),
        };

        app.init_resource::<OceanSettings>()
            .insert_resource(shaders.clone())
            .add_plugins((
                CascadesPlugin,
                InputsPlugin,
                OceanPlugin,
                PassesPlugin,
                QueryPlugin,
                SplinesPlugin,
                BuoyancyPlugin,
                InteractionPlugin,
                OceanMaterialPlugin,
                UnderwaterPlugin,
            ));

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(shaders);
        }
    }
}
