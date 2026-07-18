# Surfaces & Topology

> **Crate:** `manim-sci` — modules `diffgeo`, `geodesics`, `curveviz`,
> `isosurface`.

Differential geometry is where exact derivatives stop being a nicety. Gaussian
curvature is a *second*-order quantity built from the second fundamental form;
computed by finite differences on a sampled surface it is noisy enough that the
sign flips spuriously near the flat ring of a torus — precisely the feature the
picture exists to show.

<figure class="manim-figure">
  <img src="assets/surfaces/torus_curvature.png" alt="torus curvature">
  <figcaption>
    A torus colored by Gaussian curvature — positive (red) on the outer rim,
    negative (blue) on the inner rim, zero on the two circles between — with a
    geodesic traced across it under an ambient camera orbit.
  </figcaption>
</figure>

## Surfaces are generic, and that is the trick

A surface is a `SurfaceSampler`: one method, generic over the AD `Scalar` trait.

```rust,ignore
impl SurfaceSampler for Torus {
    fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
        let r = S::constant(1.0) + u.cos().scale(0.4);
        [r * v.cos(), r * v.sin(), u.sin().scale(0.4)]
    }
}
```

You write the parameterization once. Instantiating `S = f64` gives a point;
instantiating the bivariate second-order jet gives the first and second
fundamental forms, and from those come the normal, the principal curvatures, the
Gaussian and mean curvature, and the Christoffel symbols — all exact to
roundoff, all from that one closure. No separate analytic derivative to derive
by hand and get wrong.

## Curvature coloring

`surface_colored_by_curvature` bakes Gaussian or mean curvature into per-vertex
colours through a `Colormap`. On the torus the result is a theorem you can read
off the picture: the outer rim is positive (both principal curvatures bend the
same way), the inner rim negative (they oppose), and the total integrated
curvature is zero, as Gauss–Bonnet demands for a genus-1 surface.

## Geodesics and holonomy

`geodesic` integrates `u''ᵏ = −Γᵏᵢⱼ u'ⁱ u'ʲ` — the curve that is *straight in the
surface's own geometry*. On a sphere it recovers great circles. On a torus it
does something much less obvious, which is the point of drawing it.

`parallel_transport` slides a tangent vector along a curve without intrinsic
turning. Its failure to come back unchanged around a closed loop — the
**holonomy** — equals the enclosed Gaussian curvature. That is Gauss–Bonnet, and
it is checked numerically in the crate's tests rather than asserted in prose.

Both integrate with the adaptive RK45 solver in the coordinate `(u, v)` basis.

## Tubes, knots, isosurfaces

`TubeMesh::along_curve` sweeps a circular cross-section along a space curve
using a **rotation-minimizing frame** rather than the raw Frenet frame. This
matters: the Frenet normal flips discontinuously at inflection points, so a
Frenet-swept tube visibly twists where the curve straightens. An RMF stays
well-defined throughout. `trefoil` and `figure_eight` ship as ready-made knots.

`isosurface` runs marching cubes over any `ScalarField`, taking normals from the
field's own AD gradient — exact, not averaged from neighbouring faces. The
extraction is verified by Euler characteristic: a sphere comes out with χ = 2, a
torus with χ = 0, and a mesh that fails that check is a bug rather than a
rendering artifact.

## The example

<div class="source-note">

Source: [`crates/manim-sci/examples/torus_curvature.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/torus_curvature.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/torus_curvature.rs}}
```

```sh
cargo run --release -p manim-sci --example torus_curvature --features render-examples
```

## Geodesic deviation

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/surfaces/geodesic_race.mp4">
  </video>
  <figcaption>
    Four geodesics released in parallel across an egg-crate surface. They do not
    stay parallel — where the surface curves positively they converge, where it
    curves negatively they spread. This is geodesic deviation, and it is the
    mechanism behind tidal forces in general relativity.
  </figcaption>
</figure>

Starting four geodesics parallel and watching them fail to remain so is the most
direct statement of what curvature *is*. On a flat surface the separation grows
linearly forever; here the Jacobi equation couples it to the local Gaussian
curvature, and the egg-crate's alternating sign makes the focusing and
defocusing happen in the same frame.

<div class="source-note">

Source: [`crates/manim-sci/examples/geodesic_race.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/geodesic_race.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/geodesic_race.rs}}
```

```sh
cargo run --release -p manim-sci --example geodesic_race --features render-examples
```

## Knots and torsion

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/surfaces/trefoil_tube.mp4">
  </video>
  <figcaption>
    The trefoil knot swept as a solid tube along a rotation-minimizing frame,
    colored by torsion — how fast the curve is twisting out of its own
    osculating plane.
  </figcaption>
</figure>

Colouring by torsion rather than by arc length turns the tube into a readout of
the curve's third-order behaviour. The trefoil's three lobes each show the same
torsion signature, which is a visible statement of its 3-fold symmetry.

<div class="source-note">

Source: [`crates/manim-sci/examples/trefoil_tube.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/trefoil_tube.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/trefoil_tube.rs}}
```

```sh
cargo run --release -p manim-sci --example trefoil_tube --features render-examples
```
