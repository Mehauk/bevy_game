struct FftWaterParams {
    swell_tile_size: f32,
    swell_period: f32,
    swell_strength: f32,
    swell_fade_start: f32,
    swell_fade_end: f32,
    chop_tile_size: f32,
    chop_period: f32,
    chop_strength: f32,
    chop_fade_start: f32,
    chop_fade_end: f32,
    whitecap_threshold: f32,
    whitecap_gain: f32,
    frames: u32,
    pad0: u32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(108)
var<uniform> fft: FftWaterParams;

@group(constants::MATERIAL_BIND_GROUP) @binding(109)
var fft_swell_displacement: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(110)
var fft_swell_derivatives: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(111)
var fft_chop_displacement: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(112)
var fft_chop_derivatives: texture_2d_array<f32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(113)
var fft_sampler: sampler;
