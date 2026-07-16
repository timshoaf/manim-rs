# 12 — Depth-Tested 3D Mesh Pipeline

Status: **accepted** · Linear project: *3D Mesh Render Pipeline* (FE-123…FE-129)
· Implements [manim-rs#1](https://github.com/timshoaf/manim-rs/issues/1).

## 1. Problem

The renderer is a 2D vector engine: mobjects → `DisplayList` → CPU
tessellation (lyon) → wgpu, composited back-to-front by `z_index`
(painter's algorithm; every pass today has `depth_stencil: None`). The
existing `threed` module *projects* 3D bezier paths to 2D and depth-**sorts**
whole items per frame — a correct-enough approximation for simple orbit shots,
but it cannot occlude interpenetrating geometry, cannot shade per pixel, and
re-tessellates on the CPU every frame that geometry changes.

Target domains that hit this wall immediately: molecular dynamics /
proteomics (10k+ instanced atoms/bonds), differential geometry (evolving
parametric surfaces, homeomorphisms/isotopies), and 3D scalar fields.

## 2. Shape of the solution

A **second, parallel render path for triangle meshes** — depth-tested,
per-pixel shaded, GPU-instanced — that *layers under* the existing 2D vector
path. Nothing above the renderer changes semantics: the scene graph, snapshot
timeline, animation system, and every existing mobject keep working, and all
existing goldens must not change.

```
DisplayList
├── meshes: Vec<MeshItem>      ── NEW: mesh pass (depth write+test)
│                                  ├── opaque queue    (front-to-back ok)
│                                  └── translucent queue (sorted, depth read-only)
└── items:  Vec<DrawItem>      ── existing: vector pass (painter's, no depth)
                                   └── fixed_in_frame HUD pass (ortho)
```

Frame composition order: **clear → mesh opaque → mesh translucent → 2D
vector world → HUD**. 2D vector content (labels, LaTeX, annotations) draws
*over* 3D by design — CE's `add_fixed_in_frame_mobjects` semantics, and the
right default for teaching material. A `DrawItem` that must sit *inside* the
3D scene stays on the existing project-and-sort path; that path is kept, not
deprecated.

## 3. Core: `TriMesh`, `Mesh`, `Surface3D` (`manim-core`, wasm-clean)

```rust
/// Indexed triangle mesh. Positions/normals in mobject-local space.
pub struct TriMesh {
    pub positions: Vec<Vec3>,
    pub normals:   Vec<Vec3>,
    pub colors:    Option<Vec<Color>>,   // per-vertex tint; None = material color
    pub uvs:       Option<Vec<Vec2>>,
    pub indices:   Vec<u32>,
}
```

Builders: `TriMesh::grid(nu, nv)`, `::uv_sphere(rings, segs)`,
`::cylinder(segs)`, `::from_parametric(f, u_range, v_range, (nu, nv))` with
analytic-difference normals. Winding is CCW-front; normals unit-length —
both unit-tested.

Mobjects:

- **`Mesh`** — a `TriMesh` + `MeshMaterial { base_color, opacity, ambient,
  diffuse, specular, shininess, shading: Flat|Smooth }` + a model transform.
  Arena-typed like everything else (`MobjectId<Mesh>`, family ops, updaters).
- **`Surface3D`** — parametric wrapper that owns `f: Arc<dyn Fn(f64, f64) -> Vec3>`
  plus ranges/resolution and regenerates its `TriMesh` on parameter change.
  Checkerboard two-tone fill (CE `Surface` parity) via per-vertex colors.
- **`InstancedMesh`** (FE-126) — one base `TriMesh` + `Vec<Instance { transform:
  Mat4, color: Color }>`. Helpers: `::spheres(centers, radius)`,
  `::cylinders(endpoint_pairs, radius)`.
- **`HeightField`** (FE-128) — `nu × nv` grid + height data (closure or raw
  grid); rendered by vertex-shader displacement (§7).

`TriMesh` payloads live behind `Arc` in the mobject: timeline snapshots clone
the `Arc` (cheap); mutation goes through copy-on-write and bumps the global
generation counter — the same `(source, generation)` contract the
tessellation cache uses, reused as the GPU buffer cache key.

### DisplayList contract change

`DisplayList` gains a `meshes: Vec<MeshItem>` channel (struct stays
`pub`-field, additive change):

```rust
pub struct MeshItem {
    pub mesh:      Arc<TriMesh>,
    pub transform: Mat4,                 // local → world
    pub material:  MeshMaterial,
    pub instances: Option<Arc<[Instance]>>, // FE-126
    pub height:    Option<HeightPayload>,   // FE-128 (grid dims + height data)
    pub source:    AnyId,
    pub generation: u64,
}
```

`DrawItem` is untouched. A scene with no meshes produces byte-identical
frames to today (guarded by existing goldens).

### Interpolation & animation (FE-128)

- Same-topology vertex lerp: `TriMesh::lerp(a, b, t)` (positions + normals
  lerp-then-normalize, colors lerp). `Transform`/`TransformInto` between
  `Mesh`es of equal index buffers use it directly.
- `Surface3D` tweens in *parameter space*: interpolate the two parametric
  functions' outputs on the shared grid — smooth homeomorphism/isotopy
  animation without correspondence problems.
- Updaters mutate mesh data per frame exactly like path mobjects (generation
  bump → renderer re-uploads that mesh's buffers only).

## 4. Render: depth attachment & mesh pipeline (`manim-render`, FE-125)

- **Depth texture**: `Depth32Float`, same extent as the color target, owned
  by `OffscreenRenderer` / `CanvasSurface`; recreated on resize. The 2D
  passes keep `depth_stencil: None` where possible — the mesh pipeline gets
  its own pass, so existing pipelines don't even need a depth-stencil state.
- **Vertex layout** (interleaved, 48 B): `position: vec3, normal: vec3,
  color: vec4 (premultiplied linear), uv: vec2`.
- **Uniforms** (one bind group): `view_proj: mat4` — via
  `Camera2D::mesh_view_proj()`. Under a 3D camera this is identical to
  `view_proj()` (`perspective · look_at` from `ThreeDParams { phi, theta,
  gamma, focal_distance }`). Under a **2D** camera the plain orthographic
  matrix passes world `z` through untouched — fine for the depth-less vector
  pass, but with a depth attachment anything off `z = 0` falls outside
  `[0, 1]` NDC and clips away entirely; `mesh_view_proj` therefore maps
  `z ∈ ±16` → depth `[1, 0]` while leaving x/y bit-identical to the vector
  pass, so meshes render (and align) under 2D cameras too — plus
  `camera_pos: vec3`, `light_dir: vec3`, `ambient: f32`. The scene light is a
  single directional light defaulting to CE's over-the-shoulder key light;
  configurable on `Config`/scene later without shader changes.
- **Shading**: Blinn-Phong in the fragment shader, computed in **linear**
  space and premultiplied — consistent with the 2D pipeline's blending
  decision (docs 04): `color = (ambient + diffuse·N·L) · albedo +
  specular·(N·H)^shininess`, `albedo = vertex_color × material.base_color`.
- **Buffer cache**: `HashMap<(AnyId, u64), GpuMesh { vbuf, ibuf, nindices }>`
  mirroring `TessellationCache`, with the same eviction policy. Static
  meshes upload once; per-frame CPU cost for static geometry is zero.
- **Pass order** (one encoder): mesh pass (clear color+depth, depth
  write+test `LessEqual`) → vector pass (`load`, no depth) → HUD pass. The
  zoom-window inset re-runs the same sequence scissored.

### Depth ↔ painter's coexistence

The vector pass draws unconditionally over the mesh pass (no depth test).
This is deliberate: 2D content is annotation. Mixed scenes that need vector
strokes *occluded by* meshes (e.g. wireframe parameter curves on a surface)
can use the existing project-and-sort path, or a later `z_test: bool` opt-in
on `DrawItem` — recorded as future work, not in scope.

## 5. Transparency (FE-127)

Two queues split on `material.opacity < 1.0` (or any per-vertex alpha < 1):

- **Opaque**: depth write + test. Exact occlusion for free.
- **Translucent**: drawn after opaque, depth **test read-only**, back-to-front
  by camera-space centroid depth per *item* (instanced items sort per
  instance), premultiplied `SrcAlpha=One, Dst=OneMinusSrcAlpha` blending —
  identical blend state to the 2D pipeline.

Per-item sorting cannot fix self-intersecting translucent geometry; that
limitation is documented, and **weighted-blended OIT** (two extra render
targets, WebGL2-compatible via MRT) is the recorded upgrade path if teaching
material hits it.

## 6. Instancing (FE-126)

Per-instance data rides a second vertex buffer with
`step_mode: Instance`: `mat4` as four `vec4` attributes + `color: vec4`
(80 B/instance; 10k instances = 800 KB, uploaded only on generation bump).
One `draw_indexed(.., 0..n_instances)` per `InstancedMesh` — a 10k-atom
molecule is 2 draw calls (spheres + bonds). No storage buffers, so the path
is WebGL2-clean.

Animation: whole-cloud transforms tween via the mobject transform;
per-instance positions/colors mutate through updaters (generation bump →
instance-buffer re-upload, base mesh untouched).

## 7. Heightmap displacement (FE-128, `HeightField`)

For surfaces that evolve every frame, skip CPU re-meshing entirely: a static
`nu × nv` grid mesh + an `R32Float` height texture sampled in the **vertex
shader** (`textureLoad` — vertex texture fetch is core WebGL2). Normals from
finite differences of neighboring texels, also in-shader. A live wave
equation / ultrasound field costs one `nu × nv × 4 B` texture upload per
frame. Neither ManimGL nor CE has an equivalent.

Two implementation notes (as built): the displacement branch is a uniform
flag on the one mesh pipeline (a dummy 1×1 height texture satisfies the
layout for undisplaced items), and the GPU cache diffs **per resource** by
`Arc` identity — the cache holds `Arc` clones so copy-on-write must
relocate, making pointer identity sound; a heights-only change re-writes
just the `R32Float` texture (same allocation when dims are unchanged).
Caveat: a *translucent* height field sorts by its flat grid's centroid —
the displacement exists only on the GPU. Fine for ground-plane fields;
revisit if wildly-displaced translucent fields ever matter.

## 8. Portability

Everything above runs on wgpu's WebGL2 backend as well as WebGPU: depth
buffer, instancing (ANGLE_instanced_arrays is core WebGL2), vertex texture
fetch, MRT (for future WBOIT). **No compute shaders, no storage buffers**
anywhere in the mesh path. wasm CI check extends to the mesh module.

## 9. Testing

- Unit (headless, core): builder normals/winding, lerp endpoints, COW +
  generation semantics, parametric regeneration.
- Golden (lavapipe, `REQUIRE_GPU=1`): saddle self-occlusion (M1), deterministic
  instanced scene (M2), translucent-over-opaque (M3), mid-morph frame (M4),
  heightfield wave frame.
- Regression: every existing golden byte-identical (no-mesh scenes never
  touch the new pass).
- Bench: 10k instanced spheres in native preview, target 60 fps (M2
  acceptance). Measured: **0.8 ms/frame** for 10k spheres (~5.76M tris) at
  427×240 offscreen incl. readback, release, RTX 4090/Vulkan — unchanged
  after the heightfield work.

Cache identity: renderer caches (tessellation, mesh buffers, image
textures) key on `(arena, source, generation)`. A `DisplayList` carries its
`SceneState`'s process-unique arena stamp because `source` is a per-arena
slot-map key and a fresh mobject's generation is 0 — two independently
built scenes would otherwise give their first mobject identical identity
and silently share cache entries through a shared renderer. The stamp is
assigned per `SceneState::new` and **preserved by `Clone`**, so timeline
snapshots (which are clones) keep hitting the cache while independent
scenes stay separate; diverging clones are still disambiguated by the
process-global generation counter. Hand-built lists use the reserved
anonymous arena `0`.

## 10. Non-goals

- Replacing the 2D vector pipeline, LaTeX path, or project-and-sort `threed`
  module — all kept.
- lyon in the 3D path (direct WGSL + wgpu only).
- GPU simulation/compute — orthogonal.
- Shadow maps, PBR, tone mapping — future docs if ever needed.

## 11. Milestones → issues

| Milestone | Acceptance | Issue |
|---|---|---|
| Design | this doc | FE-123 |
| Core primitives | TriMesh/Mesh/Surface3D + MeshItem + lerp, wasm-clean | FE-124 |
| M1 occlusion & shading | saddle renders with correct self-occlusion, per-pixel Lambert | FE-125 |
| M2 instancing | ~10k spheres/cylinders interactive at 60 fps | FE-126 |
| M3 transparency | translucent-over-opaque without sort artifacts | FE-127 |
| M4 animation | continuous surface morph via tween system; heightfield | FE-128 |
| Integration | wasm/WebGL2 check, dioxus, gallery, docs, GH #1 closed | FE-129 |
