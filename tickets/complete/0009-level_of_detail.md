Right now, `src/terrain/chunks.rs` spawns all `25` chunks (`5x5`, `60m` each, `mesh_subdivisions = 60` -> `180,000` triangles total) eagerly at `Startup` with both a `Mesh3d` and an Avian `Collider::heightfield`. That works well at `300m x 300m`, but if you scale `TerrainConfig.world` to multi-kilometer sizes, fixed-grid chunking blows up in **both triangle count and draw-call / entity count**.

Here is how to handle LOD cleanly without seams in this architecture, plus the three hidden bottlenecks in your codebase that will break right after terrain triangles if left untouched.

---

### 1. Choose the Right LOD Topology

#### A. Fixed Chunks with Distance LOD (Easiest step from current code)
Keep `TerrainChunk`, change `mesh_subdivisions` from `60` to a **power of two** (e.g. `64`, so `lines = 65 = 2^6 + 1`), and pass `lod: u8` into `build_chunk_mesh(&sampler, coord, lod)`:
- `lod = 0` -> `64` subdivisions (`1` sample/m)
- `lod = 1` -> `32` subdivisions (`2`m spacing)
- `lod = 2` -> `16` subdivisions (`4`m spacing)
- `lod = 3` -> `8` subdivisions (`8`m spacing)

Because `64` divides cleanly by powers of 2, coarse vertices land on **exact subsets** of fine vertices.

#### B. Quadtree / CDLOD (Required once `world` exceeds ~1–2 km)
Even with `lod = 3`, a `6 km x 6 km` world of `60m` chunks still has **10,000 entities and 10,000 draw calls**.
With a **Quadtree**:
- Every leaf node builds the **same mesh grid size** (e.g. `32x32` or `64x64` quads), regardless of physical world size.
- Nodes near the viewer split into 4 children (halving world size, doubling vertex density); distant nodes stay huge (`480m`, `960m`, `1920m`).
- Both **total triangles** and **draw calls** stay roughly constant (~100–250 chunks) whether the world is `300m` or `50km` across.
- Combine this with an active radius / LRU cache so chunks beyond render distance are despawned instead of all being spawned at `Startup`.

*(Alternative: a GPU **Geometry Clipmap** reading `TerrainHeightMap` in a custom vertex shader on `WetSandExt`—zero seams and minimal draw calls, though harder to integrate with per-chunk PBR materials, CPU biomes/erosion, and shadow culling than a Quadtree.)*

---

### 2. Eliminating Seams Between LOD Levels

Seams happen because when a fine chunk (`lod = 0`) meets a coarse chunk (`lod = 1`), every odd vertex on the fine edge is evaluated at `sample_ground_height`, while the coarse chunk draws a straight chord between even vertices—opening a vertical T-junction crack (especially on slopes like `CRATER_RIM`).

There are three standard fixes; **Edge Stitching** or **Edge Stitching + Vertex Morphing** are the cleanest for this codebase:

#### Option 1: Edge Stitching with 16 Cached Index Buffers (Recommended)
1. **Enforce a max LOD delta of 1** between neighbors (every restricted quadtree or distance-ring selector does this automatically).
2. Notice that the **vertex positions** of a chunk at `lod` never depend on its neighbors—only **how the outermost ring of triangles connects** does!
3. A chunk has 4 borders (`N, S, E, W`), each either `same/finer` (`0`) or `coarser` (`1`). That is only **`2^4 = 16` possible index buffers**, which you can precompute once at `Startup` for a given `subdivisions` and share across every chunk:
   - Along any edge marked `coarser`, skip the odd boundary vertex in the boundary triangle fan (merging every pair of boundary quads `(ix, ix+1, ix+2)` into 3 triangles that terminate cleanly on the neighbor's even vertices `(ix, ix+2)`).
   - **Important:** Do not leave the skipped odd vertex dangling or draw two triangles to its displaced height—either omit it from the edge triangles (so the edge forms a single straight segment from `ix` to `ix+2` matching the coarser neighbor's chord 1:1) **and** set its normal if referenced, or better yet, keep the 16 index templates purely connecting `(0, 2, 4, ...)` along any `coarser` border.
4. Result: **Zero T-junctions, zero extra overdraw, watertight shadow casting**, and when a neighbor's LOD changes you only swap the chunk's `Indices` / `Mesh` handle without re-sampling heights on the task pool.

#### Option 2: Skirts (Simplest, works with any LOD jump)
Add a vertical downward flange around the 4 perimeter edges of `build_chunk_mesh`:
- Duplicate the edge vertices, drop their `y` by `skirt_depth` (e.g., `cell_size * 0.5`), and **keep the original `terrain.ground_normal(world.x, world.y)`** on the skirt vertices so lighting doesn't crease.
- **Pros:** Chunks need zero knowledge of neighbor LODs, so async `ChunkMeshTask`s stay 100% decoupled and any LOD jump (`lod 0` next to `lod 3`) is hidden.
- **Cons:** Adds `4 * lines * 2` edge triangles and slight shadow-acne/overdraw along seams; less crisp than stitching if viewed from straight above a cliff edge.

#### Option 3: Geomorphing (Stops LOD "popping" as well as seams)
In a custom vertex shader on `WetSandExt` (which currently only overrides `fragment_shader()` in `crates/bevy_shader_kit/src/wet.rs`), store each vertex's parent-LOD height delta `(y_coarse - y_fine)` in a vertex attribute and blend `mix(y_fine, y_coarse, morph)` based on `distance(camera_xz, world_xz)`:
- Because two adjacent chunks evaluate the **exact same world-space camera distance** at shared border vertices, fine vertices smoothly flatten onto the coarser neighbor's edge *before* the LOD switch occurs—fixing both **cracks** and **LOD popping**.

*(Note: Because `terrain.rs` already computes `ground_normal` analytically from `sample_ground_height` rather than from triangle face normals, shading across LOD seams will already match seamlessly.)*

---

### 3. Three Repo-Specific Bottlenecks to Address Alongside Terrain LOD

1. **Module Isolation (`tests/module_isolation.rs`):**
   `src/terrain/` cannot import `crate::camera` or `crate::character`. Add a `TerrainViewer { pub position: Vec3 }` resource in `src/terrain/` and update it from `MainCamera` inside `src/integration/` (matching `post_process.rs` and `submersion.rs`).
2. **Physics Colliders (`src/terrain/chunks.rs`):**
   Right now every chunk gets `Collider::heightfield` at `Startup`. On a huge terrain, decouple **render chunks** from **physics chunks**: only attach `Collider::heightfield` to chunks within a small radius around the character (or dynamic `RigidBody` entities), despawning far colliders.
3. **Water Mesh & Coastal Heightmap (`src/terrain/water.rs`, `src/terrain/heightmap.rs`):**
   - `setup_water_surface` spawns a single `Plane3d` spanning `world.x x world.y` with `SEA_SUBDIVISIONS = 256`. On a `4km` map, quads become `~15.6m` wide—larger than your `SWELLS` wavelengths (`6.5m`, `11m`, `17m`), so waves will alias badly. The water mesh will also need camera-centered clipmaps or chunked LOD.
   - `HEIGHT_MAP_RESOLUTION = 512` covers the entire `world` (`0.58m/texel` at `300m`). Scaling `world` to `4km` stretches each texel to `~7.8m`, breaking `coast.wesl` (`terrain_height_at` / `spill_height_at`), so that map will need to move with the viewer (cascaded/clipmap height textures) or be tiled per region.

---

#### Camera-Centered Water Clipmap (Best triangle density near the player)
If you want crisp `~0.25m` shoreline waves near the player without huge vertex counts:
- Build **one static mesh** made of nested concentric square rings (high density near the center, doubling grid spacing each ring outward), and snap its `Transform.translation.xz` to the coarsest ring's grid step in `Update` as the camera moves.
- **Zero dynamic LOD switching or stitching logic at runtime:** The seams between rings are stitched *once* when the static clipmap mesh is constructed.
- You just move the entity in `Update`, and `water_vertex.wesl` already samples `coast_map` and `swells` by world `xz`.

---

### Two Water-Specific LOD Details to Watch Out For

1. **Wave aliasing at coarse LODs:**
   Your third swell in `src/terrain/water.rs` has `wavelength: 6.5`. On a distant LOD tile where vertices are `8m` or `16m` apart, sampling a `6.5m` sine wave causes vertex shimmer ("swimming") as the camera moves. In `water_vertex.wesl` (or `swell_types.wesl`), attenuate swells by distance (or per-wave when `wavelength < 3.0 * cell_size`) so distant water smoothly settles to flat `water.water_height` while keeping fragment normals/foam.
2. **Vertex texture lookups (`coast.wesl`):**
   Right now `coastal_normal` calls `coastal_surface_y` 3 times (`terrain_height_at` + `terrain_gradient_at`), doing ~45 `textureLoad`s per vertex. Keeping total water vertex count bounded via LOD (e.g. ~20k–40k vertices instead of scaling with `world` area) keeps that vertex pass fast even on multi-kilometer worlds.

**Bottom line:** If you implement **Quadtree + 16 Stitched Index Buffers** for terrain, plugging `Sea` into the exact same tile/stitching setup takes very little extra code and gives both terrain and water LOD with zero seams. If you want ultra-dense near-shore waves (`<0.5m` quads) with ~1 draw call, pair quadtree terrain with a **static concentric water clipmap**.