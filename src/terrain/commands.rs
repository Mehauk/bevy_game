use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::utils::registry::commands::{CommandArgs, CommandOutput};
use crate::utils::TeleportRequest;

use crate::terrain::bench;
use crate::terrain::job::queue_regeneration;
use crate::terrain::job::TerrainJob;
use crate::terrain::map::TerrainMap;
use crate::terrain::TerrainConfig;
use crate::terrain::hydrology::Lake;
use crate::terrain::material::DebugMaterialMode;
use crate::terrain::preview;
use crate::terrain::region::Region;
use crate::terrain::season::{Season, SeasonClock};
use crate::terrain::seed::WorldSeed;
use crate::terrain::macro_landform::WorldShape;
use crate::terrain::topology::WorldTopology;
use crate::terrain::{DebugView, TerrainDebugView};

fn point(args: &CommandArgs) -> Option<(f32, f32)> {
    let x = match args.args.first() {
        Some(raw) => raw.parse::<f32>().ok()?,
        None => 0.0,
    };
    let z = match args.args.get(1) {
        Some(raw) => raw.parse::<f32>().ok()?,
        None => 0.0,
    };
    Some((x, z))
}

fn debug_view_label(view: DebugView) -> &'static str {
    match view {
        DebugView::None => "none",
        DebugView::CellBorders => "cells",
        DebugView::Region => "region",
        DebugView::Slope => "slope",
        DebugView::Precipitation => "precipitation",
        DebugView::Flow => "flow",
        DebugView::FloodZone => "flood",
    }
}

pub fn set_cell_height(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut terrain_map: ResMut<TerrainMap>,
) {
    let usage = "error: usage: set_cell_height <cell_id> <height>";
    let Some(cell_id) = args.args.first().and_then(|s| s.parse::<usize>().ok()) else {
        output.set(usage);
        warn!("Usage: set_cell_height <cell_id> <height>");
        return;
    };
    let Some(height) = args.get_f32(1) else {
        output.set(usage);
        warn!("Usage: set_cell_height <cell_id> <height>");
        return;
    };

    if terrain_map.set_cell_base_height(cell_id, height) {
        let text = format!("cell {cell_id} base_height set to {height:.2}");
        output.set(text.clone());
        info!("{text}");
    } else {
        let text = format!(
            "error: cell {cell_id} not found (valid ids: 0..{})",
            terrain_map.cells().len()
        );
        output.set(text.clone());
        warn!("{text}");
    }
}

pub fn cell_info(args: Res<CommandArgs>, mut output: ResMut<CommandOutput>, terrain_map: Res<TerrainMap>) {
    let Some((x, z)) = point(&args) else {
        output.set("error: usage: cell_info [x] [z] (coordinates must be numbers)");
        return;
    };
    let mut lines = vec![format!(
        "world: {} cells | topology {} | shape {} | seed {} | sea level {:.1}",
        terrain_map.cells().len(),
        terrain_map.topology().label(),
        terrain_map.shape().label(),
        terrain_map.seed().0,
        terrain_map.sea_level()
    )];

    let Some(id) = terrain_map.cell_id_at(x, z) else {
        lines.push(format!("error: no cell at ({x:.1}, {z:.1})"));
        let text = lines.join("\n");
        output.set(text.clone());
        info!("{text}");
        return;
    };
    let Some(cell) = terrain_map.cell(id) else {
        lines.push(format!("error: no cell at ({x:.1}, {z:.1})"));
        let text = lines.join("\n");
        output.set(text.clone());
        info!("{text}");
        return;
    };

    let parameters = &cell.parameters;
    let neighbours = terrain_map
        .cell_neighbours(id)
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let eligible = terrain_map
        .eligible_regions_at(x, z)
        .iter()
        .map(|region| region.label())
        .collect::<Vec<_>>()
        .join(", ");
    let sub_variants = cell.region.sub_variants().len();

    lines.push(format!("cell {} at ({:.1}, {:.1})", cell.id, cell.center.x, cell.center.y));
    lines.push(format!("  {:<14}{}", "region", cell.region.label()));
    lines.push(format!(
        "  {:<14}{} ({} of {})",
        "variant",
        cell.sub_variant_label().unwrap_or("none"),
        cell.sub_variant + 1,
        sub_variants
    ));
    lines.push(format!("  {:<14}{:.2}", "height", terrain_map.height_at(x, z)));
    lines.push(format!("  {:<14}{:.2}", "base height", cell.base_height));
    lines.push(format!("  {:<14}{}", "temperature", parameters.temperature.label()));
    lines.push(format!("  {:<14}{}", "humidity", parameters.humidity.label()));
    lines.push(format!("  {:<14}{}", "wind", parameters.wind.label()));
    lines.push(format!("  {:<14}{}", "sun", parameters.sun_exposure.label()));
    lines.push(format!("  {:<14}{}", "precip", parameters.precipitation.label()));
    lines.push(format!("  {:<14}{}", "band", parameters.base_height.label()));
    lines.push(format!("  {:<14}{}", "slope", parameters.base_gradient.slope.label()));
    lines.push(format!("  {:<14}{}", "hydrology", parameters.hydrology.label()));
    lines.push(format!("  {:<14}{}", "substrate", parameters.substrate.label()));
    lines.push(format!("  {:<14}{}", "geothermal", parameters.geothermal.label()));
    lines.push(format!("  {:<14}{}", "salinity", parameters.salinity.label()));
    lines.push(format!("  {:<14}{}", "neighbours", if neighbours.is_empty() { "none".to_string() } else { neighbours }));
    lines.push(format!("  {:<14}{}", "eligible", if eligible.is_empty() { "none".to_string() } else { eligible }));

    let text = lines.join("\n");
    output.set(text.clone());
    info!("{text}");
}

pub fn inspect_point(args: Res<CommandArgs>, mut output: ResMut<CommandOutput>, terrain_map: Res<TerrainMap>) {
    let Some((x, z)) = point(&args) else {
        output.set("error: usage: inspect_point <x> <z> (coordinates must be numbers)");
        return;
    };
    let lines = vec![
        format!("point ({x:.1}, {z:.1})"),
        format!("  {:<14}{:.2}", "height", terrain_map.height_at(x, z)),
        format!("  {:<14}{:.2}", "base height", terrain_map.base_height_at(x, z)),
        format!("  {:<14}{:.2}", "sea level", terrain_map.sea_level()),
        format!(
            "  {:<14}{}",
            "underwater",
            if terrain_map.is_underwater(x, z) { "yes" } else { "no" }
        ),
        format!("  {:<14}{:.2}", "slope", terrain_map.slope_at(x, z)),
        format!(
            "  {:<14}{}",
            "region",
            terrain_map
                .region_at(x, z)
                .map(|region| region.label())
                .unwrap_or("none")
        ),
    ];
    let text = lines.join("\n");
    output.set(text.clone());
    info!("{text}");
}

pub fn inspect_hydrology(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    terrain_map: Res<TerrainMap>,
) {
    let Some((x, z)) = point(&args) else {
        output.set("error: usage: inspect_hydrology <x> <z> (coordinates must be numbers)");
        return;
    };
    let lines = vec![
        format!("point ({x:.1}, {z:.1})"),
        format!("  {:<14}{:.3}", "precipitation", terrain_map.precipitation_at(x, z)),
        format!("  {:<14}{:.1}", "accumulation", terrain_map.accumulation_at(x, z)),
        format!("  {:<14}{}", "lakes", terrain_map.lakes().len()),
        format!("  {:<14}{}", "rivers", terrain_map.river_paths().len()),
        format!("  {:<14}{}", "waterfalls", terrain_map.waterfalls().len()),
    ];
    let text = lines.join("\n");
    output.set(text.clone());
    info!("{text}");
}

pub fn set_world_topology(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut map: ResMut<TerrainMap>,
    mut job: ResMut<TerrainJob>,
) {
    let Some(label) = args.args.first() else {
        output.set("error: usage: set_world_topology <finite|torus|infinite>");
        warn!("Usage: set_world_topology <finite|torus|infinite>");
        return;
    };
    let Some(topology) = WorldTopology::parse(label) else {
        let text = format!("error: unknown topology '{label}' (finite | torus | infinite)");
        output.set(text.clone());
        warn!("{text}");
        return;
    };
    map.config_mut().topology = topology;
    queue_regeneration(&mut job);
    let text = format!("topology set to {} (regeneration queued)", topology.label());
    output.set(text.clone());
    info!("{text}");
}

pub fn set_world_shape(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut map: ResMut<TerrainMap>,
    mut job: ResMut<TerrainJob>,
) {
    let Some(label) = args.args.first() else {
        output.set("error: usage: set_world_shape <continent|archipelago|island_chain|inland_sea>");
        warn!("Usage: set_world_shape <continent|archipelago|island_chain|inland_sea>");
        return;
    };
    let Some(shape) = WorldShape::parse(label) else {
        let text = format!(
            "error: unknown shape '{label}' (continent | archipelago | island_chain | inland_sea)"
        );
        output.set(text.clone());
        warn!("{text}");
        return;
    };
    map.config_mut().shape = shape;
    queue_regeneration(&mut job);
    let text = format!("shape set to {} (regeneration queued)", shape.label());
    output.set(text.clone());
    info!("{text}");
}

pub fn set_world_seed(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut map: ResMut<TerrainMap>,
    mut job: ResMut<TerrainJob>,
) {
    let Some(seed) = args.args.first().and_then(|s| s.parse::<u64>().ok()) else {
        output.set("error: usage: set_world_seed <u64>");
        warn!("Usage: set_world_seed <u64>");
        return;
    };
    map.config_mut().seed = WorldSeed::new(seed);
    queue_regeneration(&mut job);
    let text = format!("seed set to {seed} (regeneration queued)");
    output.set(text.clone());
    info!("{text}");
}

pub fn regenerate_terrain(map: Res<TerrainMap>, mut output: ResMut<CommandOutput>, mut job: ResMut<TerrainJob>) {
    let text = format!(
        "regeneration queued (seed {}, topology {}, shape {})",
        map.seed().0,
        map.topology().label(),
        map.shape().label()
    );
    output.set(text.clone());
    info!("{text}");
    queue_regeneration(&mut job);
}

pub fn set_season(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut clock: ResMut<SeasonClock>,
    mut map: ResMut<TerrainMap>,
) {
    let Some(label) = args.args.first() else {
        output.set("error: usage: set_season <spring|summer|fall|winter>");
        warn!("Usage: set_season <spring|summer|fall|winter>");
        return;
    };
    let Some(season) = Season::parse(label) else {
        let text = format!("error: unknown season '{label}' (spring | summer | fall | winter)");
        output.set(text.clone());
        warn!("{text}");
        return;
    };
    clock.set_season(season);
    clock.running = false;
    map.set_season(season);
    let text = format!("season set to {} (cycle paused)", season.label());
    output.set(text.clone());
    info!("{text}");
}

pub fn toggle_season_cycle(mut output: ResMut<CommandOutput>, mut clock: ResMut<SeasonClock>) {
    clock.running = !clock.running;
    let text = format!(
        "seasonal cycle {} (season {})",
        if clock.running { "running" } else { "paused" },
        clock.season.label()
    );
    output.set(text.clone());
    info!("{text}");
}

pub fn debug_view(args: Res<CommandArgs>, mut output: ResMut<CommandOutput>, mut view: ResMut<TerrainDebugView>) {
    let Some(label) = args.args.first() else {
        output.set("error: usage: debug_view <none|cells|region|slope|precipitation|flow|flood>");
        warn!("Usage: debug_view <none|cells|region|slope|precipitation|flow|flood>");
        return;
    };
    let Some(parsed) = DebugView::parse(label) else {
        let text = format!(
            "error: unknown debug view '{label}' (none|cells|region|slope|precipitation|flow|flood)"
        );
        output.set(text.clone());
        warn!("{text}");
        return;
    };
    view.0 = parsed;
    let text = format!("debug view set to {}", debug_view_label(parsed));
    output.set(text.clone());
    info!("{text}");
}

pub fn debug_material(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut mode: ResMut<DebugMaterialMode>,
) {
    let Some(label) = args.args.first() else {
        output.set("error: usage: debug_material <textured|flat_color|uv_grid>");
        warn!("Usage: debug_material <textured|flat_color|uv_grid>");
        return;
    };
    let Some(parsed) = DebugMaterialMode::parse(label) else {
        let text = format!("error: unknown material mode '{label}' (textured|flat_color|uv_grid)");
        output.set(text.clone());
        warn!("{text}");
        return;
    };
    *mode = parsed;
    let text = format!("material mode set to {}", parsed.label());
    output.set(text.clone());
    info!("{text}");
}

pub fn export_preview(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    map: Res<TerrainMap>,
    clock: Res<SeasonClock>,
) {
    let layer = args.args.first().map(|s| s.as_str()).unwrap_or("all");
    match preview::export(&map, &clock, layer) {
        Ok(files) => {
            let mut lines = vec![format!("wrote {} preview file(s)", files.len())];
            for file in &files {
                lines.push(format!("  {}", file.display()));
            }
            let text = lines.join("\n");
            output.set(text.clone());
            info!("{text}");
        }
        Err(error) => {
            let text = format!("error: preview export failed: {error}");
            output.set(text.clone());
            warn!("{text}");
        }
    }
}

pub fn benchmark_terrain(args: Res<CommandArgs>, mut output: ResMut<CommandOutput>, map: Res<TerrainMap>) {
    let benchmark = match args.args.first().map(|s| s.as_str()) {
        Some("compact") => bench::run(&TerrainConfig::compact()),
        Some("default") => bench::run_default(),
        Some("large") => bench::run_large(),
        _ => bench::run(map.config()),
    };
    let violations = benchmark.violations();
    let mut text = benchmark.report();
    if !violations.is_empty() {
        text.push_str("\nover budget:");
        for (stage, millis, budget) in &violations {
            text.push_str(&format!("\n  {stage} {millis:.1} ms (budget {budget:.1} ms)"));
        }
    }
    output.set(text);
    for line in benchmark.report().lines() {
        info!("{line}");
    }
    for (stage, millis, budget) in violations {
        warn!("Stage {stage} took {millis:.1} ms, budget is {budget:.1} ms");
    }
}

fn feature_target(map: &TerrainMap, x: f32, z: f32) -> Vec3 {
    Vec3::new(x, map.height_at(x, z), z)
}

fn lake_centre(map: &TerrainMap, lake: &Lake) -> Vec2 {
    let heights = map.heights();
    let mut sum = Vec2::ZERO;
    for &cell in &lake.cells {
        let (cx, cz) = heights.coords(cell);
        sum += heights.world_of(cx, cz);
    }
    sum / lake.cells.len().max(1) as f32
}

fn parse_index(args: &CommandArgs) -> Option<usize> {
    match args.args.get(1) {
        Some(raw) => raw.parse::<usize>().ok(),
        None => Some(0),
    }
}

pub fn find_feature(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    terrain_map: Res<TerrainMap>,
    mut teleport: ResMut<TeleportRequest>,
) {
    let Some(kind) = args.args.first().map(|s| s.to_ascii_lowercase()) else {
        output.set("error: usage: find_feature <lake|waterfall|river|region> [index|label]");
        return;
    };
    let mut lines = Vec::new();
    let target = match kind.as_str() {
        "lake" => {
            let Some(index) = parse_index(&args) else {
                output.set(format!(
                    "error: lake index must be a number, got '{}'",
                    args.args[1]
                ));
                return;
            };
            let Some(lake) = terrain_map.lakes().get(index) else {
                output.set(format!(
                    "error: no lake {index} (this world has {} lakes)",
                    terrain_map.lakes().len()
                ));
                return;
            };
            let centre = lake_centre(&terrain_map, lake);
            lines.push(format!("lake {index} at ({:.1}, {:.1})", centre.x, centre.y));
            lines.push(format!("  {:<14}{:.2}", "surface", lake.surface));
            lines.push(format!("  {:<14}{:.2}", "depth", lake.depth));
            lines.push(format!("  {:<14}{}", "cells", lake.cells.len()));
            feature_target(&terrain_map, centre.x, centre.y)
        }
        "waterfall" | "falls" => {
            let Some(index) = parse_index(&args) else {
                output.set(format!(
                    "error: waterfall index must be a number, got '{}'",
                    args.args[1]
                ));
                return;
            };
            let Some(fall) = terrain_map.waterfalls().get(index) else {
                output.set(format!(
                    "error: no waterfall {index} (this world has {} waterfalls)",
                    terrain_map.waterfalls().len()
                ));
                return;
            };
            lines.push(format!(
                "waterfall {index} at ({:.1}, {:.1})",
                fall.lip.x, fall.lip.y
            ));
            lines.push(format!("  {:<14}{:.2}", "drop", fall.drop));
            lines.push(format!("  {:<14}{:.2}", "pool depth", fall.pool_depth));
            feature_target(&terrain_map, fall.lip.x, fall.lip.y)
        }
        "river" => {
            let Some(index) = parse_index(&args) else {
                output.set(format!(
                    "error: river index must be a number, got '{}'",
                    args.args[1]
                ));
                return;
            };
            let Some(path) = terrain_map.river_paths().get(index) else {
                output.set(format!(
                    "error: no river {index} (this world has {} rivers)",
                    terrain_map.river_paths().len()
                ));
                return;
            };
            let Some(&mouth) = path.last() else {
                output.set(format!("error: river {index} has no mouth (empty path)"));
                return;
            };
            let heights = terrain_map.heights();
            let (mx, mz) = heights.coords(mouth);
            let point = heights.world_of(mx, mz);
            lines.push(format!("river {index} mouth at ({:.1}, {:.1})", point.x, point.y));
            lines.push(format!("  {:<14}{}", "cells", path.len()));
            lines.push(format!(
                "  {:<14}{:.1}",
                "accumulation",
                terrain_map.accumulation_at(point.x, point.y)
            ));
            feature_target(&terrain_map, point.x, point.y)
        }
        "region" => {
            if args.args.len() < 2 {
                output.set("error: usage: find_feature region <label>");
                return;
            }
            let label = args.args[1..].join(" ").to_ascii_lowercase().replace('_', " ");
            let Some(definition) = Region::all().iter().find(|definition| {
                definition.label.to_ascii_lowercase() == label
            }) else {
                let valid = Region::all()
                    .iter()
                    .map(|definition| definition.label)
                    .collect::<Vec<_>>()
                    .join(", ");
                output.set(format!("error: unknown region '{label}' (known: {valid})"));
                return;
            };
            let Some(cell) = terrain_map
                .cells()
                .iter()
                .find(|cell| cell.region == definition.region)
            else {
                output.set(format!(
                    "error: region '{}' is not present in this world",
                    definition.label
                ));
                return;
            };
            lines.push(format!(
                "region '{}' at ({:.1}, {:.1})",
                definition.label, cell.center.x, cell.center.y
            ));
            feature_target(&terrain_map, cell.center.x, cell.center.y)
        }
        other => {
            output.set(format!(
                "error: unknown feature '{other}' (lake | waterfall | river | region)"
            ));
            return;
        }
    };
    lines.push(format!("  {:<14}{:.1}", "height", target.y));
    lines.push("teleport requested".to_string());
    teleport.target = Some(target);
    let text = lines.join("\n");
    output.set(text.clone());
    info!("{text}");
}

pub fn world_summary(mut output: ResMut<CommandOutput>, terrain_map: Res<TerrainMap>) {
    let config = terrain_map.config();
    let heights = terrain_map.heights();
    let sea = terrain_map.sea_level();
    let land = heights.values.iter().filter(|&&value| value >= sea).count();
    let land_percent = 100.0 * land as f32 / heights.len().max(1) as f32;
    let mut biomes: BTreeMap<&'static str, usize> = BTreeMap::new();
    for cell in terrain_map.cells() {
        *biomes.entry(cell.region.label()).or_default() += 1;
    }
    let total = terrain_map.cells().len().max(1);
    let mut ranked: Vec<(&'static str, usize)> = biomes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let mut lines = vec![
        "world summary".to_string(),
        format!("  {:<14}{}", "seed", config.seed.0),
        format!("  {:<14}{}", "topology", terrain_map.topology().label()),
        format!("  {:<14}{}", "shape", terrain_map.shape().label()),
        format!("  {:<14}{:.1} m", "size", config.size),
        format!("  {:<14}{}", "subdivisions", config.subdivisions),
        format!(
            "  {:<14}{} x {}",
            "cell grid", config.cell_grid_dim, config.cell_grid_dim
        ),
        format!("  {:<14}{:.2}", "sea level", sea),
        format!(
            "  {:<14}{:.1} .. {:.1}",
            "height range",
            heights.min(),
            heights.max()
        ),
        format!("  {:<14}{:.1}%", "land", land_percent),
        format!(
            "  {:<14}{} lakes, {} rivers, {} waterfalls",
            "hydrology",
            terrain_map.lakes().len(),
            terrain_map.river_paths().len(),
            terrain_map.waterfalls().len()
        ),
        "  biomes:".to_string(),
    ];
    for (label, count) in ranked {
        lines.push(format!(
            "    {:<28}{:>4} ({:.1}%)",
            label,
            count,
            100.0 * count as f32 / total as f32
        ));
    }
    let text = lines.join("\n");
    output.set(text.clone());
    info!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::registry::commands::{CommandAppExt, CommandHistory, CommandRegistryPlugin, RunCommand};

    fn palette_app() -> App {
        let mut app = App::new();
        app.add_plugins(CommandRegistryPlugin)
            .insert_resource(TerrainMap::compact())
            .init_resource::<TerrainJob>()
            .init_resource::<SeasonClock>()
            .init_resource::<TerrainDebugView>()
            .init_resource::<TeleportRequest>()
            .insert_resource(DebugMaterialMode::Textured);
        app.register_command("set_cell_height", "d", set_cell_height);
        app.register_command("cell_info", "d", cell_info);
        app.register_command("inspect_point", "d", inspect_point);
        app.register_command("inspect_hydrology", "d", inspect_hydrology);
        app.register_command("set_world_topology", "d", set_world_topology);
        app.register_command("set_world_shape", "d", set_world_shape);
        app.register_command("set_world_seed", "d", set_world_seed);
        app.register_command("regenerate_terrain", "d", regenerate_terrain);
        app.register_command("set_season", "d", set_season);
        app.register_command("toggle_season_cycle", "d", toggle_season_cycle);
        app.register_command("debug_view", "d", debug_view);
        app.register_command("debug_material", "d", debug_material);
        app.register_command("export_preview", "d", export_preview);
        app.register_command("benchmark_terrain", "d", benchmark_terrain);
        app.register_command("find_feature", "d", find_feature);
        app.register_command("world_summary", "d", world_summary);
        app
    }

    fn run_palette(app: &mut App, line: &str) -> String {
        app.world_mut()
            .resource_mut::<Messages<RunCommand>>()
            .write(RunCommand(line.to_string()));
        app.update();
        app.world()
            .resource::<CommandHistory>()
            .entries
            .back()
            .and_then(|entry| entry.output.clone())
            .unwrap_or_else(|| panic!("command '{line}' produced no CommandOutput"))
    }

    fn assert_copy_friendly(text: &str) {
        assert!(!text.trim().is_empty(), "output must not be blank");
        assert!(!text.contains('\u{1b}'), "output must not contain ansi escapes");
        for line in text.lines() {
            assert!(!line.ends_with(' '), "trailing whitespace in line: '{line}'");
        }
    }

    #[test]
    fn every_terrain_command_reports_output() {
        let invocations = [
            ("set_cell_height 3 4.5", "base_height set to 4.50"),
            ("cell_info 10 10", "cell "),
            ("inspect_point 10 10", "point (10.0, 10.0)"),
            ("inspect_hydrology 10 10", "precipitation"),
            ("set_world_topology torus", "topology set to finite_torus"),
            ("set_world_shape archipelago", "shape set to archipelago"),
            ("set_world_seed 777", "seed set to 777"),
            ("regenerate_terrain", "regeneration queued"),
            ("set_season winter", "season set to winter"),
            ("toggle_season_cycle", "seasonal cycle running"),
            ("debug_view slope", "debug view set to slope"),
            ("debug_material flat_color", "material mode set to flat_color"),
            ("export_preview height", "preview file(s)"),
            ("benchmark_terrain compact", "terrain benchmark"),
            ("world_summary", "world summary"),
        ];
        for (line, needle) in invocations {
            let mut app = palette_app();
            let output = run_palette(&mut app, line);
            assert_copy_friendly(&output);
            assert!(
                output.contains(needle),
                "'{line}' output must contain '{needle}', got:\n{output}"
            );
        }
    }

    #[test]
    fn failures_report_explicit_error_lines() {
        let cases = [
            "set_cell_height",
            "set_cell_height abc 5",
            "set_cell_height 3",
            "set_cell_height 99999 5",
            "cell_info abc",
            "inspect_point 1x",
            "inspect_hydrology 1x",
            "set_world_topology",
            "set_world_topology sphere",
            "set_world_shape",
            "set_world_shape donut",
            "set_world_seed",
            "set_world_seed notanumber",
            "set_season",
            "set_season monsoon",
            "debug_view",
            "debug_view bogus",
            "debug_material",
            "debug_material bogus",
            "export_preview bogus",
        ];
        for line in cases {
            let mut app = palette_app();
            let output = run_palette(&mut app, line);
            assert_copy_friendly(&output);
            assert!(
                output.starts_with("error:"),
                "'{line}' must report an error line, got:\n{output}"
            );
        }
    }

    #[test]
    fn unknown_cell_reports_valid_range() {
        let mut app = palette_app();
        let total = app.world().resource::<TerrainMap>().cells().len();
        let output = run_palette(&mut app, "set_cell_height 99999 5");
        assert_eq!(
            output,
            format!("error: cell 99999 not found (valid ids: 0..{total})")
        );
    }

    #[test]
    fn mutating_commands_confirm_the_change() {
        let mut app = palette_app();
        let output = run_palette(&mut app, "set_cell_height 3 4.5");
        assert!(output.contains("4.50"), "confirmation must echo the new value");
        let map = app.world().resource::<TerrainMap>();
        assert!((map.cell(3).unwrap().base_height - 4.5).abs() < 1e-6);

        let mut app = palette_app();
        let output = run_palette(&mut app, "set_world_topology torus");
        assert!(output.contains("finite_torus"));
        assert_eq!(
            app.world().resource::<TerrainMap>().topology(),
            WorldTopology::FiniteTorus
        );

        let mut app = palette_app();
        let output = run_palette(&mut app, "set_season winter");
        assert!(output.contains("winter"));
        assert_eq!(app.world().resource::<SeasonClock>().season, Season::Winter);
        assert!(!app.world().resource::<SeasonClock>().running);

        let mut app = palette_app();
        let output = run_palette(&mut app, "toggle_season_cycle");
        assert!(output.contains("running"));
        assert!(app.world().resource::<SeasonClock>().running);

        let mut app = palette_app();
        let output = run_palette(&mut app, "debug_view slope");
        assert!(output.contains("slope"));
        assert_eq!(app.world().resource::<TerrainDebugView>().0, DebugView::Slope);

        let mut app = palette_app();
        let output = run_palette(&mut app, "debug_material uv_grid");
        assert!(output.contains("uv_grid"));
        assert_eq!(
            *app.world().resource::<DebugMaterialMode>(),
            DebugMaterialMode::UvGrid
        );
    }

    #[test]
    fn cell_info_is_aligned_and_free_of_debug_dumps() {
        let mut app = palette_app();
        let output = run_palette(&mut app, "cell_info 10 10");
        assert!(output.contains("world: "));
        assert!(output.contains("neighbours"), "neighbour list must be present");
        assert!(!output.contains('{') && !output.contains('}'), "no struct debug dumps");
        assert!(output.lines().nth(1).unwrap().starts_with("cell "), "second line names the cell");
        for line in output.lines().skip(2) {
            assert!(line.starts_with("  "), "detail lines must be indented: '{line}'");
        }
    }

    #[test]
    fn benchmark_output_lists_every_stage_against_its_budget() {
        let mut app = palette_app();
        let output = run_palette(&mut app, "benchmark_terrain compact");
        assert!(output.contains("terrain benchmark"));
        assert!(output.contains("voronoi"));
        assert!(output.contains("splat_bake"));
        assert!(output.contains("chunk_upload_total"));
    }

    #[test]
    fn world_summary_reports_the_whole_world_at_a_glance() {
        let mut app = palette_app();
        let output = run_palette(&mut app, "world_summary");
        assert_copy_friendly(&output);
        for needle in [
            "world summary",
            "seed",
            "topology",
            "shape",
            "size",
            "subdivisions",
            "sea level",
            "land",
            "lakes",
            "biomes",
        ] {
            assert!(
                output.contains(needle),
                "summary must mention '{needle}', got:\n{output}"
            );
        }
        assert!(
            output.lines().count() > 12,
            "biome distribution must list the world's regions:\n{output}"
        );
    }

    #[test]
    fn find_feature_region_teleports_to_a_cell_of_that_region() {
        let mut app = palette_app();
        let label = app.world().resource::<TerrainMap>().cells()[0]
            .region
            .label();
        let output = run_palette(&mut app, &format!("find_feature region {label}"));
        assert_copy_friendly(&output);
        assert!(
            output.contains(&format!("region '{label}'")),
            "got:\n{output}"
        );
        assert!(output.contains("teleport requested"));
        let target = app
            .world()
            .resource::<TeleportRequest>()
            .target
            .expect("a teleport must be queued");
        assert!(target.is_finite());
        let half = app.world().resource::<TerrainMap>().size() * 0.5;
        assert!(
            target.x.abs() <= half && target.z.abs() <= half,
            "teleport target {target} must stay inside the world"
        );
    }

    #[test]
    fn find_feature_hydro_features_teleport_when_present() {
        for kind in ["lake", "waterfall", "river"] {
            let mut app = palette_app();
            let output = run_palette(&mut app, &format!("find_feature {kind}"));
            assert_copy_friendly(&output);
            if output.starts_with("error: no") {
                continue;
            }
            assert!(
                output.contains("teleport requested"),
                "'{kind}':\n{output}"
            );
            assert!(app.world().resource::<TeleportRequest>().target.is_some());
        }
    }

    #[test]
    fn find_feature_failures_report_explicit_errors() {
        let cases = [
            "find_feature",
            "find_feature bogus",
            "find_feature lake abc",
            "find_feature lake 99999",
            "find_feature waterfall 99999",
            "find_feature river 99999",
            "find_feature region",
            "find_feature region nosuchregion",
        ];
        for line in cases {
            let mut app = palette_app();
            let output = run_palette(&mut app, line);
            assert_copy_friendly(&output);
            assert!(
                output.starts_with("error:"),
                "'{line}' must error, got:\n{output}"
            );
            assert!(app.world().resource::<TeleportRequest>().target.is_none());
        }
    }
}
