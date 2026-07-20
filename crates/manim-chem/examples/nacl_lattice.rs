//! Rock salt (NaCl): the cubic unit cell picked out of the crystal.
//!
//! Sodium chloride is the textbook ionic crystal. Its conventional cell is cubic
//! with `a = 5.64 Å`, and the structure is best read as **two interpenetrating
//! FCC sub-lattices** — one of Na⁺, one of Cl⁻ — offset from each other by
//! `a/2` along a cell edge. That offset is the whole structure: it puts every
//! ion at the centre of a regular octahedron of the *opposite* species, so the
//! **coordination number is 6** for both Na⁺ and Cl⁻, at a nearest-neighbour
//! distance of `a/2 = 2.82 Å`. (The nearest *like* neighbour is further away, at
//! `a/√2 = 3.99 Å`.) The sticks in this scene are drawn only between *unlike*
//! ions closer than 3.4 Å, so they trace exactly those octahedra — count the six
//! mutually perpendicular sticks on any interior ion.
//!
//! The spheres are drawn at **Shannon effective ionic radii**
//! ([`RadiusSource::Ionic`]), which is
//! the honest choice for a salt: Na⁺ has lost its whole 3s shell and shrinks to
//! 1.02 Å, while Cl⁻ gains an electron and swells to 1.81 Å. So the crystal
//! reads as it really is — a close-packed array of big green chloride ions with
//! small purple sodium ions tucked into its octahedral holes. (Neutral-atom
//! *covalent* radii would reverse this, drawing sodium at 1.66 Å against
//! chlorine's 1.02 Å, and give exactly the wrong intuition.) The yellow
//! wireframe is one unit cell from
//! [`Lattice::cell_edges`](manim_chem::lattice::Lattice::cell_edges), marking
//! out the cube that tiles the block.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-chem --example nacl_lattice --features render-examples
//! ```
//!
//! Frames land in `out/nacl_lattice/frame_NNNNN.png`.

use manim_chem::lattice::nacl;
use manim_chem::molecule::{Atom, Molecule};
use manim_chem::render::{self, BondRule, RadiusSource};
use manim_core::prelude::*;
use manim_render::export::VideoExporter;

/// Cells replicated along each axis. 2×2×2 is the sweet spot: 64 ions, enough
/// to close the octahedra in the interior without becoming an opaque brick.
const REPS: usize = 2;

/// Uniform shrink applied to the finished model. The 2×2×2 block is 11.3 Å on a
/// side, which would overflow the 14-unit frame once the turntable swings its
/// body diagonal (19.5 Å) across the screen.
const FIT: f32 = 0.55;

/// Scene builder for the rock-salt lattice turntable.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(68_f32.to_radians(), -40_f32.to_radians());

        let cell = nacl();
        let block = cell.replicate(REPS, REPS, REPS);
        // `replicate` tiles outward from the origin corner, so the block sits in
        // the +++ octant; recentre it on the origin the turntable orbits.
        let mid = block.centroid();
        // Recentre while preserving each ion's formal charge — `RadiusSource::Ionic`
        // reads it to pick Na⁺ vs Cl⁻ radii.
        let atoms: Vec<Atom> = block
            .atoms
            .iter()
            .map(|a| Atom {
                pos: a.pos - mid,
                ..a.clone()
            })
            .collect();

        // `BondRule::UnlikeOnly` keeps only Na–Cl contacts. The plain covalent
        // heuristic would also link Na–Na across the 3.99 Å face diagonal
        // (sodium's covalent radius is large), burying the structure in sticks;
        // requiring unlike, strongly-polarized partners leaves exactly the
        // octahedral a/2 = 2.82 Å contacts.
        let molecule = render::with_perceived_bonds_using(
            &Molecule {
                atoms,
                bonds: Vec::new(),
            },
            BondRule::UnlikeOnly,
        );
        let ions = render::ball_and_stick_sized(scene.state_mut(), &molecule, RadiusSource::Ionic);

        // One unit cell, translated by the same offset so it lands on the block's
        // origin-corner cell rather than floating beside it.
        let edges: Vec<AnyId> = cell
            .cell_edges()
            .into_iter()
            .map(|(from, to)| {
                let mut edge = Line3D::new(from - mid, to - mid);
                edge.set_stroke(YELLOW, 7.0, 1.0);
                scene.add(edge).erase()
            })
            .collect();
        let cage = VGroup::of(scene.state_mut(), edges);

        // Scale ions and cage together about the shared origin so they stay
        // registered with each other.
        let all = VGroup::of(scene.state_mut(), [ions.erase(), cage.erase()]);
        scene.state_mut().scale_about(all, FIT, Point::ZERO);

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
    VideoExporter::render_to_png_sequence(&mut scene, "out/nacl_lattice")?;
    println!("Rendered frames to out/nacl_lattice");
    Ok(())
}
