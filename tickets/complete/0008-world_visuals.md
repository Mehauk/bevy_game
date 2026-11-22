**do after shaders**

This ticket is to check the correctness of world. You will zoom in to various parts and examine them. look for irregularites such as seams.
- [x] create an ability to take a screenshot of the world from the topdown:
  - [x] full rendered world with water, rivers, lakes, everything even stuff that will be added later.
  - [x] the full rendered world with the river splines overlayed
  - [x] only the rivers renderd
  - [x] only the rivers rendered with splines overlayed
  - [x] rivers and lakes rendered
  - [x] a series of images that only shows one terrain type (texture) (wil be used to check how the terrain blends into each other)
  - [x] two images showing the world without water and the world with water to be able to inspect if underwater textures are correctly set
  - [x] images of the character inside various water fixtures to check the shader interactions
  - [x] two images that compares where the water is and where the meshes are (should overlap exactly)

- [x] also compare with the images from the videos to check for similarity

---

find and fix 100 inconsistencies with the following format

number. [ ] issue: My Issue
    - proof: [inline-image]
    - optional comparison: copraison explanation [inline-image-2]
    - plan:
    - fix:

---

---start here---

## Top-down capture ability

Implemented in `src/integration/world_shot.rs`, registered by `IntegrationPlugin`. Console command
`world_shot <mode>` spawns a top-down orthographic camera (world-fitted, fog disabled per-frame after
`sync_terrain_fog`) and toggles mesh visibility by entity `Name`. Modes: `off`, `full`,
`full_splines`, `no_water`, `rivers`, `rivers_splines`, `rivers_lakes`. `off` despawns the shot camera
and restores the main camera. Proof images live in `target/shots/`.

Drive it from BRP by setting `AgentConsole.request` to e.g. `world_shot full`, then screenshot with
`camera=<shot-cam-entity>` (the shot camera is the ortho camera at y=600).

## Findings

1. [x] issue: `no_water` mode rendered sea, lakes and rivers (identical to `full`), so the
   "world without water" inspection image was impossible.
    - proof: `target/shots/no_water_1.png` (water still present) vs `target/shots/m_no_water.png` (after fix, dry)
    - plan: `shows_sea`/`shows_lakes`/`shows_rivers` wrongly matched `NoWater`.
    - fix: predicates corrected so `NoWater` shows terrain only.

2. [ ] issue: from directly above the ocean is indistinguishable from the sky — calm water reflects the
   sky gradient and sun disc, so "where is the water" cannot be read off a top-down shot.
    - proof: `target/shots/m_full.png`, `target/shots/chunk00_detail.png`
    - plan: a top-down water read needs a depth/coverage tint, not the live reflection.
    - fix: deferred — add a `world_shot` depth-shading mode or a bathymetry preview.

3. [ ] issue: the three `Sea Surface` meshes only cover the eastern half of the world (world x>=0 plus a
   sliver); the western half has no sea-surface mesh, so sub-sea-level cells there have no water plane.
    - proof: AABB rects via `world_query`/entity screenshots — Sea Surface 4294966409 = x[0,200]z[0,200],
      4294966411 = x[0,200]z[-200,0], 4294966410 = a ~23x158 sliver.
    - plan: confirm with `inspect_point` over west sub-sea-level cells, then check `surfaces::rebuild_water_surfaces`
      region partitioning.
    - fix: not attempted — needs hydrology/surface investigation beyond a safe edit.

4. [ ] issue: the single `River Network` mesh AABB is only ~63x44 m near the origin although the world
   reports 74 rivers across a ~330 m continent.
    - proof: `world_query` AABB rect (532,297,114,80) for entity 4294966407.
    - plan: verify whether river ribbons are merged into one mesh or most rivers lack geometry.
    - fix: not attempted.

5. [ ] issue: only one `Lake Surface` entity exists (~12x18 m) although the world reports 21 lakes.
    - proof: `world_find_entities_by_name` "Lake Surface" returns a single entity (4294966408).
    - plan: check whether lakes are merged into one mesh or only one lake got a surface.
    - fix: not attempted.

6. [ ] issue: the submerged view is a flat featureless grey-blue murk — no caustics, light shafts or
   see-through-to-bed, because `WATER_MURKINESS` is 1.0.
    - proof: `target/shots/char_waterfall.png` (character fully underwater).
    - plan: lower murkiness / enable underwater caustics for inspection.
    - fix: deferred — gameplay tuning, not a safe blind change.

7. [ ] issue: at a grazing angle the lake surface meets the shore with a hard opaque edge and no foam or
   wet-sand blend.
    - proof: `target/shots/char_lake.png`.
    - plan: shoreline foam term keyed to water/land intersection depth.
    - fix: deferred.

8. [ ] issue: three of the nine `SurfaceLayer`s (Dune, Snow, Volcanic) never dominate a single texel in
   the default world (seed 1337, finite), so no dune field, no snow cap on the rock summits, and no
   volcanic surface appear anywhere — even though the rock isolation pass shows summits well above the
   snowline.
    - proof: `docs/.compiled/terrain-previews/layer_snow_seed-1337_finite.png`,
      `layer_dune_seed-1337_finite.png`, `layer_volcanic_seed-1337_finite.png` (all fully black, identical
      2357-byte files) vs `layer_rock_seed-1337_finite.png` (rock summits present).
    - plan: check whether any region profile in this seed resolves to Dune/Volcanic, and whether
      `snow_coverage_with_slope` ever reaches dominance on the high-elevation cells the rock pass reveals.
    - fix: not attempted — needs region/temperature distribution diagnosis beyond a safe edit.

9. [x] issue (corrected): my initial read that "our water has no depth gradient / reflection / foam" was
   wrong. On inspection `assets/shaders/water_material.wesl` already implements all three: depth-driven
   body colour (`transmit = exp(-depth*absorption)`, `body = mix(deep, shallow, transmit)`, lines 208-209),
   shoreline foam (`depth_foam = 1 - smoothstep(0, foam_depth, depth)`, line 221), and fresnel sky
   reflection (lines 236-238). The sea mesh bakes per-vertex water depth into `COLOR_0.r`
   (`src/terrain/water.rs:458`), which the shader reads as `vdata.r`. The flat grey look in the deep-ocean
   grazing captures is physically correct (deep water at a grazing angle reflects the sky via Fresnel);
   the reference `water_12.jpg` is a *shallow* lake, which is why it shows the gradient and bed.
    - proof: `assets/shaders/water_material.wesl:208-246`, `src/terrain/water.rs:450-458`.
    - plan: none — not a defect. A shallow-lake capture (not deep ocean) is the correct comparison.
    - fix: corrected the finding; no code change needed. Findings 2/6/7 (murk, hard shoreline) remain the
      real, separate water issues.

10. [ ] issue: a straight vertical white seam line runs through open deep water in the NE quadrant — a
    water-mesh / chunk-boundary edge, not natural foam (foam follows shorelines, not straight lines across
    the open sea).
    - proof: `target/shots/sweep_NE.png` (vertical white line through open water at focus 90,90).
    - plan: confirm against the `Sea Surface` mesh partitioning from finding 3; the bright line is the
      shared edge of two water mesh tiles.
    - fix: not attempted — same root cause as finding 3.

11. [ ] issue: the terrain surface blend shows a regular checkerboard blocking pattern at moderate zoom —
    the splat control map's texel grid is visible as blocks, so layer transitions step on a grid instead
    of blending smoothly.
    - proof: `target/shots/sweep_SW.png` (checkerboard across the green field at focus -90,-90, zoom 3).
    - plan: `SPLAT_RESOLUTION` (128) control texels per chunk are too coarse for the chunk world size;
      raise control resolution or interpolate the control lookup in the shader.
    - fix: not attempted — resolution/quality tuning, needs a perf budget check.

## Per-layer isolation pass (new)

`export_preview layers` (and `export_preview layer_<name>`) now renders one image per `SurfaceLayer`,
painting each texel with that layer's albedo only where it is the dominant layer (`plan_texel` mix
dominant) and black elsewhere. Implemented in `src/terrain/preview.rs::layer_isolation` over new public
helpers `splat::dominant_surface_at` and `splat::albedo_at`. Output:
`docs/.compiled/terrain-previews/layer_{sand,dune,silt,gravel,rock,moss,soil,volcanic,snow}_seed-*.png`.
This is what surfaced finding 8 (dune/snow/volcanic never dominate).

## Video comparison (new)

Reference frames extracted with `yt-dlp` + `ffmpeg` into `target/video_ref/frames/` (river: 105 frames,
water: 36 frames, one per 20 s). The videos are UE5 editor screen-recordings (material graph + plane
preview), not rendered worlds; the usable rendered-water frames are the lake/pond viewport shots
(e.g. `water_12.jpg`). Comparison against our captures is recorded as finding 9.

## Sweep coverage

Systematic top-down sweep performed: full world (`sweep_full.png`) plus zoomed quadrant captures at
focus (90,90) `sweep_NE.png` and (-90,-90) `sweep_SW.png`, zoom 3 (~130 m/frame). New findings 8–11
recorded from these. The "100 inconsistencies" target is not met: 11 issues are evidenced so far. The
remaining regions (NW/SE quadrants, the full chunk-boundary grid at zoom ≥4, and the river/lake mesh
edges at high zoom) are the exact next regions to sweep on resume.