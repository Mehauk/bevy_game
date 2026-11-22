Right now, rivers are built in `src/terrain/water.rs` (`river_ribbon_mesh`) and `src/terrain/surfaces.rs` (`rebuild_water_surfaces` / `drain_water_builds`) as one `Mesh` and `WaterRenderMaterial` per `path` in `map.river_paths()`. That creates four structural issues:

1. **Fragmented overlapping meshes:** Every tributary spawns its own entity (and 9 copies on `FiniteTorus`). At confluences, separate translucent (`AlphaMode::Blend`) ribbons overlap, double-darken, z-fight, and expose blunt end-caps.
2. **Chaikin drifts out of the thalweg:** `chaikin(&chaikin(&local))` pulls bends toward the convex hull instead of interpolating the carved channel floor, and uses a flat 4-vertex cross-section whose inner edges (`station[1]` and `station[2]`) sit at `ground_center + RIBBON_LIFT`. On cross-slopes or narrow canyons, edges either float in mid-air or poke through terrain walls.
3. **Channel-blind width:** `river_half_width(accumulation, spacing)` ignores flow speed, cross-sectional channel geometry, and substrate-carved valleys—a narrow hard-rock gorge gets the same ribbon width as a wide sandy floodplain.
4. **Zero river depth in the shader:** `river_ribbon_mesh` packs `[vx, vy, flow_speed, 1.0]` into `ATTRIBUTE_COLOR`, forcing `assets/shaders/water_material.wesl` to hardcode `depth = 0.0` for `KIND_RIVER` and lose Beer-Lambert absorption and center-to-bank depth shading.

---

### Architecture for the Replacement

#### 1. River Tree & Centripetal Catmull-Rom Spline (`RiverNetwork`)
- **Dominant-parent reach decomposition:** From `map.river_paths()` and `accumulation`, build a directed forest of reaches:
  - At every confluence vertex, the incoming branch with the highest `accumulation` continues smoothly into the downstream reach (forming the trunk / higher-order parent); smaller incoming branches terminate as tributaries onto that parent.
  - Because `river_paths()` already stops tributaries at the shared confluence vertex, sorting paths by terminal `accumulation` descending and merging each continuation creates a clean hierarchy without touching `Hydrology` or breaking `TerrainMap::checksum()`.
- **Centripetal Catmull-Rom ($\alpha = 0.5$):** Parameterizing knots by $t_{i+1} = t_i + \|P_{i+1} - P_i\|^{0.5}$ (with mirrored phantom endpoints) guarantees **no loops or cusps** across mixed orthogonal/diagonal grid steps, passes directly through the carved thalweg vertices, and provides exact analytic tangents $P'(t)$.

#### 2. Coupled Water-Volume & Channel-Volume Solver
At each sampled spline station $\mathbf{p}$ with unit tangent $\hat{\mathbf{t}}$ and perpendicular $\hat{\mathbf{n}} = (-t_z, t_x)$:
- **Water volume per unit length ($V_{\text{water}}$):**
  - Physical volumetric discharge: $Q = k_Q \cdot A_{\text{acc}}(\mathbf{p}) \cdot \text{spacing}^2 \cdot s_{\text{season}}$ (multiplying `accumulation` by $\text{spacing}^2$ makes discharge scale-invariant across `subdivisions`).
  - Slope-driven flow speed: $v = \text{clamp}(v_0 \sqrt{\max(S, S_{\min})},\, v_{\min},\, v_{\max})$ from `flow.slope_at(p)`.
  - Cross-sectional water volume (wetted area):
    $$V_{\text{water}}(\mathbf{p}) = \frac{Q(\mathbf{p})}{v(\mathbf{p})}$$
- **Channel volume per unit length ($V_{\text{channel}}(y)$):**
  - Sample the actual carved heightfield cross-section $h(\mathbf{p} + u\hat{\mathbf{n}})$ across $u \in [-W_{\max}, +W_{\max}]$ (`heights.sample_catmull_rom` for smooth $C^1$ bank profiles).
  - Find the left/right bank rim crests $h_{\text{rim},L}, h_{\text{rim},R}$ and the thalweg bed $h_0$. Set the confined rim $y_{\text{rim}} = \min(h_{\text{rim},L}, h_{\text{rim},R})$.
  - Integrate channel capacity for any trial water level $y$:
    $$A_{\text{channel}}(y) = \int_{-w_L(y)}^{w_R(y)} \max\bigl(0,\, y - h(\mathbf{p} + u\hat{\mathbf{n}})\bigr)\,du$$
  - **Solve for surface height $y^*$ and asymmetric widths $(w_L, w_R)$:**
    - If $V_{\text{water}} \le A_{\text{channel}}(y_{\text{rim}})$ (confined channel/gorge): bisect $y^* \in [h_0 + \text{RIBBON\_LIFT}, y_{\text{rim}}]$ so $A_{\text{channel}}(y^*) = V_{\text{water}}$. Narrow canyons automatically produce deeper, narrower water; wide valleys produce wider, shallower water; asymmetric banks yield asymmetric $(w_L, w_R)$ that seat cleanly against the walls.
    - If $V_{\text{water}} > A_{\text{channel}}(y_{\text{rim}})$ (flat floodplain / overflowing channel): fill to $y_{\text{rim}}$ (clamped above bed by `RIBBON_LIFT`) and spread the surplus volume laterally across the banks, clamping outer bank heights to $\min(y^*, h(\text{bank}) + \text{RIBBON\_LIFT})$ so edges never float off a tilted plain.
  - Smooth $y^*$, $w_L$, and $w_R$ along the reach and clamp downstream monotonicity ($y^*_{i+1} \le y^*_i$, pinning mouths to `sea_level` or `lake.surface`).

#### 3. Single Continuous Mesh & Confluence Stitching
- **5-vertex cross-section per station:** `[left_skirt, left_bank, center, right_bank, right_skirt]` with `uv.x = [0.0, BANK_U_EXTENT, 0.5, 1.0 - BANK_U_EXTENT, 1.0]`. The center vertex carries `depth = (y* - h(point)).max(0.05)`, giving Beer-Lambert absorption in `water_material.wesl`.
- **Zero-overlap confluences:** Before triangulating a parent reach, project each incoming tributary's mouth onto the parent's left or right bank polyline, record the mouth span `[P_A, M, P_B]` as a bank split on that parent segment, omit the parent's bank skirt across `[P_A, P_B]`, and stitch the tributary's final ring directly to `[left_skirt_A, P_A, M, P_B, right_skirt_B]` at the parent's exact $y^*$.
- **Single mesh + `FiniteTorus` seams:** All reaches emit into one shared `Mesh` (`positions`, `normals`, `uvs`, `colors`, `indices`). On `FiniteTorus`, when consecutive stations cross `±size/2` (`wrap_i != wrap_{i+1}`), bridge the seam with a local unwrapped ring and start the next segment wrapped into canonical space so the `3×3` `wrapped_copies` in `surfaces.rs` align seamlessly.

---

### Exact Code Changes

*(Following `CLAUDE.md`, all code below has zero comments and preserves module boundaries.)*

#### 1. `assets/shaders/water_material.wesl`
Update `running` (`KIND_RIVER`) depth and freeze extraction in `fragment(...)` so `vdata.z` drives Beer-Lambert absorption and `vdata.w` supports localized freezing:

Replace lines 171–185 in `assets/shaders/water_material.wesl`:
```wgsl
    var depth = max(vdata.r, 0.0);
    var frozen = water.frozen_blend;
    var grad = vec2<f32>(0.0);
    var ripple = 0.0;
    var shore = 0.0;
    let ripple_phase = depth * RIPPLE_FREQ - water.time * 2.2;
    let ripple_grad = vec2<f32>(dpdx(ripple_phase), dpdy(ripple_phase));
    if (running) {
        if (water.kind == KIND_RIVER && water.use_vertex_data == 1u) {
            depth = max(vdata.z, 0.0);
            frozen = clamp(max(vdata.w, water.frozen_blend), 0.0, 1.0);
        } else {
            depth = 0.0;
        }
        grad = running_gradient(world_pos.xz, flow, vertex_output.uv.x, min(slope, 3.0));
    } else {
        frozen = clamp(max(vdata.g, water.frozen_blend), 0.0, 1.0);
        grad = standing_gradient(world_pos.xz);
        shore = 1.0 - smoothstep(0.0, max(water.foam_depth, 0.01) * 2.0, depth);
        ripple = sin(ripple_phase);
        grad = grad + ripple_grad * (RIPPLE_AMP * shore);
    }
```

#### 2. `src/terrain/surfaces.rs`
Replace `WaterBuild::River { anchor: Vec2, mesh: Mesh }` with a single `WaterBuild::RiverNetwork { mesh: Mesh }`, and spawn one async task for the whole river network instead of one per path:

In `WaterBuild`:
```rust
    RiverNetwork {
        mesh: Mesh,
    },
```

In `rebuild_water_surfaces`: replace the `for path in map.river_paths()` loop with:
```rust
    if !map.river_paths().is_empty() {
        let map = shared.clone();
        let season = clock.season;
        queue.jobs.push(AsyncComputeTaskPool::get().spawn(async move {
            let mesh = water::river_network_mesh(
                map.river_paths(),
                map.heights(),
                map.accumulation_field(),
                map.flow_field(),
                map.lakes(),
                map.ocean_mask(),
                map.water_frozen(),
                map.sea_level(),
                season,
            )?;
            Some(WaterBuild::RiverNetwork { mesh })
        }));
    }
```

In `drain_water_builds`: replace the `WaterBuild::River` arm with:
```rust
                    WaterBuild::RiverNetwork { mesh } => {
                        let material = materials.add(gpu::water_render_material(
                            water::river_material(),
                            water::WaterKind::River,
                            false,
                            true,
                        ));
                        let handle = meshes.add(mesh);
                        for offset in &copies {
                            commands.spawn((
                                Name::new("River Network"),
                                WaterSurface,
                                Transform::from_xyz(offset.x, 0.0, offset.y),
                                Mesh3d(handle.clone()),
                                MeshMaterial3d(material.clone()),
                            ));
                        }
                    }
```

#### 3. `src/terrain/water.rs`
Replace `river_half_width`, `chaikin`, and `river_ribbon_mesh` (and update `WaterField::build` if desired so `WaterSample` matches) with the centripetal Catmull-Rom spline, volume-coupled cross-section solver, confluence stitcher, and single continuous mesh builder:

```rust
const RIVER_RUNOFF_SCALE: f32 = 0.02;
const RIVER_VELOCITY_COEFF: f32 = 1.8;
const RIVER_MIN_VELOCITY: f32 = 0.35;
const RIVER_MAX_VELOCITY: f32 = 4.5;
const RIVER_MIN_WIDTH_SPACING: f32 = 0.25;
const RIVER_MAX_WIDTH_SPACING: f32 = 4.5;
const RIVER_PROFILE_STEPS: usize = 24;
const RIVER_SPLINE_SUBDIV: usize = 4;
const RIVER_OVERFLOW_SPREAD: f32 = 1.6;
const RIVER_MAX_SHEET_DEPTH: f32 = 0.45;
const RIVER_SOLVER_STEPS: usize = 12;

fn seasonal_runoff_scale(season: Season) -> f32 {
    match season {
        Season::Spring => 1.35,
        Season::Summer => 0.85,
        Season::Fall => 1.05,
        Season::Winter => 0.70,
    }
}

fn unwrap_path(path: &[usize], heights: &ScalarField) -> Vec<Vec2> {
    let mut points = Vec::with_capacity(path.len());
    if path.is_empty() {
        return points;
    }
    let first = heights.world_of(heights.coords(path[0]).0, heights.coords(path[0]).1);
    points.push(first);
    let mut cursor = first;
    for pair in path.windows(2) {
        let a = heights.world_of(heights.coords(pair[0]).0, heights.coords(pair[0]).1);
        let b = heights.world_of(heights.coords(pair[1]).0, heights.coords(pair[1]).1);
        cursor += heights.topology.delta_xz(a, b, heights.size);
        if cursor.distance_squared(*points.last().unwrap()) > 1e-6 {
            points.push(cursor);
        }
    }
    points
}

fn centripetal_segment(
    p0: Vec2,
    p1: Vec2,
    p2: Vec2,
    p3: Vec2,
    t_norm: f32,
) -> (Vec2, Vec2) {
    let chord = |a: Vec2, b: Vec2| a.distance(b).sqrt().max(1e-4);
    let t0 = 0.0f32;
    let t1 = t0 + chord(p0, p1);
    let t2 = t1 + chord(p1, p2);
    let t3 = t2 + chord(p2, p3);
    let t = t1 + (t2 - t1) * t_norm.clamp(0.0, 1.0);

    let lerp_pt = |a: Vec2, b: Vec2, ta: f32, tb: f32| -> (Vec2, Vec2) {
        let span = (tb - ta).max(1e-5);
        let u = (t - ta) / span;
        (a + (b - a) * u, (b - a) / span)
    };

    let (a1, da1) = lerp_pt(p0, p1, t0, t1);
    let (a2, da2) = lerp_pt(p1, p2, t1, t2);
    let (a3, da3) = lerp_pt(p2, p3, t2, t3);

    let (b1, db1) = {
        let span = (t2 - t0).max(1e-5);
        let u = (t - t0) / span;
        (
            a1 + (a2 - a1) * u,
            da1 * (1.0 - u) + da2 * u + (a2 - a1) / span,
        )
    };
    let (b2, db2) = {
        let span = (t3 - t1).max(1e-5);
        let u = (t - t1) / span;
        (
            a2 + (a3 - a2) * u,
            da2 * (1.0 - u) + da3 * u + (a3 - a2) / span,
        )
    };

    let span = (t2 - t1).max(1e-5);
    let u = (t - t1) / span;
    let pos = b1 + (b2 - b1) * u;
    let tan = (db1 * (1.0 - u) + db2 * u + (b2 - b1) / span).normalize_or_zero();
    (pos, tan)
}

fn sample_centripetal_spline(
    control: &[Vec2],
    subdiv: usize,
    start_hint: Option<Vec2>,
    end_hint: Option<Vec2>,
) -> Vec<(Vec2, Vec2)> {
    if control.len() < 2 {
        return Vec::new();
    }
    let subdiv = subdiv.max(1);
    let mut out = Vec::with_capacity((control.len() - 1) * subdiv + 1);
    for i in 0..control.len() - 1 {
        let p1 = control[i];
        let p2 = control[i + 1];
        let p0 = if i > 0 {
            control[i - 1]
        } else {
            start_hint.unwrap_or(p1 * 2.0 - p2)
        };
        let p3 = if i + 2 < control.len() {
            control[i + 2]
        } else {
            end_hint.unwrap_or(p2 * 2.0 - p1)
        };
        let count = if i + 2 == control.len() {
            subdiv + 1
        } else {
            subdiv
        };
        for s in 0..count {
            let t = s as f32 / subdiv as f32;
            let (pos, tan) = centripetal_segment(p0, p1, p2, p3, t);
            let tan = if tan.length_squared() > 1e-8 {
                tan
            } else {
                (p2 - p1).normalize_or_zero()
            };
            out.push((pos, tan));
        }
    }
    out
}

#[derive(Clone, Debug)]
struct RiverStation {
    pos: Vec2,
    tan: Vec2,
    side: Vec2,
    surface_y: f32,
    left_w: f32,
    right_w: f32,
    bed_y: f32,
    velocity: Vec2,
    frozen: bool,
    travelled: f32,
}

fn channel_cross_section(
    heights: &ScalarField,
    center: Vec2,
    side: Vec2,
    max_half: f32,
) -> ([f32; RIVER_PROFILE_STEPS * 2 + 1], f32, f32, f32, f32) {
    let mut profile = [0.0f32; RIVER_PROFILE_STEPS * 2 + 1];
    let step = max_half / RIVER_PROFILE_STEPS as f32;
    for k in 0..profile.len() {
        let offset = (k as isize - RIVER_PROFILE_STEPS as isize) as f32 * step;
        profile[k] = heights.sample_catmull_rom(center + side * offset);
    }
    let bed_y = profile[RIVER_PROFILE_STEPS - 1..=RIVER_PROFILE_STEPS + 1]
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let mut left_rim = bed_y;
    for k in (0..RIVER_PROFILE_STEPS).rev() {
        left_rim = left_rim.max(profile[k]);
    }
    let mut right_rim = bed_y;
    for k in (RIVER_PROFILE_STEPS + 1)..profile.len() {
        right_rim = right_rim.max(profile[k]);
    }
    (profile, step, bed_y, left_rim, right_rim)
}

fn wetted_area_and_widths(
    profile: &[f32; RIVER_PROFILE_STEPS * 2 + 1],
    step: f32,
    surface_y: f32,
    min_half: f32,
) -> (f32, f32, f32) {
    let mut area = 0.0f32;
    let mut left_w = min_half;
    for k in (0..RIVER_PROFILE_STEPS).rev() {
        let d0 = surface_y - profile[k + 1];
        let d1 = surface_y - profile[k];
        if d0 <= 0.0 && d1 <= 0.0 {
            break;
        }
        let dist = (RIVER_PROFILE_STEPS - k) as f32 * step;
        if d1 >= 0.0 {
            area += (d0.max(0.0) + d1) * 0.5 * step;
            left_w = left_w.max(dist);
        } else {
            let frac = (d0 / (d0 - d1).max(1e-5)).clamp(0.0, 1.0);
            area += d0 * 0.5 * step * frac;
            left_w = left_w.max(((RIVER_PROFILE_STEPS - k - 1) as f32 + frac) * step);
            break;
        }
    }
    let mut right_w = min_half;
    for k in (RIVER_PROFILE_STEPS + 1)..profile.len() {
        let d0 = surface_y - profile[k - 1];
        let d1 = surface_y - profile[k];
        if d0 <= 0.0 && d1 <= 0.0 {
            break;
        }
        let dist = (k - RIVER_PROFILE_STEPS) as f32 * step;
        if d1 >= 0.0 {
            area += (d0.max(0.0) + d1) * 0.5 * step;
            right_w = right_w.max(dist);
        } else {
            let frac = (d0 / (d0 - d1).max(1e-5)).clamp(0.0, 1.0);
            area += d0 * 0.5 * step * frac;
            right_w = right_w.max(((k - RIVER_PROFILE_STEPS - 1) as f32 + frac) * step);
            break;
        }
    }
    (area, left_w, right_w)
}

#[allow(clippy::too_many_arguments)]
fn solve_station_volume(
    pos: Vec2,
    tan: Vec2,
    heights: &ScalarField,
    accumulation: &ScalarField,
    flow: &FlowField,
    frozen_mask: &[bool],
    season: Season,
    travelled: f32,
) -> RiverStation {
    let spacing = heights.spacing();
    let min_half = spacing * RIVER_MIN_WIDTH_SPACING;
    let max_half = spacing * RIVER_MAX_WIDTH_SPACING;
    let side = if tan.length_squared() > 1e-8 {
        Vec2::new(-tan.y, tan.x).normalize()
    } else {
        Vec2::Y
    };

    let accum = accumulation.sample_world(pos).max(0.0);
    let discharge = accum * spacing * spacing * RIVER_RUNOFF_SCALE * seasonal_runoff_scale(season);
    let slope = flow.slope_at(pos).max(0.01);
    let speed = (RIVER_VELOCITY_COEFF * slope.sqrt()).clamp(RIVER_MIN_VELOCITY, RIVER_MAX_VELOCITY);
    let water_volume = (discharge / speed).max(0.05);

    let baked = flow.direction_at(pos);
    let flow_dir = if baked.length_squared() > 1e-6 {
        baked.normalize()
    } else {
        tan
    };
    let velocity = flow_dir * speed;

    let (gx, gz) = heights.grid_of(pos);
    let frozen = frozen_mask.get(heights.index(gx, gz)).copied().unwrap_or(false);

    let (profile, step, bed_y, left_rim, right_rim) =
        channel_cross_section(heights, pos, side, max_half);
    let channel_rim = left_rim.min(right_rim);
    let min_surface = bed_y + RIBBON_LIFT;

    let (surface_y, mut left_w, mut right_w) = if channel_rim > min_surface {
        let (channel_volume, rim_lw, rim_rw) =
            wetted_area_and_widths(&profile, step, channel_rim, min_half);
        if water_volume <= channel_volume {
            let mut lo = min_surface;
            let mut hi = channel_rim;
            for _ in 0..RIVER_SOLVER_STEPS {
                let mid = 0.5 * (lo + hi);
                let (area, _, _) = wetted_area_and_widths(&profile, step, mid, min_half);
                if area < water_volume {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let y = 0.5 * (lo + hi);
            let (_, lw, rw) = wetted_area_and_widths(&profile, step, y, min_half);
            (y, lw, rw)
        } else {
            let excess = water_volume - channel_volume;
            let spread = (excess * RIVER_OVERFLOW_SPREAD).sqrt() * 0.5;
            let sheet = (excess / ((rim_lw + rim_rw + 2.0 * spread).max(1e-3)))
                .min(RIVER_MAX_SHEET_DEPTH);
            (
                channel_rim + sheet,
                (rim_lw + spread).clamp(min_half, max_half),
                (rim_rw + spread).clamp(min_half, max_half),
            )
        }
    } else {
        let sheet = (water_volume.sqrt() * 0.25).clamp(RIBBON_LIFT, RIVER_MAX_SHEET_DEPTH);
        let half = (water_volume / (2.0 * sheet)).clamp(min_half, max_half);
        (bed_y + sheet, half, half)
    };

    left_w = left_w.clamp(min_half, max_half);
    right_w = right_w.clamp(min_half, max_half);

    RiverStation {
        pos,
        tan,
        side,
        surface_y,
        left_w,
        right_w,
        bed_y,
        velocity,
        frozen,
        travelled,
    }
}

fn smooth_stations(stations: &mut [RiverStation], outlet_y: Option<f32>) {
    if stations.len() < 3 {
        return;
    }
    let snapshot_w: Vec<(f32, f32, f32)> = stations
        .iter()
        .map(|s| (s.surface_y, s.left_w, s.right_w))
        .collect();
    for i in 1..stations.len() - 1 {
        stations[i].surface_y =
            snapshot_w[i - 1].0 * 0.25 + snapshot_w[i].0 * 0.5 + snapshot_w[i + 1].0 * 0.25;
        stations[i].left_w =
            snapshot_w[i - 1].1 * 0.25 + snapshot_w[i].1 * 0.5 + snapshot_w[i + 1].1 * 0.25;
        stations[i].right_w =
            snapshot_w[i - 1].2 * 0.25 + snapshot_w[i].2 * 0.5 + snapshot_w[i + 1].2 * 0.25;
    }
    if let Some(target_y) = outlet_y {
        if let Some(last) = stations.last_mut() {
            last.surface_y = last.surface_y.min(target_y);
        }
    }
    for i in 1..stations.len() {
        let min_y = stations[i].bed_y + RIBBON_LIFT;
        stations[i].surface_y = stations[i].surface_y.min(stations[i - 1].surface_y).max(min_y);
    }
}

struct Reach {
    stations: Vec<RiverStation>,
    parent: Option<(usize, usize)>,
}

struct BankSplit {
    station_idx: usize,
    left_side: bool,
    u_center: f32,
    u_a: f32,
    u_b: f32,
    pos_a: Vec2,
    pos_center: Vec2,
    pos_b: Vec2,
    y_a: f32,
    y_center: f32,
    y_b: f32,
    skirt_a: Vec2,
    skirt_b: Vec2,
    vel: Vec2,
    depth_center: f32,
    frozen: bool,
    travelled: f32,
}

fn bank_point(station: &RiverStation, left: bool) -> Vec2 {
    if left {
        station.pos - station.side * station.left_w
    } else {
        station.pos + station.side * station.right_w
    }
}

fn skirt_point(
    station: &RiverStation,
    left: bool,
    skirt_dist: f32,
    heights: &ScalarField,
) -> Vec3 {
    let dir = if left { -station.side } else { station.side };
    let w = if left { station.left_w } else { station.right_w };
    let xz = station.pos + dir * (w + skirt_dist);
    let y = (heights.sample_world(xz) - BANK_TUCK).min(station.surface_y - BANK_TUCK);
    Vec3::new(xz.x, y, xz.y)
}

fn bank_vertex_y(station: &RiverStation, bank_xz: Vec2, heights: &ScalarField) -> f32 {
    let local_ground = heights.sample_world(bank_xz);
    station.surface_y.min(local_ground + RIBBON_LIFT)
}

fn seg_intersect(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> Option<(f32, f32, Vec2)> {
    let da = a1 - a0;
    let db = b1 - b0;
    let denom = da.x * db.y - da.y * db.x;
    if denom.abs() < 1e-6 {
        return None;
    }
    let diff = b0 - a0;
    let ua = (diff.x * db.y - diff.y * db.x) / denom;
    let ub = (diff.x * da.y - diff.y * da.x) / denom;
    if (-0.05..=1.05).contains(&ua) && (-0.05..=1.05).contains(&ub) {
        Some((ua.clamp(0.0, 1.0), ub.clamp(0.0, 1.0), a0 + da * ua.clamp(0.0, 1.0)))
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub fn river_network_mesh(
    paths: &[Vec<usize>],
    heights: &ScalarField,
    accumulation: &ScalarField,
    flow: &FlowField,
    lakes: &[Lake],
    ocean: &[bool],
    frozen_mask: &[bool],
    sea_level: f32,
    season: Season,
) -> Option<Mesh> {
    if paths.is_empty() {
        return None;
    }

    let mut ordered: Vec<(f32, usize)> = paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.len() >= 2)
        .map(|(i, p)| {
            let end_acc = p.last().map(|&v| accumulation.get_at(v)).unwrap_or(0.0);
            (end_acc, i)
        })
        .collect();
    if ordered.is_empty() {
        return None;
    }
    ordered.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut owner_of_vertex: std::collections::HashMap<usize, (usize, usize)> =
        std::collections::HashMap::new();
    let mut reaches: Vec<Reach> = Vec::with_capacity(ordered.len());
    let spacing = heights.spacing();
    let uv_v_scale = (spacing * RIVER_UV_V_TILE_SPACING).max(1e-3);

    for (_, path_idx) in ordered {
        let path = &paths[path_idx];
        let unwrapped = unwrap_path(path, heights);
        if unwrapped.len() < 2 {
            continue;
        }
        let end_vertex = *path.last().unwrap();
        let parent = owner_of_vertex.get(&end_vertex).copied();
        let outlet_y = if ocean.get(end_vertex).copied().unwrap_or(false) {
            Some(sea_level)
        } else {
            lakes
                .iter()
                .find(|l| l.cells.contains(&end_vertex))
                .map(|l| l.surface)
        };

        let samples = sample_centripetal_spline(&unwrapped, RIVER_SPLINE_SUBDIV, None, None);
        if samples.len() < 2 {
            continue;
        }

        let mut stations = Vec::with_capacity(samples.len());
        let mut travelled = 0.0f32;
        for (idx, &(pos, tan)) in samples.iter().enumerate() {
            if idx > 0 {
                travelled += pos.distance(samples[idx - 1].0);
            }
            stations.push(solve_station_volume(
                pos,
                tan,
                heights,
                accumulation,
                flow,
                frozen_mask,
                season,
                travelled,
            ));
        }
        smooth_stations(&mut stations, outlet_y);

        let reach_id = reaches.len();
        for (step_idx, &vertex) in path.iter().enumerate() {
            let station_idx = (step_idx * RIVER_SPLINE_SUBDIV).min(stations.len() - 1);
            owner_of_vertex.entry(vertex).or_insert((reach_id, station_idx));
        }
        reaches.push(Reach { stations, parent });
    }

    if reaches.is_empty() {
        return None;
    }

    let skirt_dist = spacing * RIVER_BANK_SKIRT_SPACING;
    let mut splits_by_reach: Vec<Vec<BankSplit>> = vec![Vec::new(); reaches.len()];
    let mut trimmed_end: Vec<Option<(usize, BankSplit)>> = vec![None; reaches.len()];

    for reach_id in 0..reaches.len() {
        let Some((parent_id, approx_p_idx)) = reaches[reach_id].parent else {
            continue;
        };
        let parent_len = reaches[parent_id].stations.len();
        if parent_len < 2 {
            continue;
        }
        let approx_p_idx = approx_p_idx.min(parent_len - 1);
        let parent_anchor = reaches[parent_id].stations[approx_p_idx].pos;
        let child_end = reaches[reach_id].stations.last().unwrap().pos;
        let shift = heights
            .topology
            .delta_xz(child_end, parent_anchor, heights.size)
            - (parent_anchor - child_end);
        if shift.length_squared() > 1e-4 {
            for s in &mut reaches[reach_id].stations {
                s.pos += shift;
            }
        }

        let p_station = &reaches[parent_id].stations[approx_p_idx];
        let c_last = reaches[reach_id].stations.last().unwrap();
        let entry_offset = c_last.pos - p_station.pos;
        let left_side = if entry_offset.length_squared() > 1e-4 {
            entry_offset.dot(-p_station.side) >= 0.0
        } else {
            c_last.tan.dot(p_station.side) > 0.0
        };

        let p_start = approx_p_idx.saturating_sub(RIVER_SPLINE_SUBDIV * 2);
        let p_end = (approx_p_idx + RIVER_SPLINE_SUBDIV * 2).min(parent_len - 1);
        let child_len = reaches[reach_id].stations.len();
        let c_start = child_len.saturating_sub(RIVER_SPLINE_SUBDIV * 3);

        let mut hit: Option<(usize, usize, f32, Vec2)> = None;
        'search: for ci in (c_start..child_len - 1).rev() {
            let ca = reaches[reach_id].stations[ci].pos;
            let cb = reaches[reach_id].stations[ci + 1].pos;
            for pi in p_start..p_end {
                let pa = bank_point(&reaches[parent_id].stations[pi], left_side);
                let pb = bank_point(&reaches[parent_id].stations[pi + 1], left_side);
                if let Some((up, _, pt)) = seg_intersect(pa, pb, ca, cb) {
                    hit = Some((ci, pi, up, pt));
                    break 'search;
                }
            }
        }

        let (cut_ci, pi, u_center, hit_pt) = hit.unwrap_or((
            child_len - 2,
            approx_p_idx.min(parent_len - 2),
            0.5,
            bank_point(&reaches[parent_id].stations[approx_p_idx.min(parent_len - 2)], left_side),
        ));

        let pa_station = &reaches[parent_id].stations[pi];
        let pb_station = &reaches[parent_id].stations[pi + 1];
        let bank_a = bank_point(pa_station, left_side);
        let bank_b = bank_point(pb_station, left_side);
        let seg_len = bank_a.distance(bank_b).max(1e-3);

        let mouth_half = ((reaches[reach_id].stations[cut_ci].left_w
            + reaches[reach_id].stations[cut_ci].right_w)
            * 0.5)
            .min(seg_len * 0.45);
        let du = mouth_half / seg_len;
        let u_a = (u_center - du).clamp(0.02, 0.96);
        let u_b = (u_center + du).clamp((u_a + 0.02).min(0.98), 0.98);
        let pos_a = bank_a.lerp(bank_b, u_a);
        let pos_center = bank_a.lerp(bank_b, u_center);
        let pos_b = bank_a.lerp(bank_b, u_b);

        let parent_y = pa_station.surface_y + (pb_station.surface_y - pa_station.surface_y) * u_center;
        let y_a = pa_station.surface_y + (pb_station.surface_y - pa_station.surface_y) * u_a;
        let y_b = pa_station.surface_y + (pb_station.surface_y - pa_station.surface_y) * u_b;

        let skirt_dir = if left_side { -pa_station.side } else { pa_station.side };
        let skirt_a = pos_a + skirt_dir * skirt_dist;
        let skirt_b = pos_b + skirt_dir * skirt_dist;

        let mut split = BankSplit {
            station_idx: pi,
            left_side,
            u_center,
            u_a,
            u_b,
            pos_a,
            pos_center: hit_pt.lerp(pos_center, 0.5),
            pos_b,
            y_a,
            y_center: parent_y,
            y_b,
            skirt_a,
            skirt_b,
            vel: reaches[reach_id].stations[cut_ci].velocity,
            depth_center: (parent_y - heights.sample_world(pos_center)).max(0.05),
            frozen: reaches[reach_id].stations[cut_ci].frozen,
            travelled: reaches[reach_id].stations[cut_ci].travelled,
        };
        if split.u_a > split.u_b {
            std::mem::swap(&mut split.u_a, &mut split.u_b);
        }

        splits_by_reach[parent_id].push(BankSplit {
            station_idx: split.station_idx,
            left_side: split.left_side,
            u_center: split.u_center,
            u_a: split.u_a,
            u_b: split.u_b,
            pos_a: split.pos_a,
            pos_center: split.pos_center,
            pos_b: split.pos_b,
            y_a: split.y_a,
            y_center: split.y_center,
            y_b: split.y_b,
            skirt_a: split.skirt_a,
            skirt_b: split.skirt_b,
            vel: split.vel,
            depth_center: split.depth_center,
            frozen: split.frozen,
            travelled: split.travelled,
        });
        trimmed_end[reach_id] = Some((cut_ci, split));
    }

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for (reach_id, reach) in reaches.iter().enumerate() {
        let end_count = trimmed_end[reach_id]
            .as_ref()
            .map(|(cut, _)| (*cut + 1).max(1))
            .unwrap_or(reach.stations.len());
        let stations = &reach.stations[..end_count.min(reach.stations.len())];
        if stations.is_empty() {
            continue;
        }

        let mut push_station = |s: &RiverStation, shift: Vec2| -> u32 {
            let base = positions.len() as u32;
            let shifted = RiverStation {
                pos: s.pos + shift,
                ..*s
            };
            let lb_xz = bank_point(&shifted, true);
            let rb_xz = bank_point(&shifted, false);
            let ls = skirt_point(&shifted, true, skirt_dist, heights);
            let rs = skirt_point(&shifted, false, skirt_dist, heights);
            let lb_y = bank_vertex_y(&shifted, lb_xz, heights);
            let rb_y = bank_vertex_y(&shifted, rb_xz, heights);
            let c_depth = (shifted.surface_y - heights.sample_world(shifted.pos)).max(0.05);
            let f_flag = if shifted.frozen { 1.0 } else { 0.0 };
            let v = shifted.travelled / uv_v_scale;
            let col = [shifted.velocity.x, shifted.velocity.y, c_depth, f_flag];
            let edge_col = [shifted.velocity.x, shifted.velocity.y, 0.0, f_flag];

            positions.push(ls.to_array());
            positions.push([lb_xz.x, lb_y, lb_xz.y]);
            positions.push([shifted.pos.x, shifted.surface_y, shifted.pos.y]);
            positions.push([rb_xz.x, rb_y, rb_xz.y]);
            positions.push(rs.to_array());

            for _ in 0..5 {
                normals.push([0.0, 1.0, 0.0]);
            }
            uvs.push([0.0, v]);
            uvs.push([BANK_U_EXTENT, v]);
            uvs.push([0.5, v]);
            uvs.push([1.0 - BANK_U_EXTENT, v]);
            uvs.push([1.0, v]);

            colors.push(edge_col);
            colors.push(edge_col);
            colors.push(col);
            colors.push(edge_col);
            colors.push(edge_col);
            base
        };

        let wrap_shift = |pos: Vec2| -> Vec2 {
            if heights.topology.wraps() {
                heights.topology.wrap_xz(pos, heights.size) - pos
            } else {
                Vec2::ZERO
            }
        };

        let mut prev_shift = wrap_shift(stations[0].pos);
        let mut prev_base = push_station(&stations[0], prev_shift);

        for i in 0..stations.len().saturating_sub(1) {
            let next_shift = wrap_shift(stations[i + 1].pos);
            let bridge_base = push_station(&stations[i + 1], prev_shift);

            let split = splits_by_reach[reach_id]
                .iter()
                .find(|sp| sp.station_idx == i);

            if let Some(sp) = split {
                let mut push_pt = |xz: Vec2, y: f32, u: f32, v_t: f32, depth: f32| -> u32 {
                    let id = positions.len() as u32;
                    let p = xz + prev_shift;
                    positions.push([p.x, y, p.y]);
                    normals.push([0.0, 1.0, 0.0]);
                    uvs.push([u, v_t]);
                    colors.push([sp.vel.x, sp.vel.y, depth, if sp.frozen { 1.0 } else { 0.0 }]);
                    id
                };
                let v_a = (stations[i].travelled + (stations[i + 1].travelled - stations[i].travelled) * sp.u_a) / uv_v_scale;
                let v_c = (stations[i].travelled + (stations[i + 1].travelled - stations[i].travelled) * sp.u_center) / uv_v_scale;
                let v_b = (stations[i].travelled + (stations[i + 1].travelled - stations[i].travelled) * sp.u_b) / uv_v_scale;
                let u_bank = if sp.left_side { BANK_U_EXTENT } else { 1.0 - BANK_U_EXTENT };
                let u_skirt = if sp.left_side { 0.0 } else { 1.0 };

                let pa = push_pt(sp.pos_a, sp.y_a, u_bank, v_a, 0.0);
                let pm = push_pt(sp.pos_center, sp.y_center, 0.5, v_c, sp.depth_center);
                let pb = push_pt(sp.pos_b, sp.y_b, u_bank, v_b, 0.0);
                let ska_y = (heights.sample_world(sp.skirt_a) - BANK_TUCK).min(sp.y_a - BANK_TUCK);
                let skb_y = (heights.sample_world(sp.skirt_b) - BANK_TUCK).min(sp.y_b - BANK_TUCK);
                let ska = push_pt(sp.skirt_a, ska_y, u_skirt, v_a, 0.0);
                let skb = push_pt(sp.skirt_b, skb_y, u_skirt, v_b, 0.0);

                if sp.left_side {
                    indices.extend_from_slice(&[prev_base, prev_base + 1, pa, prev_base, pa, ska]);
                    indices.extend_from_slice(&[pb, bridge_base + 1, bridge_base, pb, bridge_base, skb]);
                    indices.extend_from_slice(&[prev_base + 1, prev_base + 2, pa, pa, prev_base + 2, pm]);
                    indices.extend_from_slice(&[pm, prev_base + 2, bridge_base + 2, pm, bridge_base + 2, pb]);
                    indices.extend_from_slice(&[pb, bridge_base + 2, bridge_base + 1]);
                    for k in 2..4u32 {
                        indices.extend_from_slice(&[
                            prev_base + k,
                            prev_base + k + 1,
                            bridge_base + k,
                            bridge_base + k,
                            prev_base + k + 1,
                            bridge_base + k + 1,
                        ]);
                    }
                } else {
                    for k in 0..2u32 {
                        indices.extend_from_slice(&[
                            prev_base + k,
                            prev_base + k + 1,
                            bridge_base + k,
                            bridge_base + k,
                            prev_base + k + 1,
                            bridge_base + k + 1,
                        ]);
                    }
                    indices.extend_from_slice(&[prev_base + 2, prev_base + 3, pa, prev_base + 2, pa, pm]);
                    indices.extend_from_slice(&[prev_base + 2, pm, bridge_base + 2, pm, pb, bridge_base + 2]);
                    indices.extend_from_slice(&[pb, bridge_base + 3, bridge_base + 2]);
                    indices.extend_from_slice(&[prev_base + 3, prev_base + 4, ska, prev_base + 3, ska, pa]);
                    indices.extend_from_slice(&[pb, skb, bridge_base + 4, pb, bridge_base + 4, bridge_base + 3]);
                }
            } else {
                for k in 0..4u32 {
                    indices.extend_from_slice(&[
                        prev_base + k,
                        prev_base + k + 1,
                        bridge_base + k,
                        bridge_base + k,
                        prev_base + k + 1,
                        bridge_base + k + 1,
                    ]);
                }
            }

            if (next_shift - prev_shift).length_squared() > 1e-4 {
                prev_shift = next_shift;
                prev_base = push_station(&stations[i + 1], prev_shift);
            } else {
                prev_base = bridge_base;
            }
        }

        if let Some((_, ref sp)) = trimmed_end[reach_id] {
            let mut push_end = |xz: Vec2, y: f32, u: f32, depth: f32| -> u32 {
                let id = positions.len() as u32;
                let p = xz + prev_shift;
                positions.push([p.x, y, p.y]);
                normals.push([0.0, 1.0, 0.0]);
                uvs.push([u, sp.travelled / uv_v_scale]);
                colors.push([sp.vel.x, sp.vel.y, depth, if sp.frozen { 1.0 } else { 0.0 }]);
                id
            };
            let ska_y = (heights.sample_world(sp.skirt_a) - BANK_TUCK).min(sp.y_a - BANK_TUCK);
            let skb_y = (heights.sample_world(sp.skirt_b) - BANK_TUCK).min(sp.y_b - BANK_TUCK);
            let (l_pos, l_y, r_pos, r_y, ls_pos, ls_y, rs_pos, rs_y) = if sp.left_side {
                (sp.pos_b, sp.y_b, sp.pos_a, sp.y_a, sp.skirt_b, skb_y, sp.skirt_a, ska_y)
            } else {
                (sp.pos_a, sp.y_a, sp.pos_b, sp.y_b, sp.skirt_a, ska_y, sp.skirt_b, skb_y)
            };
            let e0 = push_end(ls_pos, ls_y, 0.0, 0.0);
            let e1 = push_end(l_pos, l_y, BANK_U_EXTENT, 0.0);
            let e2 = push_end(sp.pos_center, sp.y_center, 0.5, sp.depth_center);
            let e3 = push_end(r_pos, r_y, 1.0 - BANK_U_EXTENT, 0.0);
            let e4 = push_end(rs_pos, rs_y, 1.0, 0.0);

            let end_ring = [e0, e1, e2, e3, e4];
            for k in 0..4usize {
                indices.extend_from_slice(&[
                    prev_base + k as u32,
                    prev_base + k as u32 + 1,
                    end_ring[k],
                    end_ring[k],
                    prev_base + k as u32 + 1,
                    end_ring[k + 1],
                ]);
            }
        }
    }

    if indices.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}
```

- [ ] complete the above

---

#### 4. Things to Double-Check When Integrating
1. **Confluence orientation check (`l_pos` vs `r_pos` on `trimmed_end`)**:
   - In `trimmed_end`, if `sp.left_side` is `true`, the tributary approaches from the left bank moving toward the right. Looking downstream along the tributary, `sp.pos_b` (further downstream on the parent) is on the tributary's **left** when joining at acute angles, and `sp.pos_a` (further upstream on the parent) is on the tributary's **right**. Check `(l_pos - sp.pos_center).dot(stations.last().unwrap().side)`: if `> 0.0`, swap `(l_pos, r_pos)` and `(ls_pos, rs_pos)` so the final quad never twists into a bowtie!
2. **Same-segment multiple splits**:
   - Currently `splits_by_reach[reach_id].iter().find(|sp| sp.station_idx == i)` handles one split per parent sub-segment. With `RIVER_SPLINE_SUBDIV = 4`, each grid cell has 4 sub-segments, so two tributaries joining the same grid vertex can target distinct sub-segments (`approx_p_idx ± 1`) or if two hit the same sub-segment `i`, clamp/offset `station_idx` by `±1` if occupied so neither split is skipped.
3. **Syncing `WaterField::build` (`water.rs`)**:
   - Update `WaterField::build` to read `map.lakes()` / `river_network` surface heights if you want `water_surface_at` swimming depth to match the exact `surface_y` solved by `solve_station_volume`, rather than `ground + RIBBON_LIFT`.
4. **Update `water.rs` unit tests & run verification**:
   - Replace the old `river_ribbon_mesh` tests in `src/terrain/water.rs` with tests against `river_network_mesh` covering:
     - Single continuous `Mesh` across a multi-tributary confluence (zero gaps, vertex count divisible/aligned, non-empty indices).
     - Channel-confined narrow canyon vs wide flat valley giving narrower vs wider width for the same `accumulation`.
     - Higher discharge / lower slope producing wider/deeper cross-sections.
     - Center vertex encoding positive depth in `ATTRIBUTE_COLOR[2]` and bank skirts sitting below local bank terrain.
     - `FiniteTorus` seam crossing keeping edge lengths bounded (no map-wide stretched triangles).
   - Run `cargo test` and `cargo check --all-targets` after applying.

---

## Water-to-Terrain Gap & Seam Fixes

- [ ] **Seal lake & sea shoreline perimeters under dry banks (`water.rs`)**
  - When building basin-clipped lake or sea chunk meshes, include every grid quad with **at least 1 submerged corner** (extending the water mesh 1 cell ring under the dry bank at `surface_y`, or clipping boundary quads at the `height == surface_y` contour) so no 1-cell shoreline gap ever appears around shores or islands.
  - Ensure `CHUNK_MESH_STEPS` matches `ScalarField` spacing so terrain triangles and water quads never disagree on shoreline elevation.
- [ ] **Anchor river ribbon edges into terrain banks (`water.rs::river_ribbon_mesh`)**
  - Replace the 2-vertex flat cross-section with a **4-vertex channel cross-section**: flat/crowned water surface across the stream center (`max(ground_at_center, water_level) + RIBBON_LIFT`), plus outer bank-skirt vertices tucked `~0.25m` below `map.height_at(edge.x, edge.z)` so edges never float on sloped hillsides.
  - Subdivide smoothed `river_paths` finely enough (sampling `map.height_at` per step) that river chords never cut underneath convex crests or bridge above concave valley floors.
- [ ] **Bridge river mouths, lakes, waterfalls & seasonal sea levels seamlessly (`hydrology.rs`, `water.rs`, `map.rs`)**
  - Extend `river_paths` 1–2 steps past the shoreline/lake boundary (`height <= sea_levels.min()` for sea mouths, or inside `lake.cells` for lake inlets) so river mouths remain submerged into the ocean even at `Winter` low tide (`-1.6m`).
  - Clamp river station elevation to `max(ground + lift, target_lake_surface, current_sea_level)` near inlets/mouths so rivers meet lakes and oceans flush without diving into the bed or stopping mid-air.
  - Match `cascade_mesh` lip/plunge positions, elevation offsets, and width directly to the upstream/downstream `river_ribbon_mesh` stations so waterfalls connect continuously without lip/plunge gaps.
- [ ] **Recompute/validate `lake.surface` after final heightfield passes (`map.rs`, `hydrology.rs`)**
  - Re-run or update `find_lakes` / `lake.surface` after `carve_plunge_pools` and `coastal_erosion` settle final `heights` (and after `river_paths` split across `FiniteTorus` seams) so lake surfaces never detach from altered rims.

---

## Fix Floating Lake Meshes

- [ ] **Group lakes strictly by equal `filled` water level in `find_lakes` (`hydrology.rs`)**
  - In `find_lakes`'s BFS, only merge `neighbour` into the current lake if `(hydrology.filled.get_at(neighbour) - level).abs() <= DEPRESSION_EPSILON` so upper perched basins never merge with lower valley basins and pull their `surface` up into the sky.
  - Set each lake's `surface` from its own shared `filled` level (verified `<=` all surrounding non-lake rim vertices) rather than `max()` across disparate basins.
- [ ] **Replace square `lake_planes` & fix torus centroid wrapping (`water.rs`)**
  - Replace the bounding-square quad (`water_surface_mesh(body.radius)`) with the basin-clipped grid mesh tucked 1 cell under the shoreline bank (Section 19), so lake geometry never overhangs off hillsides or ridges.
  - Unwrap `lake.cells` coordinates relative to the first cell via `topology.delta_xz` on `FiniteTorus` before computing centroids or building meshes so seam-wrapping lakes don't span the entire map.
- [ ] **Gate lake formation by catchment water budget (`hydrology.rs`, `map.rs`)**
  - Filter out dry/evaporating high-altitude dimples and noise micro-pits (require minimum catchment inflow / `accumulation` relative to lake surface area and aridity, or clamp lake level below the full spill rim in arid climates) instead of filling every geometric depression to the brim.

---

If you'd like to push visuals even further and want to share your Rust material definitions / mesh generation files (e.g., sky.rs, terrain.rs, water.rs, camera setup):

- [ ] Depth Prepass & Screen-Space Water Refraction (DepthPrepass + ViewDepthTexture / ViewTransmissionTexture): Gives true depth-based shoreline feathering without relying on vertex-baked vdata.r, plus underwater refraction distortion.

- [ ] Vertex Displacement for Water Waves: Adding a custom vertex shader (vertex_shader()) to WaterMaterial lets large ocean swells physically bob up and down in 3D instead of normal-only shading.

- [ ] Animated Clouds & Wind-Blown Snow in SkySurface: Passing time: f32 (using existing padding or adding a field) lets the procedural cirrus clouds drift smoothly across the sky.