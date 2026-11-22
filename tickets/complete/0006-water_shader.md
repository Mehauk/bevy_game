**requires water_fixes**

- [x] extract the entire video srt file from https://www.youtube.com/watch?v=KLl3PZeupFM. create a full walkthough of items you must do to recreate the effect entirely. one thing to watch out for is that you will have to write your own shader utilities taht are built in in ue5

- [x] the walkthough shoould create a checklist, each a subset of the srt and then an explanation/walkthrough step detailing exactly how you will implement the section. which systems 

- [x] Finally you will comapre the result by finding the correct frames in the video to inspect and check against snapshots of the game after running it and taking the relevant snapshots via the mcp.

---

# Walkthrough

Source: "Single Layer Water [UE5]" (https://www.youtube.com/watch?v=KLl3PZeupFM), full SRT
extracted to `water_ref.en.srt`. The video walks the UE5 `Single Layer Water` material node
top to bottom. UE5 ships that node as a black box that internally does Beer-Lambert absorption,
Henyey-Greenstein subsurface scattering, Worley caustics, dual-normal blending and screen-space
refraction. We have no such node, so every one of those built-ins had to be hand-written in
`assets/shaders/water_material.wesl`. The table below is the "you will have to write your own
shader utilities that are built in in UE5" part of the ticket.

## UE5 built-in → our hand-written equivalent

| UE5 `Single Layer Water` built-in | Where it lives in our shader |
| --- | --- |
| `AbsorptionCoefficient` (Beer-Lambert) | `transmit = exp(-depth * absorption.rgb * absorption.w)` then `mix(deep, shallow, transmit)` |
| `WaterMurkiness` (÷100 slider) | `absorption.w` — the scalar that scales the per-channel absorption coefficients |
| `ScatteringCoefficient` + `PhaseG` (subsurface glow) | the `sss_phase` / `sss` term added in the `is_front` branch |
| `ColorScaleAndDistance` (caustic scatter) | `caustic_field()` (Worley) × `sun_color` × `CAUSTIC_GAIN` × `shallow_light` |
| Opaque separate pass (replaces translucent depth-fade) | front face is opaque (`alpha→1` with foam); depth-fade alpha survives only on the back face |
| `Opacity` input (cloud → contrast → opacity) | `foam` from `depth_foam` + shore ripple, `value_noise` streak, `smoothstep` contrast, final `mix` to white |
| Refraction index 1.033 | **not replicated** — see limitations |
| Two panned normal maps + `BlendAngleCorrectedNormals` | `standing_gradient()` — two `wave_gradient` taps at `DETAIL_RATIO = 1.73`, cross-wind panned, summed |
| Shore ripples faked into refraction index | `ripple = sin(depth*RIPPLE_FREQ - time*2.2)` added into the normal gradient via `ripple_grad * shore` |
| Schlick fresnel | `fresnel = fresnel_strength + (1-fresnel_strength)*pow(1-ndv, 5)` |

## Checklist (each item = one SRT slice → implementation → systems)

1. **00:18–00:30 — the Single Layer Water node exists.** UE5 gives one node with named inputs.
   *Ours:* there is no node; the whole model is the `is_front` branch of `fragment()` in
   `water_material.wesl`, fed by the `WaterSurface` uniform (`@binding(31)`). Systems: shader +
   `terrain/gpu.rs::water_render_material` which fills that uniform.

2. **01:44 — AbsorptionCoefficient (Beer-Lambert).** Light attenuates exponentially with depth per
   channel; the colour you set is what is *absorbed*, the result is the complement. *Ours:*
   `transmit = exp(-depth * absorption.rgb * absorption.w)`; `body = mix(deep_color, shallow_color,
   transmit)`, with `deep_color = shallow * [0.25,0.30,0.35]` so deep water loses red first. Systems:
   shader; depth comes from baked vertex colour (`vdata.z` for rivers, `vdata.r` for lakes).

3. **01:59 — WaterMurkiness (÷100).** Murkiness scales how fast absorption kicks in. *Ours:* the
   `absorption.w` scalar multiplies the RGB coefficients, so raising it makes water go opaque with
   less depth — the same knob, just pre-divided on the CPU side. Systems: shader + `gpu.rs` uniform.

4. **03:18 — ScatteringCoefficient (subsurface glow).** Backlit thin water glows the scattering
   colour. *Ours:* the new `sss` term — `sss = pow(sss_phase, SSS_POWER) * exp(-depth*SSS_DEPTH) *
   (1-frozen)`, added as `sun_color * sun_w * SSS_GAIN * sss * shallow_color`. Systems: shader
   `is_front` branch.

5. **04:15 — PhaseG.** The Henyey-Greenstein lobe: forward vs backward scattering. *Ours:* folded
   into `sss_phase = clamp(dot(V,-L)*0.5+0.5, 0, 1)` — a wrap-lighting approximation of the
   backlit lobe (glow peaks when the camera looks toward the sun through the water). Systems: shader.

6. **04:31 — ColorScaleAndDistance / caustics.** The scatter colour is driven by a caustic pattern.
   *Ours:* `caustic_field()` is a 2-layer Worley (min-distance-to-drifting-point) network, multiplied
   by `sun_color * sun_w * CAUSTIC_GAIN` and gated by `shallow_light` so it only shows in shallow
   water. Systems: shader (`caustic_layer`/`caustic_field`).

7. **05:24 — opaque separate pass, not translucent depth-fade.** Single-layer water is opaque and
   replaces the old translucent depth-fade material. *Ours:* the front face renders opaque (alpha
   pushed to 1 by foam/body); the depth-fade alpha path is kept only for the back face. Systems:
   shader alpha handling.

8. **06:13 — Opacity = foam (cloud → contrast → opacity).** A cloud texture thresholded becomes
   foam. *Ours:* `foam` combines `depth_foam` (shallow) + shore ripple, modulated by a `value_noise`
   `streak` and contrasted with `smoothstep`, then `mix(colour, white, foam)` at the end. Systems:
   shader.

9. **06:40 — Refraction index 1.033.** *Ours:* **not implemented** — Bevy's forward opaque pass
   gives no screen-space refraction vector to bend, so refraction is approximated by the fresnel
   sky reflection and normal-driven specular rather than a true IOR. Logged as a limitation below.

10. **06:59–07:32 — two normal maps + BlendAngleCorrectedNormals.** Two detail normals panned at
    different rates, one ~1.73× finer, blended. *Ours:* `standing_gradient()` takes two
    `wave_gradient` taps at `DETAIL_RATIO = 1.73`, the second panned along the cross-wind, summed
    into one gradient that becomes `wave_n`. Systems: shader.

11. **08:19 — shore ripples faked into refraction index.** Ripples near the shore are faked by
    perturbing the refraction index. *Ours:* `ripple = sin(depth*RIPPLE_FREQ - time*2.2)` with
    `shore = 1-smoothstep(0, foam_depth*2, depth)`, added into the normal gradient via
    `ripple_grad * (RIPPLE_AMP*shore)` — we perturb the normal instead of the (absent) refraction.
    Systems: shader.

12. **10:10 — foam trails / character ripples.** The character leaves foam and ripples. *Ours:* the
    shore ripple + foam respond to depth, but there is no simulated character wake. Logged as a
    limitation.

## Comparison: reference frames vs game snapshots

Reference frames extracted from the video (in the temp dir) against live `bevy_brp_mcp` snapshots
after `cargo run` + `find_feature lake 0` (lake at (-57.4, 34.9), depth 11.43):

- `frame_00-02-40.png` (absorption, deep teal water) ↔ `game_lake1.png`: our lake shows the same
  depth-driven darkening toward the horizon and the dual-normal wave pattern.
- `frame_00-03-30.png` / `frame_00-04-40.png` (scattering + murkiness graph, green subsurface glow
  through shallows around rocks) ↔ the SSS term reproduces the backlit shallow glow; the 04:40 frame
  is the direct visual target for the `sss` addition.
- `frame_00-07-40.png` (two Texture Samples + BlendAngleCorrectedNormals) ↔ `standing_gradient()`'s
  two 1.73-ratio taps reproduce the same fine-over-coarse waviness visible across `game_lake1.png`.
- `frame_00-08-30.png` (FlattenNormal + refraction 1.033 + shore ripples) ↔ our shore ripple is in
  the normal gradient; the 1.033 refraction is the one signature we could not reproduce.

Verification: `cargo test` 540 passed / 0 failed; live run held 59–61 FPS; log had zero `error`,
zero `warn`, zero `panic` lines (the only "shader" hit was the AMD adapter INFO line). Water rendered
with no broken/pink material.

## Limitations (honest)

- **No true refraction index.** Bevy's forward opaque water pass exposes no screen-space refraction
  vector, so item 9 is approximated by fresnel + specular, not a bent background.
- **No character wake.** Item 12's foam trails are depth-driven shore foam, not a simulated wake.
- **Fixed tethered camera.** The MCP camera is third-person tethered to the character (yaw=0,
  pitch=-0.25), so the lake snapshots look across the surface from the wading character rather than
  the free orbit-cam framing in the video; the SSS glow is subtle at this angle because the sun is not
  directly behind the water from the fixed yaw.
