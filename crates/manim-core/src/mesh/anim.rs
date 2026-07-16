//! Mesh-aware animations: [`MorphMesh`] and [`MorphSurface`].
//!
//! # Why not `Transform`?
//!
//! [`Transform`](crate::animations::Transform) morphs *paths*: it aligns two
//! mobjects' bezier point counts and lerps them. A mesh mobject's path is only
//! its model transform (see the [module docs](super)), so `Transform` between two
//! meshes would tween placement and leave the geometry alone — silently doing a
//! quarter of the job. These two animations are the mesh-side equivalents, and
//! they tween both the geometry and the transform.
//!
//! ```
//! use manim_core::prelude::*;
//! use manim_core::mesh::{Mesh, MorphMesh, TriMesh};
//!
//! let mut scene = Scene::new(Config::default());
//! let flat = scene.add(Mesh::grid(8, 8));
//! // Morph the flat grid into a saddle.
//! let saddle = TriMesh::from_parametric(
//!     |u, v| glam::Vec3::new(u as f32, v as f32, (u * u - v * v) as f32),
//!     (-0.5, 0.5),
//!     (-0.5, 0.5),
//!     (8, 8),
//! );
//! scene.play(MorphMesh::into(flat, Mesh::new(saddle))).unwrap();
//! assert!(scene[flat].mesh().positions.iter().any(|p| p.z != 0.0));
//! ```

use std::sync::Arc;

use glam::{Mat4, Vec3};

use super::{Mesh, ParametricFn, Surface3D, TriMesh};
use crate::animation::{anim_builders, anim_config_accessors, AnimConfig, Animation};
use crate::mobject::MobjectId;
use crate::scene_state::SceneState;

/// Interpolates two model matrices component-wise.
///
/// Adequate for the transform *accompanying* a vertex morph — the geometry is
/// doing the visible work. A rotation-heavy tween wants
/// [`Rotate`](crate::animations::Rotate) instead, which slerps properly.
fn lerp_mat4(a: Mat4, b: Mat4, t: f32) -> Mat4 {
    Mat4::from_cols(
        a.x_axis.lerp(b.x_axis, t),
        a.y_axis.lerp(b.y_axis, t),
        a.z_axis.lerp(b.z_axis, t),
        a.w_axis.lerp(b.w_axis, t),
    )
}

/// The end state a [`MorphMesh`] drives toward.
struct MeshState {
    mesh: Arc<TriMesh>,
    transform: Mat4,
}

impl MeshState {
    fn of(mesh: &Mesh) -> Self {
        Self {
            mesh: Arc::clone(mesh.mesh_arc()),
            transform: mesh.transform(),
        }
    }
}

/// Morphs a [`Mesh`] into another mesh's geometry and placement by
/// [`TriMesh::lerp`].
///
/// The two meshes must share an index buffer — same topology. If they do not,
/// the geometry snaps to the target at the end and only the transform tweens;
/// build both sides from the same builder at the same resolution to avoid that.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::mesh::{Mesh, MorphMesh};
///
/// let mut scene = Scene::new(Config::default());
/// let a = scene.add(Mesh::grid(4, 4));
/// let b = scene.add(Mesh::grid(4, 4).with_shift(4.0 * RIGHT));
/// scene.play(MorphMesh::new(a, b)).unwrap();
/// // `a` has arrived at `b`; `b` is untouched.
/// assert!((scene[a].get_center() - 4.0 * RIGHT).length() < 1e-4);
/// ```
pub struct MorphMesh {
    source: MobjectId<Mesh>,
    /// The target mobject, for the scene-to-scene form; `None` for the free form.
    target: Option<MobjectId<Mesh>>,
    /// The free target, resolved at `begin` for the scene-to-scene form.
    end: Option<MeshState>,
    start: Option<MeshState>,
    config: AnimConfig,
}
anim_builders!(MorphMesh);

impl MorphMesh {
    /// Morphs `source` into `target`'s current geometry and placement, leaving
    /// `target` untouched (manim CE's `Transform`, for meshes).
    pub fn new(source: MobjectId<Mesh>, target: MobjectId<Mesh>) -> Self {
        Self {
            source,
            target: Some(target),
            end: None,
            start: None,
            config: AnimConfig::default(),
        }
    }

    /// Morphs `source` into a free (not-yet-added) [`Mesh`]'s geometry and
    /// placement (manim CE's `Transform` with a target mobject, for meshes).
    pub fn into(source: MobjectId<Mesh>, target: Mesh) -> Self {
        Self {
            source,
            target: None,
            end: Some(MeshState::of(&target)),
            start: None,
            config: AnimConfig::default(),
        }
    }
}

impl Animation for MorphMesh {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = state.try_get(self.source).map(MeshState::of);
        if let Some(target) = self.target {
            self.end = state.try_get(target).map(MeshState::of);
        }
    }

    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let (Some(start), Some(end)) = (&self.start, &self.end) else {
            return;
        };
        let Some(mesh) = state.try_get_mut(self.source) else {
            return;
        };
        match TriMesh::lerp(&start.mesh, &end.mesh, alpha) {
            Ok(blended) => {
                mesh.set_mesh(blended);
            }
            // Mismatched topology: hold the start geometry, then land on the
            // target at the end, so the animation still terminates correctly.
            Err(_) if alpha >= 1.0 => {
                mesh.set_mesh(Arc::clone(&end.mesh));
            }
            Err(_) => {}
        }
        mesh.set_transform(lerp_mat4(start.transform, end.transform, alpha));
    }

    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }

    anim_config_accessors!();
}

/// One end of a [`MorphSurface`]: a parameterization plus its domain.
#[derive(Clone)]
struct SurfaceParams {
    f: ParametricFn,
    u_range: (f64, f64),
    v_range: (f64, f64),
}

impl SurfaceParams {
    fn of(surface: &Surface3D) -> Self {
        Self {
            f: Arc::clone(surface.parametric()),
            u_range: surface.u_range(),
            v_range: surface.v_range(),
        }
    }
}

/// Morphs a [`Surface3D`] into another parameterization by tweening in
/// **parameter space**: each grid point moves from `f₀(u, v)` to `f₁(u, v)`.
///
/// This is the homeomorphism/isotopy animation. Because both sides are evaluated
/// on the same `(u, v)` grid, there is no correspondence problem to solve — the
/// surface deforms continuously and stays correctly meshed and normaled
/// throughout. The `u`/`v` ranges tween too, and the source's resolution is kept.
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::mesh::{Surface3D, MorphSurface};
/// use glam::Vec3;
///
/// let mut scene = Scene::new(Config::default());
/// // A flat sheet …
/// let sheet = scene.add(
///     Surface3D::new(|u, v| Vec3::new(u as f32, v as f32, 0.0), (-1.0, 1.0), (-1.0, 1.0))
///         .with_resolution(8, 8),
/// );
/// // … bending into a saddle.
/// scene.play(MorphSurface::new(
///     sheet,
///     |u, v| Vec3::new(u as f32, v as f32, (u * u - v * v) as f32),
///     (-1.0, 1.0),
///     (-1.0, 1.0),
/// ))
/// .unwrap();
/// assert!(scene[sheet].mesh().positions.iter().any(|p| p.z.abs() > 0.1));
/// ```
pub struct MorphSurface {
    source: MobjectId<Surface3D>,
    target: SurfaceParams,
    start: Option<SurfaceParams>,
    config: AnimConfig,
}
anim_builders!(MorphSurface);

impl MorphSurface {
    /// Morphs `source` into the parameterization `f` over `u_range × v_range`.
    pub fn new(
        source: MobjectId<Surface3D>,
        f: impl Fn(f64, f64) -> Vec3 + Send + Sync + 'static,
        u_range: (f64, f64),
        v_range: (f64, f64),
    ) -> Self {
        Self::from_arc(source, Arc::new(f), u_range, v_range)
    }

    /// Morphs `source` into an already-shared parameterization — e.g. another
    /// surface's, via [`Surface3D::parametric`].
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::mesh::{Surface3D, MorphSurface};
    /// use glam::Vec3;
    /// let mut scene = Scene::new(Config::default());
    /// let a = scene.add(Surface3D::new(
    ///     |u, v| Vec3::new(u as f32, v as f32, 0.0), (0.0, 1.0), (0.0, 1.0),
    /// ).with_resolution(4, 4));
    /// let b = Surface3D::new(|u, v| Vec3::new(u as f32, v as f32, 1.0), (0.0, 1.0), (0.0, 1.0));
    /// let anim = MorphSurface::from_arc(a, b.parametric().clone(), b.u_range(), b.v_range());
    /// scene.play(anim).unwrap();
    /// assert!(scene[a].mesh().positions.iter().all(|p| (p.z - 1.0).abs() < 1e-4));
    /// ```
    pub fn from_arc(
        source: MobjectId<Surface3D>,
        f: ParametricFn,
        u_range: (f64, f64),
        v_range: (f64, f64),
    ) -> Self {
        Self {
            source,
            target: SurfaceParams {
                f,
                u_range,
                v_range,
            },
            start: None,
            config: AnimConfig::default(),
        }
    }
}

impl Animation for MorphSurface {
    fn begin(&mut self, state: &mut SceneState) {
        self.start = state.try_get(self.source).map(SurfaceParams::of);
    }

    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        let Some(start) = self.start.clone() else {
            return;
        };
        let (f0, u0, v0) = (start.f, start.u_range, start.v_range);
        let (f1, u1, v1) = (
            Arc::clone(&self.target.f),
            self.target.u_range,
            self.target.v_range,
        );
        // Both sides are re-parameterized onto the *current* ranges, so a tween
        // whose endpoints have different domains stays continuous.
        let (u, v) = (lerp_range(u0, u1, alpha), lerp_range(v0, v1, alpha));
        let t = alpha as f64;
        let blended = move |cu: f64, cv: f64| {
            // Where this (u, v) sits proportionally in the current domain …
            let (pu, pv) = (unlerp(u, cu), unlerp(v, cv));
            // … is where it is read from in each endpoint's own domain.
            let a = f0(lerp_f64(u0, pu), lerp_f64(v0, pv));
            let b = f1(lerp_f64(u1, pu), lerp_f64(v1, pv));
            a.lerp(b, t as f32)
        };
        if let Some(surface) = state.try_get_mut(self.source) {
            surface.set_ranges(u, v);
            surface.set_parametric(blended);
        }
    }

    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
        // Land on the target function itself rather than a blend at t = 1, so
        // the surface keeps tweening cleanly if it is animated again.
        if let Some(surface) = state.try_get_mut(self.source) {
            surface.set_ranges(self.target.u_range, self.target.v_range);
            surface.set_parametric_arc(Arc::clone(&self.target.f));
        }
    }

    anim_config_accessors!();
}

/// Interpolates a `(start, end)` range pair.
fn lerp_range(a: (f64, f64), b: (f64, f64), t: f32) -> (f64, f64) {
    let t = t as f64;
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// The value at proportion `t` through `range`.
fn lerp_f64(range: (f64, f64), t: f64) -> f64 {
    range.0 + (range.1 - range.0) * t
}

/// The proportion of `value` through `range`; 0 for a degenerate range.
fn unlerp(range: (f64, f64), value: f64) -> f64 {
    let span = range.1 - range.0;
    if span.abs() < 1e-12 {
        0.0
    } else {
        (value - range.0) / span
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::Animation;
    use crate::mobject::{Buildable, MobjectExt};
    use crate::scene_state::SceneState;
    use manim_math::RIGHT;

    fn saddle_mesh() -> TriMesh {
        TriMesh::from_parametric(
            |u, v| Vec3::new(u as f32, v as f32, (u * u - v * v) as f32),
            (-0.5, 0.5),
            (-0.5, 0.5),
            (4, 4),
        )
    }

    fn drive(anim: &mut impl Animation, state: &mut SceneState, alpha: f32) {
        anim.begin(state);
        anim.interpolate(state, alpha);
    }

    #[test]
    fn morph_mesh_midpoint_is_halfway() {
        let mut scene = SceneState::new();
        let a = scene.add(Mesh::grid(4, 4));
        let target = Mesh::new(saddle_mesh());
        let expect = target.mesh().positions.clone();

        let mut anim = MorphMesh::into(a, target);
        drive(&mut anim, &mut scene, 0.5);

        for (i, p) in scene[a].mesh().positions.iter().enumerate() {
            assert!((p.z - expect[i].z * 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn morph_mesh_finishes_on_the_target() {
        let mut scene = SceneState::new();
        let a = scene.add(Mesh::grid(4, 4));
        let target = Mesh::new(saddle_mesh());
        let expect = target.mesh().positions.clone();

        let mut anim = MorphMesh::into(a, target);
        anim.begin(&mut scene);
        anim.finish(&mut scene);
        assert_eq!(scene[a].mesh().positions, expect);
    }

    #[test]
    fn morph_mesh_tweens_the_transform_and_spares_the_target() {
        let mut scene = SceneState::new();
        let a = scene.add(Mesh::grid(4, 4));
        let b = scene.add(Mesh::grid(4, 4).with_shift(4.0 * RIGHT));

        let mut anim = MorphMesh::new(a, b);
        drive(&mut anim, &mut scene, 0.5);
        assert!((scene[a].get_center().x - 2.0).abs() < 1e-4);
        // The target never moves.
        assert!((scene[b].get_center().x - 4.0).abs() < 1e-4);
    }

    /// Mismatched topology must not corrupt the mesh or hang the animation.
    #[test]
    fn morph_mesh_with_mismatched_topology_still_lands_on_the_target() {
        let mut scene = SceneState::new();
        let a = scene.add(Mesh::grid(2, 2));
        let target = Mesh::grid(8, 8);
        let expect = target.mesh().len();

        let mut anim = MorphMesh::into(a, target);
        anim.begin(&mut scene);
        anim.interpolate(&mut scene, 0.5);
        // Mid-flight it holds the start geometry rather than a corrupt blend.
        assert_eq!(scene[a].mesh().len(), 9);
        anim.finish(&mut scene);
        assert_eq!(scene[a].mesh().len(), expect);
    }

    #[test]
    fn morph_surface_tweens_in_parameter_space() {
        let mut scene = SceneState::new();
        let s = scene.add(
            Surface3D::new(
                |u, v| Vec3::new(u as f32, v as f32, 0.0),
                (0.0, 1.0),
                (0.0, 1.0),
            )
            .with_resolution(4, 4),
        );

        let mut anim = MorphSurface::new(
            s,
            |u, v| Vec3::new(u as f32, v as f32, 2.0),
            (0.0, 1.0),
            (0.0, 1.0),
        );
        drive(&mut anim, &mut scene, 0.5);
        // Halfway: every point has risen half the way to z = 2.
        assert!(scene[s]
            .mesh()
            .positions
            .iter()
            .all(|p| (p.z - 1.0).abs() < 1e-5));

        anim.finish(&mut scene);
        assert!(scene[s]
            .mesh()
            .positions
            .iter()
            .all(|p| (p.z - 2.0).abs() < 1e-5));
    }

    #[test]
    fn morph_surface_keeps_topology_throughout() {
        let mut scene = SceneState::new();
        let s = scene.add(
            Surface3D::new(
                |u, v| Vec3::new(u as f32, v as f32, 0.0),
                (0.0, 1.0),
                (0.0, 1.0),
            )
            .with_resolution(4, 4),
        );
        let indices = scene[s].mesh().indices.clone();

        let mut anim = MorphSurface::new(
            s,
            |u, v| Vec3::new((u * 2.0) as f32, v as f32, (u * v) as f32),
            (0.0, 2.0),
            (0.0, 1.0),
        );
        anim.begin(&mut scene);
        for step in 0..=4 {
            anim.interpolate(&mut scene, step as f32 / 4.0);
            assert_eq!(scene[s].mesh().indices, indices);
            assert_eq!(scene[s].resolution(), (4, 4));
        }
    }

    /// Endpoints with different domains must still tween continuously.
    #[test]
    fn morph_surface_tweens_ranges() {
        let mut scene = SceneState::new();
        let s = scene.add(
            Surface3D::new(
                |u, _v| Vec3::new(u as f32, 0.0, 0.0),
                (0.0, 1.0),
                (0.0, 1.0),
            )
            .with_resolution(2, 2),
        );
        let mut anim = MorphSurface::new(
            s,
            |u, _v| Vec3::new(u as f32, 0.0, 0.0),
            (0.0, 3.0),
            (0.0, 1.0),
        );
        anim.begin(&mut scene);
        anim.interpolate(&mut scene, 0.5);
        assert_eq!(scene[s].u_range(), (0.0, 2.0));
        // The far edge is halfway between u = 1 and u = 3.
        let max_x = scene[s]
            .mesh()
            .positions
            .iter()
            .fold(f32::MIN, |m, p| m.max(p.x));
        assert!((max_x - 2.0).abs() < 1e-4, "{max_x}");
    }
}
