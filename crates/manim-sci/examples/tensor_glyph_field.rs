//! A diffusion-tensor field drawn as ellipsoid glyphs — the glyph *is* the tensor.
//!
//! A symmetric tensor `D = Dᵀ` at a point has a spectral decomposition
//! `D = Σₖ λₖ vₖ vₖᵀ` with orthonormal eigenvectors `vₖ` and real eigenvalues
//! `λ₁ ≥ λ₂ ≥ λ₃`. The natural picture is the ellipsoid `{x : xᵀD⁻²x = 1}`: its
//! **axes point along the eigenvectors** and its **radii are the eigenvalues**.
//! Nothing is lost — the glyph carries all six independent components.
//!
//! This field models diffusion around a spherical inclusion at the origin:
//!
//! ```text
//! D(r) = ε·I + w(r)·(r̂ ⊗ r̂),    w(r) = exp(−r²/σ²)
//! ```
//!
//! Near the inclusion `w ≈ 1` and the tensor is strongly **prolate**: one large
//! eigenvalue along the radial direction `r̂` and two small ones, so the glyph is
//! a cigar pointing at the origin. Far away `w → 0` and `D → ε·I`, whose three
//! eigenvalues are equal — an **isotropic** sphere, with the eigenvector frame
//! degenerate (the solver's Gram–Schmidt completion keeps it well-defined).
//! Sweeping outward across the 5×5×3 grid, the viewer watches cigars relax into
//! spheres, which is precisely the fractional-anisotropy gradient that diffusion
//! MRI measures.
//!
//! Components are packed as `[xx, xy, xz, yy, yz, zz]` (upper triangle, row-major).
//!
//! Run with:
//!
//! ```sh
//! cargo run -p manim-sci --example tensor_glyph_field --features render-examples
//! ```
//!
//! Frames land in `out/tensor_glyph_field/frame_NNNNN.png`.

use std::f64::consts::TAU;

use glam::DVec3;

use manim_core::prelude::*;
use manim_sci::vector_field_3d::tensor_glyphs;

/// Isotropic floor `ε` and anisotropy falloff `σ`.
const EPS: f64 = 0.28;
const SIGMA: f64 = 1.6;

/// `D(r) = ε·I + exp(−r²/σ²)·(r̂ ⊗ r̂)`, packed `[xx, xy, xz, yy, yz, zz]`.
fn diffusion_tensor(p: DVec3) -> [f64; 6] {
    let r2 = p.length_squared();
    // Radial unit vector; at the origin fall back to isotropic (w·r̂⊗r̂ = 0).
    let d = if r2 > 1e-12 {
        p / r2.sqrt()
    } else {
        DVec3::ZERO
    };
    let w = (-r2 / (SIGMA * SIGMA)).exp();
    [
        EPS + w * d.x * d.x,
        w * d.x * d.y,
        w * d.x * d.z,
        EPS + w * d.y * d.y,
        w * d.y * d.z,
        EPS + w * d.z * d.z,
    ]
}

/// Scene builder for the `tensor_glyph_field` gallery example.
pub struct Demo;

impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        scene.set_camera_orientation(68_f32.to_radians(), -45_f32.to_radians());

        // 5×5×3 = 75 sample sites spanning the inclusion and its isotropic surround.
        let mut grid = Vec::with_capacity(75);
        for i in 0..5 {
            for j in 0..5 {
                for k in 0..3 {
                    grid.push(DVec3::new(
                        -2.4 + 1.2 * i as f64,
                        -2.4 + 1.2 * j as f64,
                        -1.2 + 1.2 * k as f64,
                    ));
                }
            }
        }

        // One ellipsoid per site; `scale` maps eigenvalues to scene-unit radii.
        tensor_glyphs(scene.state_mut(), &diffusion_tensor, &grid, 0.55);

        // Turntable: the radial cigars near the centre only read as pointing at
        // the origin once the scene turns.
        for _ in 0..100 {
            scene.rotate_camera(TAU as f32 / 100.0);
            scene.wait(0.06);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::build(&Demo, Config::low())?;
    manim_render::export::VideoExporter::render_to_png_sequence(
        &mut scene,
        "out/tensor_glyph_field",
    )?;
    println!("Rendered frames to out/tensor_glyph_field");
    Ok(())
}
