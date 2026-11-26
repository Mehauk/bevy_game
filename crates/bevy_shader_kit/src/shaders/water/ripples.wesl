import package::shaders::math::normal::tilt_normal;

struct RippleWave {
    origin: vec2<f32>,
    delta: vec2<f32>,
    birth: f32,
    birth_span: f32,
    strength: f32,
    frequency: f32,
    speed: f32,
    decay: f32,
    spread: f32,
    radius: f32,
}

struct RippleSet {
    count: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    waves: array<RippleWave, 32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var<uniform> ripples: RippleSet;

fn ripple_point_slope(
    wave: RippleWave,
    origin: vec2<f32>,
    birth: f32,
    p: vec2<f32>,
    time: f32,
) -> vec2<f32> {
    let offset = p - origin;
    let distance = length(offset);
    if (distance <= wave.radius) {
        return vec2<f32>(0.0);
    }
    let rim = distance - wave.radius;
    let behind = wave.speed * (time - birth) - rim;
    if (behind < 0.0) {
        return vec2<f32>(0.0);
    }
    let envelope = exp(-(behind * wave.decay + rim * wave.spread));
    let phase = wave.frequency * behind;
    let slope = wave.strength * envelope * ((wave.decay - wave.spread) * sin(phase) - wave.frequency * cos(phase));
    return slope * offset / max(distance, 1.0e-3);
}

fn ripple_wave_slope(wave: RippleWave, p: vec2<f32>, time: f32) -> vec2<f32> {
    let steps = clamp(i32(length(wave.delta) * 3.0), 1, 4);
    var slope = vec2<f32>(0.0);
    for (var i: i32 = 0; i < steps; i = i + 1) {
        let along = f32(i) / f32(max(steps - 1, 1));
        slope = slope + ripple_point_slope(wave, wave.origin + wave.delta * along, wave.birth + wave.birth_span * along, p, time);
    }
    return slope / f32(steps);
}

fn ripple_slope(p: vec2<f32>, time: f32) -> vec2<f32> {
    var slope = vec2<f32>(0.0);
    let count = min(ripples.count, 32u);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        slope = slope + ripple_wave_slope(ripples.waves[i], p, time);
    }
    return slope;
}

fn ripple_normal(normal: vec3<f32>, slope: vec2<f32>, gain: f32) -> vec3<f32> {
    return tilt_normal(normal, slope * gain);
}

fn ripple_foam_edge(slope: f32, gain: f32) -> f32 {
    return clamp(slope * gain, 0.0, 1.0);
}
