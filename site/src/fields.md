# Fields & Automatic Differentiation

> **Crate:** [`manim-fields`](./reference.md) — no manim dependencies. `glam` for
> vectors, `rustfft` for the Schrödinger stepper, nothing else.

Everything scientific in this library rests on one decision: **the mathematics
is a value that knows nothing about drawing.** A `ScalarField` does not know
what a mobject is. A `SpaceMap` cannot render itself. They are ordinary Rust
values with differential operators on them, and they are tested the way
numerical code should be — against analytic solutions and conservation laws, to
tolerances.

That separation buys two things. Visualizers become interchangeable (the same
field feeds a heatmap, an isosurface, and a stream-tube plot), and the
mathematics becomes *falsifiable* independently of whether the picture looks
plausible. A pretty figure computed from a wrong Jacobian is the worst possible
outcome, and it is exactly what a fused math-and-drawing layer invites.

## Forward-mode AD

Derivatives here are never finite-differenced. Three dual-number types share a
`Scalar` trait, so one generic closure serves value *and* derivative
evaluation:

| Type | Carries | Used for |
|---|---|---|
| `Dual` | value + first derivative | 1-D slopes, arc length |
| `Dual2` | value + first + second | curvature, Laplacians |
| `Dual3` | value + full 3-gradient | Jacobians, divergence, curl, normals |

```rust
use manim_fields::ad::{Dual, Scalar};

// d/dx [x³] at x = 2 is 3·2² = 12 — exact to roundoff, no step size to tune.
let x = Dual::var(2.0);
assert!((x.powi(3).du - 12.0).abs() < 1e-12);
```

The `Scalar` trait is the reason a surface can be defined *once* and then
queried for its position, its tangent frame, and its Gaussian curvature. You
write the parameterization generically and the AD machinery supplies the jets:

```rust,ignore
impl SurfaceSampler for Torus {
    fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
        let r = S::constant(1.0) + u.cos().scale(0.4);
        [r * v.cos(), r * v.sin(), u.sin().scale(0.4)]
    }
}
```

Instantiate `S = f64` and you get a point. Instantiate `S = Dual2` and the same
line yields the second fundamental form. That is the whole trick behind the
[curvature chapter](./surfaces.md).

## Fields

Four field types wrap `Fn` closures and carry differential operators:

```rust,ignore
ScalarField   // + grad(), laplacian()
VectorField3  // + div(), curl(), flow integrators
ComplexField  // + phase/modulus views
TensorField2  // symmetric 2-tensors: stress, metric
```

They compose — add, scale, compose with a map, restrict to a region — and each
differential operator is an AD evaluation rather than a stencil, so a field
sampled on a coarse grid still reports its exact divergence at every point.
Time dependence is a `TimeField<T> = Fn(Point, f64) -> T`.

## SpaceMap — the deformation primitive

`SpaceMap` is a point-to-point map with an AD Jacobian, and it is the hinge the
whole [deformations chapter](./deformations.md) turns on:

```rust,ignore
let map = SpaceMap::complex_power(2);       // z ↦ z²
let j   = map.jacobian(p);                  // exact, via Dual3
let h   = map.homotopy_to(&other);          // H(x, t)
let flow = SpaceMap::from_flow(&v, 1.0);    // time-1 flow map of a field (RK45)
```

The Jacobian is not a curiosity — `DeformationGrid` subdivides its lines
according to local Jacobian distortion, so a grid under `z ↦ z²` gets dense
exactly where the map stretches and stays cheap where it doesn't.

## Numerics

**ODE integrators.** `rk4`, adaptive `rk45` (Dormand–Prince), and — because
Hamiltonian demos must *visibly* conserve energy over long runs — symplectic
`leapfrog` and 4th-order `yoshida4`. A non-symplectic integrator on an orbit
demo spirals, and a student watching it learns something false about the
physics.

**PDE steppers.** Uniform-grid heat and wave steppers, plus split-step
Schrödinger evolution in 1-D and 2-D via `rustfft`. The split-step method
alternates a kinetic half-step in momentum space with a potential step in
position space; it is unitary by construction, which is why the
[tunneling example](./quantum.md) can assert `T + R ≈ 1` to four digits after
twenty units of simulated time.

**Geodesics and parallel transport** on parametric surfaces, with Christoffel
symbols taken from AD of the first fundamental form.

Everything is deterministic and I/O-free.

## Structure beats order

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/fields/symplectic_vs_rk4.mp4">
  </video>
  <figcaption>
    The same pendulum, integrated two ways, drawn in phase space. Teal is
    Yoshida's 4th-order symplectic composition; gold is classic RK4 at the same
    formal order and the same step size.
  </figcaption>
</figure>

The pendulum is the separable Hamiltonian `H(q, p) = p²/2 − cos q`, so its exact
trajectories are the level sets `H = const` — closed lens-shaped loops for
`H < 1`. Both curves start from the same near-separatrix state `(q, p) = (3, 0)`
and take the same step `h = 0.7`.

They are the same formal order. They do not behave the same at all.

The symplectic method does **not** conserve `H` exactly — but it conserves a
nearby *shadow* Hamiltonian exactly, which bounds its energy error forever. The
orbit retraces one closed loop. RK4 has no symplectic structure, so its energy
error accumulates secularly: the orbit spirals inward, shedding about **17% of
its amplitude in a dozen swings**.

Twelve swings is nothing. A Hamiltonian demo that runs for a minute would show
RK4 collapsing to a point — and a reader would learn something false about
pendulums. For long-time Hamiltonian dynamics, *structure* beats *order*.

<div class="source-note">

Source: [`crates/manim-sci/examples/symplectic_vs_rk4.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/symplectic_vs_rk4.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/symplectic_vs_rk4.rs}}
```

```sh
cargo run --release -p manim-sci --example symplectic_vs_rk4 --features render-examples
```

## Kepler's laws, from Newton's law alone

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/fields/kepler_orbits.mp4">
  </video>
  <figcaption>
    Three planets released from the same point with different transverse speeds,
    integrated symplectically from the inverse-square law. Every orbit passes
    through the white dot at a <em>focus</em>. Shaded wedges sweep equal areas in
    equal times.
  </figcaption>
</figure>

With `G = M = m = 1` the force is `F(r) = −r/|r|³` and the Hamiltonian
`H = |p|²/2 − 1/|r|` is separable, so Yoshida's composition keeps the energy
bounded and the orbits closed over a full revolution. Nothing about ellipses is
put in by hand — the orbits are integrated from the force law and come out
elliptical.

Three planets leave `r₀ = (1, 0)` with different transverse speeds `v`. The
eccentricity is `e = |v² − 1|` and the semi-major axis `a = 1/(2 − v²)`, so
`v = 1` gives a circle and larger `v` progressively stretched ellipses. Yet
**every** orbit passes through the same white dot at the origin — which sits at
a *focus*, never at the centre. That is Kepler's first law, and drawing three
orbits at once is what makes it a statement rather than a coincidence.

Over the outermost orbit, three shaded wedges are swept over equal *time*
intervals. Near perihelion the wedge is short and fat; near aphelion long and
thin; the two areas match. That is Kepler's second law, `dA/dt = L/2 = const`.

<div class="source-note">

Source: [`crates/manim-sci/examples/kepler_orbits.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-sci/examples/kepler_orbits.rs)

</div>

```rust
{{#include ../../crates/manim-sci/examples/kepler_orbits.rs}}
```

```sh
cargo run --release -p manim-sci --example kepler_orbits --features render-examples
```

> Both examples live in `manim-sci` rather than `manim-fields`: the integrators
> are pure computation with no rendering, so the crate that can *draw* the
> result is the one that hosts the scene.
