# Quantum Mechanics

> **Crate:** `manim-quantum` — modules `wavefunction`, `eigenstates`,
> `superposition`, `bloch`, `wells`.

A wavefunction is complex-valued, and every visualization that throws away the
phase throws away the physics. Interference, tunneling, and the whole
time-evolution story live in `arg ψ`. So the default styles here keep it:
`|ψ|²` drawn as a curve whose *colour* is the local phase, so the carrier
oscillations sweep the hue wheel as the packet moves.

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/quantum/wavepacket_barrier.mp4">
  </video>
  <figcaption>
    A Gaussian wavepacket with mean momentum <code>k₀ &gt; 0</code> striking a
    rectangular barrier. Part reflects, part tunnels through, and the packet
    separates into two lobes. Hue is the local phase of ψ.
  </figcaption>
</figure>

## Evolution that is actually unitary

The tunneling scene evolves ψ with the **split-step Schrödinger** stepper from
`manim-fields::pde`: alternate a kinetic half-step in momentum space (via
`rustfft`) with a potential step in position space. Each factor is a pure phase
rotation, so the method is unitary by construction — probability is conserved
to machine precision rather than approximately.

That is not an aesthetic preference. A naive finite-difference stepper leaks
norm, and over the twenty units of simulated time in this example the packet
would visibly dim. Worse, the transmission coefficient you would read off the
final frame would be wrong. With split-step, the crate's tests assert
`T + R ≈ 1` after the full evolution and it holds to four digits.

## Analytic eigenstates

`eigenstates` supplies closed-form solutions where they exist, because a
numerically-evolved ground state that slowly decays is a bad teaching aid:

- **Particle in a box** — `sin(nπx/L)`.
- **Harmonic oscillator** — Hermite polynomials × Gaussian.
- **Hydrogen** — associated Laguerre radial functions × real spherical
  harmonics, exposed both as a `ScalarField` and, through
  `orbital_isosurface`, as marching-cubes geometry. The familiar `p`, `d`, and
  `f` lobes are isosurfaces of `|ψ|²` with normals from the field's AD
  gradient.

`superposition` builds time-evolving combinations with the `e^{-iEₙt/ħ}` phase
factors, which is where you get beating between eigenstates for free. Coherent
states are included, and their expectation value provably tracks the classical
trajectory — the crate tests exactly that.

## The Bloch sphere

`BlochSphere` renders a qubit state as a vector on the mesh sphere, with gates
as animated rotations. Gate composition is checked as geometry rather than
asserted: `HZH = X` is verified by composing the rotations and comparing the
resulting frames.

## Potential wells

`wells` draws well diagrams (finite/infinite square, harmonic, arbitrary) with
their bound-state energy levels, and carries `TunnelingScene` — the driver the
example below uses.

## The example

<div class="source-note">

Source: [`crates/manim-quantum/examples/wavepacket_barrier.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-quantum/examples/wavepacket_barrier.rs)

</div>

Note the custom `Animation` impl: the scene owns a `TunnelingScene` and, on each
`alpha`, steps the PDE forward and repaints the phase-hue curve. This is the
general pattern for animating a simulation — the animation *is* the timestepper,
not a precomputed path.

```rust
{{#include ../../crates/manim-quantum/examples/wavepacket_barrier.rs}}
```

```sh
cargo run --release -p manim-quantum --example wavepacket_barrier --features render-examples
```

## Hydrogen orbitals, with their signs

<figure class="manim-figure">
  <img src="assets/quantum/hydrogen_orbitals.png" alt="2p_z and 3d_xy hydrogen orbitals as signed blue/red isosurfaces">
  <figcaption>
    Left: <code>2p_z</code>, two lobes separated by one nodal plane. Right:
    <code>3d_xy</code>, four alternating lobes and two nodal planes. Blue is
    <code>ψ = +c</code>, red is <code>ψ = −c</code> — these are level sets of the
    wavefunction, not of <code>|ψ|²</code>.
  </figcaption>
</figure>

A bound state is `ψ_{nlm}(r, θ, φ) = R_{nl}(r)·Yₗᵐ(θ, φ)`: an associated-Laguerre
radial factor times a real spherical harmonic. The radial factor sets the *size*,
the harmonic sets the *shape*.

On the left, `2p_z` (`n=2, l=1, m=0`): `Y₁⁰ ∝ cos θ`, so one lobe up, one down,
separated by the single nodal plane `z = 0`. On the right, `3d_xy`
(`n=3, l=2, m=−2`): `Y₂⁻² ∝ sin²θ·sin 2φ`, giving four alternating lobes in the
`xy`-plane and *two* nodal planes. The count generalizes: `Yₗᵐ` has `l − |m|`
nodal cones plus `|m|` nodal planes.

**Why the sign matters.** `|ψ|²` alone cannot explain chemistry. When two atoms
approach, lobes of *like* sign overlap constructively into a bonding orbital and
lobes of *opposite* sign cancel into an antibonding one. Squaring first throws
that information away — so this scene renders the pair of level sets `ψ = ±c` and
colours by sign. The [chemistry chapter](./chemistry.md) picks the story up from
here with σ and σ* in H₂.

<div class="source-note">

Source: [`crates/manim-quantum/examples/hydrogen_orbitals.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-quantum/examples/hydrogen_orbitals.rs)

</div>

```rust
{{#include ../../crates/manim-quantum/examples/hydrogen_orbitals.rs}}
```

```sh
cargo run --release -p manim-quantum --example hydrogen_orbitals --features render-examples
```

## HZH = X, watched on the Bloch sphere

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/quantum/bloch_gates.mp4">
  </video>
  <figcaption>
    Three π arcs from <code>|0⟩</code>: <code>H</code> swings north pole to
    equator, <code>Z</code> spins the equator by π, <code>H</code> lands on the
    south pole. Exactly what a single <code>X</code> would have done.
  </figcaption>
</figure>

A pure qubit `|ψ⟩ = cos(θ/2)|0⟩ + e^{iφ} sin(θ/2)|1⟩` is a unit vector in ℝ³, and
every single-qubit gate is a *rotation* of that sphere: `X`, `Y`, `Z` are π turns
about `x̂`, `ŷ`, `ẑ`, and `H` is a π turn about the diagonal axis `(x̂ + ẑ)/√2`.

Global phase — the one thing that distinguishes `HZH` from `X` as 2×2 matrices —
is exactly what the Bloch picture quotients out. So on the sphere the identity is
*exact* in SO(3), and the animation is a proof rather than an illustration:

- `H` swings `+ẑ → +x̂`, i.e. `|0⟩ → |+⟩`, onto the equator;
- `Z` spins the equator by π, `+x̂ → −x̂`, i.e. `|+⟩ → |−⟩`;
- `H` swings `−x̂ → −ẑ`, landing on `|1⟩`.

North pole to south pole. The reason is conjugation: because `H` is a π rotation
about the bisector of `x̂` and `ẑ`, it *exchanges* those two axes, so `H(Z)H` is
the same rotation as `Z` but performed about `x̂` — which is `X`. Blue marks `ẑ`,
red marks `x̂`; the traced arc is green for each `H` and orange for the `Z`.

<div class="source-note">

Source: [`crates/manim-quantum/examples/bloch_gates.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-quantum/examples/bloch_gates.rs)

</div>

```rust
{{#include ../../crates/manim-quantum/examples/bloch_gates.rs}}
```

```sh
cargo run --release -p manim-quantum --example bloch_gates --features render-examples
```

> **Try changing the barrier.** `TunnelingParams` carries the barrier height and
> width. Raise the height past the packet's mean energy and the transmitted lobe
> collapses toward zero — exponentially in the barrier width, which is the
> single most counterintuitive fact in introductory quantum mechanics and the
> one most worth watching happen.
