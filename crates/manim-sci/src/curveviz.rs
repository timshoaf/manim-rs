//! Curvature visualization and swept tubes.
//!
//! - [`surface_colored_by_curvature`] bakes a surface's Gaussian or mean
//!   curvature into per-vertex colors through a [`Colormap`].
//! - [`TubeMesh::along_curve`] sweeps a circular cross-section along a space
//!   curve using a **rotation-minimizing frame** (not the raw Frenet frame),
//!   which stays well-defined through inflection points where the Frenet normal
//!   flips.
//! - [`TubeMesh::along_polyline`] does the same for an already-sampled polyline
//!   — a traced geodesic, an integrated field line — where there is no
//!   closed-form curve to differentiate.
//! - [`SpaceCurve`] is the mobject-level builder over those: it puts a curve in
//!   a scene as a **depth-tested tube** by default, so a curve drawn on a
//!   surface is occluded by it instead of floating over it, with a
//!   [`flat`](SpaceCurve::flat) opt-out back to a 2-D stroke.
//! - [`trefoil`] / [`figure_eight`] are ready-made knot curves.

use glam::{DVec3, Vec3};

use manim_core::display::Colormap;
// The full palette, reached through manim-core's re-export rather than a
// direct `manim-color` dependency.
use manim_core::manim_color::{Color, WHITE};
use manim_core::mesh::{Mesh, TriMesh};
use manim_core::mobject::MobjectId;
use manim_core::scene_state::SceneState;

use manim_fields::ad::Scalar;

use crate::diffgeo::{
    frenet_frame, gaussian_curvature, mean_curvature, normal, CurveSampler, SurfaceSampler,
};

/// Which curvature scalar to visualize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurvatureKind {
    /// Gaussian curvature `K = κ₁κ₂`.
    Gaussian,
    /// Mean curvature `H = (κ₁+κ₂)/2`.
    Mean,
}

fn to_vec3(p: DVec3) -> Vec3 {
    p.as_vec3()
}

fn sample_position<Sf: SurfaceSampler>(s: &Sf, u: f64, v: f64) -> DVec3 {
    let [x, y, z] = s.eval::<f64>(u, v);
    DVec3::new(x, y, z)
}

/// Builds a triangulated surface mesh colored by curvature: samples an
/// `nu × nv` grid over `u_range × v_range`, evaluates the chosen curvature at
/// each vertex, and maps it through `colormap` (auto-ranged to the sampled
/// min/max).
///
/// ```
/// use manim_core::prelude::*;
/// use manim_core::display::Colormap;
/// use manim_fields::ad::Scalar;
/// use manim_sci::curveviz::{surface_colored_by_curvature, CurvatureKind};
/// use manim_sci::diffgeo::SurfaceSampler;
/// // A torus surface.
/// struct Torus;
/// impl SurfaceSampler for Torus {
///     fn eval<S: Scalar>(&self, u: S, v: S) -> [S; 3] {
///         let r = S::constant(1.0) + u.cos().scale(0.4);
///         [r * v.cos(), r * v.sin(), u.sin().scale(0.4)]
///     }
/// }
/// let mut scene = Scene::new(Config::default());
/// let m = surface_colored_by_curvature(
///     scene.state_mut(), &Torus, CurvatureKind::Gaussian, Colormap::Coolwarm,
///     (0.0, std::f64::consts::TAU), (0.0, std::f64::consts::TAU), (24, 24));
/// assert!(scene.state().contains(m));
/// ```
pub fn surface_colored_by_curvature<Sf: SurfaceSampler>(
    scene: &mut SceneState,
    sampler: &Sf,
    kind: CurvatureKind,
    colormap: Colormap,
    u_range: (f64, f64),
    v_range: (f64, f64),
    resolution: (usize, usize),
) -> MobjectId<Mesh> {
    let (nu, nv) = (resolution.0.max(1), resolution.1.max(1));
    let (mut positions, mut normals, mut values) = (Vec::new(), Vec::new(), Vec::new());

    for i in 0..=nu {
        let u = u_range.0 + (u_range.1 - u_range.0) * i as f64 / nu as f64;
        for j in 0..=nv {
            let v = v_range.0 + (v_range.1 - v_range.0) * j as f64 / nv as f64;
            positions.push(to_vec3(sample_position(sampler, u, v)));
            normals.push(to_vec3(normal(sampler, u, v).normalize()));
            values.push(match kind {
                CurvatureKind::Gaussian => gaussian_curvature(sampler, u, v),
                CurvatureKind::Mean => mean_curvature(sampler, u, v),
            });
        }
    }

    // Auto-range the colormap to the sampled curvature extent.
    let vmin = values.iter().copied().fold(f64::INFINITY, f64::min);
    let vmax = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (vmax - vmin).max(1e-12);
    let colors = values
        .iter()
        .map(|&k| colormap.sample(((k - vmin) / span) as f32))
        .collect();

    let mut indices = Vec::with_capacity(nu * nv * 6);
    let idx = |i: usize, j: usize| (i * (nv + 1) + j) as u32;
    for i in 0..nu {
        for j in 0..nv {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }

    let mesh = TriMesh {
        positions,
        normals,
        colors: Some(colors),
        uvs: None,
        indices,
    };
    scene.add(Mesh::new(mesh))
}

/// A tube swept along a space curve.
pub struct TubeMesh;

impl TubeMesh {
    /// Sweeps a circle of `radius` along `curve` over `t_range`, using a
    /// rotation-minimizing frame propagated by the double-reflection method
    /// (Wang et al. 2008) — robust through inflection points, unlike the Frenet
    /// frame. `n_along` rings × `n_around` sides. `closed` welds the last ring to
    /// the first (for knots / closed loops).
    ///
    /// ```
    /// use manim_fields::ad::Scalar;
    /// use manim_sci::curveviz::TubeMesh;
    /// use manim_sci::diffgeo::CurveSampler;
    /// // A circle.
    /// struct Circle;
    /// impl CurveSampler for Circle {
    ///     fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
    ///         [t.cos(), t.sin(), S::constant(0.0)]
    ///     }
    /// }
    /// let tube = TubeMesh::along_curve(&Circle, (0.0, std::f64::consts::TAU), 0.1, 40, 12, true);
    /// assert!(!tube.positions.is_empty() && !tube.indices.is_empty());
    /// ```
    pub fn along_curve<C: CurveSampler>(
        curve: &C,
        t_range: (f64, f64),
        radius: f64,
        n_along: usize,
        n_around: usize,
        closed: bool,
    ) -> TriMesh {
        let n_along = n_along.max(2);
        let n_around = n_around.max(3);

        // Sample centre points and unit tangents.
        let mut centers = Vec::with_capacity(n_along);
        let mut tangents = Vec::with_capacity(n_along);
        for i in 0..n_along {
            let t = t_range.0 + (t_range.1 - t_range.0) * i as f64 / (n_along - 1) as f64;
            let [x, y, z] = curve.eval::<f64>(t);
            centers.push(DVec3::new(x, y, z));
            tangents.push(frenet_frame(curve, t).t);
        }

        // Rotation-minimizing frame: seed a normal ⟂ the first tangent, then
        // propagate by double reflection.
        let mut nrm = seed_normal(tangents[0]);
        let mut frames = Vec::with_capacity(n_along);
        frames.push(nrm);
        for i in 1..n_along {
            nrm = rmf_step(
                centers[i - 1],
                centers[i],
                tangents[i - 1],
                tangents[i],
                nrm,
            );
            frames.push(nrm);
        }

        // Ring vertices + outward normals.
        let mut positions = Vec::with_capacity(n_along * n_around);
        let mut vnormals = Vec::with_capacity(n_along * n_around);
        for i in 0..n_along {
            let n = frames[i];
            let b = tangents[i].cross(n).normalize();
            for j in 0..n_around {
                let theta = std::f64::consts::TAU * j as f64 / n_around as f64;
                let dir = n * theta.cos() + b * theta.sin();
                positions.push(to_vec3(centers[i] + dir * radius));
                vnormals.push(to_vec3(dir));
            }
        }

        // Triangulate quads between consecutive rings.
        let ring_count = if closed { n_along } else { n_along - 1 };
        let mut indices = Vec::with_capacity(ring_count * n_around * 6);
        for i in 0..ring_count {
            let i1 = (i + 1) % n_along;
            for j in 0..n_around {
                let j1 = (j + 1) % n_around;
                let a = (i * n_around + j) as u32;
                let b = (i1 * n_around + j) as u32;
                let c = (i1 * n_around + j1) as u32;
                let d = (i * n_around + j1) as u32;
                indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }

        TriMesh {
            positions,
            normals: vnormals,
            colors: None,
            uvs: None,
            indices,
        }
    }

    /// Sweeps a circle of `radius` along an already-sampled polyline, using the
    /// same rotation-minimizing frame as [`along_curve`](Self::along_curve).
    ///
    /// Use this when the curve exists only as points — a traced geodesic, an
    /// integrated streamline, imported data — so there is no analytic `eval` to
    /// differentiate. Tangents come from central differences between
    /// neighbouring samples, so the tube is only as smooth as the sampling.
    /// Consecutive duplicate points are dropped (a zero-length segment has no
    /// tangent); fewer than two distinct points yields an empty mesh.
    ///
    /// ```
    /// use glam::Vec3;
    /// use manim_sci::curveviz::TubeMesh;
    ///
    /// let pts: Vec<Vec3> = (0..32)
    ///     .map(|i| {
    ///         let t = i as f32 / 31.0 * std::f32::consts::TAU;
    ///         Vec3::new(t.cos(), t.sin(), 0.0)
    ///     })
    ///     .collect();
    /// let tube = TubeMesh::along_polyline(&pts, 0.1, 12, false);
    /// assert_eq!(tube.positions.len(), 32 * 12);
    /// ```
    pub fn along_polyline(points: &[Vec3], radius: f64, n_around: usize, closed: bool) -> TriMesh {
        let n_around = n_around.max(3);

        // Drop consecutive duplicates: a repeated point has no tangent.
        let mut centers: Vec<DVec3> = Vec::with_capacity(points.len());
        for p in points {
            let p = DVec3::new(p.x as f64, p.y as f64, p.z as f64);
            if centers.last().is_none_or(|last| last.distance(p) > 1e-12) {
                centers.push(p);
            }
        }
        // A closed loop whose ends coincide has one redundant sample.
        if closed && centers.len() > 2 && centers[0].distance(*centers.last().unwrap()) < 1e-12 {
            centers.pop();
        }
        let n_along = centers.len();
        if n_along < 2 {
            return TriMesh::default();
        }

        // Central-difference tangents; one-sided at the ends of an open curve.
        let tangents: Vec<DVec3> = (0..n_along)
            .map(|i| {
                let d = if closed {
                    centers[(i + 1) % n_along] - centers[(i + n_along - 1) % n_along]
                } else if i == 0 {
                    centers[1] - centers[0]
                } else if i == n_along - 1 {
                    centers[n_along - 1] - centers[n_along - 2]
                } else {
                    centers[i + 1] - centers[i - 1]
                };
                d.normalize_or_zero()
            })
            .collect();

        let mut nrm = seed_normal(tangents[0]);
        let mut frames = Vec::with_capacity(n_along);
        frames.push(nrm);
        for i in 1..n_along {
            nrm = rmf_step(
                centers[i - 1],
                centers[i],
                tangents[i - 1],
                tangents[i],
                nrm,
            );
            frames.push(nrm);
        }

        let mut positions = Vec::with_capacity(n_along * n_around);
        let mut vnormals = Vec::with_capacity(n_along * n_around);
        for i in 0..n_along {
            let n = frames[i];
            let b = tangents[i].cross(n).normalize_or_zero();
            for j in 0..n_around {
                let theta = std::f64::consts::TAU * j as f64 / n_around as f64;
                let dir = n * theta.cos() + b * theta.sin();
                positions.push(to_vec3(centers[i] + dir * radius));
                vnormals.push(to_vec3(dir));
            }
        }

        let ring_count = if closed { n_along } else { n_along - 1 };
        let mut indices = Vec::with_capacity(ring_count * n_around * 6);
        for i in 0..ring_count {
            let i1 = (i + 1) % n_along;
            for j in 0..n_around {
                let j1 = (j + 1) % n_around;
                let a = (i * n_around + j) as u32;
                let b = (i1 * n_around + j) as u32;
                let c = (i1 * n_around + j1) as u32;
                let d = (i * n_around + j1) as u32;
                indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }

        TriMesh {
            positions,
            normals: vnormals,
            colors: None,
            uvs: None,
            indices,
        }
    }
}

/// Default tube radius for [`SpaceCurve`], in scene units.
pub const DEFAULT_CURVE_TUBE_RADIUS: f64 = 0.03;

/// Default cross-section sides for a [`SpaceCurve`] tube. Eight reads as round
/// at typical curve radii without spending triangles on a thin object.
pub const DEFAULT_CURVE_TUBE_SIDES: usize = 8;

/// Default stroke width used by [`SpaceCurve::flat`].
pub const DEFAULT_CURVE_STROKE_WIDTH: f32 = 5.0;

/// A space curve placed in a scene, as a depth-tested tube by default.
///
/// A curve that lives *on* a surface — a geodesic, a curvature line, a flow
/// line — must be occluded by that surface where it passes behind. A 2-D
/// stroked path cannot be: the vector pipeline draws over the mesh pass, so the
/// curve floats on top and the picture reads inside-out. Sweeping a tube puts
/// the curve in the mesh pass, where the depth buffer settles it correctly.
///
/// [`flat`](Self::flat) opts back out to a stroke, which is the right choice in
/// a genuinely 2-D scene (a plane plot), where a tube would be pointless
/// geometry and a hairline stroke reads better.
///
/// ```
/// use glam::Vec3;
/// use manim_core::scene_state::SceneState;
/// use manim_sci::curveviz::SpaceCurve;
/// use manim_core::manim_color::TEAL;
///
/// let pts: Vec<Vec3> = (0..20).map(|i| Vec3::new(i as f32 * 0.1, 0.0, 0.0)).collect();
/// let mut scene = SceneState::new();
///
/// // Depth-tested by default: it lands on the mesh channel.
/// let _tube = SpaceCurve::new(pts.clone()).with_color(TEAL).add_to(&mut scene);
/// assert_eq!(scene.display_list().meshes().len(), 1);
///
/// // …and `.flat()` puts it back on the 2-D draw list.
/// let _stroke = SpaceCurve::new(pts).flat().add_to(&mut scene);
/// assert_eq!(scene.display_list().len(), 1);
/// ```
pub struct SpaceCurve {
    points: Vec<Vec3>,
    radius: f64,
    sides: usize,
    closed: bool,
    flat: bool,
    color: Color,
}

impl SpaceCurve {
    /// A curve through `points`, as a tube of [`DEFAULT_CURVE_TUBE_RADIUS`].
    pub fn new(points: impl Into<Vec<Vec3>>) -> Self {
        Self {
            points: points.into(),
            radius: DEFAULT_CURVE_TUBE_RADIUS,
            sides: DEFAULT_CURVE_TUBE_SIDES,
            closed: false,
            flat: false,
            color: WHITE,
        }
    }

    /// Sets the tube radius in scene units (ignored when [`flat`](Self::flat)).
    pub fn with_radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }

    /// Sets the tube's cross-section sides (ignored when [`flat`](Self::flat)).
    pub fn with_sides(mut self, sides: usize) -> Self {
        self.sides = sides;
        self
    }

    /// Sets the curve color.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Welds the curve's end back to its start (a closed loop).
    pub fn closed(mut self) -> Self {
        self.closed = true;
        self
    }

    /// Draws a flat 2-D stroke instead of a tube.
    ///
    /// The opt-out for 2-D scenes. In a 3-D scene this reintroduces the
    /// draw-over-the-mesh problem the tube exists to solve.
    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    /// Adds the curve to `scene`, returning its erased id (a mesh mobject, or a
    /// [`VMobject`](manim_core::geometry::VMobject) when [`flat`](Self::flat)).
    pub fn add_to(self, scene: &mut SceneState) -> manim_core::mobject::AnyId {
        // `with_stroke` is a `Buildable` builder, not a `MobjectExt` transform.
        use manim_core::mobject::Buildable;

        if self.flat {
            let path = manim_math::path::Path::from_corners(&self.points, self.closed);
            let vm = manim_core::geometry::VMobject::from_path(path).with_stroke(
                self.color,
                DEFAULT_CURVE_STROKE_WIDTH,
                1.0,
            );
            return scene.add(vm).erase();
        }

        let mesh = TubeMesh::along_polyline(&self.points, self.radius, self.sides, self.closed);
        scene
            .add(Mesh::new(mesh).with_material(manim_core::mesh::MeshMaterial::new(self.color)))
            .erase()
    }
}

/// A unit-length vector perpendicular to `t`.
fn seed_normal(t: DVec3) -> DVec3 {
    let a = if t.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    (a - t * a.dot(t)).normalize()
}

/// One double-reflection RMF step: transport `n` from the frame at `p0` (tangent
/// `t0`) to `p1` (tangent `t1`).
fn rmf_step(p0: DVec3, p1: DVec3, t0: DVec3, t1: DVec3, n: DVec3) -> DVec3 {
    let v1 = p1 - p0;
    let c1 = v1.dot(v1);
    if c1 < 1e-18 {
        return n;
    }
    let r_l = n - v1 * (2.0 / c1 * v1.dot(n));
    let t_l = t0 - v1 * (2.0 / c1 * v1.dot(t0));
    let v2 = t1 - t_l;
    let c2 = v2.dot(v2);
    if c2 < 1e-18 {
        return r_l.normalize();
    }
    (r_l - v2 * (2.0 / c2 * v2.dot(r_l))).normalize()
}

/// The trefoil knot — the `(2, 3)` torus knot `((2+cos 3t)cos 2t,
/// (2+cos 3t)sin 2t, sin 3t)`, `t ∈ [0, 2π]`.
pub fn trefoil() -> impl CurveSampler {
    struct Trefoil;
    impl CurveSampler for Trefoil {
        fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
            let r = t.scale(3.0).cos() + S::constant(2.0);
            [
                r * t.scale(2.0).cos(),
                r * t.scale(2.0).sin(),
                t.scale(3.0).sin(),
            ]
        }
    }
    Trefoil
}

/// The figure-eight knot `((2+cos 2t)cos 3t, (2+cos 2t)sin 3t, sin 4t)`,
/// `t ∈ [0, 2π]`.
pub fn figure_eight() -> impl CurveSampler {
    struct FigureEight;
    impl CurveSampler for FigureEight {
        fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
            let r = t.scale(2.0).cos() + S::constant(2.0);
            [
                r * t.scale(3.0).cos(),
                r * t.scale(3.0).sin(),
                t.scale(4.0).sin(),
            ]
        }
    }
    FigureEight
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    struct Circle;
    impl CurveSampler for Circle {
        fn eval<S: Scalar>(&self, t: S) -> [S; 3] {
            [t.cos(), t.sin(), S::constant(0.0)]
        }
    }

    #[test]
    fn tube_vertices_sit_at_radius_from_the_axis() {
        // Every tube vertex is `radius` from its ring centre on the unit circle.
        let r = 0.15;
        let tube = TubeMesh::along_curve(&Circle, (0.0, TAU), r, 60, 10, true);
        assert_eq!(tube.positions.len(), 60 * 10);
        for p in &tube.positions {
            let planar = (p.x * p.x + p.y * p.y).sqrt();
            let d = ((planar - 1.0).powi(2) + p.z * p.z).sqrt();
            assert!((d - r as f32).abs() < 1e-4, "off-tube distance {d}");
        }
    }

    /// A polyline sampling of the same circle must produce the same tube
    /// geometry as the analytic sweep, to sampling accuracy.
    #[test]
    fn polyline_tube_matches_the_analytic_sweep() {
        let r = 0.15;
        let n = 60;
        let pts: Vec<Vec3> = (0..n)
            .map(|i| {
                let t = TAU * i as f64 / n as f64;
                Vec3::new(t.cos() as f32, t.sin() as f32, 0.0)
            })
            .collect();
        let tube = TubeMesh::along_polyline(&pts, r, 10, true);
        assert_eq!(tube.positions.len(), n * 10);
        for p in &tube.positions {
            let planar = (p.x * p.x + p.y * p.y).sqrt();
            let d = ((planar - 1.0).powi(2) + p.z * p.z).sqrt();
            assert!((d - r as f32).abs() < 1e-4, "off-tube distance {d}");
        }
    }

    #[test]
    fn polyline_tube_tolerates_degenerate_input() {
        // Duplicate points carry no tangent and must be dropped, not divided by.
        let dupes = vec![Vec3::ZERO, Vec3::ZERO, Vec3::X, Vec3::X, 2.0 * Vec3::X];
        let tube = TubeMesh::along_polyline(&dupes, 0.1, 6, false);
        assert_eq!(tube.positions.len(), 3 * 6);
        assert!(tube.positions.iter().all(|p| p.is_finite()));

        // Too few distinct points → an empty mesh rather than a panic.
        assert!(TubeMesh::along_polyline(&[Vec3::ZERO; 4], 0.1, 6, false)
            .positions
            .is_empty());
        assert!(TubeMesh::along_polyline(&[], 0.1, 6, false)
            .positions
            .is_empty());
    }

    /// The FE-142b contract: tubes go on the depth-tested mesh channel, and
    /// only `.flat()` puts a curve back on the 2-D draw list.
    #[test]
    fn space_curve_is_depth_tested_unless_flattened() {
        let pts: Vec<Vec3> = (0..10)
            .map(|i| Vec3::new(i as f32 * 0.2, 0.0, 0.0))
            .collect();

        let mut scene = SceneState::new();
        SpaceCurve::new(pts.clone()).add_to(&mut scene);
        let dl = scene.display_list();
        assert_eq!(dl.meshes().len(), 1, "tube should be a mesh");
        assert_eq!(dl.len(), 0, "tube should emit no 2-D draw items");

        let mut flat_scene = SceneState::new();
        SpaceCurve::new(pts).flat().add_to(&mut flat_scene);
        let dl = flat_scene.display_list();
        assert_eq!(dl.meshes().len(), 0);
        assert_eq!(dl.len(), 1, "flat curve should be a 2-D stroke");
    }

    #[test]
    fn space_curve_builders_apply() {
        let pts: Vec<Vec3> = (0..8)
            .map(|i| Vec3::new(i as f32 * 0.3, 0.0, 0.0))
            .collect();
        let mut scene = SceneState::new();
        SpaceCurve::new(pts)
            .with_radius(0.5)
            .with_sides(16)
            .add_to(&mut scene);
        let meshes = scene.display_list();
        let mesh = &meshes.meshes()[0];
        // 8 samples × 16 sides.
        assert_eq!(mesh.mesh.positions.len(), 8 * 16);
    }

    #[test]
    fn rmf_frame_stays_orthonormal() {
        let n0 = seed_normal(DVec3::X);
        assert!((n0.length() - 1.0).abs() < 1e-12);
        assert!(n0.dot(DVec3::X).abs() < 1e-12);
        let n1 = rmf_step(
            DVec3::ZERO,
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::Y,
            DVec3::Y,
            n0,
        );
        assert!((n1.length() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn knots_are_closed_loops() {
        let close = |g: [f64; 3], h: [f64; 3]| {
            (g[0] - h[0]).abs() + (g[1] - h[1]).abs() + (g[2] - h[2]).abs() < 1e-9
        };
        assert!(close(
            trefoil().eval::<f64>(0.0),
            trefoil().eval::<f64>(TAU)
        ));
        assert!(close(
            figure_eight().eval::<f64>(0.0),
            figure_eight().eval::<f64>(TAU)
        ));
    }
}
