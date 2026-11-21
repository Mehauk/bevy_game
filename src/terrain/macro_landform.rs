use bevy::prelude::Vec2;

use crate::terrain::params::{BaseHeightBand, ParamLevel};
use crate::terrain::seed::{WorldSeed, STAGE_MACRO};
use crate::terrain::topology::WorldTopology;
use crate::terrain::voronoi::VoronoiLayout;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldShape {
    Continent,
    Archipelago,
    IslandChain,
    InlandSea,
}

impl WorldShape {
    pub fn all() -> [WorldShape; 4] {
        [
            WorldShape::Continent,
            WorldShape::Archipelago,
            WorldShape::IslandChain,
            WorldShape::InlandSea,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            WorldShape::Continent => "continent",
            WorldShape::Archipelago => "archipelago",
            WorldShape::IslandChain => "island_chain",
            WorldShape::InlandSea => "inland_sea",
        }
    }

    pub fn parse(label: &str) -> Option<WorldShape> {
        WorldShape::all().into_iter().find(|s| s.label() == label)
    }
}

pub const SEA_LEVEL: f32 = 0.0;

#[derive(Clone, Copy, Debug)]
pub struct Hotspot {
    pub center: Vec2,
    pub radius: f32,
    pub strength: f32,
    pub bulge: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Plate {
    pub center: Vec2,
    pub velocity: Vec2,
    pub continental: bool,
    pub crest: f32,
    pub spreading_axis: Vec2,
}

#[derive(Clone, Debug)]
pub struct MacroField {
    pub elevation: Vec<f32>,
    pub bands: Vec<BaseHeightBand>,
    pub sea_proximity: Vec<f32>,
    pub latitude: Vec<f32>,
    pub rain_shadow: Vec<f32>,
    pub geothermal_intensity: Vec<f32>,
    pub prevailing_wind: Vec2,
    pub downslope: Vec<Vec2>,
    pub slope_magnitude: Vec<f32>,
    pub hotspots: Vec<Hotspot>,
}

#[derive(Clone, Copy, Debug)]
struct ShapeModel {
    plate_count: usize,
    continental_share: f32,
    convergence: f32,
    hotspot_count: usize,
    hotspot_track: bool,
    basin: bool,
    sea_offset: f32,
    detail: f32,
    arc_crest: f32,
    arc_width: f32,
    arc_threshold: f32,
    subsidence: f32,
}

fn model_for(shape: WorldShape) -> ShapeModel {
    match shape {
        WorldShape::Continent => ShapeModel {
            plate_count: 6,
            continental_share: 0.50,
            convergence: 1.0,
            hotspot_count: 2,
            hotspot_track: false,
            basin: false,
            sea_offset: 0.02,
            detail: 0.10,
            arc_crest: 0.55,
            arc_width: 0.05,
            arc_threshold: 0.2,
            subsidence: 0.20,
        },
        WorldShape::Archipelago => ShapeModel {
            plate_count: 8,
            continental_share: 0.50,
            convergence: 2.4,
            hotspot_count: 3,
            hotspot_track: false,
            basin: false,
            sea_offset: 0.02,
            detail: 0.12,
            arc_crest: 0.85,
            arc_width: 0.095,
            arc_threshold: 0.1,
            subsidence: 0.06,
        },
        WorldShape::IslandChain => ShapeModel {
            plate_count: 5,
            continental_share: 0.10,
            convergence: 0.7,
            hotspot_count: 7,
            hotspot_track: true,
            basin: false,
            sea_offset: -0.04,
            detail: 0.12,
            arc_crest: 0.60,
            arc_width: 0.06,
            arc_threshold: 0.15,
            subsidence: 0.10,
        },
        WorldShape::InlandSea => ShapeModel {
            plate_count: 4,
            continental_share: 0.75,
            convergence: 0.8,
            hotspot_count: 2,
            hotspot_track: false,
            basin: true,
            sea_offset: 0.0,
            detail: 0.08,
            arc_crest: 0.50,
            arc_width: 0.05,
            arc_threshold: 0.2,
            subsidence: 0.20,
        },
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MacroConfig {
    pub size: f32,
    pub topology: WorldTopology,
    pub sea_level: f32,
    pub relief: f32,
}

impl Default for MacroConfig {
    fn default() -> Self {
        MacroConfig {
            size: 400.0,
            topology: WorldTopology::Finite,
            sea_level: SEA_LEVEL,
            relief: 90.0,
        }
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn bump(x: f32, width: f32) -> f32 {
    let t = (x / width.max(1e-3)).clamp(-1.0, 1.0);
    (1.0 - t * t).powi(2)
}

fn lattice(seed: WorldSeed, x: f32, y: f32, cell: f32, period: i64) -> f32 {
    let fx = x / cell;
    let fy = y / cell;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = smoothstep(fx - x0);
    let ty = smoothstep(fy - y0);
    let wrap = |v: f64| v.rem_euclid(period as f64) as i64;
    let (x0, y0) = if period > 0 {
        (wrap(x0 as f64), wrap(y0 as f64))
    } else {
        (x0 as i64, y0 as i64)
    };
    let key = |ix: i64, iz: i64| {
        (ix as u64)
            .wrapping_mul(0x517c_c1b7_2722_0a95)
            ^ (iz as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
    };
    let v00 = seed.unit(STAGE_MACRO, key(x0, y0), 0);
    let v10 = seed.unit(STAGE_MACRO, key(x0 + 1, y0), 0);
    let v01 = seed.unit(STAGE_MACRO, key(x0, y0 + 1), 0);
    let v11 = seed.unit(STAGE_MACRO, key(x0 + 1, y0 + 1), 0);
    let a = v00 + (v10 - v00) * tx;
    let b = v01 + (v11 - v01) * tx;
    a + (b - a) * ty
}

fn value_noise(
    seed: WorldSeed,
    x: f32,
    y: f32,
    cell: f32,
    topology: WorldTopology,
    size: f32,
) -> f32 {
    let period = if topology.wraps() {
        (size / cell).round().max(1.0) as i64
    } else {
        0
    };
    lattice(seed, x, y, cell, period) * 2.0 - 1.0
}

fn fbm(
    seed: WorldSeed,
    x: f32,
    y: f32,
    base_cell: f32,
    octaves: usize,
    topology: WorldTopology,
    size: f32,
) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut cell = base_cell;
    for _ in 0..octaves.max(1) {
        sum += value_noise(seed, x, y, cell, topology, size) * amp;
        norm += amp;
        amp *= 0.5;
        cell *= 0.5;
    }
    sum / norm.max(1e-6)
}

pub fn prevailing_wind_for(seed: WorldSeed) -> Vec2 {
    let angle = seed.unit(STAGE_MACRO, 0, 1) * std::f32::consts::TAU;
    Vec2::new(angle.cos(), angle.sin())
}

fn plates_for(seed: WorldSeed, shape: WorldShape, config: &MacroConfig) -> Vec<Plate> {
    let model = model_for(shape);
    let half = config.size * 0.5;
    let anchored = matches!(shape, WorldShape::Continent | WorldShape::InlandSea);
    let count = model.plate_count;
    let target = ((count as f32 * model.continental_share).round() as usize).clamp(1, count);
    let centers: Vec<Vec2> = (0..count)
        .map(|index| {
            let id = index as u64;
            let spread = if index == 0 && anchored {
                half * 0.25
            } else {
                half
            };
            Vec2::new(
                seed.range(STAGE_MACRO, id, 0, -spread, spread),
                seed.range(STAGE_MACRO, id, 1, -spread, spread),
            )
        })
        .collect();
    let mut continental = vec![false; count];
    if anchored {
        continental[0] = true;
        let mut chosen = 1;
        while chosen < target {
            let mut best = usize::MAX;
            let mut best_d = -1.0;
            for candidate in 0..count {
                if continental[candidate] {
                    continue;
                }
                let mut nearest = f32::MAX;
                for chosen_index in 0..count {
                    if continental[chosen_index] {
                        let d = config.topology.distance_xz(
                            centers[chosen_index],
                            centers[candidate],
                            config.size,
                        );
                        nearest = nearest.min(d);
                    }
                }
                if nearest > best_d {
                    best_d = nearest;
                    best = candidate;
                }
            }
            if best == usize::MAX {
                break;
            }
            continental[best] = true;
            chosen += 1;
        }
    } else {
        let mut order: Vec<usize> = (0..count).collect();
        order.sort_by(|a, b| {
            seed.unit(STAGE_MACRO, *b as u64, 4)
                .total_cmp(&seed.unit(STAGE_MACRO, *a as u64, 4))
        });
        for &index in order.iter().take(target) {
            continental[index] = true;
        }
    }
    let mut plates = Vec::with_capacity(count);
    for index in 0..count {
        let id = index as u64;
        let center = centers[index];
        let angle = seed.unit(STAGE_MACRO, id, 2) * std::f32::consts::TAU;
        let speed = model.convergence * (0.35 + 0.65 * seed.unit(STAGE_MACRO, id, 3));
        let velocity = Vec2::new(angle.cos(), angle.sin()) * speed;
        let crest = if continental[index] {
            0.33 + 0.14 * seed.unit(STAGE_MACRO, id, 5)
        } else {
            -(0.34 + 0.18 * seed.unit(STAGE_MACRO, id, 5))
        };
        let axis_angle = seed.unit(STAGE_MACRO, id, 6) * std::f32::consts::TAU;
        let spreading_axis = Vec2::new(axis_angle.cos(), axis_angle.sin());
        plates.push(Plate {
            center,
            velocity,
            continental: continental[index],
            crest,
            spreading_axis,
        });
    }
    plates
}

fn hotspots_for(
    seed: WorldSeed,
    shape: WorldShape,
    config: &MacroConfig,
    plates: &[Plate],
) -> Vec<Hotspot> {
    let model = model_for(shape);
    let half = config.size * 0.5;
    let mut hotspots = Vec::new();
    if model.hotspot_track {
        let angle = seed.unit(STAGE_MACRO, 40, 0) * std::f32::consts::TAU;
        let direction = Vec2::new(angle.cos(), angle.sin());
        let origin = Vec2::new(
            seed.range(STAGE_MACRO, 40, 1, -half * 0.3, half * 0.3),
            seed.range(STAGE_MACRO, 40, 2, -half * 0.3, half * 0.3),
        );
        let spacing = config.size * 0.15;
        for k in 0..model.hotspot_count {
            let age = k as f32 / model.hotspot_count as f32;
            hotspots.push(Hotspot {
                center: config.topology.wrap_xz(
                    origin + direction * (k as f32 * spacing - config.size * 0.25),
                    config.size,
                ),
                radius: config.size * (0.10 - 0.04 * age),
                strength: 0.95 - 0.55 * age,
                bulge: 0.78 - 0.35 * age,
            });
        }
    } else {
        for k in 0..model.hotspot_count {
            let id = 40 + k as u64;
            hotspots.push(Hotspot {
                center: Vec2::new(
                    seed.range(STAGE_MACRO, id, 0, -half, half),
                    seed.range(STAGE_MACRO, id, 1, -half, half),
                ),
                radius: config.size * (0.10 + 0.06 * seed.unit(STAGE_MACRO, id, 2)),
                strength: 0.70 + 0.30 * seed.unit(STAGE_MACRO, id, 3),
                bulge: 0.06,
            });
        }
    }
    for i in 0..plates.len() {
        for j in (i + 1)..plates.len() {
            let a = plates[i];
            let b = plates[j];
            if a.continental && b.continental {
                continue;
            }
            let delta = config.topology.delta_xz(a.center, b.center, config.size);
            let len = delta.length();
            if len < 1e-3 {
                continue;
            }
            let normal = delta / len;
            let convergence = (a.velocity - b.velocity).dot(normal);
            if convergence <= 0.25 {
                continue;
            }
            let overriding = if b.continental { -1.0 } else { 1.0 };
            hotspots.push(Hotspot {
                center: config.topology.wrap_xz(
                    a.center + delta * 0.5 + normal * overriding * len * 0.18,
                    config.size,
                ),
                radius: config.size * 0.05,
                strength: 0.70,
                bulge: 0.12,
            });
        }
    }
    hotspots
}

fn oceanic_subsidence(plate: &Plate, point: Vec2, config: &MacroConfig, model: &ShapeModel) -> f32 {
    if plate.continental {
        return 0.0;
    }
    let delta = config.topology.delta_xz(plate.center, point, config.size);
    let perp = delta
        .dot(Vec2::new(-plate.spreading_axis.y, plate.spreading_axis.x))
        .abs();
    let age = (perp / (config.size * 0.5)).clamp(0.0, 1.0).sqrt();
    model.subsidence * age
}

fn crustal_elevation(
    plates: &[Plate],
    point: Vec2,
    config: &MacroConfig,
    model: &ShapeModel,
) -> f32 {
    let scale = config.size * 0.10;
    let mut sum = 0.0;
    let mut weight_sum = 0.0;
    let mut continental_weight = 0.0;
    for plate in plates {
        let d = config.topology.distance_xz(plate.center, point, config.size);
        let w = 1.0 / (1.0 + (d / scale) * (d / scale));
        sum += (plate.crest - oceanic_subsidence(plate, point, config, model)) * w;
        weight_sum += w;
        if plate.continental {
            continental_weight += w;
        }
    }
    let crust = sum / weight_sum.max(1e-6);
    let share = continental_weight / weight_sum.max(1e-6);
    let margin =
        smoothstep((0.62 - share) / 0.30) * smoothstep((share - 0.06) / 0.12);
    crust + (-0.115 - crust) * margin * 0.85
}

fn boundary_uplift(
    a: &Plate,
    b: &Plate,
    point: Vec2,
    config: &MacroConfig,
    model: &ShapeModel,
) -> f32 {
    let delta = config.topology.delta_xz(a.center, b.center, config.size);
    let len = delta.length();
    if len < 1e-3 {
        return 0.0;
    }
    let normal = delta / len;
    let tangent = Vec2::new(-normal.y, normal.x);
    let midpoint = a.center + delta * 0.5;
    let offset = config.topology.delta_xz(midpoint, point, config.size);
    let across = offset.dot(normal);
    let along = offset.dot(tangent);
    let fade = 1.0 - smoothstep((along.abs() / (len * 0.5) - 0.75) / 0.5);
    if fade <= 0.0 {
        return 0.0;
    }
    let convergence = (a.velocity - b.velocity).dot(normal);
    let width = config.size * 0.05;
    let mut uplift = 0.0;
    if convergence > model.arc_threshold {
        if a.continental && b.continental {
            uplift += 0.55 * bump(across, width * 1.6);
        } else {
            let trench_side = if b.continental { -1.0 } else { 1.0 };
            let arc_width = config.size * model.arc_width;
            uplift -= 0.45 * bump(across - trench_side * width * 0.4, width * 0.8);
            uplift += model.arc_crest * bump(across + trench_side * arc_width * 0.9, arc_width);
        }
    } else if convergence < -0.2 {
        uplift += 0.20 * bump(across, width);
    }
    uplift * fade
}

fn basin_term(point: Vec2, config: &MacroConfig) -> f32 {
    let radius = config.size * 0.36;
    let d = point.length();
    -1.05 * bump(d, radius) + 0.20 * bump(d - radius, radius * 0.55)
}

fn hotspot_bulge(hotspots: &[Hotspot], point: Vec2, config: &MacroConfig) -> f32 {
    let mut sum = 0.0;
    for hotspot in hotspots {
        let d = config
            .topology
            .distance_xz(hotspot.center, point, config.size);
        sum += hotspot.bulge * bump(d, hotspot.radius);
    }
    sum
}

fn hotspot_intensity(hotspots: &[Hotspot], point: Vec2, config: &MacroConfig) -> f32 {
    let mut peak = 0.0;
    for hotspot in hotspots {
        let d = config
            .topology
            .distance_xz(hotspot.center, point, config.size);
        let t = (d / hotspot.radius.max(1e-3)).clamp(0.0, 1.0);
        let value = hotspot.strength * (1.0 - t * t);
        if value > peak {
            peak = value;
        }
    }
    peak
}

fn tectonic_elevation(
    seed: WorldSeed,
    shape: WorldShape,
    config: &MacroConfig,
    plates: &[Plate],
    hotspots: &[Hotspot],
    point: Vec2,
) -> f32 {
    let model = model_for(shape);
    let wrapped = config.topology.wrap_xz(point, config.size);
    let crust = crustal_elevation(plates, wrapped, config, &model);
    let mut boundary = 0.0;
    let mut nearest = usize::MAX;
    let mut second = usize::MAX;
    let mut nearest_d = f32::MAX;
    let mut second_d = f32::MAX;
    for index in 0..plates.len() {
        let d = config
            .topology
            .distance_xz(plates[index].center, wrapped, config.size);
        if d < nearest_d {
            second = nearest;
            second_d = nearest_d;
            nearest = index;
            nearest_d = d;
        } else if d < second_d {
            second = index;
            second_d = d;
        }
    }
    if nearest != usize::MAX && second != usize::MAX {
        boundary = boundary_uplift(&plates[nearest], &plates[second], wrapped, config, &model);
    }
    let hot = hotspot_bulge(hotspots, wrapped, config);
    let basin = if model.basin {
        basin_term(wrapped, config)
    } else {
        0.0
    };
    let detail = fbm(
        seed,
        wrapped.x,
        wrapped.y,
        config.size / 9.0,
        3,
        config.topology,
        config.size,
    ) * model.detail;
    crust + boundary + hot + basin + detail + model.sea_offset
}

pub fn macro_elevation_at(
    seed: WorldSeed,
    shape: WorldShape,
    point: Vec2,
    config: &MacroConfig,
) -> f32 {
    let plates = plates_for(seed, shape, config);
    let hotspots = hotspots_for(seed, shape, config, &plates);
    config.sea_level
        + tectonic_elevation(seed, shape, config, &plates, &hotspots, point) * config.relief
}

fn geothermal_at(
    seed: WorldSeed,
    config: &MacroConfig,
    hotspots: &[Hotspot],
    point: Vec2,
) -> f32 {
    let wrapped = config.topology.wrap_xz(point, config.size);
    let background = (fbm(
        seed,
        wrapped.x,
        wrapped.y,
        config.size / 5.0,
        2,
        config.topology,
        config.size,
    ) + 1.0)
        * 0.5;
    let intensity = background * background * 0.30;
    intensity
        .max(hotspot_intensity(hotspots, wrapped, config))
        .clamp(0.0, 1.0)
}

fn sample_raster(raster: &[f32], side: usize, config: &MacroConfig, point: Vec2) -> f32 {
    let spacing = config.size / side as f32;
    let wrapped = config.topology.wrap_xz(point, config.size);
    let gx = (wrapped.x + config.size * 0.5) / spacing - 0.5;
    let gz = (wrapped.y + config.size * 0.5) / spacing - 0.5;
    let x0 = gx.floor();
    let z0 = gz.floor();
    let fx = gx - x0;
    let fz = gz - z0;
    let index = |ix: i64, iz: i64| -> usize {
        let side = side as i64;
        let ix = if config.topology.wraps() {
            ix.rem_euclid(side)
        } else {
            ix.clamp(0, side - 1)
        };
        let iz = if config.topology.wraps() {
            iz.rem_euclid(side)
        } else {
            iz.clamp(0, side - 1)
        };
        (iz * side + ix) as usize
    };
    let (x0, z0) = (x0 as i64, z0 as i64);
    let v00 = raster[index(x0, z0)];
    let v10 = raster[index(x0 + 1, z0)];
    let v01 = raster[index(x0, z0 + 1)];
    let v11 = raster[index(x0 + 1, z0 + 1)];
    let a = v00 + (v10 - v00) * fx;
    let b = v01 + (v11 - v01) * fx;
    a + (b - a) * fz
}

fn wind_march(
    elev: &dyn Fn(Vec2) -> f32,
    centres: &[Vec2],
    config: &MacroConfig,
    wind: Vec2,
) -> Vec<f32> {
    let side = ((centres.len() as f32).sqrt() as usize * 6).clamp(24, 72);
    let spacing = config.size / side as f32;
    let mut raster = vec![0.0f32; side * side];
    for iz in 0..side {
        for ix in 0..side {
            let p = Vec2::new(
                -config.size * 0.5 + (ix as f32 + 0.5) * spacing,
                -config.size * 0.5 + (iz as f32 + 0.5) * spacing,
            );
            raster[iz * side + ix] = elev(p);
        }
    }
    let steps = 40usize;
    let step_len = config.size / steps as f32;
    centres
        .iter()
        .map(|&centre| {
            let mut moisture = 1.0f32;
            let mut position = centre - wind * config.size;
            let mut previous = sample_raster(&raster, side, config, position);
            for _ in 0..steps {
                position += wind * step_len;
                let height = sample_raster(&raster, side, config, position);
                let rise = (height - previous) / step_len;
                if rise > 0.0 {
                    moisture -= rise * 2.2;
                } else if height < config.sea_level {
                    moisture += 0.05;
                } else {
                    moisture += 0.008;
                }
                moisture = moisture.clamp(0.04, 1.0);
                previous = height;
            }
            (1.0 - moisture).clamp(0.0, 1.0)
        })
        .collect()
}

pub fn generate_macro_field(
    seed: WorldSeed,
    layout: &VoronoiLayout,
    shape: WorldShape,
    config: &MacroConfig,
) -> MacroField {
    let centres: Vec<Vec2> = layout.cells().iter().map(|cell| cell.center).collect();
    let plates = plates_for(seed, shape, config);
    let hotspots = hotspots_for(seed, shape, config, &plates);
    let elevation: Vec<f32> = centres
        .iter()
        .map(|&centre| {
            config.sea_level
                + tectonic_elevation(seed, shape, config, &plates, &hotspots, centre)
                    * config.relief
        })
        .collect();
    let bands: Vec<BaseHeightBand> = elevation.iter().map(|&e| band_for_elevation(e)).collect();
    let sea_proximity = sea_proximity_field(layout, &bands, config);
    let latitude: Vec<f32> = centres
        .iter()
        .map(|&centre| latitude_at(centre, config.size))
        .collect();
    let prevailing_wind = prevailing_wind_for(seed);
    let step = (config.size * 0.02).max(1e-3);
    let mut downslope = Vec::with_capacity(centres.len());
    let mut slope_magnitude = Vec::with_capacity(centres.len());
    for &centre in &centres {
        let height = |p: Vec2| {
            config.sea_level
                + tectonic_elevation(seed, shape, config, &plates, &hotspots, p) * config.relief
        };
        let gx = (height(centre + Vec2::X * step) - height(centre - Vec2::X * step)) / (2.0 * step);
        let gz = (height(centre + Vec2::Y * step) - height(centre - Vec2::Y * step)) / (2.0 * step);
        let gradient = Vec2::new(gx, gz);
        let length = gradient.length();
        if length > 1e-6 {
            downslope.push(-gradient / length);
        } else {
            downslope.push(Vec2::ZERO);
        }
        slope_magnitude.push(length * config.size / config.relief.max(1.0));
    }
    let rain_shadow = wind_march(
        &|p| {
            config.sea_level
                + tectonic_elevation(seed, shape, config, &plates, &hotspots, p) * config.relief
        },
        &centres,
        config,
        prevailing_wind,
    );
    let geothermal_intensity: Vec<f32> = centres
        .iter()
        .map(|&centre| geothermal_at(seed, config, &hotspots, centre))
        .collect();
    MacroField {
        elevation,
        bands,
        sea_proximity,
        latitude,
        rain_shadow,
        geothermal_intensity,
        prevailing_wind,
        downslope,
        slope_magnitude,
        hotspots,
    }
}

pub fn band_for_elevation(elevation: f32) -> BaseHeightBand {
    if elevation < -34.0 {
        BaseHeightBand::Abyssal
    } else if elevation < -14.0 {
        BaseHeightBand::DeepOcean
    } else if elevation < -1.5 {
        BaseHeightBand::CoastalShelf
    } else if elevation < 2.5 {
        BaseHeightBand::SeaLevel
    } else if elevation < 18.0 {
        BaseHeightBand::Lowland
    } else if elevation < 45.0 {
        BaseHeightBand::Highland
    } else if elevation < 75.0 {
        BaseHeightBand::Alpine
    } else {
        BaseHeightBand::Peak
    }
}

pub fn latitude_at(point: Vec2, size: f32) -> f32 {
    (point.y / (size * 0.5)).clamp(-1.0, 1.0).abs()
}

pub const SEA_PROXIMITY_DECAY_METRES: f32 = 40.0;

fn sea_distance_metres(
    layout: &VoronoiLayout,
    bands: &[BaseHeightBand],
    config: &MacroConfig,
) -> Vec<f32> {
    let count = layout.cells().len();
    let mut dist = vec![f32::INFINITY; count];
    let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(u32, usize)>> =
        std::collections::BinaryHeap::new();
    for cell in layout.cells() {
        let band = bands[cell.id];
        if band.index() <= BaseHeightBand::CoastalShelf.index() {
            dist[cell.id] = 0.0;
            heap.push(std::cmp::Reverse((0u32, cell.id)));
        }
    }
    while let Some(std::cmp::Reverse((bits, id))) = heap.pop() {
        let here = f32::from_bits(bits);
        if here > dist[id] {
            continue;
        }
        let Some(cell) = layout.cell(id) else {
            continue;
        };
        for &neighbour in &cell.neighbours {
            let Some(other) = layout.cell(neighbour) else {
                continue;
            };
            let step = config
                .topology
                .distance_xz(cell.center, other.center, config.size);
            let next = here + step;
            if next < dist[neighbour] {
                dist[neighbour] = next;
                heap.push(std::cmp::Reverse((next.to_bits(), neighbour)));
            }
        }
    }
    dist
}

fn sea_proximity_field(
    layout: &VoronoiLayout,
    bands: &[BaseHeightBand],
    config: &MacroConfig,
) -> Vec<f32> {
    sea_distance_metres(layout, bands, config)
        .iter()
        .map(|&d| {
            if d.is_finite() {
                1.0 / (1.0 + d / SEA_PROXIMITY_DECAY_METRES)
            } else {
                0.0
            }
        })
        .collect()
}

pub fn land_fraction(elevation: &[f32], sea_level: f32) -> f32 {
    if elevation.is_empty() {
        return 0.0;
    }
    let above = elevation.iter().filter(|&&e| e > sea_level).count();
    above as f32 / elevation.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_for(dim: usize, size: f32, topology: WorldTopology) -> VoronoiLayout {
        VoronoiLayout::generate(
            WorldSeed::new(7),
            crate::terrain::voronoi::VoronoiConfig {
                size,
                grid_dim: dim as u32,
                topology,
                ..Default::default()
            },
        )
    }

    fn config(topology: WorldTopology) -> MacroConfig {
        MacroConfig {
            size: 400.0,
            topology,
            sea_level: SEA_LEVEL,
            relief: 90.0,
        }
    }

    #[test]
    fn shape_labels_round_trip() {
        for shape in WorldShape::all() {
            assert_eq!(WorldShape::parse(shape.label()), Some(shape));
        }
        assert_eq!(WorldShape::parse("nope"), None);
    }

    #[test]
    fn field_is_deterministic() {
        let layout = layout_for(10, 400.0, WorldTopology::Finite);
        let a = generate_macro_field(
            WorldSeed::new(31),
            &layout,
            WorldShape::Continent,
            &config(WorldTopology::Finite),
        );
        let b = generate_macro_field(
            WorldSeed::new(31),
            &layout,
            WorldShape::Continent,
            &config(WorldTopology::Finite),
        );
        assert_eq!(a.elevation, b.elevation);
        assert_eq!(a.rain_shadow, b.rain_shadow);
        assert_eq!(a.geothermal_intensity, b.geothermal_intensity);
        assert_eq!(a.slope_magnitude, b.slope_magnitude);
    }

    #[test]
    fn torus_elevation_is_periodic() {
        let cfg = config(WorldTopology::FiniteTorus);
        for shape in WorldShape::all() {
            for k in 0..24u64 {
                let seed = WorldSeed::new(77 + k * 131);
                let point = Vec2::new(
                    cfg.size * (seed.unit(STAGE_MACRO, k, 7) - 0.5),
                    cfg.size * (seed.unit(STAGE_MACRO, k, 8) - 0.5),
                );
                let shifted = point + Vec2::splat(cfg.size);
                let a = macro_elevation_at(seed, shape, point, &cfg);
                let b = macro_elevation_at(seed, shape, shifted, &cfg);
                assert!(
                    (a - b).abs() < 1e-3,
                    "{shape:?} not periodic at {point:?}: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn finite_field_varies_across_the_map() {
        let layout = layout_for(10, 400.0, WorldTopology::Finite);
        let field = generate_macro_field(
            WorldSeed::new(5),
            &layout,
            WorldShape::Continent,
            &config(WorldTopology::Finite),
        );
        let min = field.elevation.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = field.elevation.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(max - min > 10.0, "span was {}", max - min);
    }

    #[test]
    fn sea_proximity_is_highest_at_the_coast() {
        let layout = layout_for(12, 400.0, WorldTopology::Finite);
        let field = generate_macro_field(
            WorldSeed::new(12),
            &layout,
            WorldShape::Continent,
            &config(WorldTopology::Finite),
        );
        let peak = field
            .sea_proximity
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);
        assert!(peak > 0.9);
    }

    #[test]
    fn sea_proximity_measures_metres_not_hops() {
        let cfg = config(WorldTopology::Finite);
        let coarse = layout_for(10, 400.0, WorldTopology::Finite);
        let fine = layout_for(20, 400.0, WorldTopology::Finite);
        let bands_for = |layout: &VoronoiLayout| -> Vec<BaseHeightBand> {
            layout
                .cells()
                .iter()
                .map(|cell| {
                    if cell.center.y < -80.0 {
                        BaseHeightBand::CoastalShelf
                    } else {
                        BaseHeightBand::Lowland
                    }
                })
                .collect()
        };
        let coarse_dist = sea_distance_metres(&coarse, &bands_for(&coarse), &cfg);
        let fine_dist = sea_distance_metres(&fine, &bands_for(&fine), &cfg);

        let mut worst = 0.0f32;
        let mut compared = 0usize;
        for (index, cell) in coarse.cells().iter().enumerate() {
            if !coarse_dist[index].is_finite() || coarse_dist[index] < 150.0 {
                continue;
            }
            let mut nearest_gap = f32::INFINITY;
            let mut nearest_dist = f32::INFINITY;
            for (other_index, other) in fine.cells().iter().enumerate() {
                let gap = cell.center.distance(other.center);
                if gap < nearest_gap {
                    nearest_gap = gap;
                    nearest_dist = fine_dist[other_index];
                }
            }
            if !nearest_dist.is_finite() {
                continue;
            }
            worst = worst.max((coarse_dist[index] - nearest_dist).abs());
            compared += 1;
        }
        eprintln!("sea distance coarse-vs-fine worst drift: {worst:.1} m over {compared} cells");
        assert!(compared > 20, "only {compared} comparable cells");
        assert!(
            worst <= 25.0,
            "the same coastline drifted {worst:.1} m between grid resolutions over {compared} cells"
        );
    }

    #[test]
    fn rain_shadow_stays_normalised() {
        for shape in WorldShape::all() {
            let layout = layout_for(10, 400.0, WorldTopology::Finite);
            let field = generate_macro_field(
                WorldSeed::new(4),
                &layout,
                shape,
                &config(WorldTopology::Finite),
            );
            for &s in &field.rain_shadow {
                assert!((0.0..=1.0).contains(&s), "shadow {s} out of range");
            }
        }
    }

    #[test]
    fn rain_shadow_falls_leeward_of_a_ridge() {
        let cfg = config(WorldTopology::Finite);
        let wind = Vec2::X;
        let ridge = |p: Vec2| -> f32 { 40.0 * bump(p.x, 30.0) };
        let centres = [Vec2::new(-60.0, 0.0), Vec2::new(60.0, 0.0)];
        let shadow = wind_march(&ridge, &centres, &cfg, wind);
        assert!(
            shadow[1] > shadow[0] + 0.1,
            "leeward {} should exceed windward {}",
            shadow[1],
            shadow[0]
        );
    }

    #[test]
    fn downslope_points_downhill() {
        let cfg = config(WorldTopology::Finite);
        let layout = layout_for(10, 400.0, WorldTopology::Finite);
        let field = generate_macro_field(
            WorldSeed::new(9),
            &layout,
            WorldShape::Continent,
            &cfg,
        );
        let step = cfg.size * 0.02;
        let mut checked = 0;
        for (index, cell) in layout.cells().iter().enumerate() {
            if field.slope_magnitude[index] < 0.3 {
                continue;
            }
            let centre = cell.center;
            let down = field.downslope[index];
            let lower = macro_elevation_at(
                WorldSeed::new(9),
                WorldShape::Continent,
                centre + down * step,
                &cfg,
            );
            let higher = macro_elevation_at(
                WorldSeed::new(9),
                WorldShape::Continent,
                centre - down * step,
                &cfg,
            );
            assert!(
                lower < higher,
                "cell {} downslope does not descend: {lower} vs {higher}",
                index
            );
            checked += 1;
        }
        assert!(checked > 10, "only {checked} sloped cells");
    }

    #[test]
    fn geothermal_peaks_at_hotspots() {
        let cfg = config(WorldTopology::Finite);
        let layout = layout_for(10, 400.0, WorldTopology::Finite);
        let field = generate_macro_field(
            WorldSeed::new(23),
            &layout,
            WorldShape::Continent,
            &cfg,
        );
        assert!(!field.hotspots.is_empty());
        let strong = field
            .geothermal_intensity
            .iter()
            .filter(|&&g| g > 0.45)
            .count();
        assert!(strong > 0, "no hot geothermal cells");
        assert!(
            strong * 4 <= field.geothermal_intensity.len(),
            "geothermal too widespread: {strong}"
        );
        for &g in &field.geothermal_intensity {
            assert!((0.0..=1.0).contains(&g));
        }
    }

    #[test]
    fn continent_has_a_shelf() {
        let layout = layout_for(14, 400.0, WorldTopology::Finite);
        let field = generate_macro_field(
            WorldSeed::new(3),
            &layout,
            WorldShape::Continent,
            &config(WorldTopology::Finite),
        );
        let shelf = field
            .bands
            .iter()
            .filter(|&&b| b == BaseHeightBand::CoastalShelf)
            .count();
        assert!(
            shelf * 12 >= field.bands.len(),
            "shelf fraction too small: {shelf}/{}",
            field.bands.len()
        );
    }

    #[test]
    fn shapes_differ_in_land_fraction() {
        let layout = layout_for(12, 400.0, WorldTopology::Finite);
        let cfg = config(WorldTopology::Finite);
        let mut fractions = Vec::new();
        for shape in WorldShape::all() {
            let field = generate_macro_field(WorldSeed::new(500), &layout, shape, &cfg);
            let fraction = land_fraction(&field.elevation, cfg.sea_level);
            assert!(
                (0.02..=0.98).contains(&fraction),
                "{shape:?} fraction {fraction}"
            );
            fractions.push((shape, fraction));
        }
        let continent = fractions[0].1;
        let archipelago = fractions[1].1;
        assert!((continent - archipelago).abs() > 0.01);
    }

    #[test]
    fn land_fraction_targets_per_shape() {
        let cfg = config(WorldTopology::Finite);
        for shape in WorldShape::all() {
            let mut samples = Vec::new();
            for seed_index in 1..=12u64 {
                let layout = layout_for(14, 400.0, WorldTopology::Finite);
                let field = generate_macro_field(
                    WorldSeed::new(seed_index * 977),
                    &layout,
                    shape,
                    &cfg,
                );
                samples.push(land_fraction(&field.elevation, cfg.sea_level));
            }
            let mean = samples.iter().sum::<f32>() / samples.len() as f32;
            let min = samples.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = samples
                .iter()
                .cloned()
                .fold(f32::NEG_INFINITY, f32::max);
            eprintln!("land fraction {shape:?}: mean {mean:.3} min {min:.3} max {max:.3}");
            let (low, high) = match shape {
                WorldShape::Continent | WorldShape::Archipelago => (0.30, 0.60),
                WorldShape::IslandChain => (0.05, 0.45),
                WorldShape::InlandSea => (0.30, 0.85),
            };
            assert!(
                mean >= low && mean <= high,
                "{shape:?} mean land fraction {mean:.3} outside {low}..{high} (min {min:.3}, max {max:.3})"
            );
        }
    }
}
