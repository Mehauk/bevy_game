mod craters;
mod droplet;
mod flow;
mod geom;
mod lakes;
mod precipitation;
mod rivers;
mod thermal;
mod waterfalls;
mod wind;

#[cfg(test)]
mod test_support;

pub use craters::{CraterConfig, apply_craters, place_craters};
pub use droplet::{
    ErosionConfig, droplet_erosion, droplet_erosion_with_hardness, erosion_geometry,
    scaled_droplet_count,
};
pub use flow::{FlowField, Hydrology, bake_flow_vectors, flow_accumulation, priority_flood};
pub use lakes::{Lake, find_lakes, validate_lake_surfaces};
pub use precipitation::{PrecipitationInput, generate_precipitation_blended};
pub use rivers::{carve_rivers, river_paths};
pub use thermal::{repose_tan, thermal_relax_repose};
pub use waterfalls::{Waterfall, carve_plunge_pools, find_waterfalls};
pub use wind::{apply_wind_erosion, wind_erodibility_field};
