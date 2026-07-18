//! ODE integrators for `manim-fields`.
//!
//! This module provides three families of fixed- and adaptive-step ordinary
//! differential equation solvers, all in pure `f64` and free of I/O:
//!
//! - Classic 4th-order Runge–Kutta ([`rk4_step`], [`rk4`]) for a general
//!   first-order system `dy/dt = f(t, y)` with the state held as a slice.
//! - Adaptive Dormand–Prince 5(4) ([`rk45`]), an embedded pair with automatic
//!   step-size control from absolute/relative tolerances.
//! - Symplectic integrators ([`leapfrog`], [`yoshida4`]) for a separable
//!   Hamiltonian `H = |p|²/(2m) + V(q)` driven by a force `F(q) = -∇V(q)`. These
//!   conserve energy (up to bounded oscillation) over long integrations far
//!   better than a non-symplectic method of comparable order.
//!
//! The RK methods take a closure `f(t, &[f64]) -> Vec<f64>` returning the time
//! derivative of the state; the symplectic methods take a force closure
//! `F(&[f64]) -> Vec<f64>` returning `-∇V` at a configuration.

/// Compute `y + s * k` component-wise into a fresh vector.
fn add_scaled(y: &[f64], s: f64, k: &[f64]) -> Vec<f64> {
    y.iter().zip(k).map(|(yi, ki)| yi + s * ki).collect()
}

/// Perform one classic 4th-order Runge–Kutta step of `dy/dt = f(t, y)`.
///
/// Advances the state `y` at time `t` by a step of size `h`, returning the new
/// state. The closure `f` maps `(t, y)` to the time derivative `dy/dt`.
///
/// ```
/// use manim_fields::integrate::rk4_step;
/// // dy/dt = y, one step from y(0) = 1.
/// let f = |_t: f64, y: &[f64]| vec![y[0]];
/// let y1 = rk4_step(&f, 0.0, &[1.0], 0.1);
/// // RK4 matches e^0.1 to ~O(h^5).
/// assert!((y1[0] - 0.1_f64.exp()).abs() < 1e-6);
/// ```
pub fn rk4_step<F: Fn(f64, &[f64]) -> Vec<f64>>(f: &F, t: f64, y: &[f64], h: f64) -> Vec<f64> {
    let k1 = f(t, y);
    let k2 = f(t + 0.5 * h, &add_scaled(y, 0.5 * h, &k1));
    let k3 = f(t + 0.5 * h, &add_scaled(y, 0.5 * h, &k2));
    let k4 = f(t + h, &add_scaled(y, h, &k3));
    y.iter()
        .enumerate()
        .map(|(i, yi)| yi + (h / 6.0) * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]))
        .collect()
}

/// Integrate `dy/dt = f(t, y)` with `steps` fixed RK4 steps of size `h`.
///
/// Starts at `(t0, y0)` and returns the final state after `steps` steps (i.e. at
/// time `t0 + steps * h`).
///
/// ```
/// use manim_fields::integrate::rk4;
/// // dy/dt = y from 0 to 1 in 1000 steps ≈ e.
/// let f = |_t: f64, y: &[f64]| vec![y[0]];
/// let y = rk4(&f, 0.0, &[1.0], 1e-3, 1000);
/// assert!((y[0] - 1.0_f64.exp()).abs() < 1e-6);
/// ```
pub fn rk4<F: Fn(f64, &[f64]) -> Vec<f64>>(
    f: &F,
    t0: f64,
    y0: &[f64],
    h: f64,
    steps: usize,
) -> Vec<f64> {
    let mut y = y0.to_vec();
    let mut t = t0;
    for _ in 0..steps {
        y = rk4_step(f, t, &y, h);
        t += h;
    }
    y
}

/// Adaptive Dormand–Prince 5(4) integration of `dy/dt = f(t, y)` from `t0` to `t1`.
///
/// Uses the standard Dormand–Prince embedded pair: a 5th-order solution advances
/// the state while a 4th-order companion estimates the local error. The step size
/// is grown or shrunk to keep the scaled error norm
/// `sqrt(mean((err_i / (atol + rtol * max(|y_i|, |y_new_i|)))^2))` near one, and
/// rejected steps are retried with a smaller `h`. The final step is clamped to
/// land exactly on `t1`. Assumes `t1 > t0`.
///
/// ```
/// use manim_fields::integrate::rk45;
/// // dy/dt = y from 0 to 1 with tight tolerances ≈ e.
/// let f = |_t: f64, y: &[f64]| vec![y[0]];
/// let y = rk45(&f, 0.0, &[1.0], 1.0, 1e-8, 1e-8);
/// assert!((y[0] - 1.0_f64.exp()).abs() < 1e-6);
/// ```
pub fn rk45<F: Fn(f64, &[f64]) -> Vec<f64>>(
    f: &F,
    t0: f64,
    y0: &[f64],
    t1: f64,
    atol: f64,
    rtol: f64,
) -> Vec<f64> {
    // Dormand–Prince nodes (c) and stage weights (a).
    const C2: f64 = 1.0 / 5.0;
    const C3: f64 = 3.0 / 10.0;
    const C4: f64 = 4.0 / 5.0;
    const C5: f64 = 8.0 / 9.0;

    const A21: f64 = 1.0 / 5.0;

    const A31: f64 = 3.0 / 40.0;
    const A32: f64 = 9.0 / 40.0;

    const A41: f64 = 44.0 / 45.0;
    const A42: f64 = -56.0 / 15.0;
    const A43: f64 = 32.0 / 9.0;

    const A51: f64 = 19372.0 / 6561.0;
    const A52: f64 = -25360.0 / 2187.0;
    const A53: f64 = 64448.0 / 6561.0;
    const A54: f64 = -212.0 / 729.0;

    const A61: f64 = 9017.0 / 3168.0;
    const A62: f64 = -355.0 / 33.0;
    const A63: f64 = 46732.0 / 5247.0;
    const A64: f64 = 49.0 / 176.0;
    const A65: f64 = -5103.0 / 18656.0;

    // 5th-order solution weights (also the 7th-stage node coefficients, FSAL).
    const B1: f64 = 35.0 / 384.0;
    const B3: f64 = 500.0 / 1113.0;
    const B4: f64 = 125.0 / 192.0;
    const B5: f64 = -2187.0 / 6784.0;
    const B6: f64 = 11.0 / 84.0;

    // 4th-order companion weights.
    const BS1: f64 = 5179.0 / 57600.0;
    const BS3: f64 = 7571.0 / 16695.0;
    const BS4: f64 = 393.0 / 640.0;
    const BS5: f64 = -92097.0 / 339200.0;
    const BS6: f64 = 187.0 / 2100.0;
    const BS7: f64 = 1.0 / 40.0;

    let n = y0.len();
    let mut y = y0.to_vec();
    let mut t = t0;
    let span = t1 - t0;
    let mut h = span / 100.0;

    while t < t1 {
        if t + h > t1 {
            h = t1 - t;
        }

        let k1 = f(t, &y);
        let k2 = f(t + C2 * h, &add_scaled(&y, h * A21, &k1));
        let y3: Vec<f64> = (0..n)
            .map(|i| y[i] + h * (A31 * k1[i] + A32 * k2[i]))
            .collect();
        let k3 = f(t + C3 * h, &y3);
        let y4: Vec<f64> = (0..n)
            .map(|i| y[i] + h * (A41 * k1[i] + A42 * k2[i] + A43 * k3[i]))
            .collect();
        let k4 = f(t + C4 * h, &y4);
        let y5: Vec<f64> = (0..n)
            .map(|i| y[i] + h * (A51 * k1[i] + A52 * k2[i] + A53 * k3[i] + A54 * k4[i]))
            .collect();
        let k5 = f(t + C5 * h, &y5);
        let y6: Vec<f64> = (0..n)
            .map(|i| {
                y[i] + h * (A61 * k1[i] + A62 * k2[i] + A63 * k3[i] + A64 * k4[i] + A65 * k5[i])
            })
            .collect();
        let k6 = f(t + h, &y6);

        // 5th-order candidate.
        let y_new: Vec<f64> = (0..n)
            .map(|i| y[i] + h * (B1 * k1[i] + B3 * k3[i] + B4 * k4[i] + B5 * k5[i] + B6 * k6[i]))
            .collect();
        let k7 = f(t + h, &y_new);

        // Local error estimate = 5th-order minus 4th-order solution.
        let err_norm = {
            let mut acc = 0.0;
            for i in 0..n {
                let y4th = y[i]
                    + h * (BS1 * k1[i]
                        + BS3 * k3[i]
                        + BS4 * k4[i]
                        + BS5 * k5[i]
                        + BS6 * k6[i]
                        + BS7 * k7[i]);
                let err = y_new[i] - y4th;
                let scale = atol + rtol * y[i].abs().max(y_new[i].abs());
                let ratio = err / scale;
                acc += ratio * ratio;
            }
            (acc / n as f64).sqrt()
        };

        if err_norm <= 1.0 {
            t += h;
            y = y_new;
        }

        // PI-ish step-size update: target err_norm ≈ 1, clamp growth/shrink.
        let factor = if err_norm > 0.0 {
            (0.9 * err_norm.powf(-0.2)).clamp(0.2, 5.0)
        } else {
            5.0
        };
        h *= factor;
    }

    y
}

/// Drift: advance configuration `q` by `c * p / mass`.
fn drift(q: &mut [f64], p: &[f64], mass: f64, c: f64) {
    for (qi, pi) in q.iter_mut().zip(p) {
        *qi += c * pi / mass;
    }
}

/// Kick: advance momentum `p` by `c * force`.
fn kick(p: &mut [f64], force: &[f64], c: f64) {
    for (pi, fi) in p.iter_mut().zip(force) {
        *pi += c * fi;
    }
}

/// Symplectic velocity-Verlet (leapfrog) integration of a separable Hamiltonian.
///
/// Integrates `dq/dt = p/mass`, `dp/dt = F(q)` with a kick–drift–kick scheme:
/// a half-kick on the momentum, a full drift on the configuration, and a second
/// half-kick using the force at the new configuration. This is a 2nd-order,
/// time-reversible, symplectic method that conserves the Hamiltonian to within a
/// bounded oscillation over long runs. Returns the final `(q, p)` after `steps`
/// steps of size `dt`.
///
/// ```
/// use manim_fields::integrate::leapfrog;
/// // 1-D harmonic oscillator: F(q) = -q, unit mass, starting at (q, p) = (1, 0).
/// let force = |q: &[f64]| vec![-q[0]];
/// let (q, p) = leapfrog(&force, &[1.0], &[0.0], 1.0, 1e-3, 1000);
/// // After t = 1, exact solution is (cos 1, -sin 1).
/// assert!((q[0] - 1.0_f64.cos()).abs() < 1e-3);
/// assert!((p[0] + 1.0_f64.sin()).abs() < 1e-3);
/// ```
pub fn leapfrog<Force: Fn(&[f64]) -> Vec<f64>>(
    force: &Force,
    q0: &[f64],
    p0: &[f64],
    mass: f64,
    dt: f64,
    steps: usize,
) -> (Vec<f64>, Vec<f64>) {
    let mut q = q0.to_vec();
    let mut p = p0.to_vec();
    for _ in 0..steps {
        kick(&mut p, &force(&q), 0.5 * dt);
        drift(&mut q, &p, mass, dt);
        kick(&mut p, &force(&q), 0.5 * dt);
    }
    (q, p)
}

/// Symplectic 4th-order Yoshida integration of a separable Hamiltonian.
///
/// Composes three time-scaled leapfrog substeps with the standard Yoshida
/// coefficients `w1 = 1/(2 - 2^(1/3))` and `w0 = -2^(1/3) · w1`. Merging the
/// adjacent drifts of the three DKD substeps yields the interleaved
/// drift/kick coefficient sequence
/// `[w1/2, w1, (w0+w1)/2, w0, (w0+w1)/2, w1, w1/2]`, which cancels the 2nd-order
/// error term and leaves a symplectic 4th-order method. Returns the final
/// `(q, p)` after `steps` steps of size `dt`.
///
/// ```
/// use manim_fields::integrate::yoshida4;
/// // 1-D harmonic oscillator: F(q) = -q, unit mass, starting at (q, p) = (1, 0).
/// let force = |q: &[f64]| vec![-q[0]];
/// let (q, p) = yoshida4(&force, &[1.0], &[0.0], 1.0, 0.05, 20);
/// // After t = 1, exact solution is (cos 1, -sin 1); 4th order is very accurate.
/// assert!((q[0] - 1.0_f64.cos()).abs() < 1e-5);
/// assert!((p[0] + 1.0_f64.sin()).abs() < 1e-5);
/// ```
pub fn yoshida4<Force: Fn(&[f64]) -> Vec<f64>>(
    force: &Force,
    q0: &[f64],
    p0: &[f64],
    mass: f64,
    dt: f64,
    steps: usize,
) -> (Vec<f64>, Vec<f64>) {
    let cbrt2 = 2.0_f64.powf(1.0 / 3.0);
    let w1 = 1.0 / (2.0 - cbrt2);
    let w0 = -cbrt2 * w1;

    // Interleaved drift/kick coefficients (times dt), starting and ending on a drift.
    let d0 = 0.5 * w1;
    let k0 = w1;
    let d1 = 0.5 * (w0 + w1);
    let k1 = w0;
    // d2 == d1, k2 == k0, d3 == d0 by symmetry.

    let mut q = q0.to_vec();
    let mut p = p0.to_vec();
    for _ in 0..steps {
        drift(&mut q, &p, mass, d0 * dt);
        kick(&mut p, &force(&q), k0 * dt);
        drift(&mut q, &p, mass, d1 * dt);
        kick(&mut p, &force(&q), k1 * dt);
        drift(&mut q, &p, mass, d1 * dt);
        kick(&mut p, &force(&q), k0 * dt);
        drift(&mut q, &p, mass, d0 * dt);
    }
    (q, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Newtonian gravity force for the 2-D Kepler problem: F(q) = -q / |q|³.
    fn kepler_force(q: &[f64]) -> Vec<f64> {
        let r = (q[0] * q[0] + q[1] * q[1]).sqrt();
        let r3 = r * r * r;
        vec![-q[0] / r3, -q[1] / r3]
    }

    /// Kepler energy E = ½|p|² - 1/|q| (unit mass).
    fn kepler_energy(q: &[f64], p: &[f64]) -> f64 {
        let r = (q[0] * q[0] + q[1] * q[1]).sqrt();
        0.5 * (p[0] * p[0] + p[1] * p[1]) - 1.0 / r
    }

    #[test]
    fn rk45_matches_exponential() {
        // dy/dt = y, y(0) = 1 → y(1) = e.
        let f = |_t: f64, y: &[f64]| vec![y[0]];
        let y = rk45(&f, 0.0, &[1.0], 1.0, 1e-8, 1e-8);
        assert!((y[0] - std::f64::consts::E).abs() < 1e-6, "got {}", y[0]);
    }

    #[test]
    fn rk45_harmonic_returns_to_start() {
        // y'' = -y as a 2-state system (y0 = position, y1 = velocity).
        // Over one full period [0, 2π] the state returns to its start.
        let f = |_t: f64, y: &[f64]| vec![y[1], -y[0]];
        let y0 = [1.0, 0.0];
        let period = std::f64::consts::TAU;
        let y = rk45(&f, 0.0, &y0, period, 1e-9, 1e-9);
        assert!((y[0] - y0[0]).abs() < 1e-6, "pos drifted: {}", y[0]);
        assert!((y[1] - y0[1]).abs() < 1e-6, "vel drifted: {}", y[1]);
    }

    #[test]
    fn rk4_matches_exponential() {
        let f = |_t: f64, y: &[f64]| vec![y[0]];
        let y = rk4(&f, 0.0, &[1.0], 1e-3, 1000);
        assert!((y[0] - std::f64::consts::E).abs() < 1e-6, "got {}", y[0]);
    }

    #[test]
    fn symplectic_conserves_energy_far_better_than_rk4() {
        // Circular Kepler orbit: q = (1, 0), p = (0, 1), unit mass → E0 = -0.5.
        let q0 = [1.0, 0.0];
        let p0 = [0.0, 1.0];
        let e0 = kepler_energy(&q0, &p0);
        assert!((e0 + 0.5).abs() < 1e-12);

        let dt = 0.01;
        let steps = 10_000;

        // Symplectic: track max |E - E0| by stepping one leapfrog/yoshida step at a time.
        let mut q = q0.to_vec();
        let mut p = p0.to_vec();
        let mut sympl_drift = 0.0_f64;
        for _ in 0..steps {
            let (qn, pn) = yoshida4(&kepler_force, &q, &p, 1.0, dt, 1);
            q = qn;
            p = pn;
            sympl_drift = sympl_drift.max((kepler_energy(&q, &p) - e0).abs());
        }

        // RK4 on the same system, phase state y = (x, y, vx, vy).
        let f = |_t: f64, y: &[f64]| {
            let force = kepler_force(&[y[0], y[1]]);
            vec![y[2], y[3], force[0], force[1]]
        };
        let mut y = vec![q0[0], q0[1], p0[0], p0[1]];
        let mut t = 0.0;
        let mut rk4_drift = 0.0_f64;
        for _ in 0..steps {
            y = rk4_step(&f, t, &y, dt);
            t += dt;
            let e = kepler_energy(&[y[0], y[1]], &[y[2], y[3]]);
            rk4_drift = rk4_drift.max((e - e0).abs());
        }

        // Observed (dt = 0.01, 1e4 steps, circular orbit): yoshida4 ≈ 1.0e-14
        // (near machine precision), rk4 ≈ 1.4e-10. Ratio ≈ 1.4e4; assert a
        // conservative 50× separation with wide margin.
        println!("Kepler energy drift: yoshida4 = {sympl_drift:.3e}, rk4 = {rk4_drift:.3e}");
        println!("  ratio rk4/yoshida4 = {:.1}", rk4_drift / sympl_drift);
        assert!(
            sympl_drift * 50.0 < rk4_drift,
            "symplectic drift {sympl_drift:.3e} not ≥50× smaller than rk4 {rk4_drift:.3e}"
        );
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn yoshida4_beats_leapfrog_on_kepler() {
        let q0 = [1.0, 0.0];
        let p0 = [0.0, 1.0];
        let e0 = kepler_energy(&q0, &p0);
        let dt = 0.01;
        let steps = 10_000;

        let max_drift = |mut stepper: Box<dyn FnMut(&[f64], &[f64]) -> (Vec<f64>, Vec<f64>)>| {
            let mut q = q0.to_vec();
            let mut p = p0.to_vec();
            let mut d = 0.0_f64;
            for _ in 0..steps {
                let (qn, pn) = stepper(&q, &p);
                q = qn;
                p = pn;
                d = d.max((kepler_energy(&q, &p) - e0).abs());
            }
            d
        };

        let leap_drift = max_drift(Box::new(|q: &[f64], p: &[f64]| {
            leapfrog(&kepler_force, q, p, 1.0, dt, 1)
        }));
        let yosh_drift = max_drift(Box::new(|q: &[f64], p: &[f64]| {
            yoshida4(&kepler_force, q, p, 1.0, dt, 1)
        }));

        println!("Kepler energy drift: leapfrog = {leap_drift:.3e}, yoshida4 = {yosh_drift:.3e}");
        assert!(
            yosh_drift < leap_drift,
            "yoshida4 drift {yosh_drift:.3e} should be < leapfrog {leap_drift:.3e}"
        );
    }
}
