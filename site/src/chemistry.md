# Chemistry

> **Crate:** `manim-chem` — modules `molecule`, `parsers`, `element`, `render`,
> `lattice`, `cube`.

Molecular graphics is a rendering problem before it is a chemistry problem. A
protein is hundreds of thousands of spheres and cylinders; a crystal lattice is
unbounded by construction. Drawn as individual mobjects, a molecule of any
interest destroys the frame budget.

<figure class="manim-figure">
  <video controls loop muted playsinline
         src="assets/chem/caffeine.mp4">
  </video>
  <figcaption>
    Caffeine (C<sub>8</sub>H<sub>10</sub>N<sub>4</sub>O<sub>2</sub>, 24 atoms)
    as a ball-and-stick model under a turntable orbit. Two GPU-instanced draw
    calls total: one atom cloud, one bond cloud.
  </figcaption>
</figure>

## Instancing is the whole design

`render::ball_and_stick` emits exactly **two** draw calls regardless of molecule
size: one instanced sphere mesh for the atoms, one instanced cylinder for the
bonds, each instance carrying its own transform and colour. A 294-atom demo runs
in the same two calls as caffeine's 24.

`space_filling` (van-der-Waals radii, no bonds) and `wireframe` (bonds only)
share the same path. All three return a `VGroup`, so they compose with the
ordinary animation catalog — you can `Create` a molecule, fade it, or morph
between conformers.

## Parsers with no dependencies

`parsers::from_xyz` and `parsers::from_sdf` read the two formats that actually
circulate, in a few hundred lines and with no crates behind them. The SDF parser
handles MDL V2000 and is whitespace-tolerant rather than column-exact, because
real files in the wild are not column-exact.

The example embeds a PubChem molblock (CID 2519) as a string constant, which
means the example is self-contained — no data file to fetch, no path to get
wrong.

When a file carries no bond block, `bond perception` infers bonds from
interatomic distances against covalent radii. It is a heuristic and it is
documented as one; explicit bonds are always preferred when the file has them,
which is why the caffeine example uses SDF rather than XYZ.

## The element table

`element` carries the CPK data for H through Xe: atomic number, the conventional
CPK colour, covalent radius, and van-der-Waals radius. The colours are the
ones every chemist already reads without a legend — oxygen red, nitrogen blue,
carbon grey — and departing from them for aesthetic reasons would cost more in
comprehension than it gains.

## Lattices and orbitals

`lattice` builds a unit cell and replicates it under the cell symmetry, with
presets for rock salt (NaCl), diamond, and graphene. `cube` parses the Gaussian
`.cube` volumetric format into a `ScalarField`, which then goes straight through
the [marching-cubes path](./surfaces.md) to give molecular-orbital isosurfaces —
the same extraction code that handles hydrogen orbitals in the
[quantum chapter](./quantum.md), because they are the same problem.

## The example

<div class="source-note">

Source: [`crates/manim-chem/examples/caffeine.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-chem/examples/caffeine.rs)

</div>

```rust
{{#include ../../crates/manim-chem/examples/caffeine.rs}}
```

```sh
cargo run --release -p manim-chem --example caffeine --features render-examples
```

## Rock salt: the unit cell picked out of the crystal

<figure class="manim-figure">
  <img src="assets/chem/nacl_lattice.png" alt="NaCl rock-salt lattice block with one cubic unit cell outlined in yellow">
  <figcaption>
    Sodium chloride, with one conventional cubic cell outlined in yellow. Sticks
    are drawn only between <em>unlike</em> ions closer than 3.4 Å, so they trace
    the coordination octahedra directly.
  </figcaption>
</figure>

NaCl is the textbook ionic crystal. Its conventional cell is cubic with
`a = 5.64 Å`, and the structure reads best as **two interpenetrating FCC
sub-lattices** — one Na⁺, one Cl⁻ — offset by `a/2` along a cell edge.

That offset is the whole structure. It puts every ion at the centre of a regular
octahedron of the *opposite* species, so the **coordination number is 6** for
both, at a nearest-neighbour distance of `a/2 = 2.82 Å`. The nearest *like*
neighbour is further off, at `a/√2 = 3.99 Å`. Because the sticks are drawn only
between unlike ions under 3.4 Å, they trace exactly those octahedra — count the
six mutually perpendicular sticks on any interior ion.

One honest caveat the picture cannot show. The spheres use CPK *neutral-atom
covalent* radii, so sodium (purple, 1.66 Å) comes out larger than chlorine
(green, 1.02 Å). Ionisation **reverses** this: Na⁺ has lost its whole 3s shell and
shrinks to 1.02 Å, while Cl⁻ gains an electron and swells to 1.81 Å. The real
crystal is a close-packed array of big chloride ions with small sodium ions tucked
into its octahedral holes — very nearly the inverse of what is drawn here.

<div class="source-note">

Source: [`crates/manim-chem/examples/nacl_lattice.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-chem/examples/nacl_lattice.rs)

</div>

```rust
{{#include ../../crates/manim-chem/examples/nacl_lattice.rs}}
```

```sh
cargo run --release -p manim-chem --example nacl_lattice --features render-examples
```

## σ and σ*: why H₂ exists and He₂ does not

<figure class="manim-figure">
  <img src="assets/chem/orbital_isosurface.png" alt="H2 bonding sigma and antibonding sigma-star molecular orbitals as signed isosurfaces">
  <figcaption>
    Left: <code>σ</code>, one continuous lobe swallowing both protons. Right:
    <code>σ*</code>, two lobes of opposite sign with a visible gap where the nodal
    plane cuts through.
  </figcaption>
</figure>

Two hydrogen 1s orbitals combine two ways, and the difference is the covalent
bond.

The in-phase (bonding) sum `σ = 1sₐ + 1s_b` has **no node between the nuclei**:
the atomic tails interfere constructively, piling electron density into the
internuclear region where it is attracted by *both* protons. That is the bond —
and it is why H₂ has a bond length of 0.74 Å and a dissociation energy of 4.52 eV.

The out-of-phase difference `σ* = 1sₐ − 1s_b` has a **nodal plane exactly midway**
between the nuclei, evacuating density from precisely the region that would have
bonded them. Occupying σ* costs more energy than σ gains, which is why He₂ — which
would have to fill both — does not exist.

The field comes from a Gaussian `.cube` grid the example writes itself, so there
is no data file to fetch, and the surfaces come out of the same marching-cubes
path as the [hydrogen orbitals](./quantum.md) and every other isosurface in this
book. Again the level sets are of `ψ`, not `|ψ|²`: squaring would erase the sign
difference that is the entire point.

<div class="source-note">

Source: [`crates/manim-chem/examples/orbital_isosurface.rs`](https://github.com/cryptex-ai/manim_rust/blob/main/crates/manim-chem/examples/orbital_isosurface.rs)

</div>

```rust
{{#include ../../crates/manim-chem/examples/orbital_isosurface.rs}}
```

```sh
cargo run --release -p manim-chem --example orbital_isosurface --features render-examples
```
