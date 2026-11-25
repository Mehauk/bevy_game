import package::shaders::debug::region_color::debug_region_color;

const SURFACE_DEBUG_MAX_CELLS: u32 = 64u;

struct SurfaceDebug {
    origin: vec2<f32>,
    cell: vec2<f32>,
    count: vec2<f32>,
    line_width: f32,
    tint_alpha: f32,
    mode: u32,
    use_values: u32,
    cell_values: array<vec4<u32>, 16>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var<uniform> surface_debug: SurfaceDebug;

fn surface_debug_cell_id(cell: vec2<f32>) -> u32 {
    if (cell.x < 0.0 || cell.y < 0.0 || cell.x >= surface_debug.count.x || cell.y >= surface_debug.count.y) {
        return 0u;
    }
    let index = u32(cell.x) + u32(cell.y) * u32(surface_debug.count.x);
    if (index >= SURFACE_DEBUG_MAX_CELLS) {
        return 0u;
    }
    if (surface_debug.use_values != 0u) {
        return surface_debug.cell_values[index / 4u][index % 4u];
    }
    return index + 1u;
}

fn surface_debug_id_at(world_xz: vec2<f32>) -> u32 {
    return surface_debug_cell_id(floor((world_xz - surface_debug.origin) / surface_debug.cell));
}

fn surface_debug_edge_distance(world_xz: vec2<f32>) -> f32 {
    let local = (world_xz - surface_debug.origin) / surface_debug.cell;
    let cell = floor(local);
    let frac = local - cell;
    let here = surface_debug_cell_id(cell);
    if (here == 0u) {
        return 1.0e9;
    }
    var dist = 1.0e9;
    if (surface_debug_cell_id(cell + vec2<f32>(1.0, 0.0)) != here) {
        dist = min(dist, (1.0 - frac.x) * surface_debug.cell.x);
    }
    if (surface_debug_cell_id(cell + vec2<f32>(-1.0, 0.0)) != here) {
        dist = min(dist, frac.x * surface_debug.cell.x);
    }
    if (surface_debug_cell_id(cell + vec2<f32>(0.0, 1.0)) != here) {
        dist = min(dist, (1.0 - frac.y) * surface_debug.cell.y);
    }
    if (surface_debug_cell_id(cell + vec2<f32>(0.0, -1.0)) != here) {
        dist = min(dist, frac.y * surface_debug.cell.y);
    }
    return dist;
}

fn surface_debug_overlay(color: vec4<f32>, world_xz: vec2<f32>) -> vec4<f32> {
    if (surface_debug.mode == 0u) {
        return color;
    }
    var tinted = color;
    if (surface_debug.mode >= 2u) {
        let id = surface_debug_id_at(world_xz);
        if (id != 0u) {
            tinted = vec4<f32>(mix(tinted.rgb, debug_region_color(id), surface_debug.tint_alpha), tinted.a);
        }
    }
    let dist = surface_debug_edge_distance(world_xz);
    let aa = max(abs(dpdx(dist)), abs(dpdy(dist)));
    let line = 1.0 - smoothstep(0.0, max(aa * surface_debug.line_width, 1.0e-4), dist);
    return vec4<f32>(mix(tinted.rgb, vec3<f32>(1.0), line), tinted.a);
}
