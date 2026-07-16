//! The model transform of a mesh mobject, encoded as an ordinary [`Path`].
//!
//! # Why a path, not a `Mat4` field
//!
//! Every shared behavior in this crate — [`MobjectExt`](crate::mobject::MobjectExt)
//! transforms, [`SceneState`](crate::scene_state::SceneState) family ops,
//! updaters, `save_state`/`Restore`, timeline snapshots, `.animate()` — is
//! expressed as a mutation of [`MobjectData::path`](crate::mobject::MobjectData::path)
//! through [`Path::apply`]. A mesh mobject that stored its transform in a private
//! `Mat4` would be invisible to all of it, and would need every one of those
//! features re-implemented.
//!
//! So a mesh mobject's path *is* its transform: six anchor points — the ends of a
//! cross through the mesh's local bounding box — carried in world space. The
//! model matrix is read back out of them ([`LocalFrame::transform_of`]). Because
//! the encoding is affine, every affine mutation of the path maps to exactly the
//! same affine change of the model matrix, for free.
//!
//! Two consequences worth knowing:
//!
//! - [`apply_function`](crate::mobject::MobjectExt::apply_function) with a
//!   *non-affine* function only affects the mesh through the affine part it
//!   induces on the six anchors.
//! - A mesh's bounding box is that of the cross, i.e. of the octahedron inscribed
//!   in the local box. It is exact for an axis-aligned transform and can
//!   under-report a rotated one, but its center is always exactly the transformed
//!   mesh center — so `move_to`, `next_to`, and friends place meshes correctly.

use glam::{Mat3, Mat4, Vec3};
use manim_math::path::Path;

use super::TriMesh;

/// Half-extent substituted for a locally-flat axis (e.g. the `z` of a grid), so
/// the frame stays invertible.
const DEGENERATE_HALF: f32 = 1.0;

/// The local reference cross of a mesh: its bounding-box center and half-extents.
///
/// Paired with a world-space [`Path`] of six anchors, it encodes a model matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LocalFrame {
    center: Vec3,
    /// Per-axis half-extent, never zero (see [`DEGENERATE_HALF`]).
    half: Vec3,
}

impl LocalFrame {
    /// The reference frame of `mesh`, from its local bounding box.
    pub(crate) fn of(mesh: &TriMesh) -> Self {
        Self::of_bounds(mesh.bounds())
    }

    /// The reference frame for explicit local bounds (`None` = an empty mesh).
    pub(crate) fn of_bounds(bounds: Option<(Vec3, Vec3)>) -> Self {
        let (min, max) = bounds.unwrap_or((Vec3::ZERO, Vec3::ZERO));
        let half = (max - min) * 0.5;
        let fix = |h: f32| if h.abs() > 1e-6 { h } else { DEGENERATE_HALF };
        Self {
            center: (min + max) * 0.5,
            half: Vec3::new(fix(half.x), fix(half.y), fix(half.z)),
        }
    }

    /// The six local anchors, in the order the encoding expects: `+x, -x, +y,
    /// -y, +z, -z` offsets from the center.
    fn anchors(&self) -> [Vec3; 6] {
        let (c, h) = (self.center, self.half);
        [
            c + Vec3::X * h.x,
            c - Vec3::X * h.x,
            c + Vec3::Y * h.y,
            c - Vec3::Y * h.y,
            c + Vec3::Z * h.z,
            c - Vec3::Z * h.z,
        ]
    }

    /// The world-space path encoding model matrix `m`.
    pub(crate) fn path_for(&self, m: Mat4) -> Path {
        let pts: Vec<Vec3> = self
            .anchors()
            .iter()
            .map(|p| m.transform_point3(*p))
            .collect();
        Path::from_corners(&pts, false)
    }

    /// The model matrix encoded by `path`, or [`Mat4::IDENTITY`] if `path` is not
    /// one this frame produced (e.g. a caller overwrote the mesh's points).
    pub(crate) fn transform_of(&self, path: &Path) -> Mat4 {
        let Some(p) = read_anchors(path) else {
            return Mat4::IDENTITY;
        };
        // Antipodal pairs average to the transformed center, so the mean of all
        // six is exactly `m * center` for any affine `m`.
        let origin = p.iter().fold(Vec3::ZERO, |a, b| a + *b) / 6.0;
        // (p[+i] - p[-i]) / 2 is the linear part applied to half.i * ê_i.
        let linear = Mat3::from_cols(
            (p[0] - p[1]) / (2.0 * self.half.x),
            (p[2] - p[3]) / (2.0 * self.half.y),
            (p[4] - p[5]) / (2.0 * self.half.z),
        );
        let translation = origin - linear * self.center;
        Mat4::from_cols(
            linear.x_axis.extend(0.0),
            linear.y_axis.extend(0.0),
            linear.z_axis.extend(0.0),
            translation.extend(1.0),
        )
    }
}

/// The six anchors of a frame path, or `None` if it is not shaped like one.
fn read_anchors(path: &Path) -> Option<[Vec3; 6]> {
    let sub = path.subpaths.first()?;
    if sub.curves.len() != 5 {
        return None;
    }
    Some([
        sub.curves[0].p0,
        sub.curves[1].p0,
        sub.curves[2].p0,
        sub.curves[3].p0,
        sub.curves[4].p0,
        sub.curves[4].p3,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobject::{apply_rotate_about, apply_shift, MobjectData};
    use crate::style::Style;

    fn frame_and_path(m: Mat4) -> (LocalFrame, Path) {
        let frame = LocalFrame::of(&TriMesh::uv_sphere(8, 12));
        let path = frame.path_for(m);
        (frame, path)
    }

    fn assert_close(a: Mat4, b: Mat4) {
        for i in 0..4 {
            assert!(
                (a.col(i) - b.col(i)).length() < 1e-4,
                "column {i}: {:?} vs {:?}",
                a.col(i),
                b.col(i)
            );
        }
    }

    #[test]
    fn identity_round_trips() {
        let (frame, path) = frame_and_path(Mat4::IDENTITY);
        assert_close(frame.transform_of(&path), Mat4::IDENTITY);
    }

    #[test]
    fn affine_transforms_round_trip() {
        for m in [
            Mat4::from_translation(Vec3::new(1.0, -2.0, 3.0)),
            Mat4::from_scale(Vec3::new(2.0, 3.0, 0.5)),
            Mat4::from_rotation_z(0.7),
            Mat4::from_rotation_x(1.1) * Mat4::from_scale(Vec3::splat(3.0)),
            Mat4::from_translation(Vec3::Y * 4.0) * Mat4::from_rotation_y(-0.4),
        ] {
            let (frame, path) = frame_and_path(m);
            assert_close(frame.transform_of(&path), m);
        }
    }

    /// The whole point of the encoding: the crate's existing path mutators drive
    /// the model matrix without knowing meshes exist.
    #[test]
    fn path_mutation_drives_the_model_matrix() {
        let (frame, path) = frame_and_path(Mat4::IDENTITY);
        let mut data = MobjectData::new(path, Style::default());

        apply_shift(&mut data, Vec3::new(2.0, 0.0, 0.0));
        assert_close(
            frame.transform_of(&data.path),
            Mat4::from_translation(Vec3::X * 2.0),
        );

        apply_rotate_about(&mut data, std::f32::consts::FRAC_PI_2, Vec3::ZERO, Vec3::Z);
        assert_close(
            frame.transform_of(&data.path),
            Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)
                * Mat4::from_translation(Vec3::X * 2.0),
        );
    }

    /// A locally-flat axis (a grid has no z extent) must not make the frame
    /// singular.
    #[test]
    fn degenerate_axis_stays_invertible() {
        let frame = LocalFrame::of(&TriMesh::grid(2, 2));
        let m = Mat4::from_rotation_x(0.5) * Mat4::from_translation(Vec3::Y);
        assert_close(frame.transform_of(&frame.path_for(m)), m);
    }

    #[test]
    fn unrecognizable_path_falls_back_to_identity() {
        let frame = LocalFrame::of(&TriMesh::grid(1, 1));
        let junk = Path::from_corners(&[Vec3::ZERO, Vec3::X], false);
        assert_close(frame.transform_of(&junk), Mat4::IDENTITY);
        assert_close(frame.transform_of(&Path::default()), Mat4::IDENTITY);
    }

    #[test]
    fn empty_mesh_frame_is_usable() {
        let frame = LocalFrame::of(&TriMesh::default());
        let m = Mat4::from_translation(Vec3::X);
        assert_close(frame.transform_of(&frame.path_for(m)), m);
    }
}
