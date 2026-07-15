//! Rate (easing) functions, ported from `manim.utils.rate_functions`.
//!
//! Each function maps animation progress `t ∈ [0, 1]` to an eased value. The
//! plain `fn(f32) -> f32` items mirror manim CE one-for-one; the [`RateFn`] enum
//! wraps the common ones as pattern-matchable, cloneable data with a
//! [`RateFn::Custom`] escape hatch.
//!
//! Endpoint contracts: the monotone functions satisfy `f(0) ≈ 0` and
//! `f(1) ≈ 1`; the [`there_and_back`] family and [`wiggle`] instead return to
//! `0` at both ends.

use std::sync::Arc;

const INFLECTION: f32 = 10.0;

/// The logistic sigmoid, `1 / (1 + e^-x)` — manim CE's `sigmoid`.
///
/// ```
/// use manim_math::rate_functions::sigmoid;
/// assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
/// ```
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// The identity rate function. Mirrors manim CE's `linear`.
///
/// ```
/// use manim_math::rate_functions::linear;
/// assert_eq!(linear(0.3), 0.3);
/// ```
pub fn linear(t: f32) -> f32 {
    t
}

/// The default manim easing: a sigmoid S-curve flat at both ends.
///
/// Mirrors manim CE's `smooth` with its default `inflection = 10`.
///
/// ```
/// use manim_math::rate_functions::smooth;
/// assert!(smooth(0.0).abs() < 1e-6);
/// assert!((smooth(1.0) - 1.0).abs() < 1e-6);
/// assert!((smooth(0.5) - 0.5).abs() < 1e-6);
/// ```
pub fn smooth(t: f32) -> f32 {
    let error = sigmoid(-INFLECTION / 2.0);
    ((sigmoid(INFLECTION * (t - 0.5)) - error) / (1.0 - 2.0 * error)).clamp(0.0, 1.0)
}

/// Cubic Hermite smoothstep, `3t² − 2t³` clamped to `[0, 1]`.
///
/// Mirrors manim CE's `smoothstep`.
///
/// ```
/// use manim_math::rate_functions::smoothstep;
/// assert_eq!(smoothstep(0.0), 0.0);
/// assert_eq!(smoothstep(1.0), 1.0);
/// ```
pub fn smoothstep(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t < 1.0 {
        3.0 * t * t - 2.0 * t * t * t
    } else {
        1.0
    }
}

/// Quintic smootherstep, `6t⁵ − 15t⁴ + 10t³` clamped to `[0, 1]`.
///
/// Mirrors manim CE's `smootherstep`.
///
/// ```
/// use manim_math::rate_functions::smootherstep;
/// assert_eq!(smootherstep(0.0), 0.0);
/// assert_eq!(smootherstep(1.0), 1.0);
/// ```
pub fn smootherstep(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t < 1.0 {
        6.0 * t.powi(5) - 15.0 * t.powi(4) + 10.0 * t.powi(3)
    } else {
        1.0
    }
}

/// Septic "smoothererstep", `35t⁴ − 84t⁵ + 70t⁶ − 20t⁷`.
///
/// Mirrors manim CE's `smoothererstep`.
///
/// ```
/// use manim_math::rate_functions::smoothererstep;
/// assert_eq!(smoothererstep(0.0), 0.0);
/// assert_eq!(smoothererstep(1.0), 1.0);
/// ```
pub fn smoothererstep(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t < 1.0 {
        35.0 * t.powi(4) - 84.0 * t.powi(5) + 70.0 * t.powi(6) - 20.0 * t.powi(7)
    } else {
        1.0
    }
}

/// Ease-in that starts slow and accelerates: `2 · smooth(t/2)`.
///
/// Mirrors manim CE's `rush_into`.
///
/// ```
/// use manim_math::rate_functions::rush_into;
/// assert!(rush_into(0.0).abs() < 1e-6);
/// assert!((rush_into(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn rush_into(t: f32) -> f32 {
    2.0 * smooth(t / 2.0)
}

/// Ease-out that starts fast and decelerates: `2 · smooth(t/2 + 0.5) − 1`.
///
/// Mirrors manim CE's `rush_from`.
///
/// ```
/// use manim_math::rate_functions::rush_from;
/// assert!(rush_from(0.0).abs() < 1e-6);
/// assert!((rush_from(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn rush_from(t: f32) -> f32 {
    2.0 * smooth(t / 2.0 + 0.5) - 1.0
}

/// A quarter-circle ease-out, `√(1 − (1 − t)²)`.
///
/// Mirrors manim CE's `slow_into`.
///
/// ```
/// use manim_math::rate_functions::slow_into;
/// assert!(slow_into(0.0).abs() < 1e-6);
/// assert!((slow_into(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn slow_into(t: f32) -> f32 {
    (1.0 - (1.0 - t) * (1.0 - t)).sqrt()
}

/// Two [`smooth`] curves stitched at the midpoint, easing at `0`, `0.5`, and `1`.
///
/// Mirrors manim CE's `double_smooth`.
///
/// ```
/// use manim_math::rate_functions::double_smooth;
/// assert!(double_smooth(0.0).abs() < 1e-6);
/// assert!((double_smooth(1.0) - 1.0).abs() < 1e-6);
/// assert!((double_smooth(0.5) - 0.5).abs() < 1e-6);
/// ```
pub fn double_smooth(t: f32) -> f32 {
    if t < 0.5 {
        0.5 * smooth(2.0 * t)
    } else {
        0.5 * (1.0 + smooth(2.0 * t - 1.0))
    }
}

/// Go to `1` and smoothly return to `0`.
///
/// Mirrors manim CE's `there_and_back`. Note the endpoint contract differs:
/// `f(0) ≈ 0` and `f(1) ≈ 0`.
///
/// ```
/// use manim_math::rate_functions::there_and_back;
/// assert!(there_and_back(0.0).abs() < 1e-6);
/// assert!(there_and_back(1.0).abs() < 1e-6);
/// assert!((there_and_back(0.5) - 1.0).abs() < 1e-6);
/// ```
pub fn there_and_back(t: f32) -> f32 {
    let new_t = if t < 0.5 { 2.0 * t } else { 2.0 * (1.0 - t) };
    smooth(new_t)
}

/// Go to `1`, hold, then return to `0`, with a pause of `1/3` of the run at the top.
///
/// Mirrors manim CE's `there_and_back_with_pause` with its default
/// `pause_ratio = 1/3`.
///
/// ```
/// use manim_math::rate_functions::there_and_back_with_pause;
/// assert!(there_and_back_with_pause(0.0).abs() < 1e-6);
/// assert!(there_and_back_with_pause(1.0).abs() < 1e-6);
/// assert!((there_and_back_with_pause(0.5) - 1.0).abs() < 1e-6);
/// ```
pub fn there_and_back_with_pause(t: f32) -> f32 {
    let pause_ratio = 1.0 / 3.0;
    let a = 2.0 / (1.0 - pause_ratio);
    if t < 0.5 - pause_ratio / 2.0 {
        smooth(a * t)
    } else if t < 0.5 + pause_ratio / 2.0 {
        1.0
    } else {
        smooth(a - a * t)
    }
}

/// Pull back below `0` before launching forward, ending at `1`.
///
/// Mirrors manim CE's `running_start` with its default `pull_factor = −0.5`.
///
/// ```
/// use manim_math::rate_functions::running_start;
/// assert!(running_start(0.0).abs() < 1e-6);
/// assert!((running_start(1.0) - 1.0).abs() < 1e-6);
/// // It dips negative early on.
/// assert!(running_start(0.2) < 0.0);
/// ```
pub fn running_start(t: f32) -> f32 {
    let pull_factor = -0.5;
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;
    let t6 = t5 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    let mt4 = mt3 * mt;
    15.0 * t2 * mt4 * pull_factor
        + 20.0 * t3 * mt3 * pull_factor
        + 15.0 * t4 * mt2
        + 6.0 * t5 * mt
        + t6
}

/// [`smooth`] scaled to only reach `0.7` — "not quite there".
///
/// Mirrors manim CE's `not_quite_there` with its default `func = smooth` and
/// `proportion = 0.7`.
///
/// ```
/// use manim_math::rate_functions::not_quite_there;
/// assert!(not_quite_there(0.0).abs() < 1e-6);
/// assert!((not_quite_there(1.0) - 0.7).abs() < 1e-6);
/// ```
pub fn not_quite_there(t: f32) -> f32 {
    0.7 * smooth(t)
}

/// Oscillate with a decaying there-and-back envelope.
///
/// Mirrors manim CE's `wiggle` with its default `wiggles = 2`. Endpoint
/// contract: `f(0) ≈ 0` and `f(1) ≈ 0`.
///
/// ```
/// use manim_math::rate_functions::wiggle;
/// assert!(wiggle(0.0).abs() < 1e-6);
/// assert!(wiggle(1.0).abs() < 1e-6);
/// ```
pub fn wiggle(t: f32) -> f32 {
    there_and_back(t) * (2.0 * std::f32::consts::PI * t).sin()
}

/// Compress a rate function `func` into the sub-interval `[a, b]`, holding its
/// endpoint values outside it.
///
/// Mirrors manim CE's `squish_rate_func`. Higher-order: returns a closure.
///
/// ```
/// use manim_math::rate_functions::{squish_rate_func, smooth};
/// let squished = squish_rate_func(smooth, 0.25, 0.75);
/// assert!(squished(0.1).abs() < 1e-6); // clamped to func(0)
/// assert!((squished(0.9) - 1.0).abs() < 1e-6); // clamped to func(1)
/// ```
pub fn squish_rate_func<F>(func: F, a: f32, b: f32) -> impl Fn(f32) -> f32
where
    F: Fn(f32) -> f32,
{
    move |t: f32| {
        if a == b {
            return a;
        }
        let new_t = if t < a {
            0.0
        } else if t > b {
            1.0
        } else {
            (t - a) / (b - a)
        };
        func(new_t)
    }
}

/// Linger near the start, then rush to `1` in the last fifth.
///
/// Mirrors manim CE's `lingering`, i.e. `squish_rate_func(identity, 0, 0.8)`.
///
/// ```
/// use manim_math::rate_functions::lingering;
/// assert!(lingering(0.0).abs() < 1e-6);
/// assert!((lingering(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn lingering(t: f32) -> f32 {
    squish_rate_func(linear, 0.0, 0.8)(t)
}

/// Exponential approach to `1` with the given half-life (`0.1` here).
///
/// Mirrors manim CE's `exponential_decay` with its default `half_life = 0.1`.
/// Note this does not exactly reach `1` at `t = 1` (it reaches `1 − e^-10`).
///
/// ```
/// use manim_math::rate_functions::exponential_decay;
/// assert!(exponential_decay(0.0).abs() < 1e-6);
/// assert!(exponential_decay(1.0) > 0.9999);
/// ```
pub fn exponential_decay(t: f32) -> f32 {
    let half_life = 0.1;
    1.0 - (-t / half_life).exp()
}

// -------------------------------------------------------------------------
// Standard easing suite (easings.net formulas, as used by manim CE).
// -------------------------------------------------------------------------

/// Sine ease-in. Mirrors manim CE's `ease_in_sine`.
///
/// ```
/// use manim_math::rate_functions::ease_in_sine;
/// assert!(ease_in_sine(0.0).abs() < 1e-6);
/// assert!((ease_in_sine(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_sine(t: f32) -> f32 {
    1.0 - ((t * std::f32::consts::PI) / 2.0).cos()
}

/// Sine ease-out. Mirrors manim CE's `ease_out_sine`.
///
/// ```
/// use manim_math::rate_functions::ease_out_sine;
/// assert!(ease_out_sine(0.0).abs() < 1e-6);
/// assert!((ease_out_sine(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_sine(t: f32) -> f32 {
    ((t * std::f32::consts::PI) / 2.0).sin()
}

/// Sine ease-in-out. Mirrors manim CE's `ease_in_out_sine`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_sine;
/// assert!(ease_in_out_sine(0.0).abs() < 1e-6);
/// assert!((ease_in_out_sine(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_sine(t: f32) -> f32 {
    -((std::f32::consts::PI * t).cos() - 1.0) / 2.0
}

/// Quadratic ease-in. Mirrors manim CE's `ease_in_quad`.
///
/// ```
/// use manim_math::rate_functions::ease_in_quad;
/// assert!(ease_in_quad(0.0).abs() < 1e-6);
/// assert!((ease_in_quad(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_quad(t: f32) -> f32 {
    t * t
}

/// Quadratic ease-out. Mirrors manim CE's `ease_out_quad`.
///
/// ```
/// use manim_math::rate_functions::ease_out_quad;
/// assert!(ease_out_quad(0.0).abs() < 1e-6);
/// assert!((ease_out_quad(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_quad(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Quadratic ease-in-out. Mirrors manim CE's `ease_in_out_quad`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_quad;
/// assert!(ease_in_out_quad(0.0).abs() < 1e-6);
/// assert!((ease_in_out_quad(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// Cubic ease-in. Mirrors manim CE's `ease_in_cubic`.
///
/// ```
/// use manim_math::rate_functions::ease_in_cubic;
/// assert!(ease_in_cubic(0.0).abs() < 1e-6);
/// assert!((ease_in_cubic(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

/// Cubic ease-out. Mirrors manim CE's `ease_out_cubic`.
///
/// ```
/// use manim_math::rate_functions::ease_out_cubic;
/// assert!(ease_out_cubic(0.0).abs() < 1e-6);
/// assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Cubic ease-in-out. Mirrors manim CE's `ease_in_out_cubic`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_cubic;
/// assert!(ease_in_out_cubic(0.0).abs() < 1e-6);
/// assert!((ease_in_out_cubic(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Quartic ease-in. Mirrors manim CE's `ease_in_quart`.
///
/// ```
/// use manim_math::rate_functions::ease_in_quart;
/// assert!(ease_in_quart(0.0).abs() < 1e-6);
/// assert!((ease_in_quart(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_quart(t: f32) -> f32 {
    t * t * t * t
}

/// Quartic ease-out. Mirrors manim CE's `ease_out_quart`.
///
/// ```
/// use manim_math::rate_functions::ease_out_quart;
/// assert!(ease_out_quart(0.0).abs() < 1e-6);
/// assert!((ease_out_quart(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_quart(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(4)
}

/// Quartic ease-in-out. Mirrors manim CE's `ease_in_out_quart`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_quart;
/// assert!(ease_in_out_quart(0.0).abs() < 1e-6);
/// assert!((ease_in_out_quart(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_quart(t: f32) -> f32 {
    if t < 0.5 {
        8.0 * t * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
    }
}

/// Quintic ease-in. Mirrors manim CE's `ease_in_quint`.
///
/// ```
/// use manim_math::rate_functions::ease_in_quint;
/// assert!(ease_in_quint(0.0).abs() < 1e-6);
/// assert!((ease_in_quint(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_quint(t: f32) -> f32 {
    t * t * t * t * t
}

/// Quintic ease-out. Mirrors manim CE's `ease_out_quint`.
///
/// ```
/// use manim_math::rate_functions::ease_out_quint;
/// assert!(ease_out_quint(0.0).abs() < 1e-6);
/// assert!((ease_out_quint(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(5)
}

/// Quintic ease-in-out. Mirrors manim CE's `ease_in_out_quint`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_quint;
/// assert!(ease_in_out_quint(0.0).abs() < 1e-6);
/// assert!((ease_in_out_quint(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_quint(t: f32) -> f32 {
    if t < 0.5 {
        16.0 * t * t * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(5) / 2.0
    }
}

/// Exponential ease-in. Mirrors manim CE's `ease_in_expo`.
///
/// ```
/// use manim_math::rate_functions::ease_in_expo;
/// assert_eq!(ease_in_expo(0.0), 0.0);
/// assert!((ease_in_expo(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_expo(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else {
        2.0_f32.powf(10.0 * t - 10.0)
    }
}

/// Exponential ease-out. Mirrors manim CE's `ease_out_expo`.
///
/// ```
/// use manim_math::rate_functions::ease_out_expo;
/// assert!(ease_out_expo(0.0).abs() < 1e-6);
/// assert_eq!(ease_out_expo(1.0), 1.0);
/// ```
pub fn ease_out_expo(t: f32) -> f32 {
    if t == 1.0 {
        1.0
    } else {
        1.0 - 2.0_f32.powf(-10.0 * t)
    }
}

/// Exponential ease-in-out. Mirrors manim CE's `ease_in_out_expo`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_expo;
/// assert_eq!(ease_in_out_expo(0.0), 0.0);
/// assert_eq!(ease_in_out_expo(1.0), 1.0);
/// assert!((ease_in_out_expo(0.5) - 0.5).abs() < 1e-6);
/// ```
pub fn ease_in_out_expo(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else if t < 0.5 {
        2.0_f32.powf(20.0 * t - 10.0) / 2.0
    } else {
        (2.0 - 2.0_f32.powf(-20.0 * t + 10.0)) / 2.0
    }
}

/// Circular ease-in. Mirrors manim CE's `ease_in_circ`.
///
/// ```
/// use manim_math::rate_functions::ease_in_circ;
/// assert!(ease_in_circ(0.0).abs() < 1e-6);
/// assert!((ease_in_circ(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_circ(t: f32) -> f32 {
    1.0 - (1.0 - t.powi(2)).sqrt()
}

/// Circular ease-out. Mirrors manim CE's `ease_out_circ`.
///
/// ```
/// use manim_math::rate_functions::ease_out_circ;
/// assert!(ease_out_circ(0.0).abs() < 1e-6);
/// assert!((ease_out_circ(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_circ(t: f32) -> f32 {
    (1.0 - (t - 1.0).powi(2)).sqrt()
}

/// Circular ease-in-out. Mirrors manim CE's `ease_in_out_circ`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_circ;
/// assert!(ease_in_out_circ(0.0).abs() < 1e-6);
/// assert!((ease_in_out_circ(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_circ(t: f32) -> f32 {
    if t < 0.5 {
        (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
    } else {
        ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
    }
}

/// Overshooting back ease-in. Mirrors manim CE's `ease_in_back`.
///
/// ```
/// use manim_math::rate_functions::ease_in_back;
/// assert!(ease_in_back(0.0).abs() < 1e-6);
/// assert!((ease_in_back(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    c3 * t * t * t - c1 * t * t
}

/// Overshooting back ease-out. Mirrors manim CE's `ease_out_back`.
///
/// ```
/// use manim_math::rate_functions::ease_out_back;
/// assert!(ease_out_back(0.0).abs() < 1e-6);
/// assert!((ease_out_back(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Overshooting back ease-in-out. Mirrors manim CE's `ease_in_out_back`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_back;
/// assert!(ease_in_out_back(0.0).abs() < 1e-6);
/// assert!((ease_in_out_back(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c2 = c1 * 1.525;
    if t < 0.5 {
        ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
    } else {
        ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
    }
}

/// Elastic ease-in. Mirrors manim CE's `ease_in_elastic`.
///
/// ```
/// use manim_math::rate_functions::ease_in_elastic;
/// assert_eq!(ease_in_elastic(0.0), 0.0);
/// assert_eq!(ease_in_elastic(1.0), 1.0);
/// ```
pub fn ease_in_elastic(t: f32) -> f32 {
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else {
        -(2.0_f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
    }
}

/// Elastic ease-out. Mirrors manim CE's `ease_out_elastic`.
///
/// ```
/// use manim_math::rate_functions::ease_out_elastic;
/// assert_eq!(ease_out_elastic(0.0), 0.0);
/// assert_eq!(ease_out_elastic(1.0), 1.0);
/// ```
pub fn ease_out_elastic(t: f32) -> f32 {
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else {
        2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
    }
}

/// Elastic ease-in-out. Mirrors manim CE's `ease_in_out_elastic`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_elastic;
/// assert_eq!(ease_in_out_elastic(0.0), 0.0);
/// assert_eq!(ease_in_out_elastic(1.0), 1.0);
/// ```
pub fn ease_in_out_elastic(t: f32) -> f32 {
    let c5 = (2.0 * std::f32::consts::PI) / 4.5;
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else if t < 0.5 {
        -(2.0_f32.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
    } else {
        (2.0_f32.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0 + 1.0
    }
}

/// Bouncing ease-in. Mirrors manim CE's `ease_in_bounce`.
///
/// ```
/// use manim_math::rate_functions::ease_in_bounce;
/// assert!(ease_in_bounce(0.0).abs() < 1e-6);
/// assert!((ease_in_bounce(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_bounce(t: f32) -> f32 {
    1.0 - ease_out_bounce(1.0 - t)
}

/// Bouncing ease-out. Mirrors manim CE's `ease_out_bounce`.
///
/// ```
/// use manim_math::rate_functions::ease_out_bounce;
/// assert!(ease_out_bounce(0.0).abs() < 1e-6);
/// assert!((ease_out_bounce(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_out_bounce(t: f32) -> f32 {
    let n1 = 7.5625;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

/// Bouncing ease-in-out. Mirrors manim CE's `ease_in_out_bounce`.
///
/// ```
/// use manim_math::rate_functions::ease_in_out_bounce;
/// assert!(ease_in_out_bounce(0.0).abs() < 1e-6);
/// assert!((ease_in_out_bounce(1.0) - 1.0).abs() < 1e-6);
/// ```
pub fn ease_in_out_bounce(t: f32) -> f32 {
    if t < 0.5 {
        (1.0 - ease_out_bounce(1.0 - 2.0 * t)) / 2.0
    } else {
        (1.0 + ease_out_bounce(2.0 * t - 1.0)) / 2.0
    }
}

macro_rules! rate_fn_enum {
    ($($variant:ident => $func:path),+ $(,)?) => {
        /// A pattern-matchable, cloneable rate function.
        ///
        /// Wraps the common rate functions as data (so animations can store and
        /// serialize their easing) with a [`RateFn::Custom`] escape hatch for
        /// arbitrary closures. Apply one with [`RateFn::apply`].
        ///
        /// ```
        /// use manim_math::rate_functions::RateFn;
        /// use std::sync::Arc;
        /// assert!((RateFn::Smooth.apply(0.5) - 0.5).abs() < 1e-6);
        /// assert_eq!(RateFn::Linear.apply(0.25), 0.25);
        /// let doubled = RateFn::Custom(Arc::new(|t| 2.0 * t));
        /// assert_eq!(doubled.apply(0.3), 0.6);
        /// assert_eq!(format!("{doubled:?}"), "Custom");
        /// ```
        #[derive(Clone)]
        pub enum RateFn {
            $(
                #[doc = concat!("The [`", stringify!($func), "`] rate function.")]
                $variant,
            )+
            /// An arbitrary user-supplied rate function.
            Custom(Arc<dyn Fn(f32) -> f32 + Send + Sync>),
        }

        impl RateFn {
            /// Evaluate this rate function at progress `t`.
            ///
            /// ```
            /// use manim_math::rate_functions::RateFn;
            /// assert_eq!(RateFn::Linear.apply(0.4), 0.4);
            /// ```
            pub fn apply(&self, t: f32) -> f32 {
                match self {
                    $( RateFn::$variant => $func(t), )+
                    RateFn::Custom(f) => f(t),
                }
            }
        }

        impl core::fmt::Debug for RateFn {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self {
                    $( RateFn::$variant => f.write_str(stringify!($variant)), )+
                    RateFn::Custom(_) => f.write_str("Custom"),
                }
            }
        }
    };
}

rate_fn_enum! {
    Linear => linear,
    Smooth => smooth,
    Smoothstep => smoothstep,
    Smootherstep => smootherstep,
    Smoothererstep => smoothererstep,
    RushInto => rush_into,
    RushFrom => rush_from,
    SlowInto => slow_into,
    DoubleSmooth => double_smooth,
    ThereAndBack => there_and_back,
    ThereAndBackWithPause => there_and_back_with_pause,
    RunningStart => running_start,
    NotQuiteThere => not_quite_there,
    Wiggle => wiggle,
    Lingering => lingering,
    ExponentialDecay => exponential_decay,
    EaseInSine => ease_in_sine,
    EaseOutSine => ease_out_sine,
    EaseInOutSine => ease_in_out_sine,
    EaseInQuad => ease_in_quad,
    EaseOutQuad => ease_out_quad,
    EaseInOutQuad => ease_in_out_quad,
    EaseInCubic => ease_in_cubic,
    EaseOutCubic => ease_out_cubic,
    EaseInOutCubic => ease_in_out_cubic,
    EaseInQuart => ease_in_quart,
    EaseOutQuart => ease_out_quart,
    EaseInOutQuart => ease_in_out_quart,
    EaseInQuint => ease_in_quint,
    EaseOutQuint => ease_out_quint,
    EaseInOutQuint => ease_in_out_quint,
    EaseInExpo => ease_in_expo,
    EaseOutExpo => ease_out_expo,
    EaseInOutExpo => ease_in_out_expo,
    EaseInCirc => ease_in_circ,
    EaseOutCirc => ease_out_circ,
    EaseInOutCirc => ease_in_out_circ,
    EaseInBack => ease_in_back,
    EaseOutBack => ease_out_back,
    EaseInOutBack => ease_in_out_back,
    EaseInElastic => ease_in_elastic,
    EaseOutElastic => ease_out_elastic,
    EaseInOutElastic => ease_in_out_elastic,
    EaseInBounce => ease_in_bounce,
    EaseOutBounce => ease_out_bounce,
    EaseInOutBounce => ease_in_out_bounce,
}

// `Smooth` is not the first variant, so a derive would pick the wrong default.
#[allow(clippy::derivable_impls)]
impl Default for RateFn {
    fn default() -> Self {
        RateFn::Smooth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// Functions with the standard `f(0)=0, f(1)=1` endpoint contract.
    const MONOTONE: &[fn(f32) -> f32] = &[
        linear,
        smooth,
        smoothstep,
        smootherstep,
        smoothererstep,
        rush_into,
        rush_from,
        slow_into,
        double_smooth,
        running_start,
        lingering,
        ease_in_sine,
        ease_out_sine,
        ease_in_out_sine,
        ease_in_quad,
        ease_out_quad,
        ease_in_out_quad,
        ease_in_cubic,
        ease_out_cubic,
        ease_in_out_cubic,
        ease_in_expo,
        ease_out_expo,
        ease_in_out_expo,
        ease_in_circ,
        ease_out_circ,
        ease_in_out_circ,
        ease_in_back,
        ease_out_back,
        ease_in_out_back,
        ease_in_bounce,
        ease_out_bounce,
        ease_in_out_bounce,
    ];

    #[test]
    fn monotone_endpoints() {
        for f in MONOTONE {
            assert_relative_eq!(f(0.0), 0.0, epsilon = 1e-5);
            assert_relative_eq!(f(1.0), 1.0, epsilon = 1e-5);
        }
    }

    #[test]
    fn there_and_back_family_returns_to_zero() {
        for f in [there_and_back, there_and_back_with_pause, wiggle] {
            assert_relative_eq!(f(0.0), 0.0, epsilon = 1e-5);
            assert_relative_eq!(f(1.0), 0.0, epsilon = 1e-5);
        }
        assert_relative_eq!(there_and_back(0.5), 1.0, epsilon = 1e-5);
    }

    #[test]
    fn smooth_is_symmetric() {
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            assert_relative_eq!(smooth(t) + smooth(1.0 - t), 1.0, epsilon = 1e-5);
        }
    }

    #[test]
    fn ratefn_matches_free_function() {
        assert_relative_eq!(RateFn::Smooth.apply(0.37), smooth(0.37));
        assert_relative_eq!(RateFn::EaseOutBounce.apply(0.6), ease_out_bounce(0.6));
        assert!(matches!(RateFn::default(), RateFn::Smooth));
    }
}
