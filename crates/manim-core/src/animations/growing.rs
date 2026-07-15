//! Growing / spiral entrance animations: [`GrowFromPoint`], [`GrowFromCenter`],
//! [`GrowFromEdge`], [`GrowArrow`], [`SpinInFromNothing`], and [`SpiralIn`].

use manim_math::Point;

use crate::animation::AnimConfig;
use crate::animation::{anim_config_accessors, morph_from, Animation, FamilyMorph, PathFn};
use crate::animations::paths::{path_along_arc, spiral_path};
use crate::mobject::AnyId;
use crate::scene_state::SceneState;

/// Where a grow animation expands from.
#[derive(Clone, Copy)]
enum Origin {
    Point(Point),
    Center,
    Edge(Point),
    FirstAnchor,
}

impl Origin {
    fn resolve(&self, state: &SceneState, id: AnyId) -> Point {
        match self {
            Origin::Point(p) => *p,
            Origin::Center => state.family_bounding_box(id).center(),
            Origin::Edge(dir) => state.family_bounding_box(id).point_in_direction(*dir),
            Origin::FirstAnchor => state
                .family(id)
                .into_iter()
                .find_map(|m| {
                    state
                        .get_dyn(m)
                        .data()
                        .path
                        .subpaths
                        .iter()
                        .find_map(|s| s.curves.first())
                        .map(|c| c.p0)
                })
                .unwrap_or_else(|| state.family_bounding_box(id).center()),
        }
    }
}

/// The shared grow implementation: expand from a collapsed copy at `origin`.
struct Grow {
    id: AnyId,
    origin: Origin,
    path_fn: Option<PathFn>,
    fade: bool,
    config: AnimConfig,
    morph: Option<FamilyMorph>,
}

impl Grow {
    fn new(id: AnyId, origin: Origin) -> Self {
        Self {
            id,
            origin,
            path_fn: None,
            fade: false,
            config: AnimConfig::default(),
            morph: None,
        }
    }
}

impl Animation for Grow {
    fn begin(&mut self, state: &mut SceneState) {
        let id = self.id;
        let point = self.origin.resolve(state, id);
        let fade = self.fade;
        self.morph = Some(
            morph_from(state, id, |s| {
                s.scale_about(id, 0.0, point);
                if fade {
                    s.set_style_family(id, |st| {
                        st.set_opacity(0.0);
                    });
                }
            })
            .with_path_fn(self.path_fn.clone()),
        );
    }
    fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
        if let Some(m) = &self.morph {
            m.apply(state, alpha);
        }
    }
    fn finish(&mut self, state: &mut SceneState) {
        self.interpolate(state, 1.0);
    }
    anim_config_accessors!();
}

/// Generates a grow-family wrapper type that forwards to [`Grow`].
macro_rules! grow_wrapper {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        pub struct $name {
            inner: Grow,
        }

        impl $name {
            /// Sets the run time in seconds.
            pub fn run_time(mut self, run_time: f32) -> Self {
                self.inner.config.run_time = run_time;
                self
            }

            /// Sets the easing curve.
            pub fn rate_fn(mut self, rate_fn: manim_math::rate_functions::RateFn) -> Self {
                self.inner.config.rate_fn = rate_fn;
                self
            }
        }

        impl Animation for $name {
            fn begin(&mut self, state: &mut SceneState) {
                self.inner.begin(state);
            }
            fn interpolate(&mut self, state: &mut SceneState, alpha: f32) {
                self.inner.interpolate(state, alpha);
            }
            fn finish(&mut self, state: &mut SceneState) {
                self.inner.finish(state);
            }
            fn duration(&self) -> f32 {
                self.inner.duration()
            }
            fn rate_fn(&self) -> manim_math::rate_functions::RateFn {
                Animation::rate_fn(&self.inner)
            }
        }
    };
}

grow_wrapper! {
    /// Grows a mobject from a single point. Port of manim CE's `GrowFromPoint`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::GrowFromPoint;
    /// use manim_math::Point;
    /// let mut scene = Scene::new(Config::default());
    /// let sq = scene.add(Square::new());
    /// scene.play(GrowFromPoint::new(sq, Point::ZERO)).unwrap();
    /// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
    /// ```
    GrowFromPoint
}

impl GrowFromPoint {
    /// Grows `id` from `point`.
    pub fn new(id: impl Into<AnyId>, point: Point) -> Self {
        Self {
            inner: Grow::new(id.into(), Origin::Point(point)),
        }
    }
}

grow_wrapper! {
    /// Grows a mobject from its center. Port of manim CE's `GrowFromCenter`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::GrowFromCenter;
    /// let mut scene = Scene::new(Config::default());
    /// let sq = scene.add(Square::new());
    /// scene.play(GrowFromCenter::new(sq)).unwrap();
    /// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
    /// ```
    GrowFromCenter
}

impl GrowFromCenter {
    /// Grows `id` from its center.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            inner: Grow::new(id.into(), Origin::Center),
        }
    }
}

grow_wrapper! {
    /// Grows a mobject from one edge of its bounding box. Port of manim CE's
    /// `GrowFromEdge`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::GrowFromEdge;
    /// use manim_math::DOWN;
    /// let mut scene = Scene::new(Config::default());
    /// let sq = scene.add(Square::new());
    /// scene.play(GrowFromEdge::new(sq, DOWN)).unwrap();
    /// assert!((scene[sq].bounding_box().height() - 2.0).abs() < 1e-3);
    /// ```
    GrowFromEdge
}

impl GrowFromEdge {
    /// Grows `id` from its `edge` (a direction like `DOWN`).
    pub fn new(id: impl Into<AnyId>, edge: Point) -> Self {
        Self {
            inner: Grow::new(id.into(), Origin::Edge(edge)),
        }
    }
}

grow_wrapper! {
    /// Grows an arrow from its tail (first anchor). Port of manim CE's
    /// `GrowArrow`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::GrowArrow;
    /// use manim_math::RIGHT;
    /// let mut scene = Scene::new(Config::default());
    /// let arrow = scene.add(Arrow::new(manim_math::ORIGIN, 3.0 * RIGHT));
    /// scene.play(GrowArrow::new(arrow)).unwrap();
    /// assert!((scene[arrow].get_length() - 3.0).abs() < 1e-3);
    /// ```
    GrowArrow
}

impl GrowArrow {
    /// Grows the arrow `id` from its tail.
    pub fn new(id: impl Into<AnyId>) -> Self {
        Self {
            inner: Grow::new(id.into(), Origin::FirstAnchor),
        }
    }
}

grow_wrapper! {
    /// Grows a mobject from its center while spinning it in (approximated with an
    /// arc path). Port of manim CE's `SpinInFromNothing`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::SpinInFromNothing;
    /// let mut scene = Scene::new(Config::default());
    /// let sq = scene.add(Square::new());
    /// scene.play(SpinInFromNothing::new(sq)).unwrap();
    /// assert!((scene[sq].bounding_box().width() - 2.0).abs() < 1e-3);
    /// ```
    SpinInFromNothing
}

impl SpinInFromNothing {
    /// Spins `id` in from nothing at its center.
    pub fn new(id: impl Into<AnyId>) -> Self {
        let mut inner = Grow::new(id.into(), Origin::Center);
        inner.path_fn = Some(path_along_arc(std::f32::consts::PI));
        Self { inner }
    }
}

grow_wrapper! {
    /// Fades a mobject in along a spiral toward its place (approximated). Port of
    /// manim CE's `SpiralIn`.
    ///
    /// ```
    /// use manim_core::prelude::*;
    /// use manim_core::animations::SpiralIn;
    /// let mut scene = Scene::new(Config::default());
    /// let sq = scene.add(Square::new().with_fill(BLUE, 1.0));
    /// scene.play(SpiralIn::new(sq)).unwrap();
    /// // Ends fully opaque.
    /// assert!((scene[sq].data().style.fill_opacity - 1.0).abs() < 1e-4);
    /// ```
    SpiralIn
}

impl SpiralIn {
    /// Spirals `id` in from nothing at its center, fading in.
    pub fn new(id: impl Into<AnyId>) -> Self {
        let mut inner = Grow::new(id.into(), Origin::Center);
        inner.fade = true;
        inner.path_fn = Some(spiral_path(2.0 * std::f32::consts::PI));
        Self { inner }
    }
}
