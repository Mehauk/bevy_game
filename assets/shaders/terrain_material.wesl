import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
};

struct TerrainSurface {
    tint: vec4<f32>,
    season_tint: vec4<f32>,
    chunk_low: vec2<f32>,
    chunk_size: f32,
    control_resolution: f32,
    sea_level: f32,
    flood_low: f32,
    flood_high: f32,
    snow_height: f32,
    snow_slope_limit: f32,
    snow_amount: f32,
    tile_scale: f32,
    far_scale: f32,
    macro_scale: f32,
    roughness: f32,
    reflectance: f32,
    wetness_gain: f32,
    debug_mode: u32,
    layer_count: u32,
    cliff_layer: u32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(31) var<uniform> terrain: TerrainSurface;
@group(constants::MATERIAL_BIND_GROUP) @binding(32) var albedo_array: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(33) var albedo_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(34) var normal_array: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(35) var normal_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(36) var orm_array: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(37) var orm_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(38) var splat_weights: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(39) var weights_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(40) var splat_indices: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(41) var cell_data: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(42) var cell_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(43) var cliff_meta: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(44) var region_tint: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(45) var region_sampler: sampler;

const TILE_TEXELS: f32 = 256.0;
const MAX_MIP: f32 = 8.0;
const SNOW_LAYER: u32 = 8u;
const STOCH_GRID: f32 = 0.37;
const REGION_ROUGHNESS_MIX: f32 = 0.5;

fn hash21(p: vec2<f32>) -> f32 {
    let s = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(s) * 43758.5453123);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let sx = fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123);
    let sy = fract(sin(dot(p, vec2<f32>(269.5, 183.3))) * 43758.5453123);
    return vec2<f32>(sx, sy);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn tile_mip(uv: vec2<f32>) -> f32 {
    let dx = abs(dpdx(uv.x)) + abs(dpdy(uv.x));
    let dy = abs(dpdx(uv.y)) + abs(dpdy(uv.y));
    let footprint = max(dx, dy) * TILE_TEXELS;
    return clamp(log2(max(footprint, 1.0)), 0.0, MAX_MIP);
}

fn sample_albedo(layer: u32, uv: vec2<f32>, mip: f32) -> vec3<f32> {
    return textureSampleLevel(albedo_array, albedo_sampler, uv, layer, mip).rgb;
}

fn sample_normal(layer: u32, uv: vec2<f32>, mip: f32) -> vec3<f32> {
    return textureSampleLevel(normal_array, normal_sampler, uv, layer, mip).xyz * 2.0 - 1.0;
}

fn sample_orm(layer: u32, uv: vec2<f32>, mip: f32) -> vec3<f32> {
    return textureSampleLevel(orm_array, orm_sampler, uv, layer, mip).rgb;
}

struct StochasticField {
    o0: vec2<f32>,
    o1: vec2<f32>,
    o2: vec2<f32>,
    w: vec3<f32>,
};

fn stochastic_offsets(uv: vec2<f32>) -> StochasticField {
    let g = floor(uv * STOCH_GRID);
    let f = fract(uv * STOCH_GRID);
    if (f.x + f.y < 1.0) {
        return StochasticField(
            hash22(g) - 0.5,
            hash22(g + vec2<f32>(1.0, 0.0)) - 0.5,
            hash22(g + vec2<f32>(0.0, 1.0)) - 0.5,
            vec3<f32>(1.0 - f.x - f.y, f.x, f.y),
        );
    }
    return StochasticField(
        hash22(g + vec2<f32>(1.0, 1.0)) - 0.5,
        hash22(g + vec2<f32>(0.0, 1.0)) - 0.5,
        hash22(g + vec2<f32>(1.0, 0.0)) - 0.5,
        vec3<f32>(f.x + f.y - 1.0, 1.0 - f.x, 1.0 - f.y),
    );
}

fn stochastic_shift(field: StochasticField) -> vec2<f32> {
    return field.w.x * field.o0 + field.w.y * field.o1 + field.w.z * field.o2;
}

fn sample_albedo_st(layer: u32, uv: vec2<f32>, field: StochasticField, mip: f32) -> vec3<f32> {
    let a = sample_albedo(layer, uv + field.o0, mip);
    let b = sample_albedo(layer, uv + field.o1, mip);
    let c = sample_albedo(layer, uv + field.o2, mip);
    return field.w.x * a + field.w.y * b + field.w.z * c;
}

fn sample_normal_st(layer: u32, uv: vec2<f32>, field: StochasticField, mip: f32) -> vec3<f32> {
    let a = sample_normal(layer, uv + field.o0, mip);
    let b = sample_normal(layer, uv + field.o1, mip);
    let c = sample_normal(layer, uv + field.o2, mip);
    return normalize(field.w.x * a + field.w.y * b + field.w.z * c);
}

fn sample_orm_st(layer: u32, uv: vec2<f32>, field: StochasticField, mip: f32) -> vec3<f32> {
    let a = sample_orm(layer, uv + field.o0, mip);
    let b = sample_orm(layer, uv + field.o1, mip);
    let c = sample_orm(layer, uv + field.o2, mip);
    return field.w.x * a + field.w.y * b + field.w.z * c;
}

fn control_uv(world_xz: vec2<f32>) -> vec2<f32> {
    return (world_xz - terrain.chunk_low) / terrain.chunk_size;
}

fn control_texel(world_xz: vec2<f32>) -> vec2<i32> {
    let local = control_uv(world_xz);
    let res = i32(terrain.control_resolution);
    let cx = clamp(i32(local.x * terrain.control_resolution), 0, res - 1);
    let cy = clamp(i32(local.y * terrain.control_resolution), 0, res - 1);
    return vec2<i32>(cx, cy);
}

fn uv_grid_colour(uv: vec2<f32>) -> vec3<f32> {
    let g = uv * 8.0;
    let lx = min(fract(g.x), 1.0 - fract(g.x));
    let ly = min(fract(g.y), 1.0 - fract(g.y));
    let line = 1.0 - smoothstep(0.0, 0.045, min(lx, ly));
    return mix(vec3<f32>(0.55, 0.85, 1.0), vec3<f32>(1.0, 1.0, 1.0), line);
}

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(vertex_output, is_front);
    let world_pos = vertex_output.world_position.xyz;
    var out: FragmentOutput;

    if (terrain.debug_mode == 1u) {
        pbr_input.material.base_color = vec4<f32>(terrain.tint.rgb, 1.0);
        pbr_input.material.perceptual_roughness = terrain.roughness;
        out.color = apply_pbr_lighting(pbr_input);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }

    if (terrain.debug_mode == 2u) {
        pbr_input.material.base_color = vec4<f32>(uv_grid_colour(vertex_output.uv), 1.0);
        pbr_input.material.perceptual_roughness = 0.6;
        out.color = apply_pbr_lighting(pbr_input);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }

    if (terrain.debug_mode == 3u) {
        @if(VERTEX_COLORS)
        pbr_input.material.base_color = vertex_output.color;
        @else
        pbr_input.material.base_color = vec4<f32>(terrain.tint.rgb, 1.0);
        @if(true)
        pbr_input.material.perceptual_roughness = terrain.roughness;
        out.color = apply_pbr_lighting(pbr_input);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }

    let c_uv = control_uv(world_pos.xz);
    let c_texel = control_texel(world_pos.xz);
    let weights = textureSampleLevel(splat_weights, weights_sampler, c_uv, 0.0);
    let idx_raw = textureLoad(splat_indices, c_texel, 0) * 255.0 + 0.5;
    let layers = vec4<u32>(u32(idx_raw.x), u32(idx_raw.y), u32(idx_raw.z), u32(idx_raw.w));
    let region = textureSampleLevel(region_tint, region_sampler, c_uv, 0.0);

    let uv_near = world_pos.xz / terrain.tile_scale;
    let uv_far = world_pos.xz / terrain.far_scale;
    let st_near = stochastic_offsets(uv_near);
    let st_far = stochastic_offsets(uv_far);
    let mip_near = tile_mip(uv_near + stochastic_shift(st_near));
    let mip_far = tile_mip(uv_far + stochastic_shift(st_far));
    let view_dist = length(pbr_input.V);
    let far_w = smoothstep(45.0, 140.0, view_dist);

    var albedo = vec3<f32>(0.0);
    var orm = vec3<f32>(0.0);
    var nrm = vec3<f32>(0.0);
    var total = 0.0;
    for (var i: u32 = 0u; i < 4u; i = i + 1u) {
        let w = weights[i];
        let layer = layers[i];
        if (layer == 0u || w <= 0.0) {
            continue;
        }
        let l = layer - 1u;
        let near_a = sample_albedo_st(l, uv_near, st_near, mip_near);
        let far_a = sample_albedo_st(l, uv_far, st_far, mip_far);
        albedo = albedo + w * mix(near_a, far_a, far_w);
        orm = orm + w * sample_orm_st(l, uv_near, st_near, mip_near);
        nrm = nrm + w * sample_normal_st(l, uv_near, st_near, mip_near);
        total = total + w;
    }

    if (total <= 0.0) {
        albedo = terrain.tint.rgb;
        orm = vec3<f32>(1.0, terrain.roughness, 0.0);
        nrm = vec3<f32>(0.0, 0.0, 1.0);
        total = 1.0;
    } else {
        albedo = albedo / total;
        orm = orm / total;
        nrm = normalize(nrm / total);
        let tint_ok = max(max(region.r, region.g), region.b) > 0.004;
        albedo = albedo * select(vec3<f32>(1.0), region.rgb, tint_ok);
        let region_rough = select(terrain.roughness, region.a, region.a > 0.0);
        orm.y = mix(orm.y, region_rough, REGION_ROUGHNESS_MIX);
    }

    let macro_tint = value_noise(world_pos.xz / terrain.macro_scale);
    albedo = albedo * mix(vec3<f32>(1.0), mix(vec3<f32>(0.82, 0.86, 0.90), vec3<f32>(1.18, 1.12, 1.00), macro_tint), 0.35);
    albedo = albedo * terrain.season_tint.rgb;

    let dpos_dx = dpdx(vertex_output.world_position.xyz);
    let dpos_dy = dpdy(vertex_output.world_position.xyz);
    let du = vec2<f32>(dpdx(uv_near.x), dpdy(uv_near.x));
    let dv = vec2<f32>(dpdx(uv_near.y), dpdy(uv_near.y));
    let det = du.x * dv.y - du.y * dv.x;
    var tangent = vec3<f32>(1.0, 0.0, 0.0);
    if (abs(det) > 1e-8) {
        tangent = normalize((dpos_dx * dv.y - dpos_dy * du.y) / det);
    }
    let bitangent = cross(pbr_input.N, tangent);
    var detail_n = normalize(nrm.x * tangent + nrm.y * bitangent + nrm.z * pbr_input.N);
    let geo_steep = 1.0 - abs(pbr_input.N.y);

    let cell = textureSampleLevel(cell_data, cell_sampler, c_uv, 0.0);
    let cliff_texel = textureLoad(cliff_meta, c_texel, 0) * 255.0 + 0.5;

    let perturbed_steep = 1.0 - abs(detail_n.y);
    let cliff_w = clamp(max(cell.z, smoothstep(0.55, 1.15, perturbed_steep)), 0.0, 1.0);
    if (cliff_w > 0.0 && cliff_texel.x >= 1.0) {
        let cliff_l = u32(cliff_texel.x) - 1u;
        let p = vertex_output.world_position.xyz;
        let n = abs(detail_n);
        let wsum = n.x + n.y + n.z + 1e-5;
        let wx = n.x / wsum;
        let wy = n.y / wsum;
        let wz = n.z / wsum;
        let ca = wx * sample_albedo(cliff_l, p.zy / terrain.tile_scale, mip_near)
            + wy * sample_albedo(cliff_l, p.xz / terrain.tile_scale, mip_near)
            + wz * sample_albedo(cliff_l, p.xy / terrain.tile_scale, mip_near);
        let cn_x = sample_normal(cliff_l, p.zy / terrain.tile_scale, mip_near);
        let cn_y = sample_normal(cliff_l, p.xz / terrain.tile_scale, mip_near);
        let cn_z = sample_normal(cliff_l, p.xy / terrain.tile_scale, mip_near);
        let cliff_n = normalize(
            wx * vec3<f32>(cn_x.z, cn_x.y, cn_x.x)
            + wy * vec3<f32>(cn_y.x, cn_y.z, cn_y.y)
            + wz * cn_z,
        );
        albedo = mix(albedo, ca * terrain.season_tint.rgb, cliff_w);
        detail_n = normalize(mix(detail_n, cliff_n, cliff_w));
        orm.y = mix(orm.y, 0.85, cliff_w);
    }

    let flat_factor = 1.0 - smoothstep(terrain.snow_slope_limit, terrain.snow_slope_limit + 0.2, geo_steep);
    let alt_factor = smoothstep(terrain.snow_height - 6.0, terrain.snow_height + 6.0, world_pos.y);
    let snow_w = clamp(alt_factor * flat_factor * terrain.snow_amount, 0.0, 1.0);
    if (snow_w > 0.0) {
        let snow_a = sample_albedo_st(SNOW_LAYER, uv_near, st_near, mip_near);
        albedo = mix(albedo, snow_a * terrain.season_tint.rgb, snow_w);
        orm.y = mix(orm.y, 0.62, snow_w);
        orm.z = mix(orm.z, 0.0, snow_w);
        let snow_n = sample_normal_st(SNOW_LAYER, uv_near, st_near, mip_near);
        let snow_world = normalize(snow_n.x * tangent + snow_n.y * bitangent + snow_n.z * pbr_input.N);
        detail_n = normalize(mix(detail_n, snow_world, snow_w));
    }

    let sea_wet = 1.0 - smoothstep(terrain.sea_level, terrain.sea_level + 2.0, world_pos.y);
    let flood_wet = 1.0 - smoothstep(terrain.flood_high - 1.0, terrain.flood_high + 1.0, world_pos.y);
    let wet = clamp(max(max(sea_wet, flood_wet), cell.w), 0.0, 1.0) * terrain.wetness_gain;
    albedo = albedo * (1.0 - 0.35 * wet);
    orm.y = mix(orm.y, 0.18, wet * 0.8);

    pbr_input.N = detail_n;
    pbr_input.world_normal = detail_n;
    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness = clamp(orm.y, 0.03, 1.0);
    pbr_input.material.metallic = orm.z;
    pbr_input.material.reflectance = vec3<f32>(terrain.reflectance);
    pbr_input.diffuse_occlusion = vec3<f32>(orm.x);

    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}