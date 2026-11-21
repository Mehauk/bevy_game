use bevy::prelude::*;

use crate::terrain::map::TerrainMap;
use crate::terrain::region::Region;
use crate::terrain::DebugView;

const VALLEY_BAND: f32 = 0.45;
const SLOPE_BAND: f32 = 0.72;
const HILLSIDE_BAND: f32 = SLOPE_BAND - VALLEY_BAND;
const RIDGE_BAND: f32 = 1.0 - SLOPE_BAND;

const VALLEY_COLOUR: Color = Color::srgb(0.20, 0.34, 0.16);
const HILLSIDE_COLOUR: Color = Color::srgb(0.28, 0.42, 0.20);
const SLOPE_ROCK_COLOUR: Color = Color::srgb(0.38, 0.36, 0.31);
const RIDGE_COLOUR: Color = Color::srgb(0.48, 0.46, 0.43);

const SHELF_COLOUR: Color = Color::srgb(0.24, 0.62, 0.66);
const DEEP_SHELF_COLOUR: Color = Color::srgb(0.05, 0.30, 0.55);
const ABYSS_COLOUR: Color = Color::srgb(0.02, 0.06, 0.20);
const SHELF_TINT: f32 = 0.30;

pub fn normalized_height(y: f32, amplitude: f32) -> f32 {
    ((y / amplitude) * 0.5 + 0.5).clamp(0.0, 1.0)
}

pub fn height_colour(height: f32) -> LinearRgba {
    let (from, to, t) = if height < VALLEY_BAND {
        (VALLEY_COLOUR, HILLSIDE_COLOUR, height / VALLEY_BAND)
    } else if height < SLOPE_BAND {
        (
            HILLSIDE_COLOUR,
            SLOPE_ROCK_COLOUR,
            (height - VALLEY_BAND) / HILLSIDE_BAND,
        )
    } else {
        (
            SLOPE_ROCK_COLOUR,
            RIDGE_COLOUR,
            (height - SLOPE_BAND) / RIDGE_BAND,
        )
    };
    let (from, to) = (from.to_linear(), to.to_linear());
    from.mix(&to, t)
}

pub fn bathymetric_colour(depth: f32, extent: f32) -> LinearRgba {
    let t = (depth / extent.max(1.0)).clamp(0.0, 1.0);
    if t < SHELF_TINT {
        SHELF_COLOUR
            .to_linear()
            .mix(&DEEP_SHELF_COLOUR.to_linear(), t / SHELF_TINT)
    } else {
        DEEP_SHELF_COLOUR
            .to_linear()
            .mix(&ABYSS_COLOUR.to_linear(), (t - SHELF_TINT) / (1.0 - SHELF_TINT))
    }
}

pub fn region_colour(region: Region) -> LinearRgba {
    let hue = region.index() as f32 / Region::count() as f32 * 360.0;
    Color::hsl(hue, 0.55, 0.45).to_linear()
}

pub fn flow_intensity(accumulation: f32, peak: f32) -> f32 {
    (1.0 + accumulation.max(0.0)).ln() / (1.0 + peak.max(1.0)).ln()
}

fn ramp_colour(value: f32, low: Color, high: Color) -> LinearRgba {
    low.to_linear()
        .mix(&high.to_linear(), value.clamp(0.0, 1.0))
}

pub fn debug_colour(
    map: &TerrainMap,
    view: DebugView,
    point: Vec2,
    height: f32,
    extent: f32,
) -> LinearRgba {
    match view {
        DebugView::None => {
            if height < map.sea_level() {
                bathymetric_colour(map.sea_level() - height, extent)
            } else {
                height_colour(normalized_height(height, extent))
            }
        }
        DebugView::CellBorders => cell_border_colour(map, point),
        DebugView::Region => map
            .region_at(point.x, point.y)
            .map(region_colour)
            .unwrap_or(LinearRgba::BLACK),
        DebugView::Slope => ramp_colour(
            map.slope_at(point.x, point.y) / 1.6,
            Color::srgb(0.10, 0.40, 0.20),
            Color::srgb(0.95, 0.25, 0.15),
        ),
        DebugView::Precipitation => ramp_colour(
            map.precipitation_at(point.x, point.y),
            Color::srgb(0.95, 0.90, 0.60),
            Color::srgb(0.10, 0.30, 0.90),
        ),
        DebugView::Flow => {
            let peak = map.accumulation_field().max().max(1.0);
            ramp_colour(
                flow_intensity(map.accumulation_at(point.x, point.y), peak),
                Color::srgb(0.05, 0.10, 0.12),
                Color::srgb(0.20, 0.90, 1.00),
            )
        }
        DebugView::FloodZone => {
            if map.flood_zone().contains(height) {
                LinearRgba::new(0.95, 0.75, 0.15, 1.0)
            } else if height < map.sea_level() {
                LinearRgba::new(0.05, 0.20, 0.60, 1.0)
            } else {
                height_colour(normalized_height(height, extent))
            }
        }
    }
}

const BORDER_DOMINANCE: f32 = 0.96;

fn cell_border_colour(map: &TerrainMap, point: Vec2) -> LinearRgba {
    let weights = map.cell_weights_at(point.x, point.y);
    let Some(dominant) = weights.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
        return LinearRgba::BLACK;
    };
    if dominant.1 < BORDER_DOMINANCE {
        return LinearRgba::new(1.0, 1.0, 1.0, 1.0);
    }
    map.region_of_cell(dominant.0)
        .map(region_colour)
        .unwrap_or(LinearRgba::BLACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_bands_tile_the_unit_range_exactly() {
        assert!(VALLEY_BAND < SLOPE_BAND, "band edges must be ordered");
        let total = VALLEY_BAND + HILLSIDE_BAND + RIDGE_BAND;
        assert!(
            (total - 1.0).abs() < 1e-6,
            "bands must tile 0..1 exactly, got {total}"
        );
    }

    #[test]
    fn normalized_height_spans_and_clamps_to_zero_one() {
        assert!(
            (normalized_height(0.0, 22.0) - 0.5).abs() < 1e-6,
            "mid height is mid range"
        );
        assert_eq!(normalized_height(22.0, 22.0), 1.0);
        assert_eq!(normalized_height(-22.0, 22.0), 0.0);
        assert_eq!(normalized_height(1000.0, 22.0), 1.0, "must clamp above");
        assert_eq!(normalized_height(-1000.0, 22.0), 0.0, "must clamp below");
    }

    #[test]
    fn height_colour_is_continuous_across_band_boundaries() {
        for edge in [VALLEY_BAND, SLOPE_BAND] {
            let below = height_colour(edge - 1e-4).to_f32_array();
            let above = height_colour(edge + 1e-4).to_f32_array();
            let delta: f32 = (0..3).map(|i| (below[i] - above[i]).abs()).sum();
            assert!(
                delta < 0.01,
                "colour jumps at band edge {edge}: delta {delta}"
            );
        }
    }

    #[test]
    fn height_colour_endpoints_are_the_outer_band_colours() {
        assert_eq!(
            height_colour(0.0).to_f32_array(),
            VALLEY_COLOUR.to_linear().to_f32_array()
        );
        assert_eq!(
            height_colour(1.0).to_f32_array(),
            RIDGE_COLOUR.to_linear().to_f32_array()
        );
    }

    #[test]
    fn bathymetric_colour_darkens_monotonically_with_depth() {
        let luminance = |c: LinearRgba| 0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue;
        let mut previous = f32::MAX;
        for step in 0..=10 {
            let depth = step as f32 / 10.0 * 40.0;
            let value = luminance(bathymetric_colour(depth, 40.0));
            assert!(
                value <= previous + 1e-6,
                "deeper water must not brighten at depth {depth}"
            );
            previous = value;
        }
        assert_ne!(
            bathymetric_colour(0.0, 40.0).to_f32_array(),
            bathymetric_colour(40.0, 40.0).to_f32_array()
        );
    }

    #[test]
    fn region_colours_are_distinct_for_every_region() {
        let mut seen: Vec<[f32; 4]> = Vec::new();
        for definition in Region::all().iter() {
            let colour = region_colour(definition.region).to_f32_array();
            assert!(
                !seen.contains(&colour),
                "{:?} shares a colour with another region",
                definition.region
            );
            seen.push(colour);
        }
    }

    #[test]
    fn cell_borders_are_thin_strips_not_whole_interiors() {
        use crate::terrain::map::TerrainConfig;
        let map = TerrainMap::new(TerrainConfig::compact());
        let white = LinearRgba::new(1.0, 1.0, 1.0, 1.0);
        for cell in map.cells() {
            assert_ne!(
                cell_border_colour(&map, cell.center),
                white,
                "cell {} centre must not read as a border",
                cell.id
            );
        }
        let mut checked = 0usize;
        for boundary in map.layout().boundaries() {
            assert_eq!(
                cell_border_colour(&map, boundary.midpoint),
                white,
                "midpoint of edge {}|{} must read as a border",
                boundary.cell_a,
                boundary.cell_b
            );
            checked += 1;
        }
        assert!(checked > 0, "the compact world must have boundaries");
    }

    #[test]
    fn flow_intensity_is_log_scaled_across_orders_of_magnitude() {
        let headwater = flow_intensity(5.0, 5000.0);
        let mouth = flow_intensity(5000.0, 5000.0);
        assert!(headwater > 0.15, "a headwater cell must stay visible: {headwater}");
        assert!((mouth - 1.0).abs() < 1e-5, "the mouth must map to full intensity");
        assert!(
            mouth - headwater > 0.3,
            "mouth and headwater must be clearly separated: {mouth} vs {headwater}"
        );
        assert_eq!(flow_intensity(0.0, 5000.0), 0.0);
    }

    #[test]
    fn flow_debug_view_separates_headwater_from_mouth_on_a_real_map() {
        use crate::terrain::map::TerrainConfig;
        let map = TerrainMap::new(TerrainConfig::compact());
        let field = map.accumulation_field();
        let peak = field.max();
        assert!(peak > 20.0, "the compact world must develop real drainage: {peak}");

        let mut mouth_index = 0usize;
        let mut head_index = 0usize;
        let mut head_value = f32::INFINITY;
        for (index, value) in field.values.iter().enumerate() {
            if *value > field.values[mouth_index] {
                mouth_index = index;
            }
            if *value >= 1.0 && *value < head_value {
                head_value = *value;
                head_index = index;
            }
        }
        assert!(head_value < f32::INFINITY, "the compact world must have headwater cells");

        let low = Color::srgb(0.05, 0.10, 0.12);
        let high = Color::srgb(0.20, 0.90, 1.00);
        let colour_at = |index: usize| {
            let (x, z) = field.coords(index);
            let point = field.world_of(x, z);
            debug_colour(
                &map,
                DebugView::Flow,
                point,
                map.height_at(point.x, point.y),
                map.max_height_extent(),
            )
            .to_f32_array()
        };
        let delta = |a: [f32; 4], b: [f32; 4]| -> f32 {
            (0..3).map(|i| (a[i] - b[i]).abs()).sum()
        };

        let dry = ramp_colour(0.0, low, high).to_f32_array();
        let head = colour_at(head_index);
        let mouth = colour_at(mouth_index);
        let linear_reference = ramp_colour(head_value / peak, low, high).to_f32_array();

        assert!(
            delta(head, dry) > 3.0 * delta(linear_reference, dry),
            "headwater intensity must beat the old linear division by far: \
             log {} vs linear {}",
            delta(head, dry),
            delta(linear_reference, dry)
        );
        assert!(
            delta(mouth, head) > 0.2,
            "the mouth must read clearly stronger than a headwater"
        );
    }
}
