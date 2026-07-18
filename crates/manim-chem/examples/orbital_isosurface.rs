//! The H₂ molecular orbitals σ and σ*, as signed isosurfaces from a Gaussian
//! `.cube` grid this example writes itself.
//!
//! Two hydrogen 1s orbitals combine two ways. The in-phase (bonding) sum
//! `σ = 1sₐ + 1s_b` has **no node between the nuclei**: the two atomic tails
//! interfere constructively, piling electron density into the internuclear
//! region where it is attracted by *both* protons — that is the covalent bond,
//! and it is why H₂ has a bond length of 0.74 Å and a dissociation energy of
//! 4.52 eV. The out-of-phase (antibonding) difference `σ* = 1sₐ − 1s_b` has a
//! **nodal plane exactly midway** between the nuclei, evacuating density from
//! precisely the region that would have bonded them; occupying σ* costs more
//! energy than σ gains, so He₂ (which would fill both) does not exist.
//!
//! Left in the frame is σ — one continuous blue lobe swallowing both protons.
//! Right is σ* — two lobes, blue and red for the opposite signs of the
//! wavefunction, with a visible gap where the nodal plane cuts through.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-chem --example orbital_isosurface --features render-examples
//! ```
//!
//! Frames land in `out/orbital_isosurface/frame_NNNNN.png`.

use glam::Vec3;

use manim_chem::cube::from_cube;
use manim_chem::render;
use manim_core::mesh::{Mesh, MeshMaterial};
use manim_core::prelude::*;
use manim_render::export::VideoExporter;

/// Spatial magnification of the whole picture. `molecular_orbital_isosurface`
/// marches a fixed 48³ grid over ±6 Å (0.25 Å cells); at true scale an H₂ lobe
/// is only ~2 Å across and comes out faceted, so the cube, the nuclei, and the
/// scene are all drawn 1.8× life size. Every number quoted below is *true* Å.
const MAG: f32 = 1.8;

/// Slater exponent scale for hydrogen 1s: `ψ ∝ exp(−r/a₀)`, `a₀ = 0.529 Å`.
const A0: f32 = 0.529 * MAG;
/// Half the H–H bond length: the protons sit at `z = ±0.37 Å`.
const HALF_BOND: f32 = 0.37 * MAG;
/// Iso-value. Low enough that the σ surface still encloses *both* protons as one
/// lobe (ψ ≈ 0.99 at the bond midpoint), high enough that the two MOs do not
/// bleed into each other on screen.
const LEVEL: f64 = 0.22;
/// Radius of the drawn proton markers (a visual cue, not a physical size).
const PROTON_R: f32 = 0.16;
/// Half-width of the sampled grid box, and its point count per axis.
const HALF: f32 = 4.0 * MAG;
const N: usize = 41;

/// Emits a valid Gaussian `.cube` file for an H₂ MO as a `String`.
///
/// The format: two comment lines; then `natoms x0 y0 z0` (negative `natoms`
/// flags an *MO* cube, which carries one extra orbital-ID line before the data);
/// then three `npts vx vy vz` axis lines (negative `npts` flags ångström rather
/// than bohr); then one `Z charge x y z` line per atom; then the values with the
/// first axis slowest and the third fastest.
fn h2_mo_cube(antibonding: bool) -> String {
    let step = 2.0 * HALF / (N - 1) as f32;
    let sign = if antibonding { -1.0 } else { 1.0 };
    let mut s = String::from("H2 molecular orbital\nsynthesised by the manim-chem gallery\n");
    s.push_str(&format!("-2 {0} {0} {0}\n", -HALF)); // MO cube, origin at the box corner
    s.push_str(&format!("-{N} {step} 0.0 0.0\n")); // negative count => ångström
    s.push_str(&format!("-{N} 0.0 {step} 0.0\n"));
    s.push_str(&format!("-{N} 0.0 0.0 {step}\n"));
    s.push_str(&format!("1 1.0 0.0 0.0 {}\n", -HALF_BOND)); // proton A
    s.push_str(&format!("1 1.0 0.0 0.0 {HALF_BOND}\n")); // proton B
    s.push_str("1 1\n"); // MO cubes list the orbital IDs they contain
    for i in 0..N {
        for j in 0..N {
            for k in 0..N {
                let p =
                    Vec3::new(-HALF, -HALF, -HALF) + step * Vec3::new(i as f32, j as f32, k as f32);
                // Unnormalised LCAO: exp(−r_A/a₀) ± exp(−r_B/a₀).
                let ra = (p - HALF_BOND * Vec3::Z).length();
                let rb = (p + HALF_BOND * Vec3::Z).length();
                let psi = (-ra / A0).exp() + sign * (-rb / A0).exp();
                s.push_str(&format!("{psi:.5} "));
            }
            s.push('\n');
        }
    }
    s
}

/// Adds one MO — its ± lobes plus the two protons — shifted to `x_offset`.
fn add_orbital(scene: &mut Scene, antibonding: bool, x_offset: f32) {
    let cube = from_cube(&h2_mo_cube(antibonding)).expect("generated cube parses");
    let lobes = render::molecular_orbital_isosurface(scene.state_mut(), &cube, LEVEL);

    // Bare proton markers, so the node (or its absence) reads relative to the
    // nuclei. Plain spheres rather than `ball_and_stick`: an H–H stick drawn
    // straight through the σ* nodal plane would say the opposite of the point.
    let mut members: Vec<AnyId> = vec![lobes.erase()];
    for z in [-HALF_BOND, HALF_BOND] {
        // A `Mesh`, not a `Surface`: it shares the depth-tested mesh pipeline
        // with the lobes, so the markers occlude correctly inside them.
        let marker = scene.add(Mesh::sphere().with_material(MeshMaterial::new(WHITE)));
        scene
            .state_mut()
            .scale_about(marker, PROTON_R, Point::ZERO) // unit sphere -> proton size
            .shift(marker, z * Point::Z);
        members.push(marker.erase());
    }

    let group = VGroup::of(scene.state_mut(), members);
    scene.shift(group, x_offset * Point::X);
}

/// Scene builder for the H₂ σ / σ* comparison turntable.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(72_f32.to_radians(), -55_f32.to_radians());

        add_orbital(scene, false, -3.4); // σ  (bonding)    on the left
        add_orbital(scene, true, 3.4); // σ* (antibonding) on the right

        // One full revolution in ~6 s.
        let steps = 100;
        for _ in 0..steps {
            scene.rotate_camera(TAU / steps as f32);
            scene.wait(0.06);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    VideoExporter::render_to_png_sequence(&mut scene, "out/orbital_isosurface")?;
    println!("Rendered frames to out/orbital_isosurface");
    Ok(())
}
