//! Two hydrogen orbitals side by side, drawn as **signed** isosurfaces.
//!
//! A bound state of hydrogen is `ψ_{nlm}(r, θ, φ) = R_{nl}(r)·Yₗᵐ(θ, φ)`: a
//! radial factor (associated Laguerre) times a real spherical harmonic. The
//! radial factor sets the *size* of the orbital; the harmonic sets its *shape*.
//! Each orbital here is rendered as the pair of level sets `ψ = +c` (blue) and
//! `ψ = −c` (red), so the surfaces show the **sign** of the wavefunction, not
//! just the probability density `|ψ|²`.
//!
//! On the left, `2p_z` (`n=2, l=1, m=0`): `Y₁⁰ ∝ cos θ`, so one lobe up, one
//! lobe down, separated by the single nodal plane `z = 0` where `ψ` vanishes.
//! On the right, `3d_xy` (`n=3, l=2, m=−2`): `Y₂⁻² ∝ sin²θ·sin 2φ`, giving four
//! alternating lobes in the `xy`-plane and *two* nodal planes (`x = 0`, `y = 0`).
//! The count generalizes: `Yₗᵐ` has `l − |m|` nodal cones plus `|m|` nodal planes.
//!
//! Why the sign matters: `|ψ|²` alone cannot explain chemistry. When two atoms
//! approach, lobes of *like* sign overlap constructively into a bonding orbital
//! and lobes of *opposite* sign cancel into an antibonding one. Squaring first
//! throws that information away — which is why this scene colors by sign.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-quantum --example hydrogen_orbitals --features render-examples
//! ```
//!
//! Frames land in `out/hydrogen_orbitals/frame_NNNNN.png`.

use glam::{Mat4, Quat, Vec3};

use manim_core::mesh::Mesh;
use manim_core::prelude::*;

use manim_quantum::eigenstates::orbital_isosurface;

/// Half-height of the faint `ẑ` guide drawn through each orbital.
const AXIS_HALF: f32 = 2.7;

/// Rescales and offsets an orbital group so both fit the same frame.
///
/// `orbital_isosurface` samples over `≈3n²` Bohr radii, so a `3d` orbital comes
/// out roughly `(3/2)² ≈ 2.3×` larger than a `2p` one in raw atomic units.
/// Isosurfaces are [`Mesh`]es, and mesh geometry lives outside the 2-D path that
/// `shift`/`scale` transform — so placement goes through [`Mesh::set_transform`].
fn place(scene: &mut Scene, group: MobjectId<VGroup>, scale: f32, x: f32) {
    let m = Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        Quat::IDENTITY,
        Vec3::new(x, 0.0, 0.0),
    );
    let children = scene.state().family(group);
    for child in children {
        if let Some(mesh) = scene
            .state_mut()
            .get_dyn_mut(child)
            .as_any_mut()
            .downcast_mut::<Mesh>()
        {
            mesh.set_transform(m);
        }
    }
}

/// A faint vertical line marking the `ẑ` quantization axis at `x`.
fn z_guide(scene: &mut Scene, x: f32) {
    let a = Point::new(x, 0.0, -AXIS_HALF);
    let b = Point::new(x, 0.0, AXIS_HALF);
    scene.add(Line::new(a, b).with_stroke(WHITE, 2.0, 0.35));
}

/// Signed isosurfaces of `2p_z` and `3d_xy` on a slow turntable.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(68_f32.to_radians(), -55_f32.to_radians());

        // 2p_z: peak |ψ| ≈ 0.073, so c = 0.045 cuts well inside each lobe.
        let p = orbital_isosurface(scene.state_mut(), 2, 1, 0, 0.045);
        place(scene, p, 0.42, -3.1);
        z_guide(scene, -3.1);

        // 3d_xy: peak |ψ| ≈ 0.024 — a diffuse orbital needs a lower contour.
        let d = orbital_isosurface(scene.state_mut(), 3, 2, -2, 0.012);
        place(scene, d, 0.20, 3.1);
        z_guide(scene, 3.1);

        // One full revolution: the p orbital's lobes stay end-on along ẑ while
        // the d orbital's four lobes sweep past in the xy-plane.
        scene.wait(0.4);
        for _ in 0..72 {
            scene.rotate_camera(TAU / 72.0);
            scene.wait(0.1);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/hydrogen_orbitals",
    )?;
    println!("Rendered frames to out/hydrogen_orbitals");
    Ok(())
}
