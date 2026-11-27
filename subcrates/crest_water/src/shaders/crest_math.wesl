const PI2_CREST: f32 = 6.28318530717;
const INVPI2_CREST: f32 = 0.63661977236;
const PI4_CREST: f32 = 0.33661977236;

fn wang_hash(seed_in: u32) -> u32 {
    var seed = (seed_in ^ 61u) ^ (seed_in >> 16u);
    seed = seed * 9u;
    seed = seed ^ (seed >> 4u);
    seed = seed * 0x27d4eb2du;
    seed = seed ^ (seed >> 15u);
    return seed;
}

fn rng_next(state: ptr<function, u32>) -> u32 {
    *state = *state ^ ((*state) << 13u);
    *state = *state ^ ((*state) >> 17u);
    *state = *state ^ ((*state) << 5u);
    return *state;
}

fn rng_float(state: ptr<function, u32>) -> f32 {
    return f32(rng_next(state)) / 4294967296.0;
}

fn rng_gauss(state: ptr<function, u32>) -> f32 {
    var u1 = rng_float(state);
    let u2 = rng_float(state);
    u1 = max(u1, 1e-6);
    return sqrt(-2.0 * log(u1)) * cos(PI2_CREST * u2);
}

fn deep_dispersion(k_mag: f32, gravity: f32, period: f32) -> vec2<f32> {
    var w = sqrt(abs(gravity * k_mag));
    if (period > 0.0) {
        let this_period = PI2_CREST / w;
        let loops = ceil(period / this_period);
        w = PI2_CREST / (period / loops);
    }
    let dwdk = gravity / (2.0 * w);
    return vec2<f32>(w, dwdk);
}

fn pm_wind_term(w: f32, gravity: f32, wind_speed: f32) -> f32 {
    let wm = 0.87 * gravity / max(wind_speed, 0.001);
    return exp(-1.291 * pow(wm / max(w, 1e-6), 4.0));
}

fn pos_cos_squared_spreading(cos_theta: f32, turbulence: f32) -> f32 {
    if (cos_theta > 0.0) {
        return mix(INVPI2_CREST * cos_theta * cos_theta, PI4_CREST, turbulence);
    }
    return PI4_CREST * turbulence;
}

fn sample_power_log(power_log: array<vec4<f32>, 14>, octave_index: f32) -> f32 {
    let t = clamp(octave_index, 0.0, 13.0);
    let lower = i32(floor(t));
    let upper = min(lower + 1, 13);
    return mix(pow(10.0, power_log[lower].x), pow(10.0, power_log[upper].x), t - floor(t));
}

fn cmul(lhs: vec2<f32>, rhs: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        lhs.x * rhs.x - lhs.y * rhs.y,
        lhs.x * rhs.y + lhs.y * rhs.x,
    );
}

fn smoothstep01(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}
