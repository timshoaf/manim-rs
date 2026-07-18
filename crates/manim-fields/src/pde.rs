//! Uniform-grid PDE steppers: explicit heat / wave finite differences and
//! spectral split-step Schrödinger evolution.
//!
//! Everything here works in `f64`, is deterministic, and performs no I/O. All
//! spatial domains are **periodic** (the natural boundary for both the FTCS
//! stencils and the Fourier method used below), so a grid of `n` points spans a
//! box of length `L = n · dx` with the point at index `n` identified with index
//! `0`.
//!
//! - [`Heat1D`] / [`Heat2D`] — explicit forward-time centred-space (FTCS)
//!   integrators for `u_t = α ∇²u`. Conditionally stable: in 1-D the diffusion
//!   number `α dt / dx²` must not exceed `1/2` (`1/4` per axis in 2-D).
//! - [`Wave1D`] — second-order leapfrog for `u_tt = c² u_xx`. Stable under the
//!   CFL condition `c dt / dx ≤ 1`.
//! - [`Schrodinger1D`] / [`Schrodinger2D`] — Strang split-step Fourier
//!   integrators for the time-dependent Schrödinger equation (ħ = 1)
//!   `i ψ_t = −1/(2m) ∇²ψ + V ψ`. Each step is norm-preserving to machine
//!   precision.
//!
//! ```
//! use manim_fields::pde::Heat1D;
//! // A single hot cell diffuses into its neighbours; the total heat is conserved.
//! let mut h = Heat1D::new(vec![0.0, 0.0, 1.0, 0.0, 0.0], 1.0, 1.0);
//! let before: f64 = h.u().iter().sum();
//! h.step(0.4);
//! let after: f64 = h.u().iter().sum();
//! assert!((before - after).abs() < 1e-12);
//! ```

use std::sync::Arc;

use rustfft::{Fft, FftPlanner};

/// Re-exported [`rustfft`] complex number type (`Complex<f64>`), used for the
/// wavefunction buffers and constructors of the Schrödinger steppers.
pub use rustfft::num_complex::Complex;

/// The signed FFT wavenumber for bin `i` of an `n`-point transform on a box of
/// length `len`: `k = 2π/len · (i or i−n)`, with the second half of the
/// spectrum interpreted as negative frequencies.
fn wavenumber(i: usize, n: usize, len: f64) -> f64 {
    let signed = if i <= n / 2 {
        i as f64
    } else {
        i as f64 - n as f64
    };
    2.0 * std::f64::consts::PI * signed / len
}

/// `e^{iθ}` as a `Complex<f64>`.
#[inline]
fn cis(theta: f64) -> Complex<f64> {
    Complex::new(theta.cos(), theta.sin())
}

// ===========================================================================
// Heat equation
// ===========================================================================

/// Explicit FTCS integrator for the 1-D heat equation `u_t = α u_xx` on a
/// periodic grid.
///
/// Each step applies `u_i ← u_i + r (u_{i+1} − 2u_i + u_{i−1})` with the
/// diffusion number `r = α dt / dx²`. The scheme is **stable only when
/// `r ≤ 1/2`**; larger steps grow high-frequency modes without bound.
///
/// ```
/// use manim_fields::pde::Heat1D;
/// let mut h = Heat1D::new(vec![1.0, 0.0, 0.0, 0.0], 0.5, 1.0);
/// let m0 = h.u().iter().cloned().fold(f64::MIN, f64::max);
/// h.step(0.1);
/// let m1 = h.u().iter().cloned().fold(f64::MIN, f64::max);
/// // Diffusion never increases the peak value.
/// assert!(m1 <= m0 + 1e-12);
/// ```
#[derive(Clone, Debug)]
pub struct Heat1D {
    u: Vec<f64>,
    scratch: Vec<f64>,
    dx: f64,
    alpha: f64,
}

impl Heat1D {
    /// Builds a stepper from an initial field `u`, grid spacing `dx`, and
    /// diffusivity `alpha`.
    pub fn new(u: Vec<f64>, dx: f64, alpha: f64) -> Self {
        let n = u.len();
        Self {
            u,
            scratch: vec![0.0; n],
            dx,
            alpha,
        }
    }

    /// The current field.
    pub fn u(&self) -> &[f64] {
        &self.u
    }

    /// The grid spacing.
    pub fn dx(&self) -> f64 {
        self.dx
    }

    /// The diffusion number `α dt / dx²` for a proposed step `dt`; keep it
    /// `≤ 1/2` for stability.
    pub fn diffusion_number(&self, dt: f64) -> f64 {
        self.alpha * dt / (self.dx * self.dx)
    }

    /// Advances the field by one explicit FTCS step of size `dt` (periodic BC).
    pub fn step(&mut self, dt: f64) {
        let n = self.u.len();
        if n == 0 {
            return;
        }
        let r = self.diffusion_number(dt);
        for i in 0..n {
            let left = self.u[(i + n - 1) % n];
            let right = self.u[(i + 1) % n];
            self.scratch[i] = self.u[i] + r * (left - 2.0 * self.u[i] + right);
        }
        std::mem::swap(&mut self.u, &mut self.scratch);
    }
}

/// Explicit FTCS integrator for the 2-D heat equation `u_t = α (u_xx + u_yy)`
/// on a periodic `nx × ny` grid stored row-major (`u[iy·nx + ix]`).
///
/// Stability requires `α dt (1/dx² + 1/dy²) ≤ 1/2` (i.e. `1/4` per axis on a
/// square grid).
///
/// ```
/// use manim_fields::pde::Heat2D;
/// let mut u = vec![0.0; 9];
/// u[4] = 1.0; // centre of a 3×3 grid
/// let mut h = Heat2D::new(u, 3, 3, 1.0, 1.0, 1.0);
/// let before: f64 = h.u().iter().sum();
/// h.step(0.1);
/// let after: f64 = h.u().iter().sum();
/// assert!((before - after).abs() < 1e-12); // heat conserved
/// ```
#[derive(Clone, Debug)]
pub struct Heat2D {
    u: Vec<f64>,
    scratch: Vec<f64>,
    nx: usize,
    ny: usize,
    dx: f64,
    dy: f64,
    alpha: f64,
}

impl Heat2D {
    /// Builds a stepper from a row-major `nx × ny` field, spacings `dx`/`dy`,
    /// and diffusivity `alpha`.
    ///
    /// # Panics
    /// Panics if `u.len() != nx * ny`.
    pub fn new(u: Vec<f64>, nx: usize, ny: usize, dx: f64, dy: f64, alpha: f64) -> Self {
        assert_eq!(u.len(), nx * ny, "field length must equal nx * ny");
        let n = u.len();
        Self {
            u,
            scratch: vec![0.0; n],
            nx,
            ny,
            dx,
            dy,
            alpha,
        }
    }

    /// The current field, row-major.
    pub fn u(&self) -> &[f64] {
        &self.u
    }

    /// Grid width (number of columns).
    pub fn nx(&self) -> usize {
        self.nx
    }

    /// Grid height (number of rows).
    pub fn ny(&self) -> usize {
        self.ny
    }

    /// The combined diffusion number `α dt (1/dx² + 1/dy²)` for a step `dt`;
    /// keep it `≤ 1/2` for stability.
    pub fn diffusion_number(&self, dt: f64) -> f64 {
        self.alpha * dt * (1.0 / (self.dx * self.dx) + 1.0 / (self.dy * self.dy))
    }

    /// Advances the field by one explicit FTCS step of size `dt` (periodic BC).
    pub fn step(&mut self, dt: f64) {
        let (nx, ny) = (self.nx, self.ny);
        if nx == 0 || ny == 0 {
            return;
        }
        let rx = self.alpha * dt / (self.dx * self.dx);
        let ry = self.alpha * dt / (self.dy * self.dy);
        for iy in 0..ny {
            let ym = ((iy + ny - 1) % ny) * nx;
            let yp = ((iy + 1) % ny) * nx;
            let y0 = iy * nx;
            for ix in 0..nx {
                let xm = (ix + nx - 1) % nx;
                let xp = (ix + 1) % nx;
                let c = self.u[y0 + ix];
                let lap_x = self.u[y0 + xm] - 2.0 * c + self.u[y0 + xp];
                let lap_y = self.u[ym + ix] - 2.0 * c + self.u[yp + ix];
                self.scratch[y0 + ix] = c + rx * lap_x + ry * lap_y;
            }
        }
        std::mem::swap(&mut self.u, &mut self.scratch);
    }
}

// ===========================================================================
// Wave equation
// ===========================================================================

/// Second-order leapfrog integrator for the 1-D wave equation
/// `u_tt = c² u_xx` on a periodic grid.
///
/// The update is `u^{n+1}_i = 2u^n_i − u^{n−1}_i + s² (u^n_{i+1} − 2u^n_i +
/// u^n_{i−1})` with Courant number `s = c dt / dx`. Stable under the CFL
/// condition `s ≤ 1`. The stepper stores the current and previous fields; the
/// previous field is seeded from the initial displacement and velocity by a
/// one-sided Taylor half-step (`u^{−1}_i = u^0_i − dt·v_i + ½ s² ∇²u^0_i`) on
/// the first call to [`step`](Self::step), so the scheme starts second-order
/// accurate for whatever `dt` is first used.
///
/// ```
/// use manim_fields::pde::Wave1D;
/// // A field released from rest stays bounded by its initial amplitude.
/// let n = 32;
/// let u0: Vec<f64> = (0..n)
///     .map(|i| (2.0 * std::f64::consts::PI * i as f64 / n as f64).sin())
///     .collect();
/// let mut w = Wave1D::at_rest(u0, 1.0, 0.5);
/// for _ in 0..10 {
///     w.step(0.5);
/// }
/// assert!(w.u().iter().all(|v| v.abs() <= 1.5));
/// ```
#[derive(Clone, Debug)]
pub struct Wave1D {
    u: Vec<f64>,
    u_prev: Vec<f64>,
    scratch: Vec<f64>,
    velocity: Vec<f64>,
    seeded: bool,
    dx: f64,
    c: f64,
}

impl Wave1D {
    /// Builds a stepper from an initial displacement `u`, initial velocity
    /// `velocity` (same length), grid spacing `dx`, and wave speed `c`.
    ///
    /// # Panics
    /// Panics if `velocity.len() != u.len()`.
    pub fn new(u: Vec<f64>, velocity: &[f64], dx: f64, c: f64) -> Self {
        assert_eq!(velocity.len(), u.len(), "velocity length must match field");
        let n = u.len();
        Self {
            u_prev: vec![0.0; n],
            scratch: vec![0.0; n],
            velocity: velocity.to_vec(),
            seeded: false,
            u,
            dx,
            c,
        }
    }

    /// Builds a stepper released from rest (zero initial velocity).
    pub fn at_rest(u: Vec<f64>, dx: f64, c: f64) -> Self {
        let n = u.len();
        Self::new(u, &vec![0.0; n], dx, c)
    }

    /// The current field.
    pub fn u(&self) -> &[f64] {
        &self.u
    }

    /// The Courant number `c dt / dx` for a proposed step `dt`; keep it `≤ 1`.
    pub fn courant(&self, dt: f64) -> f64 {
        self.c * dt / self.dx
    }

    fn laplacian(field: &[f64], i: usize, n: usize) -> f64 {
        field[(i + n - 1) % n] - 2.0 * field[i] + field[(i + 1) % n]
    }

    /// Advances the field by one leapfrog step of size `dt` (periodic BC).
    pub fn step(&mut self, dt: f64) {
        let n = self.u.len();
        if n == 0 {
            return;
        }
        let s2 = self.courant(dt).powi(2);
        if !self.seeded {
            // Seed u^{-1} consistent with the initial velocity:
            // u^{-1}_i = u^0_i − dt·v_i + ½ s² ∇²u^0_i.
            for i in 0..n {
                let lap = Self::laplacian(&self.u, i, n);
                self.u_prev[i] = self.u[i] - dt * self.velocity[i] + 0.5 * s2 * lap;
            }
            self.seeded = true;
        }
        for i in 0..n {
            let lap = Self::laplacian(&self.u, i, n);
            self.scratch[i] = 2.0 * self.u[i] - self.u_prev[i] + s2 * lap;
        }
        std::mem::swap(&mut self.u_prev, &mut self.u);
        std::mem::swap(&mut self.u, &mut self.scratch);
    }
}

// ===========================================================================
// Schrödinger equation (split-step Fourier)
// ===========================================================================

/// Split-step Fourier integrator for the 1-D time-dependent Schrödinger
/// equation (ħ = 1) `i ψ_t = −1/(2m) ψ_xx + V(x) ψ` on a periodic box.
///
/// Each [`step`](Self::step) applies a symmetric Strang splitting:
///
/// 1. half potential phase `ψ ← e^{−i V dt/2} ψ`,
/// 2. forward FFT to momentum space,
/// 3. full kinetic phase `ψ̂ ← e^{−i k²/(2m) dt} ψ̂`,
/// 4. inverse FFT (with the `1/N` normalisation `rustfft` omits),
/// 5. half potential phase again.
///
/// The kinetic step is diagonal and unitary in Fourier space and the potential
/// steps are diagonal and unitary in real space, so the norm is preserved to
/// machine precision. For a free particle (`V ≡ 0`) the method is exact up to
/// the spectral representation of the initial data.
///
/// ```
/// use manim_fields::pde::{Complex, Schrodinger1D};
/// // A zero-momentum Gaussian on a free line keeps its norm.
/// let n = 128;
/// let dx = 0.2;
/// let x_min = -(n as f64) * dx / 2.0;
/// let mut s = Schrodinger1D::from_fn(
///     n,
///     x_min,
///     dx,
///     1.0,
///     |_x| 0.0,
///     |x| Complex::new((-x * x / 2.0).exp(), 0.0),
/// );
/// let n0 = s.norm();
/// for _ in 0..20 {
///     s.step(0.05);
/// }
/// assert!(((s.norm() - n0) / n0).abs() < 1e-9);
/// ```
pub struct Schrodinger1D {
    psi: Vec<Complex<f64>>,
    x: Vec<f64>,
    v: Vec<f64>,
    /// `k²/(2m)` per FFT bin — the kinetic angular frequency.
    kinetic: Vec<f64>,
    dx: f64,
    fft_fwd: Arc<dyn Fft<f64>>,
    fft_inv: Arc<dyn Fft<f64>>,
    n: usize,
}

impl Schrodinger1D {
    /// Builds a stepper from an explicit wavefunction sample `psi`, the left
    /// edge `x_min`, grid spacing `dx`, particle mass `mass`, and a per-point
    /// potential `potential` (all vectors length `psi.len()`).
    ///
    /// # Panics
    /// Panics if `potential.len() != psi.len()` or the grid is empty.
    pub fn new(
        psi: Vec<Complex<f64>>,
        x_min: f64,
        dx: f64,
        mass: f64,
        potential: Vec<f64>,
    ) -> Self {
        let n = psi.len();
        assert!(n > 0, "grid must be non-empty");
        assert_eq!(potential.len(), n, "potential length must match psi");
        let x: Vec<f64> = (0..n).map(|i| x_min + i as f64 * dx).collect();
        let len = n as f64 * dx;
        let kinetic: Vec<f64> = (0..n)
            .map(|i| {
                let k = wavenumber(i, n, len);
                k * k / (2.0 * mass)
            })
            .collect();
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(n);
        let fft_inv = planner.plan_fft_inverse(n);
        Self {
            psi,
            x,
            v: potential,
            kinetic,
            dx,
            fft_fwd,
            fft_inv,
            n,
        }
    }

    /// Builds a stepper by sampling closures for the potential and the initial
    /// wavefunction on an `n`-point grid starting at `x_min` with spacing `dx`.
    pub fn from_fn(
        n: usize,
        x_min: f64,
        dx: f64,
        mass: f64,
        potential: impl Fn(f64) -> f64,
        psi0: impl Fn(f64) -> Complex<f64>,
    ) -> Self {
        let mut psi = Vec::with_capacity(n);
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let x = x_min + i as f64 * dx;
            psi.push(psi0(x));
            v.push(potential(x));
        }
        Self::new(psi, x_min, dx, mass, v)
    }

    /// The grid coordinates.
    pub fn x(&self) -> &[f64] {
        &self.x
    }

    /// The probability density `|ψ|²` at each grid point.
    pub fn probability_density(&self) -> Vec<f64> {
        self.psi.iter().map(|z| z.norm_sqr()).collect()
    }

    /// The total probability `∫|ψ|² dx ≈ Σ|ψ_i|² · dx`.
    pub fn norm(&self) -> f64 {
        self.psi.iter().map(|z| z.norm_sqr()).sum::<f64>() * self.dx
    }

    /// The position variance `σ² = ⟨x²⟩ − ⟨x⟩²`, weighting each grid point by
    /// `|ψ|²`.
    pub fn position_variance(&self) -> f64 {
        let mut total = 0.0;
        let mut mean = 0.0;
        let mut mean_sq = 0.0;
        for (xi, z) in self.x.iter().zip(&self.psi) {
            let w = z.norm_sqr();
            total += w;
            mean += xi * w;
            mean_sq += xi * xi * w;
        }
        if total == 0.0 {
            return 0.0;
        }
        mean /= total;
        mean_sq /= total;
        mean_sq - mean * mean
    }

    /// Advances `ψ` by one Strang split-step of size `dt`.
    pub fn step(&mut self, dt: f64) {
        // Half potential kick.
        for (z, &vi) in self.psi.iter_mut().zip(&self.v) {
            *z *= cis(-0.5 * vi * dt);
        }
        // Kinetic drift in Fourier space.
        self.fft_fwd.process(&mut self.psi);
        for (z, &kin) in self.psi.iter_mut().zip(&self.kinetic) {
            *z *= cis(-kin * dt);
        }
        self.fft_inv.process(&mut self.psi);
        let inv_n = 1.0 / self.n as f64;
        for z in self.psi.iter_mut() {
            *z *= inv_n;
        }
        // Half potential kick.
        for (z, &vi) in self.psi.iter_mut().zip(&self.v) {
            *z *= cis(-0.5 * vi * dt);
        }
    }
}

/// Split-step Fourier integrator for the 2-D time-dependent Schrödinger
/// equation (ħ = 1) `i ψ_t = −1/(2m) ∇²ψ + V ψ` on a periodic `nx × ny` box,
/// with the wavefunction stored row-major.
///
/// The 2-D transform is realised as 1-D FFTs along the rows (length `nx`)
/// followed by 1-D FFTs down the columns (length `ny`); the inverse divides by
/// `nx · ny`. As in 1-D, every step is unitary and conserves the norm.
///
/// ```
/// use manim_fields::pde::{Complex, Schrodinger2D};
/// let (nx, ny) = (16, 16);
/// let dx = 0.4;
/// let mut s = Schrodinger2D::from_fn(
///     nx,
///     ny,
///     -(nx as f64) * dx / 2.0,
///     -(ny as f64) * dx / 2.0,
///     dx,
///     dx,
///     1.0,
///     |_x, _y| 0.0,
///     |x, y| Complex::new((-(x * x + y * y) / 2.0).exp(), 0.0),
/// );
/// let n0 = s.norm();
/// s.step(0.05);
/// assert!(((s.norm() - n0) / n0).abs() < 1e-9);
/// ```
pub struct Schrodinger2D {
    psi: Vec<Complex<f64>>,
    v: Vec<f64>,
    /// `(kx² + ky²)/(2m)` per grid point, row-major.
    kinetic: Vec<f64>,
    nx: usize,
    ny: usize,
    dx: f64,
    dy: f64,
    fft_row_fwd: Arc<dyn Fft<f64>>,
    fft_row_inv: Arc<dyn Fft<f64>>,
    fft_col_fwd: Arc<dyn Fft<f64>>,
    fft_col_inv: Arc<dyn Fft<f64>>,
}

impl Schrodinger2D {
    /// Builds a stepper from a row-major wavefunction `psi`, the lower-left
    /// corner `(x_min, y_min)`, spacings `dx`/`dy`, particle mass `mass`, and a
    /// row-major potential `potential` (both vectors length `nx·ny`).
    ///
    /// # Panics
    /// Panics if either vector's length differs from `nx · ny`, or the grid is
    /// empty.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        psi: Vec<Complex<f64>>,
        nx: usize,
        ny: usize,
        x_min: f64,
        y_min: f64,
        dx: f64,
        dy: f64,
        mass: f64,
        potential: Vec<f64>,
    ) -> Self {
        let _ = (x_min, y_min);
        assert!(nx > 0 && ny > 0, "grid must be non-empty");
        assert_eq!(psi.len(), nx * ny, "psi length must equal nx * ny");
        assert_eq!(
            potential.len(),
            nx * ny,
            "potential length must equal nx * ny"
        );
        let lx = nx as f64 * dx;
        let ly = ny as f64 * dy;
        let kx: Vec<f64> = (0..nx).map(|i| wavenumber(i, nx, lx)).collect();
        let ky: Vec<f64> = (0..ny).map(|j| wavenumber(j, ny, ly)).collect();
        let mut kinetic = vec![0.0; nx * ny];
        for iy in 0..ny {
            for ix in 0..nx {
                kinetic[iy * nx + ix] = (kx[ix] * kx[ix] + ky[iy] * ky[iy]) / (2.0 * mass);
            }
        }
        let mut planner = FftPlanner::new();
        let fft_row_fwd = planner.plan_fft_forward(nx);
        let fft_row_inv = planner.plan_fft_inverse(nx);
        let fft_col_fwd = planner.plan_fft_forward(ny);
        let fft_col_inv = planner.plan_fft_inverse(ny);
        Self {
            psi,
            v: potential,
            kinetic,
            nx,
            ny,
            dx,
            dy,
            fft_row_fwd,
            fft_row_inv,
            fft_col_fwd,
            fft_col_inv,
        }
    }

    /// Builds a stepper by sampling closures for the potential and the initial
    /// wavefunction on the `nx × ny` grid.
    #[allow(clippy::too_many_arguments)]
    pub fn from_fn(
        nx: usize,
        ny: usize,
        x_min: f64,
        y_min: f64,
        dx: f64,
        dy: f64,
        mass: f64,
        potential: impl Fn(f64, f64) -> f64,
        psi0: impl Fn(f64, f64) -> Complex<f64>,
    ) -> Self {
        let mut psi = vec![Complex::new(0.0, 0.0); nx * ny];
        let mut v = vec![0.0; nx * ny];
        for iy in 0..ny {
            let y = y_min + iy as f64 * dy;
            for ix in 0..nx {
                let x = x_min + ix as f64 * dx;
                psi[iy * nx + ix] = psi0(x, y);
                v[iy * nx + ix] = potential(x, y);
            }
        }
        Self::new(psi, nx, ny, x_min, y_min, dx, dy, mass, v)
    }

    /// Grid width (columns).
    pub fn nx(&self) -> usize {
        self.nx
    }

    /// Grid height (rows).
    pub fn ny(&self) -> usize {
        self.ny
    }

    /// The probability density `|ψ|²` at each grid point, row-major.
    pub fn probability_density(&self) -> Vec<f64> {
        self.psi.iter().map(|z| z.norm_sqr()).collect()
    }

    /// The total probability `∫|ψ|² dx dy ≈ Σ|ψ|² · dx · dy`.
    pub fn norm(&self) -> f64 {
        self.psi.iter().map(|z| z.norm_sqr()).sum::<f64>() * self.dx * self.dy
    }

    fn fft_rows(&self, fft: &Arc<dyn Fft<f64>>) -> Vec<Complex<f64>> {
        // Rows are contiguous in row-major storage.
        let mut buf = self.psi.clone();
        for row in buf.chunks_mut(self.nx) {
            fft.process(row);
        }
        buf
    }

    fn fft_cols_in_place(&self, buf: &mut [Complex<f64>], fft: &Arc<dyn Fft<f64>>) {
        let mut col = vec![Complex::new(0.0, 0.0); self.ny];
        for ix in 0..self.nx {
            for iy in 0..self.ny {
                col[iy] = buf[iy * self.nx + ix];
            }
            fft.process(&mut col);
            for iy in 0..self.ny {
                buf[iy * self.nx + ix] = col[iy];
            }
        }
    }

    /// Advances `ψ` by one Strang split-step of size `dt`.
    pub fn step(&mut self, dt: f64) {
        // Half potential kick.
        for (z, &vi) in self.psi.iter_mut().zip(&self.v) {
            *z *= cis(-0.5 * vi * dt);
        }
        // Forward 2-D FFT: rows then columns.
        let mut buf = self.fft_rows(&self.fft_row_fwd);
        self.fft_cols_in_place(&mut buf, &self.fft_col_fwd);
        // Kinetic drift.
        for (z, &kin) in buf.iter_mut().zip(&self.kinetic) {
            *z *= cis(-kin * dt);
        }
        // Inverse 2-D FFT: rows then columns, then the 1/(nx·ny) normalisation.
        for row in buf.chunks_mut(self.nx) {
            self.fft_row_inv.process(row);
        }
        self.fft_cols_in_place(&mut buf, &self.fft_col_inv);
        let inv = 1.0 / (self.nx as f64 * self.ny as f64);
        for z in buf.iter_mut() {
            *z *= inv;
        }
        self.psi = buf;
        // Half potential kick.
        for (z, &vi) in self.psi.iter_mut().zip(&self.v) {
            *z *= cis(-0.5 * vi * dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn heat_conserves_total_and_bounds_peak() {
        let mut u = vec![0.0; 50];
        u[25] = 1.0;
        let mut h = Heat1D::new(u, 0.1, 1.0);
        let total0: f64 = h.u().iter().sum();
        let peak0 = h.u().iter().cloned().fold(f64::MIN, f64::max);
        for _ in 0..200 {
            h.step(0.004); // r = 1*0.004/0.01 = 0.4 ≤ 0.5
        }
        let total1: f64 = h.u().iter().sum();
        let peak1 = h.u().iter().cloned().fold(f64::MIN, f64::max);
        assert!((total0 - total1).abs() < 1e-10, "heat not conserved");
        assert!(peak1 < peak0, "peak did not decay");
    }

    #[test]
    fn heat_kernel_matches_analytic_width() {
        // Gaussian initial data on a large periodic box; variance should grow
        // as σ²(t) = σ0² + 2 α t.
        let n = 400usize;
        let dx = 0.1;
        let x_min = -(n as f64) * dx / 2.0;
        let sigma0 = 1.0;
        let alpha = 1.0;
        let u: Vec<f64> = (0..n)
            .map(|i| {
                let x = x_min + i as f64 * dx;
                (-x * x / (2.0 * sigma0 * sigma0)).exp()
            })
            .collect();
        let mut h = Heat1D::new(u, dx, alpha);

        let variance = |field: &[f64]| -> f64 {
            let mut total = 0.0;
            let mut mean = 0.0;
            let mut mean_sq = 0.0;
            for (i, &w) in field.iter().enumerate() {
                let x = x_min + i as f64 * dx;
                total += w;
                mean += x * w;
                mean_sq += x * x * w;
            }
            mean /= total;
            mean_sq /= total;
            mean_sq - mean * mean
        };

        let var0 = variance(h.u());
        assert!((var0 - sigma0 * sigma0).abs() < 1e-3);

        let dt = 0.004; // r = 0.4
        let steps = 250;
        for _ in 0..steps {
            h.step(dt);
        }
        let t = dt * steps as f64; // = 1.0
        let observed = variance(h.u());
        let analytic = sigma0 * sigma0 + 2.0 * alpha * t;
        let rel = (observed - analytic).abs() / analytic;
        println!("heat: observed var = {observed:.5}, analytic = {analytic:.5}, rel = {rel:.2e}");
        assert!(rel < 0.03, "heat width off by {rel:.3}");
    }

    #[test]
    fn heat2d_conserves_total() {
        let (nx, ny) = (20, 20);
        let mut u = vec![0.0; nx * ny];
        u[10 * nx + 10] = 1.0;
        let mut h = Heat2D::new(u, nx, ny, 0.1, 0.1, 1.0);
        let total0: f64 = h.u().iter().sum();
        for _ in 0..100 {
            h.step(0.001); // per-axis r = 0.1 each, sum 0.2 ≤ 0.5
        }
        let total1: f64 = h.u().iter().sum();
        assert!((total0 - total1).abs() < 1e-10);
    }

    #[test]
    fn wave_standing_mode_returns_after_one_period() {
        // u(x,0)=sin(kx), released from rest -> u(x,t)=cos(ωt)sin(kx),
        // ω = c k, period T = 2π/ω. After one period the field returns.
        let n = 128usize;
        let l = 1.0;
        let dx = l / n as f64;
        let c = 1.0;
        let k = 2.0 * PI / l;
        let omega = c * k;
        let period = 2.0 * PI / omega;
        let u0: Vec<f64> = (0..n).map(|i| (k * i as f64 * dx).sin()).collect();
        let mut w = Wave1D::at_rest(u0.clone(), dx, c);
        // CFL: s = c dt / dx. Choose s = 0.5.
        let dt = 0.5 * dx / c;
        let steps = (period / dt).round() as usize;
        for _ in 0..steps {
            w.step(dt);
        }
        let err: f64 = w
            .u()
            .iter()
            .zip(&u0)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        println!("wave: max err after one period = {err:.2e} over {steps} steps");
        assert!(err < 2e-2, "standing wave did not recur: {err}");
    }

    fn gaussian_1d(x: f64, x0: f64, sigma0: f64) -> Complex<f64> {
        let norm = (2.0 * PI * sigma0 * sigma0).powf(-0.25);
        let dx = x - x0;
        Complex::new(norm * (-dx * dx / (4.0 * sigma0 * sigma0)).exp(), 0.0)
    }

    #[test]
    fn schrodinger_free_particle_dispersion() {
        // Zero-momentum Gaussian on a free line spreads as
        // σ(t) = σ0 sqrt(1 + (t/(2 m σ0²))²).
        let n = 1024usize;
        let dx = 80.0 / n as f64;
        let x_min = -40.0;
        let mass = 1.0;
        let sigma0 = 1.0;
        let mut s = Schrodinger1D::from_fn(
            n,
            x_min,
            dx,
            mass,
            |_x| 0.0,
            |x| gaussian_1d(x, 0.0, sigma0),
        );

        let var0 = s.position_variance();
        println!(
            "schrodinger: σ(0) observed = {:.5}, analytic = {:.5}",
            var0.sqrt(),
            sigma0
        );
        assert!(((var0.sqrt() - sigma0) / sigma0).abs() < 1e-3);

        let dt = 0.02;
        let mut t = 0.0;
        for &target in &[1.0f64, 2.0, 4.0] {
            while t < target - 1e-9 {
                s.step(dt);
                t += dt;
            }
            let sigma = s.position_variance().sqrt();
            let analytic =
                sigma0 * (1.0 + (target / (2.0 * mass * sigma0 * sigma0)).powi(2)).sqrt();
            let rel = (sigma - analytic).abs() / analytic;
            println!(
                "schrodinger: t = {target:.1}  σ observed = {sigma:.5}  analytic = {analytic:.5}  rel = {rel:.2e}"
            );
            assert!(rel < 1e-2, "dispersion off at t={target}: rel={rel:.3e}");
        }
    }

    #[test]
    fn schrodinger_norm_conserved_free() {
        let n = 512usize;
        let dx = 60.0 / n as f64;
        let x_min = -30.0;
        let mut s =
            Schrodinger1D::from_fn(n, x_min, dx, 1.0, |_x| 0.0, |x| gaussian_1d(x, 0.0, 1.0));
        let n0 = s.norm();
        for _ in 0..1000 {
            s.step(0.01);
        }
        let drift = (s.norm() - n0).abs() / n0;
        println!("schrodinger free norm drift = {drift:.2e}");
        assert!(drift < 1e-6, "norm drifted: {drift:.3e}");
    }

    #[test]
    fn schrodinger_norm_conserved_harmonic() {
        // Harmonic potential V = ½ ω² x² exercises the potential kick.
        let n = 512usize;
        let dx = 40.0 / n as f64;
        let x_min = -20.0;
        let omega: f64 = 1.0;
        let mut s = Schrodinger1D::from_fn(
            n,
            x_min,
            dx,
            1.0,
            move |x| 0.5 * omega * omega * x * x,
            // Coherent state displaced from the origin -> it sloshes.
            |x| gaussian_1d(x, 2.0, 1.0),
        );
        let n0 = s.norm();
        for _ in 0..2000 {
            s.step(0.005);
        }
        let drift = (s.norm() - n0).abs() / n0;
        println!("schrodinger harmonic norm drift = {drift:.2e}");
        assert!(drift < 1e-6, "harmonic norm drifted: {drift:.3e}");
    }

    #[test]
    fn schrodinger2d_constructs_and_conserves_norm() {
        let (nx, ny) = (32usize, 32usize);
        let dx = 0.5;
        let mut s = Schrodinger2D::from_fn(
            nx,
            ny,
            -(nx as f64) * dx / 2.0,
            -(ny as f64) * dx / 2.0,
            dx,
            dx,
            1.0,
            |_x, _y| 0.0,
            |x, y| Complex::new((-(x * x + y * y) / 2.0).exp(), 0.0),
        );
        let n0 = s.norm();
        for _ in 0..50 {
            s.step(0.02);
        }
        let drift = (s.norm() - n0).abs() / n0;
        println!("schrodinger2d norm drift = {drift:.2e}");
        assert!(drift < 1e-6, "2D norm drifted: {drift:.3e}");
    }
}
