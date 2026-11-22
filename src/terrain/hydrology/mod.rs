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

pub use craters::{Crater, CraterConfig, apply_craters, place_craters};
pub use droplet::{
    DROPLET_REFERENCE_CELLS, EROSION_DEPOSIT_RADIUS_METRES, EROSION_ERODE_RADIUS_METRES,
    EROSION_STEP_METRES, ErosionConfig, UNCONSOLIDATED_HARDNESS, droplet_erosion,
    droplet_erosion_with_hardness, erosion_geometry, scaled_droplet_count,
};
pub use flow::{FlowField, Hydrology, bake_flow_vectors, flow_accumulation, priority_flood};
pub use lakes::{Lake, find_lakes};
pub use precipitation::{
    PrecipitationInput, generate_precipitation, generate_precipitation_blended,
    precipitation_intensity,
};
pub use rivers::{carve_rivers, river_cells, river_paths, smooth_once};
pub use thermal::{repose_tan, thermal_relax, thermal_relax_repose};
pub use waterfalls::{Waterfall, carve_plunge_pools, find_waterfalls};
pub use wind::{apply_wind_erosion, wind_erodibility_field};
