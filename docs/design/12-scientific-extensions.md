# Scientific Extensions: Math / Physics / Chemistry Graphics

Goal: extend manim_rust into a replete scientific graphics library able to
express visual complex analysis, differential geometry, topology, quantum
mechanics, chemistry, and neural networks — powering interactive,
constructivist textbook websites (Dioxus). Needham's *Visual Complex
Analysis* is the north-star aesthetic; direct manipulation is the north-star
pedagogy.

## Architecture principles

- **Fields and maps are values, not scenes.** The mathematical objects
  (`Field`, `SpaceMap`) live in a new dependency-light crate and know nothing
  about mobjects; visualizers consume them. This keeps them testable to
  numerical tolerances and reusable across domains.
- **Per-pixel where the math is per-pixel.** Domain coloring and phase plots
  are functions of the *pixel*, not the vertex — a small closed set of
  built-in GPU materials handles them. No arbitrary user shaders in v1.
- **Additive to the existing contracts.** DisplayList/mesh pipeline gain a
  `Material` axis; the snapshot timeline, arena, and Dioxus player are
  unchanged. Domain kits are separate crates so `manim-core` stays lean.
- **Textbook-grade web budgets.** Many figures per page → shared GPU device,
  render-on-demand (dirty-flagged), lazy mount. Idle figures must cost ~0.

## Crate plan

```
manim-fields    # AD (dual numbers), Field<T>, SpaceMap, integrators, PDE steppers
manim-sci       # shared scientific visualizers: domain coloring, isosurfaces,
                # streamribbons, parallel transport, tube meshes (deps: fields+core)
manim-quantum   # wavefunctions, Bloch sphere, evolution, wells/orbitals
manim-chem      # molecules (XYZ/SDF), lattices, CPK data, orbital surfaces
manim-nn        # compute graphs, layer diagrams, heatmaps, loss landscapes
```

`manim-fields` has no manim deps (glam only) — it is a standalone applied-math
crate. `manim-sci` bridges fields→mobjects/meshes.

## Tier 1 — mathematical substrate

### Automatic differentiation (`manim-fields::ad`)
Dual numbers over `f32`/`Vec3` (`Dual`, `Dual3` with 3-wide tangent):
gradients of user closures without finite differences. Jacobians for
`SpaceMap`, normals for implicit surfaces, curvature from second derivatives
(forward-over-forward). Pure, tiny, no deps.

### Fields (`manim-fields::field`)
```rust
pub trait Field<T> { fn at(&self, p: Point) -> T; }
pub struct ScalarField(Arc<dyn Fn(Point) -> f32>);      // + grad(), laplacian()
pub struct VectorField3(Arc<dyn Fn(Point) -> Vec3>);    // + div(), curl(), flow integrators
pub struct ComplexField(Arc<dyn Fn(Complex) -> Complex>); // + phase/modulus fields
pub struct TensorField2(..);                            // symmetric 2-tensors (stress, metric)
```
Combinators (add/scale/compose/restrict), time-dependence via
`TimeField<T> = Fn(Point, f32) -> T`. `Complex` is our own 2-field struct
(no num-complex dep), with exp/log/powers/Möbius helpers.

### SpaceMap (`manim-fields::map`) — the deformation primitive
```rust
pub struct SpaceMap { f: Arc<dyn Fn(Point) -> Point>, /* + optional inverse */ }
impl SpaceMap {
    fn jacobian(&self, p: Point) -> Mat3;          // via AD
    fn compose(&self, other: &SpaceMap) -> SpaceMap;
    fn homotopy_to(&self, other: &SpaceMap) -> Homotopy;  // H(x,t), straight or custom
    fn from_complex(f: ComplexMap) -> SpaceMap;    // z-plane embedding
    fn from_flow(v: VectorField3, t: f32) -> SpaceMap;    // time-t flow map (RK45)
}
```
Core-side consumption (in `manim-sci`):
- `DeformationGrid`: an ambient grid mobject that carries any SpaceMap
  (nonlinear generalization of LinearTransformationScene's plane; subdivision
  adaptive to Jacobian distortion).
- `ApplyMap` animation: per-alpha re-evaluation `H(x, α)` of the ORIGINAL
  points (snapshot in `begin()` — fits the existing discipline), unlike
  `ApplyFunction`'s endpoint lerp. `FlowMap` variant animates along the
  generating field's integral curves.
- Pullback/pushforward of fields through maps.

### Numerics (`manim-fields::integrate`, `::pde`)
- ODE: RK4 (exists in core — migrate), adaptive RK45 (Dormand–Prince),
  **symplectic** leapfrog/Verlet + 4th-order Yoshida (Hamiltonian demos must
  conserve energy visibly).
- PDE steppers on uniform grids: heat, wave, and **split-step Schrödinger**
  (needs an FFT: `rustfft`, pure Rust, wasm-clean) producing frames that feed
  HeightField / complex textures.
- Geodesic + parallel-transport integrators on parametric surfaces (Christoffel
  symbols via AD of the first fundamental form).

## Tier 2 — visualizers (`manim-sci`)

- **Domain coloring** (complex fields): GPU material (see render changes);
  phase→hue, modulus→brightness contours, optional grid-image overlay; poles/
  zeros/branch-cut markers. `RiemannSphere` (stereographic texture on the mesh
  sphere).
- **Isosurfaces**: marching cubes over ScalarField → TriMesh (mesh pipeline).
  Adaptive resolution; normals from field gradient (AD — exact, not averaged).
- **Field visualization 3D**: streamlines→tube meshes, stream ribbons (frame
  transport), arrow-glyph fields via instancing (2D version exists), flux
  through animated surfaces, tensor glyphs (ellipsoids via instancing).
- **Curves/surfaces diff-geo**: Frenet/Darboux frame animators, curvature comb
  (2D) & curvature heat coloring (`set_fill_by_value` — clears the M6
  deferral), Gauss map animation, tube/offset/normal surfaces, geodesic tracer,
  parallel-transport holonomy demo.
- **Topology**: fundamental-polygon gluing (MorphSurface chains — exists),
  parametric knot table + tubes, Hopf fibration (S³→S² fiber tubes through
  stereographic projection), covering-space unwinder (SpaceMap on the deck).
- **Probability clouds / volumetrics v1**: Monte-Carlo scatter of a density →
  instanced billboards with soft sprites; slice planes of ScalarFields with the
  heatmap material.

## Tier 2 — domain kits

- **manim-quantum**: `Wavefunction1D/2D` (ComplexField + styles: phase-hue
  height plot, probability density), superposition builder with
  `e^{-iE_n t}` evolution, split-step evolution for arbitrary potentials,
  `BlochSphere` (mesh sphere + state vector + gates as rotations), particle in
  box/harmonic well/hydrogen (analytic eigenstates; hydrogen orbitals via the
  isosurface path).
- **manim-chem**: `Molecule::from_xyz/from_sdf` (tiny parsers, no deps),
  CPK radii/colors table, ball-and-stick / space-filling via InstancedMesh
  (proven: 294-atom demo, 2 draw calls), bond perception (distance heuristic),
  `Lattice` (unit cell + symmetry replication), reaction-coordinate morphs
  (MorphMesh between conformers), orbital isosurfaces (cube-file field input).
- **manim-nn**: `ComputeGraph` (layered/rank DAG layout — extends the existing
  Graph layouts), `LayerBlockDiagram` (blocks + tensor-shape labels + skip
  connections), weight/attention heatmap mobject (matrix → ImageMobject with
  colormap), `LossLandscape` (HeightField — exists — + SGD trajectory tracer),
  activation-flow animation (pulses along edges via ShowPassingFlash — exists).

## Tier 3 — the constructivist layer (manim-dioxus + site scaffold)

- `use_parameter(name, range, default)` — slider/scrubber widget two-way bound
  to a ValueTracker in a live scene; `Parametrized` scenes re-solve on change.
- **Draggable handles**: pointer hit-testing (scene-coords exist) →
  `DragHandle` mobjects feeding trackers; e.g. drag a zero of a polynomial and
  watch the domain coloring re-render.
- `OrbitControls` (formalize the LiveUpdater camera pattern): drag=orbit,
  wheel=zoom, with inertia; one line to add to any 3D figure.
- **Figure**: a lighter embed than ManimPlayer for static-until-touched
  diagrams — renders once, re-renders on parameter/pointer events only.
- Site scaffold (separate repo later): MDX-like chapter format with inline
  `Figure`s, shared GPU device, lazy mount via IntersectionObserver.

## Implementation changes required (the honest list)

1. **Material system (render).** `Paint::Material(MaterialId, params)` on
   DrawItem/mesh: built-in WGSL for (a) `FieldTexture` (CPU/compute-sampled
   R32F/RG32F texture + colormap LUT + contour lines), (b) `PhaseHue`
   (complex RG texture → HSV shading), (c) `Heatmap` (scalar → LUT). Camera-
   aware UVs so materials live in scene space. Tessellation cache key gains
   material hash (paint-hash pattern exists).
2. **Shared GpuContext in manim-dioxus.** One device per page, per-player
   surfaces; render-on-demand with dirty flags (parameter/pointer/animation
   events mark dirty). Battery is pedagogy.
3. **ApplyMap animation family (core).** Per-alpha re-evaluation from
   snapshotted original points (existing `begin()` discipline covers it).
4. **Compute pass slot (render, phase 2).** Storage-texture field evaluation
   and split-step evolution on GPU when CPU sampling can't hold 60fps. v1
   ships CPU sampling + texture upload (128² live wave already proves it).
5. **`set_fill_by_value` (mesh)** — per-face scalar → colormap; clears the M6
   deferral and serves curvature/field coloring everywhere.

## What v1 must prove (exit criteria)

The **visual-complex-analysis vertical slice**: a Dioxus page with (1) domain
coloring of a user-editable rational function with draggable zeros/poles,
(2) a conformal-grid DeformationGrid animating `z ↦ z²` then a Möbius map via
ApplyMap, (3) a Riemann-sphere view, all sharing one GPU device, all
render-on-demand, at 60fps interaction on a mid laptop. If that page feels
like the textbook we wished we had, the architecture is right.

## Milestones

- **S0 Fields & maps**: manim-fields crate (AD, Complex, fields, SpaceMap,
  integrators incl. symplectic), property-tested.
- **S1 Materials**: render material system (FieldTexture/PhaseHue/Heatmap),
  set_fill_by_value, domain-coloring golden tests.
- **S2 Deformation**: ApplyMap/FlowMap, DeformationGrid, complex-analysis kit,
  Riemann sphere.
- **S3 Constructivist web**: shared device, render-on-demand Figure,
  use_parameter, DragHandle, OrbitControls; **the VCA vertical-slice page**.
- **S4 Surfaces & topology**: isosurfaces (marching cubes), diff-geo queries,
  geodesics/parallel transport, tubes/knots, gluing demos.
- **S5 Quantum**: manim-quantum (wavefunctions, evolution incl. split-step +
  rustfft, Bloch sphere, wells + hydrogen).
- **S6 Chemistry**: manim-chem (parsers, ball-stick/space-fill, lattices,
  orbital isosurfaces).
- **S7 Neural nets**: manim-nn (compute graphs, block diagrams, heatmaps,
  loss landscapes + trajectories).
- **S8 Fields-in-3D & volumetrics**: stream tubes/ribbons, tensor glyphs,
  Monte-Carlo clouds, compute-pass evaluation.
