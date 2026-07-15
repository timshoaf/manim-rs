//! Smoke test for the example gallery: every example's `Scene` must *construct*
//! (build its timeline) with a positive duration and a non-empty display list —
//! without touching the GPU. This catches example rot (renamed APIs, broken
//! constructs) in CI without rendering.
//!
//! Each example file is included as a module via `#[path]`; the `#[allow(...)]`
//! silences the example's unused `main`/helpers in this context.

use manim::prelude::*;

macro_rules! include_example {
    ($module:ident, $file:literal) => {
        #[allow(dead_code, unused_imports)]
        #[path = $file]
        mod $module;
    };
}

include_example!(boolean_operations, "../examples/boolean_operations.rs");
include_example!(moving_around, "../examples/moving_around.rs");
include_example!(
    point_moving_on_shapes,
    "../examples/point_moving_on_shapes.rs"
);
include_example!(vector_arrow, "../examples/vector_arrow.rs");
include_example!(gradient_text, "../examples/gradient_text.rs");
include_example!(brace_annotation, "../examples/brace_annotation.rs");
include_example!(sin_cos_plot, "../examples/sin_cos_plot.rs");
include_example!(arg_min, "../examples/arg_min.rs");
include_example!(moving_angle, "../examples/moving_angle.rs");
include_example!(
    transform_matching_tex,
    "../examples/transform_matching_tex.rs"
);

/// Builds `builder`, asserting a positive duration and a non-empty final display
/// list.
fn assert_constructs(name: &str, builder: &dyn SceneBuilder) {
    let scene = Scene::build(builder, Config::low())
        .unwrap_or_else(|e| panic!("{name} failed to construct: {e:?}"));
    assert!(
        scene.total_duration() > 0.0,
        "{name}: expected a positive timeline duration"
    );
    assert!(
        !scene.state().display_list().0.is_empty(),
        "{name}: expected a non-empty display list"
    );
}

#[test]
fn boolean_operations_constructs() {
    assert_constructs("boolean_operations", &boolean_operations::BooleanOperations);
}

#[test]
fn moving_around_constructs() {
    assert_constructs("moving_around", &moving_around::MovingAround);
}

#[test]
fn point_moving_on_shapes_constructs() {
    assert_constructs(
        "point_moving_on_shapes",
        &point_moving_on_shapes::PointMovingOnShapes,
    );
}

#[test]
fn vector_arrow_constructs() {
    assert_constructs("vector_arrow", &vector_arrow::VectorArrow);
}

#[test]
fn gradient_text_constructs() {
    assert_constructs("gradient_text", &gradient_text::GradientText);
}

#[test]
fn brace_annotation_constructs() {
    assert_constructs("brace_annotation", &brace_annotation::BraceAnnotation);
}

#[test]
fn sin_cos_plot_constructs() {
    assert_constructs("sin_cos_plot", &sin_cos_plot::SinCosPlot);
}

#[test]
fn arg_min_constructs() {
    assert_constructs("arg_min", &arg_min::ArgMin);
}

#[test]
fn moving_angle_constructs() {
    assert_constructs("moving_angle", &moving_angle::MovingAngle);
}

#[test]
fn transform_matching_tex_constructs() {
    assert_constructs(
        "transform_matching_tex",
        &transform_matching_tex::TransformMatchingTexDemo,
    );
}
