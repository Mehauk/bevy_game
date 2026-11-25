const SWELL_TAU: f32 = 6.28318530718;
const SWELL_SHOAL_DEPTH: f32 = 4.0;
const SWELL_SHORE_ZONE: f32 = 6.0;
const SWELL_REFRACTION: f32 = 0.8;
const SWELL_SKEW: f32 = 0.5;

struct SwellWave {
    direction: vec2<f32>,
    amplitude: f32,
    wavelength: f32,
    speed: f32,
    steepness: f32,
    pad0: f32,
    pad1: f32,
}

struct SwellSet {
    count: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    waves: array<SwellWave, 4>,
}

struct ShoreContext {
    rest_depth: f32,
    shore_dir: vec2<f32>,
    shore_factor: f32,
    energy: f32,
}

fn shore_vert_damp(rest_depth: f32) -> f32 {
    return smoothstep(0.0, SWELL_SHOAL_DEPTH, rest_depth);
}

fn shore_horiz_damp(rest_depth: f32) -> f32 {
    return smoothstep(0.5, SWELL_SHOAL_DEPTH, rest_depth);
}

fn shore_factor(rest_depth: f32) -> f32 {
    return 1.0 - smoothstep(0.0, SWELL_SHORE_ZONE, rest_depth);
}

fn swell_amplitude_span(swell_set: SwellSet) -> f32 {
    var span: f32 = 0.0;
    let count = min(swell_set.count, 4u);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        span = span + swell_set.waves[i].amplitude;
    }
    return span;
}

fn swell_phase(wave: SwellWave, p: vec2<f32>, time: f32) -> f32 {
    let k = SWELL_TAU / wave.wavelength;
    return k * (dot(wave.direction, p) - wave.speed * time);
}

fn swell_offset(swell_set: SwellSet, p: vec2<f32>, time: f32) -> vec3<f32> {
    var offset = vec3<f32>(0.0);
    let count = min(swell_set.count, 4u);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let wave = swell_set.waves[i];
        let phase = swell_phase(wave, p, time);
        let s = sin(phase);
        let c = cos(phase);
        let horizontal = wave.steepness * wave.amplitude * c;
        offset = offset + vec3<f32>(wave.direction.x * horizontal, wave.amplitude * s, wave.direction.y * horizontal);
    }
    return offset;
}

fn swell_height(swell_set: SwellSet, p: vec2<f32>, time: f32) -> f32 {
    let drift = swell_offset(swell_set, p, time);
    return swell_offset(swell_set, p - drift.xz, time).y;
}

fn swell_normal(swell_set: SwellSet, p: vec2<f32>, time: f32) -> vec3<f32> {
    var tangent = vec3<f32>(1.0, 0.0, 0.0);
    var binormal = vec3<f32>(0.0, 0.0, 1.0);
    let count = min(swell_set.count, 4u);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let wave = swell_set.waves[i];
        let k = SWELL_TAU / wave.wavelength;
        let phase = swell_phase(wave, p, time);
        let s = sin(phase);
        let c = cos(phase);
        let slope = wave.amplitude * k;
        let steep = wave.steepness * slope;
        tangent.y = tangent.y + slope * wave.direction.x * c;
        binormal.y = binormal.y + slope * wave.direction.y * c;
        tangent.x = tangent.x - steep * wave.direction.x * wave.direction.x * s;
        tangent.z = tangent.z - steep * wave.direction.x * wave.direction.y * s;
        binormal.x = binormal.x - steep * wave.direction.x * wave.direction.y * s;
        binormal.z = binormal.z - steep * wave.direction.y * wave.direction.y * s;
    }
    return normalize(cross(binormal, tangent));
}

fn shored_wave(wave: SwellWave, shore: ShoreContext) -> vec4<f32> {
    var direction = wave.direction;
    if (shore.shore_factor > 0.0) {
        let bent = mix(wave.direction, shore.shore_dir, shore.shore_factor * SWELL_REFRACTION);
        let bent_len = dot(bent, bent);
        if (bent_len > 1.0e-6) {
            direction = bent / sqrt(bent_len);
        }
    }
    let depth = max(shore.rest_depth, 0.25);
    let shoal = clamp(pow(SWELL_SHOAL_DEPTH / depth, 0.25), 1.0, 2.5);
    let squeeze = clamp(sqrt(depth / SWELL_SHOAL_DEPTH), 0.4, 1.0);
    let amplitude = wave.amplitude * shoal * (0.5 + shore.energy);
    let wavelength = wave.wavelength * squeeze;
    return vec4<f32>(direction, amplitude, wavelength);
}

fn swell_offset_shored(swell_set: SwellSet, p: vec2<f32>, time: f32, shore: ShoreContext) -> vec3<f32> {
    var offset = vec3<f32>(0.0);
    let vert = shore_vert_damp(shore.rest_depth);
    let horiz = shore_horiz_damp(shore.rest_depth);
    let count = min(swell_set.count, 4u);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let wave = swell_set.waves[i];
        let shaped = shored_wave(wave, shore);
        let k = SWELL_TAU / shaped.w;
        let phase = k * (dot(shaped.xy, p) - wave.speed * time);
        let s = sin(phase);
        let c = cos(phase);
        let horizontal = wave.steepness * shaped.z * c * horiz;
        offset = offset + vec3<f32>(shaped.x * horizontal, shaped.z * s * vert, shaped.y * horizontal);
        if (shore.shore_factor > 0.0) {
            let skew = shore.shore_factor * wave.steepness * shaped.z * c * SWELL_SKEW * horiz;
            offset = offset + vec3<f32>(shore.shore_dir.x * skew, 0.0, shore.shore_dir.y * skew);
        }
    }
    return offset;
}

fn swell_height_shored(swell_set: SwellSet, p: vec2<f32>, time: f32, shore: ShoreContext) -> f32 {
    let drift = swell_offset_shored(swell_set, p, time, shore);
    return swell_offset_shored(swell_set, p - drift.xz, time, shore).y;
}
