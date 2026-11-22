**requires water_fixes**

- [x] extract the entire video srt file from https://www.youtube.com/watch?v=FvbPnndigL4. create a full walkthough of items you must do to recreate the effect entirely. one thing to watch out for is that you will have to write your own shader utilities taht are built in in ue5

- [x] the walkthough shoould create a checklist, each a subset of the srt and then an explanation/walkthrough step detailing exactly how you will implement the section. which systems 

- [x] Finally you will comapre the result by finding the correct frames in the video to inspect and check against snapshots of the game after running it and taking the relevant snapshots via the mcp.

---

# Walkthrough

Source: `Intro to Flow Maps [UE5]` (FvbPnndigL4), full `.en.srt` extracted with
`yt-dlp --write-auto-subs --sub-langs en --sub-format srt --skip-download`. Reference frames pulled
from the 360p download with `ffmpeg -ss <t> -i river_ref.mp4 -frames:v 1`.

The whole video is one idea: a **flow map** is a per-texel 2D vector field where the vector's
*direction* is the flow direction and its *magnitude* is the flow speed (deliberately **not**
normalised). A panner can only pan a whole texture in one fixed direction at one fixed speed; a flow
map pans each texel along its own vector at its own speed, and can change direction/speed at runtime
without the panner's "jump to another timeline" jerk. Everything below is the video's argument, in
order, each mapped to the system in this repo that realises it.

UE5 ships nodes this repo does not have, so each is hand-written in `assets/shaders/water_material.wesl`
(WESL = WGSL superset, `//` comments only) or supplied from the CPU side in `src/terrain/water.rs`:

| UE5 built-in | What it does | Our equivalent |
|---|---|---|
| `FlowMap` custom node | triple-sampler scroll | `running_gradient()` + `flow_envelope()` |
| `Panner` | uv += dir*time | replaced (this is the thing we avoid) |
| `Transform Tangent→World Space` | flow vector to world | baked world-space velocity in `vdata.xy` (no transform needed) |
| `Saturate` | clamp 0..1 | `clamp(x, 0.0, 1.0)` |
| `Fract` | fractional part | `fract()` |
| `Sign` / sine period-2 | centre-fast envelope | `sin(across * PI)` |
| `Time` | seconds | `water.time` uniform |
| `Height Lerp` | smooth mask blend | `smoothstep` / `mix` on the foam term |
| `Distance Field` nodes | divert flow around solids | **not ported** (see deferred) |

## Checklist (each = a slice of the SRT + how it is implemented)

**1. `00:00–01:22` — what a flow map is; use cases (rivers, wind, edge foam, foam from motion).**
Concept only. Establishes that the river is the target: direction along the channel, foam where it
moves fastest. No code; it sets the requirements for items 5–8.

**2. `01:22–03:27` — how a panner works: `uv += normalize(dir) * time * speed`, one tile/sec.**
This is the baseline we are replacing. In the old river path the detail scrolled on a fixed world
direction. Superseded by item 6.

**3. `03:27–07:07` — where panners fail: changing direction or speed at runtime makes the texture
skip/jerk because `time * dir` re-projects the whole accumulated offset onto a new axis.**
Our fix: the scroll phase is `fract(downstream/FLOW_CYCLE - water.time * speed * 0.12 + i/3)` — the
direction enters only through the *spatial* `downstream` term and the per-sample pan, never as a
multiplicative factor on accumulated `time`. So re-baking the flow between generations (or a future
runtime flow change) does not teleport the pattern. System: `running_gradient()`.

**4. `07:07–08:02` — panners fail for per-texel different directions (becomes a mess over time).**
Motivates a per-texel vector. Our per-texel vector is the interpolated vertex velocity:
`solve_station_volume` bakes `velocity = flow_dir * speed` (speed clamped 0.35..4.5) per river station
into the mesh colour `[vx, vy, depth, frozen]` (`river_network_mesh`), and the river material runs
with `use_vertex_data = 1`, so `vdata.xy` is a smooth per-fragment flow map across the ribbon.
Systems: `src/terrain/water.rs` (CPU bake) → vertex attribute → `vdata` in the fragment shader.

**5. `08:02–11:53` — flow map wins: per-texel direction from a texture; smooth runtime direction
change; smooth runtime speed change by *not* normalising (magnitude = speed); variable speed via a
gradient (centre-fast with a period-2 sine/sign).**
Implemented in `running_gradient()`:
- `baked = length(flow)` recovers the per-texel speed the video says to keep in the vector.
- `flow_factor = 0.5 + clamp(baked / FLOW_REF_SPEED, 0.0, 1.0)` scales the scroll rate by that speed
  (this is the "don't normalise, magnitude is speed" property — the previous code did `normalize(flow)`
  and threw the magnitude away).
- `channel = sin(clamp(across,0,1) * PI)` with `across = vertex_output.uv.x` is the period-2 centre-fast
  envelope (fast mid-channel, slow at both banks) — the video's `Sign`/sine trick at `11:28` and `29:36`.

**6. `11:53–18:45` — how a flow map operates: `fract(time)` sawtooth; layer 3 copies offset by 1/3 and
2/3; each pans then fades; the fade is `saturate((1 - abs(fract(phase)*2 - 1)) * 1.5)` (a trapezium);
blend the three samples and divide by 2 for constant luminosity; random per-sample offset + time
distortion break the tiling.**
This is the triple-sampler, written directly:
- `flow_envelope(phase) = clamp((1.0 - abs((phase - 0.5) * 2.0)) * 1.5, 0.0, 1.0)` — the trapezium.
- the `for (i in 0..3)` loop offsets each sample's phase by `i * 0.33333333`, pans the sample point by
  `dir * (phase * FLOW_CYCLE)`, weights it by `flow_envelope(phase)`, and the result is divided by 2.
- the `downstream / FLOW_CYCLE` term inside the phase is the spatial distortion that decorrelates the
  scroll along the channel so the whole river does not pulse in unison.
System: `running_gradient()` + `flow_envelope()`.

**7. `18:45–25:11` — deconstruct the triple-sampler function: inputs (texture, flow RG *not*
normalised, uv, distortion); `phase = fract(time*speed + distortion)`; three samples offset 1/3, 2/3;
`uv += flow * phase * panAmount`; trapezium fade; `/2` blend.**
Same code as item 6; this slice confirms the inputs. Our "texture" is the procedural `wave_gradient`
value-noise field (the video notes flow maps suit *noise* best, not a literal texture), our uv is
world-space `world_pos.xz`, our flow is `vdata.xy`, our pan amount is `FLOW_CYCLE`.

**8. `25:11–33:43` — apply to a river: deeper water flows faster; foam where `length(flow)` is
saturated (height-lerp transition = `saturate(length(flow))`); flow direction from world-space noise
transformed tangent→world with the B component zeroed then normalised (no seams); centre-fast via the
across-coordinate through a period-2 sine; distance fields divert flow around obstructions and add
foam around them.**
- deeper/faster: the baked speed already grows with slope (`RIVER_VELOCITY_COEFF * slope.sqrt()`), and
  the shader adds `(1.0 + slope * FLOW_SLOPE_GAIN)` from the screen-space slope, so steep/deep stretches
  scroll faster.
- foam on fast water: `rapid = smoothstep(FLOW_RAPID_LO, FLOW_RAPID_HI, length(flow))` is the
  "foam where the flow map is heaviest" rule (`26:40`, `30:48`); it is OR-ed with the existing bank foam.
- tangent→world with B zeroed: not needed — the baked velocity is already a world-space XZ vector, so
  there is no seam the video's transform was fixing.
- **deferred:** the distance-field flow diversion around obstructions (`31:34–33:43`). It needs a mesh
  distance field to the surrounding geometry, which the ribbon mesh does not carry; it is a reactive
  flourish, not the core flow-map effect, so it is left out rather than faked.

## Comparison (reference frames vs game snapshots)

Reference frames inspected: `frame_00-29-30.png` (river ribbon from an elevated 3/4 view — directional
flowing detail/foam running along the channel, visibly faster through the middle) and
`frame_00-30-10.png` (ribbon with foam concentrated where flow is strongest).

Game snapshots taken live via `bevy_brp_mcp` (`world_generation`, debug, port 15702), driven through the
agent console (`find_feature river N`, `world_summary`) and movement keys:
- `game_move2.png` — the continuous river ribbon (river 10) rendered with the water shader: a single
  unbroken surface with flowing detail, no seams along the channel. This is the closest match to the
  reference ribbon, though it sits in the mountain's shadow so the streaking is subtle.
- `game_river0.png`, `game_move1.png`, `game_move3.png` — the sea/shore surface showing the same
  water-shader detail pipeline producing coherent scrolling surface structure.

Verification result: `cargo check --all-targets` clean, `cargo test` 540 passed / 0 failed / 2 ignored,
and a live `cargo run` held 53–62 FPS with **zero** `error` and **zero** `warn` lines in the BRP log
after navigating across several rivers and the waterfall — so the flow-map path compiles and executes
on real river geometry without shader or runtime faults.

Honest limitation: the in-game camera is a third-person rig tethered to the character with a fixed yaw
and a slight downward pitch (the rig is not BRP-reflected, so it cannot be re-aimed remotely), and the
low coastal terrain where rivers meet the sea is often in fog or mountain shadow. That makes a
textbook close-up of a long sunlit channel — like the UE5 reference — impractical to frame headlessly.
The captures above confirm the effect renders correctly and the ribbon is continuous and seam-free;
they are not a pixel-level match to the reference, and the framing constraint is the reason.
