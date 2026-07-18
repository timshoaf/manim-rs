//! The Bloch sphere and single-qubit gates as SO(3) rotations.
//!
//! A pure qubit state is a unit **Bloch vector** in ℝ³. A [`BlochSphere`]
//! stores that vector and can drop a unit sphere plus a state arrow into a
//! scene. Single-qubit gates act on the Bloch vector as real rotations
//! ([`Gate`], [`gate_rotation`]): on the sphere the global phase drops out, so
//! identities like `HZH = X` hold *exactly* as SO(3) matrices.
//!
//! ```
//! use manim_quantum::bloch::{BlochSphere, Gate};
//! use glam::DVec3;
//! // |0⟩ is the north pole; X (a π flip about x) sends it to the south pole.
//! let mut q = BlochSphere::new();
//! q.apply_gate(Gate::X);
//! assert!((q.state() - DVec3::new(0.0, 0.0, -1.0)).length() < 1e-12);
//! ```

use std::f64::consts::FRAC_1_SQRT_2;

use glam::{DMat3, DVec3};
use manim_core::geometry::VGroup;
use manim_core::mesh::Mesh;
use manim_core::mobject::MobjectId;
use manim_core::scene_state::SceneState;
use manim_core::threed::Arrow3D;
use manim_math::{Point, ORIGIN};

/// A single-qubit gate, as its action on the Bloch vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gate {
    /// Pauli-X: a π rotation about the x-axis.
    X,
    /// Pauli-Y: a π rotation about the y-axis.
    Y,
    /// Pauli-Z: a π rotation about the z-axis.
    Z,
    /// Hadamard: a π rotation about the `(x + z)/√2` axis.
    H,
    /// Phase gate: a rotation by the given angle (radians) about the z-axis.
    Phase(f64),
}

/// The rotation matrix for `angle` (radians) about the unit vector `axis`
/// (Rodrigues' formula). The result maps a column vector `v` to `R·v`.
fn rotation_about_axis(axis: DVec3, angle: f64) -> DMat3 {
    let n = axis.normalize();
    let (s, c) = angle.sin_cos();
    let d = 1.0 - c;
    let (x, y, z) = (n.x, n.y, n.z);
    // Row-major entries of the standard rotation matrix…
    let r00 = c + x * x * d;
    let r01 = x * y * d - z * s;
    let r02 = x * z * d + y * s;
    let r10 = y * x * d + z * s;
    let r11 = c + y * y * d;
    let r12 = y * z * d - x * s;
    let r20 = z * x * d - y * s;
    let r21 = z * y * d + x * s;
    let r22 = c + z * z * d;
    // …assembled as glam columns so that `mat * v` = `R·v`.
    DMat3::from_cols(
        DVec3::new(r00, r10, r20),
        DVec3::new(r01, r11, r21),
        DVec3::new(r02, r12, r22),
    )
}

/// The SO(3) rotation a [`Gate`] applies to the Bloch vector.
///
/// ```
/// use manim_quantum::bloch::{gate_rotation, Gate};
/// use glam::DVec3;
/// // X flips z → −z.
/// let r = gate_rotation(Gate::X);
/// assert!((r * DVec3::Z - DVec3::new(0.0, 0.0, -1.0)).length() < 1e-12);
/// ```
pub fn gate_rotation(g: Gate) -> DMat3 {
    use std::f64::consts::PI;
    match g {
        Gate::X => rotation_about_axis(DVec3::X, PI),
        Gate::Y => rotation_about_axis(DVec3::Y, PI),
        Gate::Z => rotation_about_axis(DVec3::Z, PI),
        Gate::H => rotation_about_axis(DVec3::new(FRAC_1_SQRT_2, 0.0, FRAC_1_SQRT_2), PI),
        Gate::Phase(theta) => rotation_about_axis(DVec3::Z, theta),
    }
}

/// A Bloch sphere carrying a single qubit's state as a unit Bloch vector.
#[derive(Clone, Copy, Debug)]
pub struct BlochSphere {
    state: DVec3,
}

impl Default for BlochSphere {
    fn default() -> Self {
        Self::new()
    }
}

impl BlochSphere {
    /// A fresh sphere in the state `|0⟩` (Bloch vector `+ẑ`, the north pole).
    pub fn new() -> Self {
        Self { state: DVec3::Z }
    }

    /// A sphere whose state is the given Bloch vector (normalized).
    ///
    /// ```
    /// use manim_quantum::bloch::BlochSphere;
    /// use glam::DVec3;
    /// let q = BlochSphere::from_vector(DVec3::new(2.0, 0.0, 0.0));
    /// assert!((q.state() - DVec3::X).length() < 1e-12);
    /// ```
    pub fn from_vector(v: DVec3) -> Self {
        Self {
            state: v.normalize_or(DVec3::Z),
        }
    }

    /// A sphere from spherical angles: Bloch vector
    /// `(sinθ cosφ, sinθ sinφ, cosθ)`.
    ///
    /// ```
    /// use manim_quantum::bloch::BlochSphere;
    /// use glam::DVec3;
    /// use std::f64::consts::PI;
    /// // θ = π/2, φ = 0 is the |+⟩ state on the +x axis.
    /// let q = BlochSphere::from_angles(PI / 2.0, 0.0);
    /// assert!((q.state() - DVec3::X).length() < 1e-12);
    /// ```
    pub fn from_angles(theta: f64, phi: f64) -> Self {
        let (st, ct) = theta.sin_cos();
        let (sp, cp) = phi.sin_cos();
        Self {
            state: DVec3::new(st * cp, st * sp, ct),
        }
    }

    /// The current Bloch vector.
    pub fn state(&self) -> DVec3 {
        self.state
    }

    /// Applies `gate` to the state (rotating the Bloch vector) and returns the
    /// SO(3) matrix that was applied, so a scene can animate the same rotation
    /// (e.g. via [`SceneState::rotate_about`] about [`ORIGIN`] and the
    /// rotation's axis).
    ///
    /// ```
    /// use manim_quantum::bloch::{BlochSphere, Gate};
    /// use glam::DVec3;
    /// let mut q = BlochSphere::new();
    /// let _r = q.apply_gate(Gate::X); // |0⟩ → |1⟩
    /// assert!((q.state() - DVec3::new(0.0, 0.0, -1.0)).length() < 1e-12);
    /// ```
    pub fn apply_gate(&mut self, gate: Gate) -> DMat3 {
        let r = gate_rotation(gate);
        self.state = r * self.state;
        r
    }

    /// Adds a unit sphere and the state arrow (from the center to the Bloch
    /// vector) to `scene`, returning the group.
    ///
    /// ```
    /// use manim_core::scene_state::SceneState;
    /// use manim_core::mobject::MobjectExt;
    /// use manim_quantum::bloch::BlochSphere;
    /// let mut scene = SceneState::new();
    /// let q = BlochSphere::new();
    /// let g = q.add_to(&mut scene);
    /// // The group holds the sphere mesh and the arrow's shaft + tip faces.
    /// assert!(scene.family(g.erase()).len() > 2);
    /// ```
    pub fn add_to(&self, scene: &mut SceneState) -> MobjectId<VGroup> {
        let sphere = scene.add(Mesh::sphere()).erase();
        let tip = Point::new(
            self.state.x as f32,
            self.state.y as f32,
            self.state.z as f32,
        );
        let arrow = Arrow3D::of(scene, ORIGIN, tip).erase();
        VGroup::of(scene, [sphere, arrow])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Largest entrywise difference between two 3×3 matrices.
    fn max_diff(a: DMat3, b: DMat3) -> f64 {
        let (a, b) = (a.to_cols_array(), b.to_cols_array());
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0_f64, f64::max)
    }

    #[test]
    fn hzh_equals_x() {
        // On the Bloch sphere / SO(3) the global phase is gone: HZH = X exactly.
        let hzh = gate_rotation(Gate::H) * gate_rotation(Gate::Z) * gate_rotation(Gate::H);
        let diff = max_diff(hzh, gate_rotation(Gate::X));
        println!("max|HZH − X| = {diff:.2e}");
        assert!(diff < 1e-6, "HZH ≠ X: {diff}");
    }

    #[test]
    fn involutions() {
        // H² = I, X² = I.
        let id = DMat3::IDENTITY;
        let hh = gate_rotation(Gate::H) * gate_rotation(Gate::H);
        let xx = gate_rotation(Gate::X) * gate_rotation(Gate::X);
        assert!(max_diff(hh, id) < 1e-9, "H² ≠ I");
        assert!(max_diff(xx, id) < 1e-9, "X² ≠ I");
    }

    #[test]
    fn hadamard_maps_z_to_x() {
        // H takes |0⟩ (+ẑ) to |+⟩ (+x̂).
        let mut q = BlochSphere::new();
        q.apply_gate(Gate::H);
        assert!((q.state() - DVec3::X).length() < 1e-12);
    }
}
