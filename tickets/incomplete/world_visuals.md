**do after shaders**

This ticket is to check the correctness of world. You will zoom in to various parts and examine them. look for irregularites such as seams.
- [x] create an ability to take a screenshot of the world from the topdown:
  - [x] full rendered world with water, rivers, lakes, everything even stuff that will be added later.
  - [x] the full rendered world with the river splines overlayed
  - [x] only the rivers renderd
  - [x] only the rivers rendered with splines overlayed
  - [x] rivers and lakes rendered
  - [ ] a series of images that only shows one terrain type (texture) (wil be used to check how the terrain blends into each other)
  - [x] two images showing the world without water and the world with water to be able to inspect if underwater textures are correctly set
  - [x] images of the character inside various water fixtures to check the shader interactions
  - [x] two images that compares where the water is and where the meshes are (should overlap exactly)

- [ ] also compare with the images from the videos to check for similarity

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

## Not completed in this environment

- "a series of images that only shows one terrain type": needs a per-`SurfaceLayer` isolation pass. The
  terrain shader blends 4 layers per texel and `export_preview` has no single-layer mode, so this needs
  new CPU preview code (paint texels where a given `SurfaceLayer` dominates) or a shader isolate uniform.
- "compare with the images from the videos": the referenced videos are the UE5 river/water tutorials
  (tickets 0005/0006). `yt-dlp` is not installed and there is no network fetch path here, so reference
  frames cannot be extracted; the prior tickets' frames were in temp dirs that are gone.

The ticket's "100 inconsistencies" target is aspirational: a genuine 100 requires the per-texture
isolation pass and a zoomed seam sweep across every chunk, both gated on the items above. The findings
above are the ones actually observed and evidenced from the top-down captures.