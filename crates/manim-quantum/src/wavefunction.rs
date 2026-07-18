//! Complex wavefunction visualizers: 1-D curves (probability density, real/
//! imaginary pair, phase-hue) and a 2-D phase-hue field.
//!
//! A wavefunction is complex, so a faithful picture has to show both modulus and
//! phase. This module offers three 1-D styles built from a sampled ψ — the
//! probability density `|ψ|²`, the real/imaginary pair `Re ψ` / `Im ψ`, and a
//! **phase-hue** density curve whose color tracks `arg ψ` — plus a 2-D
//! [`Wavefunction2D`] that produces the `RG32F` [`TextureData`] a
//! [`MaterialKind::PhaseHue`](manim_core::display::MaterialKind::PhaseHue)
//! material shades (with a per-pixel image fallback that bakes the same domain
//! coloring today).
//!
//! Data coordinates map to scene coordinates through a [`PlotTransform`] (a plain
//! affine map), so the caller controls placement and scale without an `Axes`.
//!
//! ```
//! use manim_quantum::wavefunction::{PlotTransform, Wavefunction1D};
//! use manim_core::scene_state::SceneState;
//! use manim_core::prelude::{Color, Point};
//! use manim_fields::complex::Complex;
//!
//! // A right-moving Gaussian wavepacket ψ(x) = e^{-x²/4} e^{i·2x}.
//! let xs: Vec<f64> = (0..64).map(|i| -8.0 + i as f64 * 0.25).collect();
//! let wf = Wavefunction1D::from_closure(&xs, |x| {
//!     Complex::from_polar((-x * x / 4.0).exp(), 2.0 * x)
//! });
//! let mut scene = SceneState::new();
//! let tf = PlotTransform::new(Point::new(0.0, 0.0, 0.0), 0.5, 2.0);
//! let curve = wf.probability_curve(&mut scene, &tf, Color::from_hsv(0.6, 0.7, 1.0), 0.5, 3.0);
//! assert!(scene.family(curve.erase()).len() >= 1);
//! ```

use manim_core::display::{FieldChannels, TextureData};
use manim_core::prelude::*;
use manim_fields::complex::Complex;
use manim_math::path::Path;

/// Full turn in radians, as `f64` (phase spans one hue wheel).
const TAU64: f64 = std::f64::consts::TAU;
/// Half turn in radians, as `f64` (shifts `arg ∈ (−π, π]` to `[0, 2π)`).
const PI64: f64 = std::f64::consts::PI;

/// The domain-coloring hue for a complex phase `arg ∈ (−π, π]`.
///
/// Maps the phase onto the full hue wheel (`hue = (arg + π) / 2π`) at full
/// saturation and value, so `arg = 0` → red, `arg = ±π` → cyan. The same map
/// colors both the 1-D phase-hue curve and the 2-D fallback quad.
///
/// ```
/// use manim_quantum::wavefunction::phase_color;
/// use std::f64::consts::PI;
/// // Opposite phases sit opposite on the wheel, so their colors differ.
/// assert_ne!(phase_color(0.0).to_hex(), phase_color(PI).to_hex());
/// ```
pub fn phase_color(arg: f64) -> Color {
    let hue = ((arg + PI64) / TAU64) as f32;
    Color::from_hsv(hue.rem_euclid(1.0), 1.0, 1.0)
}

/// An affine map from plot data coordinates `(x, value)` to scene space.
///
/// Data point `(0, 0)` lands at [`origin`](Self::origin); one unit of `x` is
/// [`x_scale`](Self::x_scale) scene units to the right and one unit of value is
/// [`y_scale`](Self::y_scale) scene units up. This is the lightweight stand-in
/// for an `Axes` used by every builder here.
#[derive(Clone, Copy, Debug)]
pub struct PlotTransform {
    /// Scene point that data coordinate `(0, 0)` maps to.
    pub origin: Point,
    /// Scene units per unit of `x`.
    pub x_scale: f32,
    /// Scene units per unit of value (the vertical axis).
    pub y_scale: f32,
}

impl PlotTransform {
    /// A transform placing data `(0, 0)` at `origin` with the given axis scales.
    pub fn new(origin: Point, x_scale: f32, y_scale: f32) -> Self {
        Self {
            origin,
            x_scale,
            y_scale,
        }
    }

    /// Maps a data coordinate `(x, value)` to its scene [`Point`].
    pub fn map(&self, x: f64, value: f64) -> Point {
        Point::new(
            self.origin.x + x as f32 * self.x_scale,
            self.origin.y + value as f32 * self.y_scale,
            self.origin.z,
        )
    }
}

/// A complex wavefunction ψ sampled on a 1-D grid.
///
/// Holds the grid coordinates and the complex amplitude at each; the builders
/// turn it into scene mobjects. Construct from a closure with
/// [`from_closure`](Self::from_closure) or from explicit samples with
/// [`from_samples`](Self::from_samples).
///
/// ```
/// use manim_quantum::wavefunction::Wavefunction1D;
/// use manim_fields::complex::Complex;
/// let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
/// let wf = Wavefunction1D::from_closure(&xs, |x| Complex::new(x, 0.0));
/// assert_eq!(wf.xs.len(), 10);
/// assert_eq!(wf.probability_density()[3], 9.0); // |3 + 0i|² = 9
/// ```
#[derive(Clone, Debug)]
pub struct Wavefunction1D {
    /// Grid coordinates.
    pub xs: Vec<f64>,
    /// Complex amplitude ψ at each grid point (same length as [`xs`](Self::xs)).
    pub psi: Vec<Complex>,
}

impl Wavefunction1D {
    /// Samples `psi` at each `x` in `xs`.
    pub fn from_closure(xs: &[f64], psi: impl Fn(f64) -> Complex) -> Self {
        Self {
            xs: xs.to_vec(),
            psi: xs.iter().map(|&x| psi(x)).collect(),
        }
    }

    /// Builds from explicit grid coordinates and amplitudes.
    ///
    /// # Panics
    /// Panics if `xs.len() != psi.len()`.
    pub fn from_samples(xs: Vec<f64>, psi: Vec<Complex>) -> Self {
        assert_eq!(xs.len(), psi.len(), "xs and psi must have equal length");
        Self { xs, psi }
    }

    /// The probability density `|ψ(x)|²` at each grid point.
    pub fn probability_density(&self) -> Vec<f64> {
        self.psi.iter().map(|z| z.norm_sqr()).collect()
    }

    /// Adds a filled curve of the probability density `|ψ(x)|²`.
    ///
    /// The area between the curve and the baseline (`value = 0`) is filled at
    /// `fill_opacity`; the top edge is stroked at `stroke_width`. Returns the
    /// filled [`VMobject`].
    pub fn probability_curve(
        &self,
        scene: &mut SceneState,
        tf: &PlotTransform,
        color: Color,
        fill_opacity: f32,
        stroke_width: f32,
    ) -> MobjectId<VMobject> {
        let dens = self.probability_density();
        // Top edge left→right, then baseline right→left to close the area.
        let mut pts: Vec<Point> = self
            .xs
            .iter()
            .zip(&dens)
            .map(|(&x, &p)| tf.map(x, p))
            .collect();
        for &x in self.xs.iter().rev() {
            pts.push(tf.map(x, 0.0));
        }
        let mobject = VMobject::from_path(Path::from_corners(&pts, true))
            .with_fill(color, fill_opacity)
            .with_stroke(color, stroke_width, 1.0);
        scene.add(mobject)
    }

    /// Adds two stroked curves — `Re ψ` and `Im ψ` — grouped together.
    ///
    /// Returns the [`VGroup`] holding the real-part curve (in `re_color`) and the
    /// imaginary-part curve (in `im_color`).
    pub fn re_im_curves(
        &self,
        scene: &mut SceneState,
        tf: &PlotTransform,
        re_color: Color,
        im_color: Color,
        stroke_width: f32,
    ) -> MobjectId<VGroup> {
        let re_pts: Vec<Point> = self
            .xs
            .iter()
            .zip(&self.psi)
            .map(|(&x, z)| tf.map(x, z.re))
            .collect();
        let im_pts: Vec<Point> = self
            .xs
            .iter()
            .zip(&self.psi)
            .map(|(&x, z)| tf.map(x, z.im))
            .collect();
        let re = scene.add(
            VMobject::from_path(Path::from_corners(&re_pts, false)).with_stroke(
                re_color,
                stroke_width,
                1.0,
            ),
        );
        let im = scene.add(
            VMobject::from_path(Path::from_corners(&im_pts, false)).with_stroke(
                im_color,
                stroke_width,
                1.0,
            ),
        );
        VGroup::of(scene, [re.erase(), im.erase()])
    }

    /// Adds the probability-density curve as short segments colored by `arg ψ`.
    ///
    /// Each grid interval becomes a [`Line`] from `(xᵢ, |ψᵢ|²)` to
    /// `(xᵢ₊₁, |ψᵢ₊₁|²)`, colored by the phase at its midpoint through
    /// [`phase_color`]. The segments are grouped, giving the "phase portrait"
    /// look of a density curve whose hue rotates with the local phase. Returns
    /// the [`VGroup`].
    pub fn phase_hue_curve(
        &self,
        scene: &mut SceneState,
        tf: &PlotTransform,
        stroke_width: f32,
    ) -> MobjectId<VGroup> {
        let dens = self.probability_density();
        let mut segments: Vec<AnyId> = Vec::new();
        for i in 0..self.xs.len().saturating_sub(1) {
            let a = tf.map(self.xs[i], dens[i]);
            let b = tf.map(self.xs[i + 1], dens[i + 1]);
            // Average the two anchors' phases (on the wheel, via their vector sum).
            let mean = self.psi[i] + self.psi[i + 1];
            let color = phase_color(mean.arg());
            let seg = scene.add(Line::new(a, b).with_stroke(color, stroke_width, 1.0));
            segments.push(seg.erase());
        }
        VGroup::of(scene, segments)
    }
}

/// A complex wavefunction ψ sampled on a 2-D `nx × ny` grid (row-major).
///
/// The natural GPU rendering is a phase-hue textured quad: build the `RG32F`
/// field with [`texture_data`](Self::texture_data) and shade it with a
/// [`MaterialKind::PhaseHue`](manim_core::display::MaterialKind::PhaseHue)
/// material. Attaching a [`Material`](manim_core::display::Material) to a scene
/// mobject is not yet a public core API (the base mobject carries an image paint
/// but not a material — see `SceneState::display_list`), so
/// [`add_phase_hue_quad`](Self::add_phase_hue_quad) bakes the identical domain
/// coloring into an [`ImageMobject`] today; the same [`Self::texture_data`]
/// feeds the material path once that hookup lands.
///
/// ```
/// use manim_quantum::wavefunction::Wavefunction2D;
/// use manim_fields::complex::Complex;
/// use manim_core::display::FieldChannels;
/// let wf = Wavefunction2D::from_closure(8, 6, (-2.0, 2.0), (-1.5, 1.5), |x, y| {
///     Complex::from_polar((-(x * x + y * y)).exp(), x + y)
/// });
/// let tex = wf.texture_data();
/// assert_eq!((tex.width, tex.height), (8, 6));
/// assert_eq!(tex.channels, FieldChannels::Rg);
/// assert_eq!(tex.data.len(), 8 * 6 * 2); // re, im per texel
/// ```
#[derive(Clone, Debug)]
pub struct Wavefunction2D {
    /// Grid columns.
    pub nx: usize,
    /// Grid rows.
    pub ny: usize,
    /// Inclusive data-`x` range `(min, max)` the columns span.
    pub x_range: (f64, f64),
    /// Inclusive data-`y` range `(min, max)` the rows span.
    pub y_range: (f64, f64),
    /// Row-major amplitudes, `psi[iy * nx + ix]`, with row `0` at `y_range.0`.
    pub psi: Vec<Complex>,
}

impl Wavefunction2D {
    /// Samples `psi` on an `nx × ny` grid spanning `x_range × y_range`.
    ///
    /// Row `iy = 0` sits at `y_range.0` and column `ix = 0` at `x_range.0`; both
    /// endpoints are included (grid step `(max − min) / (n − 1)`).
    pub fn from_closure(
        nx: usize,
        ny: usize,
        x_range: (f64, f64),
        y_range: (f64, f64),
        psi: impl Fn(f64, f64) -> Complex,
    ) -> Self {
        let dx = if nx > 1 {
            (x_range.1 - x_range.0) / (nx - 1) as f64
        } else {
            0.0
        };
        let dy = if ny > 1 {
            (y_range.1 - y_range.0) / (ny - 1) as f64
        } else {
            0.0
        };
        let mut data = Vec::with_capacity(nx * ny);
        for iy in 0..ny {
            let y = y_range.0 + iy as f64 * dy;
            for ix in 0..nx {
                let x = x_range.0 + ix as f64 * dx;
                data.push(psi(x, y));
            }
        }
        Self {
            nx,
            ny,
            x_range,
            y_range,
            psi: data,
        }
    }

    /// Scene-space center of the covered rectangle (data coords used directly).
    fn center(&self) -> Point {
        Point::new(
            ((self.x_range.0 + self.x_range.1) * 0.5) as f32,
            ((self.y_range.0 + self.y_range.1) * 0.5) as f32,
            0.0,
        )
    }

    /// Scene-space `(width, height)` of the covered rectangle.
    fn extent(&self) -> [f32; 2] {
        [
            (self.x_range.1 - self.x_range.0) as f32,
            (self.y_range.1 - self.y_range.0) as f32,
        ]
    }

    /// The complex field as `RG32F` [`TextureData`] — two floats `(re, im)` per
    /// texel, row-major — pinned to the data rectangle in scene space.
    ///
    /// This is exactly the input a
    /// [`MaterialKind::PhaseHue`](manim_core::display::MaterialKind::PhaseHue)
    /// material samples per pixel.
    pub fn texture_data(&self) -> TextureData {
        let mut data = Vec::with_capacity(self.nx * self.ny * 2);
        for z in &self.psi {
            data.push(z.re as f32);
            data.push(z.im as f32);
        }
        TextureData {
            width: self.nx as u32,
            height: self.ny as u32,
            channels: FieldChannels::Rg,
            data,
            center: self.center(),
            size: self.extent(),
        }
    }

    /// Adds a phase-hue quad: an [`ImageMobject`] with domain coloring baked per
    /// pixel (hue = `arg ψ`, brightness ∝ `|ψ|`), sized to the data rectangle.
    ///
    /// The fallback for the not-yet-public material-attachment API (see the type
    /// docs). Row order is flipped so data-`y` increases upward on screen.
    pub fn add_phase_hue_quad(&self, scene: &mut SceneState) -> MobjectId<ImageMobject> {
        let max = self
            .psi
            .iter()
            .map(|z| z.norm_sqr())
            .fold(0.0_f64, f64::max)
            .max(1e-300);
        let mut rgba = Vec::with_capacity(self.nx * self.ny * 4);
        // Image row 0 is the top of the quad → highest data-y → grid row ny-1.
        for iy in (0..self.ny).rev() {
            for ix in 0..self.nx {
                let z = self.psi[iy * self.nx + ix];
                let hue = (((z.arg() + PI64) / TAU64) as f32).rem_euclid(1.0);
                let value = (z.norm_sqr() / max).sqrt() as f32;
                let srgb = Color::from_hsv(hue, 1.0, value).to_srgb();
                rgba.push((srgb[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                rgba.push((srgb[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                rgba.push((srgb[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                rgba.push(255);
            }
        }
        let img = ImageMobject::from_rgba(self.nx as u32, self.ny as u32, rgba);
        let id = scene.add(img);
        let [w, h] = self.extent();
        let m = scene.get_mut(id);
        m.set_width(w, true);
        m.set_height(h, true);
        m.move_to(self.center());
        id
    }

    /// Adds this wavefunction as a [`MaterialQuad`](manim_sci::material_quad::MaterialQuad)
    /// painted by the real GPU [`PhaseHue`](manim_core::display::MaterialKind::PhaseHue)
    /// material, sampling the `RG32F` (re, im) [`texture_data`](Self::texture_data)
    /// per pixel (the S1b material path). Prefer this over
    /// [`add_phase_hue_quad`](Self::add_phase_hue_quad), which bakes the coloring
    /// into an [`ImageMobject`] and is kept as a no-material fallback.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_fields::complex::Complex;
    /// use manim_quantum::wavefunction::Wavefunction2D;
    /// let psi = Wavefunction2D::from_closure(8, 8, (-1.0, 1.0), (-1.0, 1.0), |x, y| {
    ///     Complex::from_polar((-(x * x + y * y)).exp(), 3.0 * x)
    /// });
    /// let mut scene = SceneState::new();
    /// let q = psi.add_phase_hue_material(&mut scene);
    /// assert!(scene.get_dyn(q).data().material.is_some());
    /// ```
    pub fn add_phase_hue_material(
        &self,
        scene: &mut SceneState,
    ) -> MobjectId<manim_sci::material_quad::MaterialQuad> {
        use manim_core::display::{Material, MaterialKind};
        use manim_sci::material_quad::MaterialQuad;
        let material = Material {
            kind: MaterialKind::PhaseHue {
                modulus_contours: false,
            },
            texture: std::sync::Arc::new(self.texture_data()),
            value_range: [0.0, 1.0],
            opacity: 1.0,
        };
        MaterialQuad::from_material(
            [self.x_range.0, self.x_range.1],
            [self.y_range.0, self.y_range.1],
            material,
        )
        .add_to(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaussian_packet() -> Wavefunction1D {
        // ψ(x) = e^{-x²/2} e^{i·3x} on a modest grid.
        let xs: Vec<f64> = (0..80).map(|i| -8.0 + i as f64 * 0.2).collect();
        Wavefunction1D::from_closure(&xs, |x| Complex::from_polar((-x * x / 2.0).exp(), 3.0 * x))
    }

    fn tf() -> PlotTransform {
        PlotTransform::new(Point::new(0.0, -1.0, 0.0), 0.4, 3.0)
    }

    #[test]
    fn constructors_agree() {
        let xs = vec![0.0, 1.0, 2.0];
        let a = Wavefunction1D::from_closure(&xs, |x| Complex::new(x, -x));
        let b = Wavefunction1D::from_samples(
            xs.clone(),
            vec![
                Complex::new(0.0, 0.0),
                Complex::new(1.0, -1.0),
                Complex::new(2.0, -2.0),
            ],
        );
        assert_eq!(a.psi, b.psi);
        assert_eq!(a.probability_density()[2], 8.0); // |2 - 2i|² = 8
    }

    #[test]
    fn probability_curve_adds_a_filled_mobject() {
        let wf = gaussian_packet();
        let mut scene = SceneState::new();
        let id = wf.probability_curve(&mut scene, &tf(), Color::from_hsv(0.6, 0.6, 1.0), 0.5, 3.0);
        assert!(scene.get(id).data().style.fill_opacity > 0.0);
        assert!(!scene.display_list().is_empty());
    }

    #[test]
    fn re_im_curves_group_has_two_children() {
        let wf = gaussian_packet();
        let mut scene = SceneState::new();
        let g = wf.re_im_curves(&mut scene, &tf(), RED, BLUE, 2.0);
        // family = the group itself + the two curves.
        assert_eq!(scene.family(g.erase()).len(), 3);
    }

    #[test]
    fn phase_hue_curve_has_one_segment_per_interval() {
        let wf = gaussian_packet();
        let n = wf.xs.len();
        let mut scene = SceneState::new();
        let g = wf.phase_hue_curve(&mut scene, &tf(), 4.0);
        // group + (n - 1) segments.
        assert_eq!(scene.family(g.erase()).len(), 1 + (n - 1));
    }

    #[test]
    fn wavefunction2d_texture_is_rg32f() {
        let wf = Wavefunction2D::from_closure(6, 4, (-2.0, 2.0), (-1.0, 1.0), |x, y| {
            Complex::from_polar((-(x * x + y * y)).exp(), x + y)
        });
        let tex = wf.texture_data();
        assert_eq!(tex.channels, FieldChannels::Rg);
        assert_eq!(tex.data.len(), 6 * 4 * 2);
        assert_eq!((tex.width, tex.height), (6, 4));
        // First texel is ψ(x_min, y_min).
        let z0 = wf.psi[0];
        assert!((tex.data[0] - z0.re as f32).abs() < 1e-6);
        assert!((tex.data[1] - z0.im as f32).abs() < 1e-6);
    }

    #[test]
    fn phase_hue_quad_adds_image_of_grid_size() {
        let wf = Wavefunction2D::from_closure(12, 9, (-3.0, 3.0), (-2.0, 2.0), |x, y| {
            Complex::from_polar((-(x * x + y * y) / 4.0).exp(), 2.0 * x)
        });
        let mut scene = SceneState::new();
        let id = wf.add_phase_hue_quad(&mut scene);
        assert_eq!(scene.get(id).pixel_dimensions(), (12, 9));
        // Sized to the 6×4 data rectangle, centered on the origin.
        let bb = scene.get(id).bounding_box();
        assert!((bb.width() - 6.0).abs() < 1e-3);
        assert!((bb.height() - 4.0).abs() < 1e-3);
        assert!(bb.center().length() < 1e-3);
    }
}
