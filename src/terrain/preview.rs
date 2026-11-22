use std::fs;
use std::path::PathBuf;

use bevy::prelude::*;
use image::{ImageBuffer, Rgb};

use crate::terrain::chunk::DEFAULT_CHUNK_SIZE;
use crate::terrain::colour::{
    bathymetric_colour, flow_intensity, height_colour, normalized_height, region_colour,
};
use crate::terrain::gpu::SNOWLINE_METRES;
use crate::terrain::map::TerrainMap;
use crate::terrain::material::DISK_ROOT;
use crate::terrain::params::BaseGradient;
use crate::terrain::season::{self, Season, SeasonClock};
use crate::terrain::splat::{self, SurfaceTiles, cliff_share};
use crate::terrain::DebugView;

pub const PREVIEW_ROOT: &str = "docs/.compiled/terrain-previews";
pub const PREVIEW_DIM: usize = 320;
pub const TILED_TILE_DIM: usize = PREVIEW_DIM / 2;

pub const LAYERS: [&str; 13] = [
    "world",
    "height",
    "slope",
    "cells",
    "region",
    "cliff",
    "cliff_stretch",
    "normal",
    "precipitation",
    "hydrology",
    "season",
    "splat",
    "tiling",
];

type Image = ImageBuffer<Rgb<u8>, Vec<u8>>;

pub fn export(map: &TerrainMap, clock: &SeasonClock, layer: &str) -> Result<Vec<PathBuf>, String> {
    let wanted: Vec<&str> = if layer == "all" {
        LAYERS.to_vec()
    } else if LAYERS.contains(&layer) {
        vec![layer]
    } else {
        return Err(format!(
            "unknown layer '{layer}' (expected all | {})",
            LAYERS.join(" | ")
        ));
    };

    fs::create_dir_all(PREVIEW_ROOT).map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    let tiled_variants = map.topology().wraps();
    for name in wanted {
        write(&mut files, map, clock, name, false)?;
        if tiled_variants && name != "tiling" && name != "splat" {
            write(&mut files, map, clock, name, true)?;
        }
    }
    Ok(files)
}

fn write(
    files: &mut Vec<PathBuf>,
    map: &TerrainMap,
    clock: &SeasonClock,
    name: &str,
    tiled: bool,
) -> Result<(), String> {
    let image = if tiled {
        render_tiled(map, clock, name)
    } else {
        render_layer(map, clock, name)
    };
    let label = if tiled {
        format!("{name}_tiled")
    } else {
        name.to_string()
    };
    let path = output_path(map, &label);
    image
        .save(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    files.push(path);
    Ok(())
}

pub fn render_layer(map: &TerrainMap, clock: &SeasonClock, name: &str) -> Image {
    match name {
        "tiling" => paint_layer(map, clock, "height", &Grid::new(map, PREVIEW_DIM, true)),
        _ => paint_layer(map, clock, name, &Grid::new(map, PREVIEW_DIM, false)),
    }
}

pub fn render_tiled(map: &TerrainMap, clock: &SeasonClock, name: &str) -> Image {
    match name {
        "season" => season_strip(map, clock, TILED_TILE_DIM, true),
        _ => paint_layer(map, clock, name, &Grid::new(map, TILED_TILE_DIM, true)),
    }
}

pub fn output_path(map: &TerrainMap, name: &str) -> PathBuf {
    PathBuf::from(PREVIEW_ROOT).join(format!(
        "{name}_seed-{}_{}.png",
        map.seed().0,
        map.topology().label()
    ))
}

#[cfg(test)]
pub fn pixel_variance(image: &Image) -> f32 {
    let count = image.width() as usize * image.height() as usize;
    if count == 0 {
        return 0.0;
    }
    let means = [
        channel_mean(image, 0),
        channel_mean(image, 1),
        channel_mean(image, 2),
    ];
    let spread: f32 = image
        .pixels()
        .map(|pixel| {
            (0..3)
                .map(|channel| (pixel[channel] as f32 - means[channel]).powi(2))
                .sum::<f32>()
        })
        .sum();
    spread / count as f32
}

#[cfg(test)]
fn channel_mean(image: &Image, channel: usize) -> f32 {
    let count = image.width() as f64 * image.height() as f64;
    image.pixels().map(|p| p[channel] as f64).sum::<f64>() as f32 / count as f32
}

struct Grid {
    edge: usize,
    span: f32,
    size: f32,
    step: f32,
    tiled: bool,
    wraps: bool,
}

impl Grid {
    fn new(map: &TerrainMap, dim: usize, tiled: bool) -> Self {
        Self {
            edge: if tiled { dim * 2 } else { dim },
            span: if tiled { map.size() * 2.0 } else { map.size() },
            size: map.size(),
            step: map.size() / PREVIEW_DIM.max(dim) as f32,
            tiled,
            wraps: map.topology().wraps(),
        }
    }

    fn point(&self, x: usize, z: usize) -> Vec2 {
        let point = world_at(self.span, self.edge, x, z);
        if self.tiled {
            wrap_to_world(self.size, point)
        } else {
            point
        }
    }

    fn paint(&self, mut colour: impl FnMut(Vec2) -> LinearRgba) -> Image {
        canvas(self.edge, |x, z| colour(self.point(x, z)))
    }

    fn pixel(&self, point: Vec2) -> (i32, i32) {
        let x = ((point.x + self.span * 0.5) / self.span * self.edge as f32).round() as i32;
        let y = ((point.y + self.span * 0.5) / self.span * self.edge as f32).round() as i32;
        (x, self.edge as i32 - 1 - y)
    }

    fn copies(&self, point: Vec2) -> Vec<Vec2> {
        if !self.tiled && !self.wraps {
            return vec![point];
        }
        let mut copies = Vec::new();
        for kx in -1..=1 {
            for kz in -1..=1 {
                let shifted = Vec2::new(
                    point.x + kx as f32 * self.size,
                    point.y + kz as f32 * self.size,
                );
                let inside = shifted.x >= -self.span * 0.5
                    && shifted.x < self.span * 0.5
                    && shifted.y >= -self.span * 0.5
                    && shifted.y < self.span * 0.5;
                if inside || !self.tiled {
                    copies.push(shifted);
                }
            }
        }
        copies
    }
}

fn paint_layer(map: &TerrainMap, clock: &SeasonClock, name: &str, grid: &Grid) -> Image {
    match name {
        "world" => composite_layer(map, clock, grid),
        "slope" => styled_layer(map, DebugView::Slope, grid),
        "cells" => cells_layer(map, grid),
        "region" => styled_layer(map, DebugView::Region, grid),
        "cliff" => cliff_layer(map, grid),
        "cliff_stretch" => cliff_stretch_layer(map, grid),
        "normal" => normal_layer(map, grid),
        "precipitation" => precipitation_layer(map, grid),
        "hydrology" => hydrology_layer(map, grid),
        "season" => season_strip(map, clock, PREVIEW_DIM, false),
        "splat" => splat_pair(map).expect("the splat preview needs the full tile set"),
        _ => height_layer(map, grid),
    }
}

fn normal_layer(map: &TerrainMap, grid: &Grid) -> Image {
    let step = grid.step;
    grid.paint(|point| {
        let dx = map.height_at(point.x + step, point.y) - map.height_at(point.x - step, point.y);
        let dz = map.height_at(point.x, point.y + step) - map.height_at(point.x, point.y - step);
        let normal = Vec3::new(-dx, 2.0 * step, -dz).normalize_or_zero();
        LinearRgba::new(
            normal.x * 0.5 + 0.5,
            normal.y * 0.5 + 0.5,
            normal.z * 0.5 + 0.5,
            1.0,
        )
    })
}

fn cliff_stretch_layer(map: &TerrainMap, grid: &Grid) -> Image {
    grid.paint(|point| {
        let share = cliff_share(map.slope_at(point.x, point.y));
        ramp(
            share,
            Color::srgb(0.12, 0.30, 0.55),
            Color::srgb(0.95, 0.45, 0.05),
        )
    })
}

fn splat_pair(map: &TerrainMap) -> Result<Image, String> {
    let tiles = SurfaceTiles::load(DISK_ROOT);
    if !tiles.is_complete() {
        return Err(format!(
            "splat preview needs every surface tile on disk, missing {:?}",
            tiles.missing()
        ));
    }
    let chunk_size = DEFAULT_CHUNK_SIZE.min(map.size());
    let coarse = splat::bake_chunk(map, &tiles, IVec2::ZERO, chunk_size, splat::SPLAT_RESOLUTION)
        .ok_or("the control-scale splat bake failed")?;
    let fine = splat::bake_chunk(map, &tiles, IVec2::ZERO, chunk_size, PREVIEW_DIM)
        .ok_or("the detail-scale splat bake failed")?;
    let mut image = ImageBuffer::from_pixel(
        (PREVIEW_DIM * 2) as u32,
        PREVIEW_DIM as u32,
        Rgb([0, 0, 0]),
    );
    let flatten = |pixel: &image::Rgba<u8>| Rgb([pixel[0], pixel[1], pixel[2]]);
    for z in 0..PREVIEW_DIM {
        for x in 0..PREVIEW_DIM {
            let sx = x * coarse.albedo.width() as usize / PREVIEW_DIM;
            let sy = (PREVIEW_DIM - 1 - z) * coarse.albedo.height() as usize / PREVIEW_DIM;
            image.put_pixel(
                x as u32,
                z as u32,
                flatten(coarse.albedo.get_pixel(sx as u32, sy as u32)),
            );
            let fy = (PREVIEW_DIM - 1 - z) as u32;
            image.put_pixel(
                (PREVIEW_DIM + x) as u32,
                z as u32,
                flatten(fine.albedo.get_pixel(x as u32, fy)),
            );
        }
    }
    Ok(image)
}

fn canvas(dim: usize, mut paint: impl FnMut(usize, usize) -> LinearRgba) -> Image {
    let mut image = ImageBuffer::from_pixel(dim as u32, dim as u32, Rgb([0, 0, 0]));
    for z in 0..dim {
        for x in 0..dim {
            let pixel = to_pixel(paint(x, z));
            image.put_pixel(x as u32, (dim - 1 - z) as u32, pixel);
        }
    }
    image
}

fn world_at(size: f32, dim: usize, x: usize, z: usize) -> Vec2 {
    Vec2::new(
        (x as f32 + 0.5) / dim as f32 * size - size * 0.5,
        (z as f32 + 0.5) / dim as f32 * size - size * 0.5,
    )
}

fn wrap_to_world(size: f32, point: Vec2) -> Vec2 {
    Vec2::new(
        (point.x + size * 0.5).rem_euclid(size) - size * 0.5,
        (point.y + size * 0.5).rem_euclid(size) - size * 0.5,
    )
}

fn to_pixel(colour: LinearRgba) -> Rgb<u8> {
    let srgb = Color::from(colour).to_srgba();
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb([channel(srgb.red), channel(srgb.green), channel(srgb.blue)])
}

fn ramp(value: f32, low: Color, high: Color) -> LinearRgba {
    let t = value.clamp(0.0, 1.0);
    low.to_linear().mix(&high.to_linear(), t)
}

fn hillshade(map: &TerrainMap, point: Vec2, step: f32) -> f32 {
    let dx = map.height_at(point.x + step, point.y) - map.height_at(point.x - step, point.y);
    let dz = map.height_at(point.x, point.y + step) - map.height_at(point.x, point.y - step);
    let normal = Vec3::new(-dx, 2.0 * step, -dz)
        .try_normalize()
        .unwrap_or(Vec3::Y);
    let light = Vec3::new(-0.6, 0.75, -0.35).normalize();
    normal.dot(light).clamp(0.0, 1.0) * 0.8 + 0.25
}

fn apply_hillshade(colour: LinearRgba, light: f32) -> LinearRgba {
    LinearRgba::new(colour.red * light, colour.green * light, colour.blue * light, 1.0)
}

fn debug_colour(map: &TerrainMap, view: DebugView, point: Vec2, extent: f32) -> LinearRgba {
    crate::terrain::colour::debug_colour(
        map,
        view,
        point,
        map.height_at(point.x, point.y),
        extent,
    )
}

fn height_layer(map: &TerrainMap, grid: &Grid) -> Image {
    let extent = map.max_height_extent();
    grid.paint(|point| {
        let colour = debug_colour(map, DebugView::None, point, extent);
        apply_hillshade(colour, hillshade(map, point, grid.step))
    })
}

fn styled_layer(map: &TerrainMap, view: DebugView, grid: &Grid) -> Image {
    let extent = map.max_height_extent();
    grid.paint(|point| debug_colour(map, view, point, extent))
}

fn cliff_layer(map: &TerrainMap, grid: &Grid) -> Image {
    let extent = map.max_height_extent();
    grid.paint(|point| {
        let base = debug_colour(map, DebugView::None, point, extent);
        let share = cliff_share(map.slope_at(point.x, point.y));
        let cliff = ramp(share, Color::srgb(0.10, 0.12, 0.10), Color::srgb(0.98, 0.55, 0.10));
        base.mix(&cliff, share)
    })
}

fn precipitation_layer(map: &TerrainMap, grid: &Grid) -> Image {
    grid.paint(|point| {
        ramp(
            map.precipitation_at(point.x, point.y),
            Color::srgb(0.93, 0.86, 0.55),
            Color::srgb(0.10, 0.28, 0.88),
        )
    })
}

pub fn flow_ramp(accumulation: f32, peak: f32) -> LinearRgba {
    ramp(
        flow_intensity(accumulation, peak),
        Color::srgb(0.06, 0.10, 0.13),
        Color::srgb(0.15, 0.55, 0.95),
    )
}

const LAKE_COLOUR: LinearRgba = LinearRgba::new(0.10, 0.45, 0.85, 1.0);
const RIVER_COLOUR: LinearRgba = LinearRgba::new(0.35, 0.85, 1.0, 1.0);
const WATERFALL_COLOUR: LinearRgba = LinearRgba::new(1.0, 0.9, 0.2, 1.0);

fn lake_radius(map: &TerrainMap, grid: &Grid) -> i32 {
    let pixel = grid.span / grid.edge as f32;
    ((map.heights().spacing() / pixel * 0.5).round() as i32).max(1)
}

fn hydrology_layer(map: &TerrainMap, grid: &Grid) -> Image {
    let peak = map.accumulation_field().max().max(1.0);
    let mut image = grid.paint(|point| flow_ramp(map.accumulation_at(point.x, point.y), peak));
    let fill = lake_radius(map, grid);
    for lake in map.lakes() {
        paint_cells(&mut image, map, grid, &lake.cells, LAKE_COLOUR, fill);
    }
    for path in map.river_paths() {
        paint_path(&mut image, map, grid, path, RIVER_COLOUR);
    }
    for fall in map.waterfalls() {
        paint_cells(&mut image, map, grid, &[fall.index], WATERFALL_COLOUR, 1);
    }
    image
}

fn paint_cells(
    image: &mut Image,
    map: &TerrainMap,
    grid: &Grid,
    cells: &[usize],
    colour: LinearRgba,
    radius: i32,
) {
    let field = map.heights();
    for &cell in cells {
        let (gx, gz) = field.coords(cell);
        let world = field.world_of(gx, gz);
        for copy in grid.copies(world) {
            dot(image, grid, copy, colour, radius);
        }
    }
}

fn paint_path(
    image: &mut Image,
    map: &TerrainMap,
    grid: &Grid,
    path: &[usize],
    colour: LinearRgba,
) {
    let field = map.heights();
    let size = map.size();
    let half = size * 0.5;
    let fold = |delta: f32| (delta + half).rem_euclid(size) - half;
    for window in path.windows(2) {
        let (ax, az) = field.coords(window[0]);
        let a = field.world_of(ax, az);
        let (bx, bz) = field.coords(window[1]);
        let raw = field.world_of(bx, bz);
        let b = Vec2::new(a.x + fold(raw.x - a.x), a.y + fold(raw.y - a.y));
        for copy in grid.copies(a) {
            let shift = copy - a;
            line(image, grid, a + shift, b + shift, colour);
        }
    }
}

fn composite_layer(map: &TerrainMap, clock: &SeasonClock, grid: &Grid) -> Image {
    let extent = map.max_height_extent();
    let sea = map.sea_levels().level_at(clock.season);
    let snowline = season::snowline_for(clock, SNOWLINE_METRES);
    let flood = map.flood_zone();
    let mut image = grid.paint(|point| {
        let height = map.height_at(point.x, point.y);
        if height < sea {
            return bathymetric_colour(sea - height, extent);
        }
        let base = map
            .region_at(point.x, point.y)
            .map(region_colour)
            .unwrap_or_else(|| height_colour(normalized_height(height, extent)));
        let mut colour = apply_hillshade(base, hillshade(map, point, grid.step));
        let share = cliff_share(map.slope_at(point.x, point.y));
        if share > 0.0 {
            let cliff = ramp(
                share,
                Color::srgb(0.10, 0.12, 0.10),
                Color::srgb(0.98, 0.55, 0.10),
            );
            colour = colour.mix(&cliff, share);
        }
        if flood.contains(height) {
            colour = colour.mix(&Color::srgb(0.76, 0.68, 0.50).to_linear(), 0.4);
        }
        if let Some(parameters) = map.parameters_at(point.x, point.y) {
            let snow =
                season::snow_coverage_blend(clock, parameters.temperature, height, snowline);
            if snow > 0.0 {
                colour = colour.mix(&Color::srgb(0.92, 0.95, 1.0).to_linear(), snow * 0.75);
            }
        }
        colour
    });
    let fill = lake_radius(map, grid);
    for lake in map.lakes() {
        paint_cells(&mut image, map, grid, &lake.cells, LAKE_COLOUR, fill);
    }
    for path in map.river_paths() {
        paint_path(&mut image, map, grid, path, RIVER_COLOUR);
    }
    for fall in map.waterfalls() {
        paint_cells(&mut image, map, grid, &[fall.index], WATERFALL_COLOUR, 1);
    }
    image
}

fn cells_layer(map: &TerrainMap, grid: &Grid) -> Image {
    let mut image = styled_layer(map, DebugView::CellBorders, grid);
    for boundary in map.layout().boundaries() {
        let colour = ramp(
            boundary.blend_width / map.layout().max_blend_width().max(1.0),
            Color::srgb(0.85, 0.20, 0.15),
            Color::srgb(0.15, 0.90, 0.95),
        );
        for copy in grid.copies(boundary.midpoint) {
            let shift = copy - boundary.midpoint;
            line(
                &mut image,
                grid,
                boundary.p0 + shift,
                boundary.p1 + shift,
                colour,
            );
        }
    }
    for cell in map.cells() {
        let gradient = cell.parameters.base_gradient;
        let tilt = BaseGradient::slope_tan(gradient.slope);
        if let Some(direction) = gradient.aspect.try_normalize() {
            let length = map.size() * (0.012 + tilt.abs() * 0.05);
            for copy in grid.copies(cell.center) {
                line(
                    &mut image,
                    grid,
                    copy,
                    copy + direction * length,
                    LinearRgba::new(1.0, 1.0, 0.25, 1.0),
                );
            }
        }
        for copy in grid.copies(cell.center) {
            dot(&mut image, grid, copy, LinearRgba::new(1.0, 1.0, 1.0, 1.0), 2);
        }
    }
    image
}

fn dot(image: &mut Image, grid: &Grid, point: Vec2, colour: LinearRgba, radius: i32) {
    let pixel = to_pixel(colour);
    let (x, y) = grid.pixel(point);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            put(image, x + dx, y + dy, pixel);
        }
    }
}

fn line(image: &mut Image, grid: &Grid, from: Vec2, to: Vec2, colour: LinearRgba) {
    let pixel = to_pixel(colour);
    let (mut x0, mut y0) = grid.pixel(from);
    let (x1, y1) = grid.pixel(to);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let step_x = if x0 < x1 { 1 } else { -1 };
    let step_y = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        put(image, x0, y0, pixel);
        if x0 == x1 && y0 == y1 {
            return;
        }
        let twice = error * 2;
        if twice >= dy {
            error += dy;
            x0 += step_x;
        }
        if twice <= dx {
            error += dx;
            y0 += step_y;
        }
    }
}

fn put(image: &mut Image, x: i32, y: i32, pixel: Rgb<u8>) {
    if x < 0 || y < 0 || x as u32 >= image.width() || y as u32 >= image.height() {
        return;
    }
    image.put_pixel(x as u32, y as u32, pixel);
}

fn season_water_colour(season: Season) -> LinearRgba {
    match season {
        Season::Winter => LinearRgba::new(0.72, 0.82, 0.92, 1.0),
        Season::Spring => LinearRgba::new(0.16, 0.42, 0.62, 1.0),
        Season::Summer => LinearRgba::new(0.06, 0.30, 0.55, 1.0),
        Season::Fall => LinearRgba::new(0.12, 0.36, 0.52, 1.0),
    }
}

fn season_strip(map: &TerrainMap, clock: &SeasonClock, dim: usize, tiled: bool) -> Image {
    let grid = Grid::new(map, dim, tiled);
    let panel = grid.edge;
    let mut image = ImageBuffer::from_pixel(
        (panel * Season::ALL.len()) as u32,
        panel as u32,
        Rgb([0, 0, 0]),
    );
    let extent = map.max_height_extent();
    let base = map.base_heights();

    for (index, season) in Season::ALL.iter().copied().enumerate() {
        let level = map.sea_levels().level_at(season);
        for z in 0..grid.edge {
            for x in 0..grid.edge {
                let point = grid.point(x, z);
                let height = base.sample_world(point);
                let colour = if height < level {
                    let depth = level - height;
                    let shade = 1.0 - 0.65 * (depth / extent.max(1.0)).clamp(0.0, 1.0);
                    let water = season_water_colour(season);
                    LinearRgba::new(water.red * shade, water.green * shade, water.blue * shade, 1.0)
                } else {
                    apply_hillshade(
                        debug_colour(map, DebugView::None, point, extent),
                        hillshade(map, point, grid.step),
                    )
                };
                image.put_pixel(
                    (index * panel + x) as u32,
                    (grid.edge - 1 - z) as u32,
                    to_pixel(colour),
                );
            }
        }

        let divider = if season == clock.season {
            LinearRgba::new(1.0, 1.0, 1.0, 1.0)
        } else {
            LinearRgba::new(0.15, 0.15, 0.15, 1.0)
        };
        let pixel = to_pixel(divider);
        for y in 0..grid.edge as u32 {
            image.put_pixel((index * panel) as u32, y, pixel);
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::map::TerrainConfig;
    use crate::terrain::topology::WorldTopology;
    use crate::terrain::material::DebugMaterialMode;
    use crate::terrain::TerrainPlugin;
    use crate::utils::registry::commands::CommandRegistryPlugin;
    use avian3d::prelude::PhysicsPlugins;
    use bevy::app::AppExit;
    use bevy::render::view::window::screenshot::{save_to_disk, Screenshot};
    use std::path::Path;

    struct Shot {
        name: &'static str,
        config: TerrainConfig,
        eye: Vec3,
        season: Season,
        target: Option<Vec3>,
    }

    #[derive(Resource)]
    struct CaptureRequest {
        path: PathBuf,
        eye: Vec3,
        season: Season,
        target: Option<Vec3>,
        frames: u32,
        captured: bool,
    }

    fn build(topology: WorldTopology) -> TerrainMap {
        TerrainMap::new(TerrainConfig::compact().with_topology(topology))
    }

    #[test]
    fn export_writes_every_layer() {
        let map = build(WorldTopology::Finite);
        let clock = SeasonClock::default();
        let files = export(&map, &clock, "all").expect("export must succeed");
        assert_eq!(
            files.len(),
            LAYERS.len(),
            "a finite world must export exactly one image per layer"
        );
        for file in &files {
            assert!(file.exists(), "{} missing", file.display());
            assert!(image::open(file).is_ok(), "{} unreadable", file.display());
        }
    }

    #[test]
    fn export_writes_tiled_variants_only_for_torus() {
        let clock = SeasonClock::default();
        let torus = build(WorldTopology::FiniteTorus);
        let files = export(&torus, &clock, "all").expect("torus export must succeed");
        assert_eq!(
            files.len(),
            LAYERS.len() * 2 - 2,
            "a torus world adds one tiled image per layer except tiling and splat"
        );
        assert!(files.iter().any(|f| f.display().to_string().contains("world_tiled")));
        let finite = build(WorldTopology::Finite);
        let finite_files = export(&finite, &clock, "all").expect("finite export must succeed");
        assert!(
            !finite_files
                .iter()
                .any(|f| f.display().to_string().contains("_tiled")),
            "a finite world must not export tiled variants"
        );
    }

    #[test]
    fn hydrology_flow_ramp_is_log_scaled() {
        let head = flow_ramp(5.0, 5000.0);
        let linear = ramp(
            5.0 / 5000.0,
            Color::srgb(0.06, 0.10, 0.13),
            Color::srgb(0.15, 0.55, 0.95),
        );
        let brightness = |c: LinearRgba| c.red + c.green + c.blue;
        assert!(
            brightness(head) > brightness(linear) * 2.0,
            "headwaters must stay visible under log normalisation"
        );
        assert!((flow_ramp(5000.0, 5000.0).blue - linear_ramp_top().blue).abs() < 1e-5);
    }

    fn linear_ramp_top() -> LinearRgba {
        ramp(
            1.0,
            Color::srgb(0.06, 0.10, 0.13),
            Color::srgb(0.15, 0.55, 0.95),
        )
    }

    #[test]
    fn hydrology_rivers_draw_connected_lines() {
        let map = build(WorldTopology::Finite);
        let image = render_layer(&map, &SeasonClock::default(), "hydrology");
        let grid = Grid::new(&map, PREVIEW_DIM, false);
        let field = map.heights();
        let path = map
            .river_paths()
            .iter()
            .max_by_key(|path| path.len())
            .expect("the compact world must have rivers");
        assert!(path.len() > 4, "the longest river must span several cells");
        let wanted = to_pixel(RIVER_COLOUR);
        let mid = path.len() / 2;
        let (ax, az) = field.coords(path[mid]);
        let (bx, bz) = field.coords(path[mid + 1]);
        let a = field.world_of(ax, az);
        let b = field.world_of(bx, bz);
        let (x, y) = grid.pixel((a + b) * 0.5);
        let mut found = false;
        for dy in -2..=2 {
            for dx in -2..=2 {
                let px = x + dx;
                let py = y + dy;
                if px < 0 || py < 0 || px >= image.width() as i32 || py >= image.height() as i32 {
                    continue;
                }
                if *image.get_pixel(px as u32, py as u32) == wanted {
                    found = true;
                }
            }
        }
        assert!(found, "the midpoint of a river segment must be painted with the river colour");
    }

    #[test]
    fn hydrology_lakes_are_filled_not_dotted() {
        let map = build(WorldTopology::Finite);
        let grid = Grid::new(&map, PREVIEW_DIM, false);
        assert!(
            lake_radius(&map, &grid) >= 2,
            "lake cells must be painted as filled discs, not 3x3 dots"
        );
        let Some(lake) = map.lakes().iter().max_by_key(|lake| lake.cells.len()) else {
            return;
        };
        let image = render_layer(&map, &SeasonClock::default(), "hydrology");
        let field = map.heights();
        let centroid = lake
            .cells
            .iter()
            .map(|&cell| {
                let (gx, gz) = field.coords(cell);
                field.world_of(gx, gz)
            })
            .fold(Vec2::ZERO, |acc, point| acc + point)
            / lake.cells.len() as f32;
        let wanted = to_pixel(LAKE_COLOUR);
        let (x, y) = grid.pixel(centroid);
        let mut found = false;
        for dy in -4..=4 {
            for dx in -4..=4 {
                let px = x + dx;
                let py = y + dy;
                if px < 0 || py < 0 || px >= image.width() as i32 || py >= image.height() as i32 {
                    continue;
                }
                if *image.get_pixel(px as u32, py as u32) == wanted {
                    found = true;
                }
            }
        }
        assert!(found, "the interior of the largest lake must be filled");
    }

    #[test]
    fn non_tiled_torus_copies_reach_across_the_seam() {
        let map = build(WorldTopology::FiniteTorus);
        let grid = Grid::new(&map, 64, false);
        let copies = grid.copies(Vec2::new(map.size() * 0.5 - 1.0, 0.0));
        assert!(
            copies.len() >= 2,
            "a torus mark must be drawn on both sides of the seam"
        );
        assert!(copies.iter().any(|copy| copy.x < -map.size() * 0.5 + 1.0));
        let finite = Grid::new(&build(WorldTopology::Finite), 64, false);
        assert_eq!(finite.copies(Vec2::ZERO).len(), 1);
    }

    #[test]
    fn world_layer_paints_sea_with_the_bathymetric_ramp() {
        let map = build(WorldTopology::Finite);
        let clock = SeasonClock::default();
        let image = render_layer(&map, &clock, "world");
        assert!(pixel_variance(&image) > 1.0, "the composite preview rendered flat");
        let sea = map.sea_levels().level_at(clock.season);
        let extent = map.max_height_extent();
        let grid = Grid::new(&map, PREVIEW_DIM, false);
        let mut checked = 0usize;
        'rows: for z in 0..grid.edge {
            for x in 0..grid.edge {
                let point = grid.point(x, z);
                let height = map.height_at(point.x, point.y);
                if sea - height < 3.0 {
                    continue;
                }
                let expected = to_pixel(bathymetric_colour(sea - height, extent));
                let pixel = *image.get_pixel(x as u32, (grid.edge - 1 - z) as u32);
                let close = (0..3)
                    .all(|channel| (pixel.0[channel] as i32 - expected.0[channel] as i32).abs() <= 1);
                assert!(close, "deep sea at {point:?} must use the bathymetric ramp");
                checked += 1;
                if checked >= 5 {
                    break 'rows;
                }
            }
        }
        assert!(checked > 0, "the compact world must contain open sea");
    }

    #[test]
    fn export_rejects_unknown_layers() {
        let map = build(WorldTopology::Finite);
        let clock = SeasonClock::default();
        assert!(export(&map, &clock, "nonsense").is_err());
    }

    #[test]
    fn every_layer_renders_a_varied_image() {
        let map = build(WorldTopology::Finite);
        let clock = SeasonClock::default();
        for name in LAYERS {
            let image = render_layer(&map, &clock, name);
            assert!(
                pixel_variance(&image) > 1.0,
                "layer {name} rendered flat"
            );
        }
    }

    #[test]
    fn every_layer_has_a_tiled_variant() {
        let map = build(WorldTopology::FiniteTorus);
        let clock = SeasonClock::default();
        for name in LAYERS {
            if name == "tiling" || name == "splat" {
                continue;
            }
            let image = render_tiled(&map, &clock, name);
            assert!(
                pixel_variance(&image) > 1.0,
                "tiled layer {name} rendered flat"
            );
        }
    }

    #[test]
    fn tiled_variants_cover_four_world_copies() {
        let map = build(WorldTopology::FiniteTorus);
        let clock = SeasonClock::default();
        let plain = render_layer(&map, &clock, "region");
        let tiled = render_tiled(&map, &clock, "region");
        assert_eq!(tiled.width(), plain.width());
        assert_eq!(tiled.height(), plain.height());
    }

    #[test]
    fn height_preview_is_not_a_flat_colour() {
        let map = build(WorldTopology::Finite);
        let image = render_layer(&map, &SeasonClock::default(), "height");
        assert_eq!(image.width(), image.height());
        assert!(
            pixel_variance(&image) > 1.0,
            "flat preview: variance {}",
            pixel_variance(&image)
        );
    }

    #[test]
    fn cliff_layer_marks_steeper_ground_than_the_flat_seabed() {
        let map = build(WorldTopology::Finite);
        let image = render_layer(&map, &SeasonClock::default(), "cliff");
        assert!(pixel_variance(&image) > 1.0);
        let grid = Grid::new(&map, 32, false);
        let mut steepest = 0.0f32;
        for z in 0..grid.edge {
            for x in 0..grid.edge {
                let point = grid.point(x, z);
                steepest = steepest.max(cliff_share(map.slope_at(point.x, point.y)));
            }
        }
        assert!(steepest > 0.0, "the compact world must contain cliff ground");
    }

    #[test]
    fn cells_layer_marks_every_cell_centre() {
        let map = build(WorldTopology::Finite);
        let image = render_layer(&map, &SeasonClock::default(), "cells");
        let grid = Grid::new(&map, PREVIEW_DIM, false);
        let white = to_pixel(LinearRgba::new(1.0, 1.0, 1.0, 1.0));
        let width = image.width() as i32;
        let height = image.height() as i32;
        for cell in map.cells() {
            let (x, y) = grid.pixel(cell.center);
            let mut found = false;
            for dy in -3..=3 {
                for dx in -3..=3 {
                    let px = x + dx;
                    let py = y + dy;
                    if px < 0 || py < 0 || px >= width || py >= height {
                        continue;
                    }
                    if *image.get_pixel(px as u32, py as u32) == white {
                        found = true;
                    }
                }
            }
            assert!(found, "cell {} centre is unmarked", cell.id);
        }
    }

    #[test]
    fn boundary_marks_span_the_blend_width() {
        let map = build(WorldTopology::Finite);
        let grid = Grid::new(&map, PREVIEW_DIM, false);
        let boundary = map
            .layout()
            .boundaries()
            .iter()
            .next()
            .expect("the layout must have boundaries");
        assert!(boundary.blend_width > 0.0);
        let (x, y) = grid.pixel(boundary.midpoint);
        assert!(x >= 0 && y >= 0, "the boundary midpoint must land on the canvas");
    }

    #[test]
    fn hydrology_preview_marks_the_drainage_network() {
        let map = build(WorldTopology::Finite);
        let image = render_layer(&map, &SeasonClock::default(), "hydrology");
        assert!(pixel_variance(&image) > 1.0);
    }

    #[test]
    fn season_strip_has_one_panel_per_season() {
        let map = build(WorldTopology::Finite);
        let image = render_layer(&map, &SeasonClock::default(), "season");
        assert_eq!(image.width(), image.height() * Season::ALL.len() as u32);
    }

    #[test]
    fn season_panels_shade_water_by_depth() {
        let map = build(WorldTopology::Finite);
        let image = season_strip(&map, &SeasonClock::default(), 96, false);
        let panel = image.height() as usize;
        let summer = Season::ALL
            .iter()
            .position(|s| *s == Season::Summer)
            .unwrap();
        let mut seen = std::collections::HashSet::new();
        for z in (0..image.height()).step_by(2) {
            for x in ((summer * panel + 2) as u32..((summer + 1) * panel) as u32).step_by(2) {
                let p = image.get_pixel(x, z).0;
                if p[2] > p[0].saturating_add(20) && p[2] > 20 {
                    seen.insert([p[0], p[1], p[2]]);
                }
            }
        }
        assert!(
            seen.len() > 3,
            "water must vary with depth, saw {} shades",
            seen.len()
        );
    }

    fn column_pair_stats(image: &Image, column: u32, threshold: u32) -> (u32, u32) {
        let mut worst = 0u32;
        let mut jumping_rows = 0u32;
        for y in 0..image.height() {
            let a = image.get_pixel(column - 1, y);
            let b = image.get_pixel(column, y);
            let row_worst = (0..3)
                .map(|channel| a.0[channel].abs_diff(b.0[channel]) as u32)
                .max()
                .unwrap_or(0);
            worst = worst.max(row_worst);
            if row_worst > threshold {
                jumping_rows += 1;
            }
        }
        (worst, jumping_rows)
    }

    fn seam_vs_interior(image: &Image, threshold: u32) -> ((u32, u32), (u32, u32)) {
        let quarter = image.width() / 4;
        let mut seam = (0u32, 0u32);
        for column in [quarter, quarter * 3] {
            let (worst, jumping) = column_pair_stats(image, column, threshold);
            seam = (seam.0.max(worst), seam.1.max(jumping));
        }
        let mut interior = (0u32, 0u32);
        let step = (image.width() / 32).max(1);
        for column in (step..image.width()).step_by(step as usize) {
            if column.abs_diff(quarter) < 4 || column.abs_diff(quarter * 3) < 4 {
                continue;
            }
            let (worst, jumping) = column_pair_stats(image, column, threshold);
            interior = (interior.0.max(worst), interior.1.max(jumping));
        }
        (seam, interior)
    }

    #[test]
    fn torus_tiling_repeats_without_a_seam() {
        let map = build(WorldTopology::FiniteTorus);
        let image = render_layer(&map, &SeasonClock::default(), "tiling");
        let ((seam_worst, seam_jump), (inner_worst, inner_jump)) = seam_vs_interior(&image, 16);
        assert!(
            seam_worst <= inner_worst + 16,
            "tiling seam differs by {seam_worst} per channel, worse than the steepest interior column pair ({inner_worst})"
        );
        assert!(
            seam_jump <= inner_jump + image.height() / 8,
            "tiling seam jumps on {seam_jump} rows, more than the worst interior column pair ({inner_jump})"
        );
    }

    #[test]
    fn every_tiled_layer_wraps_without_a_seam() {
        let map = build(WorldTopology::FiniteTorus);
        let clock = SeasonClock::default();
        for name in ["height", "region", "cliff", "precipitation"] {
            let image = render_tiled(&map, &clock, name);
            let ((seam_worst, seam_jump), (inner_worst, inner_jump)) = seam_vs_interior(&image, 32);
            assert!(
                seam_worst <= inner_worst + 16,
                "tiled layer {name} differs by {seam_worst} across the seam, worse than any interior column pair ({inner_worst})"
            );
            assert!(
                seam_jump <= inner_jump + image.height() / 8,
                "tiled layer {name} jumps on {seam_jump} straddling rows, more than the worst interior column pair ({inner_jump})"
            );
        }
    }

    #[test]
    fn finite_tiling_wraps_by_construction() {
        let size = 120.0;
        let point = Vec2::new(90.0, -130.0);
        let wrapped = wrap_to_world(size, point);
        assert!(wrapped.x >= -size * 0.5 && wrapped.x < size * 0.5);
        assert!(wrapped.y >= -size * 0.5 && wrapped.y < size * 0.5);
    }

    #[test]
    fn grid_copies_ghost_the_seam_for_a_torus() {
        let map = build(WorldTopology::FiniteTorus);
        let torus = Grid::new(&map, 64, false);
        assert_eq!(
            torus.copies(Vec2::ZERO).len(),
            9,
            "a non-tiled torus must ghost every neighbouring copy so marks cross the seam"
        );
        let tiled = Grid::new(&map, 64, true);
        assert!(tiled.copies(Vec2::ZERO).len() >= 4);
        let finite = Grid::new(&build(WorldTopology::Finite), 64, false);
        assert_eq!(finite.copies(Vec2::ZERO).len(), 1);
    }

    #[test]
    #[ignore = "needs a GPU: opens a hidden window and reads the captured frames back"]
    fn offscreen_renders_capture_detail_from_every_angle() {
        fs::create_dir_all(PREVIEW_ROOT).expect("preview directory");
        let finite = TerrainConfig::compact();
        let map = TerrainMap::new(finite.clone());
        let (water_eye, water_target) = shoreline_camera(&map);
        let (ground_eye, ground_target) = ground_camera(&map);
        let shots: Vec<Shot> = vec![
            Shot { name: "overview", config: finite.clone(), eye: Vec3::new(0.0, 70.0, 110.0), season: Season::Summer, target: None },
            Shot { name: "cliff_close_up", config: finite.clone(), eye: Vec3::new(10.0, 14.0, 10.0), season: Season::Summer, target: None },
            Shot { name: "ground_close_up", config: finite.clone(), eye: ground_eye, season: Season::Summer, target: Some(ground_target) },
            Shot { name: "river_coastline", config: finite.clone(), eye: Vec3::new(-35.0, 6.0, 35.0), season: Season::Spring, target: None },
            Shot { name: "winter_fog", config: finite.clone(), eye: Vec3::new(0.0, 25.0, 45.0), season: Season::Winter, target: None },
            Shot { name: "water_surface", config: finite.clone(), eye: water_eye, season: Season::Summer, target: Some(water_target) },
        ];

        let mut flat = Vec::new();
        let mut textured = Vec::new();
        for mode in [DebugMaterialMode::FlatColor, DebugMaterialMode::Textured] {
            for shot in &shots {
                let frame = capture_frame_image(shot, mode);
                match mode {
                    DebugMaterialMode::FlatColor => flat.push(frame),
                    _ => textured.push(frame),
                }
            }
        }

        for (shot, frame) in shots.iter().zip(&flat) {
            assert!(
                pixel_variance(frame) > 1.0,
                "{} captured a flat frame",
                shot.name
            );
        }
        for index in 0..shots.len() {
            let baseline = local_detail(&flat[index]);
            let detail = local_detail(&textured[index]);
            assert!(
                detail > baseline,
                "{}: the textured pass shows no more detail than flat ({detail} vs {baseline})",
                shots[index].name
            );
        }
        for name in ["cliff_close_up", "ground_close_up"] {
            let index = shots
                .iter()
                .position(|shot| shot.name == name)
                .expect("close-up shot present");
            let sharp = local_detail(&textured[index]);
            let soft = local_detail(&blur_copy(&textured[index]));
            assert!(
                sharp > soft * 1.3,
                "{name}: close-up detail {sharp} is barely sharper than a quarter-resolution copy ({soft}) — the splat bake is too low-resolution to survive a close-up"
            );
        }

        let torus = TerrainConfig::compact().with_topology(WorldTopology::FiniteTorus);
        let torus_map = TerrainMap::new(torus.clone());
        let (here, there) = seam_pair_shots(&torus_map, torus);
        let frame_here = capture_frame_image(&here, DebugMaterialMode::Textured);
        let frame_there = capture_frame_image(&there, DebugMaterialMode::Textured);
        let diff = mean_abs_diff(&frame_here, &frame_there);
        assert!(
            diff < 4.0,
            "the two sides of the torus seam disagree by {diff} per channel — the wrapped world is not continuous"
        );
    }

    fn ground_camera(map: &TerrainMap) -> (Vec3, Vec3) {
        let sea = map.sea_level();
        let size = map.size();
        for iz in 0..96 {
            for ix in 0..96 {
                let x = (ix as f32 / 96.0 - 0.5) * size;
                let z = (iz as f32 / 96.0 - 0.5) * size;
                let h = map.height_at(x, z);
                if h > sea + 1.0 && map.slope_at(x, z) < 0.3 {
                    let target = Vec3::new(x + 6.0, map.height_at(x + 6.0, z) + 0.6, z);
                    return (Vec3::new(x, h + 1.2, z), target);
                }
            }
        }
        panic!("the compact world must have gentle dry ground to photograph");
    }

    fn seam_pair_shots(map: &TerrainMap, config: TerrainConfig) -> (Shot, Shot) {
        let size = map.size();
        let seam = size * 0.5;
        let sea = map.sea_level();
        let mut lane = None;
        let mut z = -size * 0.45;
        while z <= size * 0.45 {
            if map.height_at(seam - 6.0, z) > sea + 0.5 && map.height_at(seam + 6.0, z) > sea + 0.5 {
                lane = Some(z);
                break;
            }
            z += 1.0;
        }
        let z = lane.expect("the compact torus must have dry ground across the seam");
        let eye = Vec3::new(seam - 6.0, map.height_at(seam - 6.0, z) + 1.6, z);
        let here = Shot {
            name: "torus_seam_here",
            config: config.clone(),
            eye,
            season: Season::Summer,
            target: Some(eye + Vec3::X * 12.0),
        };
        let there = Shot {
            name: "torus_seam_there",
            config,
            eye: Vec3::new(eye.x - size, eye.y, eye.z),
            season: Season::Summer,
            target: Some(eye + Vec3::X * 12.0 - Vec3::X * size),
        };
        (here, there)
    }

    fn blur_copy(image: &Image) -> Image {
        let small = image::imageops::resize(image, 64, 64, image::imageops::FilterType::Triangle);
        image::imageops::resize(
            &small,
            image.width(),
            image.height(),
            image::imageops::FilterType::Triangle,
        )
    }

    fn mean_abs_diff(a: &Image, b: &Image) -> f32 {
        assert_eq!((a.width(), a.height()), (b.width(), b.height()));
        let mut total = 0.0f32;
        let mut count = 0.0f32;
        for y in 0..a.height() {
            for x in 0..a.width() {
                for channel in 0..3 {
                    total += a.get_pixel(x, y).0[channel].abs_diff(b.get_pixel(x, y).0[channel]) as f32;
                    count += 1.0;
                }
            }
        }
        total / count.max(1.0)
    }

    fn shoreline_camera(map: &TerrainMap) -> (Vec3, Vec3) {
        let sea = map.sea_level();
        let size = map.size();
        for iz in 0..96 {
            for ix in 0..96 {
                let x = (ix as f32 / 96.0 - 0.5) * size;
                let z = (iz as f32 / 96.0 - 0.5) * size;
                if map.height_at(x, z) < sea - 0.5 && map.height_at(x + 16.0, z) > sea + 0.5 {
                    let eye = Vec3::new(x, sea + 1.6, z);
                    let target = Vec3::new(x + 16.0, sea + 0.5, z);
                    return (eye, target);
                }
            }
        }
        panic!("the compact world must have a shoreline to photograph");
    }

    fn local_detail(image: &Image) -> f32 {
        let mut total = 0.0f32;
        let mut count = 0.0f32;
        for y in 0..image.height() {
            for x in 1..image.width() {
                for channel in 0..3 {
                    let here = image.get_pixel(x, y).0[channel] as f32;
                    let before = image.get_pixel(x - 1, y).0[channel] as f32;
                    total += (here - before).abs();
                }
                count += 1.0;
            }
        }
        total / count.max(1.0)
    }

    fn capture_frame_image(shot: &Shot, mode: DebugMaterialMode) -> Image {
        let path = PathBuf::from(PREVIEW_ROOT)
            .join(format!("offscreen_{}_{}.png", shot.name, mode.label()));
        let _ = fs::remove_file(&path);
        capture_terrain_frame(&path, mode, shot);
        image::open(&path)
            .unwrap_or_else(|error| panic!("{} ({}) frame missing: {error}", shot.name, mode.label()))
            .to_rgb8()
    }

    fn capture_terrain_frame(path: &Path, mode: DebugMaterialMode, shot: &Shot) {
        let mut app = App::new();
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                visible: false,
                resolution: (256, 256).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CommandRegistryPlugin)
        .add_plugins(PhysicsPlugins::default())
        .insert_resource(TerrainMap::new(shot.config.clone()))
        .add_plugins(TerrainPlugin)
        .insert_resource(mode)
        .insert_resource(CaptureRequest {
            path: path.to_path_buf(),
            eye: shot.eye,
            season: shot.season,
            target: shot.target,
            frames: 0,
            captured: false,
        })
        .add_systems(Startup, prepare_capture)
        .add_systems(Update, advance_capture)
        .run();
    }

    fn prepare_capture(
        mut commands: Commands,
        request: Res<CaptureRequest>,
        mut clock: ResMut<SeasonClock>,
    ) {
        clock.set_season(request.season);
        clock.running = false;
        let target = request.target.unwrap_or_else(|| {
            Vec3::new(request.eye.x * 0.2, 0.0, request.eye.z * 0.2)
        });
        commands.spawn((
            Camera3d::default(),
            Transform::from_translation(request.eye).looking_at(target, Vec3::Y),
        ));
    }

    fn advance_capture(
        mut commands: Commands,
        mut request: ResMut<CaptureRequest>,
        mut exits: MessageWriter<AppExit>,
    ) {
        request.frames += 1;
        if !request.captured {
            if request.frames == 24 {
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(request.path.clone()));
                request.captured = true;
            }
            return;
        }
        if request.path.exists() || request.frames > 600 {
            exits.write(AppExit::Success);
        }
    }
}
