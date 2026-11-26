Scenario 2: Dynamic weather (Calm

        
↔
↔

      

Storm transitions in real time)

If you just bake one Storm TextureArray and scale fft_amplitude down to 0.1 for calm water, it looks slightly off: in real oceans, calm water has short, crisp ripples, whereas scaling down a storm bake gives you tiny, slow, long swells.

Instead of baking 3–4 full presets and cross-fading between them, the cleanest trick (used in many commercial games) is to bake 2 complementary frequency bands:

    Swell Bake (tile_size = 110m–140m): Long, heavy, rolling ocean swells.

    Chop Bake (tile_size = 30m–40m): Smaller, steeper, faster surface waves and chop.

Then you just expose two sliders (swell_strength and chop_strength) and add them together in vertex() and fragment():

    Calm: 0.1 * Swell + 0.25 * Chop (low detail normal & zero whitecaps)

    Moderate: 0.5 * Swell + 0.7 * Chop

    Storm: 1.2 * Swell + 1.0 * Chop (high whitecap foam & strong detail normals)

Major bonus: Combining two non-matching tile sizes (like 120m and 37m) breaks up visible repeating patterns across large open water.


Here is a concrete architectural and implementation plan for adding 2-band FFT water (`crates/bevy_shader_kit`, `src/terrain/`, and `src/integration/`), aligned with `tickets/incomplete/fft-water.md`, `tests/crate_isolation.rs`, and `tests/module_isolation.rs`.

---

### 1. Core Architecture: 2-Band Looping FFT Bake (GPU Compute → `texture_2d_array`)

Following `tickets/incomplete/fft-water.md`, rather than running a single fixed weather preset or doing full 3D/2D IFFTs every frame for every band, bake **two complementary frequency bands** into periodic looping `texture_2d_array<f32>` assets and blend them in real time with `swell_strength` and `chop_strength`:

| Band      | Tile Size | Wavelength Focus                               | Grid / Frames    | Loop Period | Purpose                                               |
| :-------- | :-------- | :--------------------------------------------- | :--------------- | :---------- | :---------------------------------------------------- |
| **Swell** | `120.0 m` | Long heavy ocean swells (`~20–120 m`)          | `128 × 128 × 64` | `12.0 s`    | Vertex heave far out, lens meniscus, CPU buoyancy     |
| **Chop**  | `37.0 m`  | Short steep surface waves & chop (`~0.6–20 m`) | `128 × 128 × 64` | `8.0 s`     | Near-camera vertex chop, steep normals, whitecap foam |

Why this fits this codebase:
- **Zero visible tiling:** `120 m` and `37 m` are coprime, breaking up periodic repetition across the `3000 × 3000 m` world and outer clipmap rings.
- **Real-time weather (`Calm ↔ Moderate ↔ Storm`):**
  - **Calm:** `0.1 * Swell + 0.25 * Chop` (crisp short ripples, low normal gain, zero whitecaps)
  - **Moderate:** `0.5 * Swell + 0.7 * Chop`
  - **Storm:** `1.2 * Swell + 1.0 * Chop` (heavy swells, pinched crests, strong whitecap foam & detail normals)
- **Zero per-frame FFT cost & zero-lag CPU buoyancy:** Each band bakes once at startup (via a `RenderApp` compute pipeline matching `crates/bevy_shader_kit/src/bake.rs`, plus an optional CPU buffer retained for the Swell band). Gameplay/post-process simply sample with two scalars.

---

### 2. Ocean Spectrum & GPU Compute FFT Baker (`crates/bevy_shader_kit`)

Keep all spectrum math and GPU baking inside `crates/bevy_shader_kit` (`tests/crate_isolation.rs`).

#### A. Spectrum & Baking Types (`crates/bevy_shader_kit/src/fft.rs`)
- Define `FftBandConfig`:
  - `tile_size: f32`, `resolution: u32` (power of 2, e.g. `128`), `frames: u32` (`64`), `loop_period: f32`
  - `wind_speed: f32`, `wind_direction: Vec2`, `fetch: f32`, `directional_spread: f32`
  - `min_wavelength: f32` / `max_wavelength: f32` (smooth band-pass filter so Swell only carries long wavelengths and Chop only carries short/mid wavelengths without double-counting)
  - `choppiness: f32` (horizontal `(dx, dz)` scale $\lambda$)
- **Time quantization for seamless looping:**
  - Deep-water dispersion $\omega_0(k) = \sqrt{g |k|}$, snapped to the nearest integer multiple of $\Delta\omega = 2\pi / T_{\text{loop}}$:
    $$\omega(k) = \max\left(\text{round}\!\left(\frac{\omega_0(k)}{\Delta\omega}\right), 1\right) \Delta\omega$$
  - This guarantees `frame[0] == frame[frames]` identically when wrapping `t mod loop_period`.
- **Per-band GPU outputs (`TextureDimension::D2`, `array_layers = frames`, `Rgba16Float`, `FILTERABLE`):**
  1. `displacement` (`dx, dy, dz, jacobian`):
     - `dy` = vertical elevation
     - `(dx, dz)` = horizontal Gerstner-like pinch displacement
     - `jacobian` = $(1 + \lambda \partial dx / \partial x)(1 + \lambda \partial dz / \partial z) - (\lambda \partial dx / \partial z)^2$, computed directly in spectral space via $ik_x, ik_z$ multipliers
  2. `derivatives` (`slope_x, slope_z, foam_energy, 0.0`):
     - Analytical spectral slopes $(\partial dy / \partial x, \partial dy / \partial z)$ so fragment normals don't suffer bilinear finite-difference blockiness
     - **Mipmaps:** Generate a full mip chain for each layer's `derivatives` texture (via compute downsample or CPU upload) so distant ocean fragments filter cleanly without sparkle/aliasing.

#### B. GPU Compute Bake Pipeline (`crates/bevy_shader_kit/src/fft_bake.rs` + `shaders/water/fft_bake_*.wesl`)
Mirror `GpuSdfBakePlugin` (`crates/bevy_shader_kit/src/bake.rs`):
- `FftBakeQueue` + `FftBakeRequest` extracted in `ExtractSchedule` and dispatched in `RenderSystems::PrepareResources`:
  1. **`fft_spectrum.wesl`**: Given wavevector $\mathbf{k}$, JONSWAP/Phillips amplitude, directional spreading, band-pass mask, and deterministic complex Gaussian pair, populate `h0(k)` and conjugate `h0*(-k)`.
  2. **`fft_evolve.wesl`**: For each frame $f \in [0, \text{frames})$, evaluate $h(\mathbf{k}, t_f)$ and spectral derivative fields (`dy`, `dx`, `dz`, `dx_dx`, `dz_dz`, `dx_dz`, `dy_dx`, `dy_dz`).
  3. **`fft_butterfly.wesl`**: Ping-pong Stockham radix-2 horizontal + vertical 1D IFFT passes across `log2(N)` stages (`7` passes for `128×128`), then pack `displacement` and `derivatives` slices directly into the `2d_array` storage layers.
  4. **CPU Swell mirror:** Bake (or read back once at startup) a lightweight `64 × 64 × 64` `dy / dx / dz` array for the Swell band on CPU (`FftSwellField`) so `FixedUpdate` buoyancy has zero-lag analytical parity without per-frame GPU readback.

---

### 3. Material & WESL Shader Integration (`crates/bevy_shader_kit`)

#### A. New Uniforms & Bindings (`crates/bevy_shader_kit/src/water.rs`)
Add `FftWaterParams` (`#[repr(C)] #[derive(ShaderType)]`):
- `swell_tile_size: f32`, `swell_period: f32`, `swell_strength: f32`, `swell_fade_start: f32`, `swell_fade_end: f32`
- `chop_tile_size: f32`, `chop_period: f32`, `chop_strength: f32`, `chop_fade_start: f32`, `chop_fade_end: f32`
- `whitecap_threshold: f32`, `whitecap_gain: f32`, `frames: u32`, `pad0: u32`

Extend `WaterMaterialExt` and `WaterMaterialKey` (continuing from existing `@binding(107)`):
- `@uniform(108) pub fft_params: FftWaterParams`
- `@texture(109, dimension = "2d_array") pub swell_displacement: Handle<Image>`
- `@texture(110, dimension = "2d_array") pub swell_derivatives: Handle<Image>`
- `@texture(111, dimension = "2d_array") pub chop_displacement: Handle<Image>`
- `@texture(112, dimension = "2d_array") pub chop_derivatives: Handle<Image>`
- `@sampler(113) pub fft_sampler: Sampler` (repeat wrap + linear/aniso mip filtering)
- `pub fft: bool` -> adds `ShaderDefVal::Bool("WATER_FFT", key.fft)` in `water_shader_defs` (which `apply_water_specialization` already forwards to **both** vertex and fragment pipelines).

#### B. WESL Modules (`crates/bevy_shader_kit/src/shaders/water/`)
- **`fft_types.wesl`**: `FftWaterParams` struct and `@binding(108..113)` declarations.
- **`fft.wesl`**:
  - `fft_sample_band_disp(tex, xz, time, tile_size, period, frames) -> vec4<f32>`: wraps UV `xz / tile_size`, computes fractional frame `u = fract(time / period) * f32(frames)`, samples `layer0` and `layer1 = (layer0 + 1) % frames` with `textureSampleLevel(..., 0.0)` (vertex-safe!) and `mix`es them.
  - `fft_sample_band_deriv(tex, xz, time, tile_size, period, frames) -> vec4<f32>`: uses `textureSample` with hardware mip-selection in fragment stage (`layer0`/`layer1` blend) for alias-free distant slopes & Jacobian folding.
  - `fft_displacement(xz, time, cam_dist) -> vec3<f32>`:
    - Evaluates `swell_disp * fft.swell_strength * swell_atten` (fading at `250..500 m` where outer clipmap cells become huge)
    - Adds `chop_disp * fft.chop_strength * chop_atten` (fading at `40..80 m` where clipmap quads widen beyond `2–4 m`).
  - `fft_surface(xz, time) -> vec3<f32>`: returns `(slope_x, slope_z, whitecap)` combined from Swell + Chop (note: in `fragment`, Swell+Chop slopes are **never** faded by the `40..80m` vertex chop cutoff so distant water retains crisp specular highlights and ripples).
  - `fft_whitecap(jacobian: f32) -> f32`: `smoothstep(whitecap_threshold, whitecap_threshold - 0.55, jacobian) * whitecap_gain` (near `0` in Calm `0.1 * Swell + 0.25 * Chop`, strong on pinched Storm crests).

#### C. Vertex, Fragment & Blueprint Updates
- **`water_vertex.wesl`**:
  - Branch on `@if(WATER_FFT)` vs `@if(!WATER_FFT)` (keeping the existing `SwellSet` path intact as fallback / toggle).
  - Compute horizontal drift offset `disp = fft_displacement(swell_rest, water.time, cam_dist)` and perturb `out.world_position += disp` and `out.world_normal` (using the sampled Swell + Chop slope attenuated to match vertex displacement).
- **`water.wesl` & `crates/bevy_shader_kit/src/compose.rs`**:
  - Update `water_blueprint()` and `water.wesl` together (keeping `water_blueprint_reproduces_the_shipped_shader` passing):
    - Under `SurfaceDetail` / `@if(WATER_FFT)`: sample `fft_surface(ctx.p.xz, water.time)`, scale `detail_slope` by `clamp(fft.chop_strength, 0.15, 1.2)`, and add `fft_slope` into `ctx.slope` before `tilt_normal`.
    - Under `FoamEdge` / `@if(WATER_FFT)`: fold `fft_whitecap` into `ctx.edge = max(ctx.edge, whitecap)` so breaking storm crests pick up the existing `scrolling_fbm_2d` foam pattern, roughness, and hemisphere shading automatically.

---

### 4. Underwater Meniscus Sync (`crates/bevy_shader_kit` & `src/integration/post_process.rs`)

Currently `crates/bevy_shader_kit/src/shaders/post_process/underwater.wesl` evaluates `swell_height(settings.swells, p, time)` so the waterline meniscus on the camera lens matches the 3D water mesh. With FFT water active:
- Extend `PostProcessPlugin` / `underwater.rs` & `underwater.wesl` (or add a small `fft_swell` sampler/2D-array binding `@binding(5..6)` on the underwater pass when `WATER_FFT` is enabled) to sample the Swell displacement texture at `lens.xz` (scaled by `swell_strength`), plus optional `chop_strength` micro-ripple, keeping the lens waterline locked 1:1 to the rendered wave surface.

---

### 5. Terrain, Weather Controls & CPU Buoyancy (`src/terrain/` & `src/integration/`)

#### A. `SeaState` & Weather Blending (`src/terrain/water.rs`)
- Add fields to `SeaState`:
  - `pub fft_enabled: bool`
  - `pub swell_strength: f32` (default `0.5`)
  - `pub chop_strength: f32` (default `0.7`)
  - `pub fft_swell: Option<FftSwellField>` (CPU mirror for the Swell band)
- Add `WeatherPreset` (`Calm`, `Moderate`, `Storm`) + continuous interpolation from `wave_energy` (`0.0..1.0`):
  - `0.0 -> (0.10, 0.25)` (`Calm`)
  - `0.5 -> (0.50, 0.70)` (`Moderate`)
  - `1.0 -> (1.20, 1.00)` (`Storm`)
- Update `SeaState::shore_surface_height_at`:
  - When `fft_enabled` and `fft_swell` is present: perform the same 2-step fixed-point solve `(x - dx, z - dz)` on `fft_swell` scaled by `swell_strength` and damped by `shore_vert_damp(rest_depth)` + `TROUGH_CLEARANCE`.
  - When `swells_enabled` is false, both `swell_strength` and `chop_strength` push `0.0` so `toggle_water_swells` keeps working cleanly.
- Update `sync_water_params` to push `fft_params` (`swell_strength`, `chop_strength`, `whitecap_gain`, etc.) and `extension.fft` to `WaterMaterial`.

#### B. Clipmap & Integration Wiring
- **`src/terrain/water.rs`**: Verify `build_water_clipmap_mesh()` inner spacing (`0.5 m` inner ring, `1.0 m` ring 1, `2.0 m` ring 2 up to `64 m`) cleanly supports `chop_fade = 40..80 m` and outer rings (`4..128 m`) support `swell_fade = 250..500 m`.
- **`src/terrain/mod.rs`**: Register console commands via `command!`:
  - `toggle_water_fft`
  - `set_water_weather <calm|moderate|storm>` (also optionally drives `detail.normal_gain` and `foam_strength` smoothly)
  - `set_swell_strength <value>`
  - `set_chop_strength <value>`
- **`src/integration/submersion.rs`, `ripples.rs`, `post_process.rs`:** Already read `SeaState::flooded_surface_height_at` and `SeaState` — no cross-feature changes required (`tests/module_isolation.rs` stays green), only forwarding `fft_swell` / `swell_strength` / `chop_strength` into `UnderwaterPostProcess` in `post_process.rs`.

---

### 6. Implementation Order & Verification

1. **Spectrum & GPU/CPU Baker (`crates/bevy_shader_kit`):**
   - Implement `fft.rs`, `fft_bake.rs`, and the Stockham compute WESL shaders.
   - Unit-test spectral Hermitian symmetry (real-valued IFFT output), dispersion period quantization (`t = 0.0` equals `t = loop_period`), periodic tile boundaries (`x = 0` matches `x = tile_size`), and finite Jacobian values across Calm/Storm wind inputs.
2. **WESL Sampling & `WaterMaterialExt` (`crates/bevy_shader_kit`):**
   - Implement `fft_types.wesl`, `fft.wesl`, update `water_vertex.wesl`, `water.wesl`, and `compose.rs` (`water_blueprint`).
   - Run `cargo test --workspace` to ensure `water_blueprint_reproduces_the_shipped_shader`, uniform `ShaderType::min_size()` alignment checks, and `apply_water_specialization` vertex+fragment def forwarding all pass.
3. **`SeaState`, Weather Presets & Commands (`src/terrain/`):**
   - Wire `FftBakeQueue` in `setup_water_surface`, add `SeaState` weather controls, CPU Swell sampling, and console commands.
   - Unit-test weather preset interpolation, `toggle_water_swells` / `toggle_water_fft` stickiness, and CPU Swell 2-step fixed-point height continuity across `loop_period` and `tile_size`.
4. **Underwater Sync & Isolation Verification:**
   - Sync `UnderwaterPostProcess` lens meniscus with the Swell band.
   - Run `cargo test --test crate_isolation`, `cargo test --test module_isolation`, and `cargo test --workspace`.
   - Verify in-game (`cargo run`): switch between `calm`, `moderate`, and `storm`, confirm whitecaps appear only on pinched Storm crests, confirm `120 m + 37 m` shows no repeating grid pattern from aerial `angle_camera` + `screenshot`, confirm character buoyancy and underwater lens meniscus match the waves, and check `profiler_report` for startup bake time and steady frame times.