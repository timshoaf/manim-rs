# Animation System

## Animations are data

A manim animation is: a target, a duration, a rate function, and an
interpolation rule. In Rust these are structs implementing:

```rust
pub trait Animation: 'static {
    /// Called once when the animation is scheduled; snapshot start state,
    /// prepare aligned copies (e.g. Transform aligns point counts here).
    fn begin(&mut self, scene: &mut SceneState);
    /// Drive the animation to progress `alpha` ∈ [0,1] (rate fn already applied).
    fn interpolate(&mut self, scene: &mut SceneState, alpha: f32);
    /// Called once at the end; commit final state, clean up temporaries.
    fn finish(&mut self, scene: &mut SceneState);

    fn duration(&self) -> f32 { 1.0 }
    fn rate_fn(&self) -> RateFn { RateFn::Smooth }
}
```

matching manim's `begin / interpolate_mobject / finish` lifecycle exactly, so
every CE animation ports mechanically.

## Construction is declarative

```rust
scene.play((
    Create::new(square),
    FadeIn::new(label).shift(UP),
    circle.animate().shift(2.0 * RIGHT).set_color(RED),
))?;
scene.wait(1.0);
```

- `play` takes `impl IntoAnimations` (single animation, tuples, or `Vec`) —
  concurrent animations in one call, like manim.
- Modifier builders on every animation: `.run_time(2.0)`, `.rate_fn(RateFn::Linear)`,
  `.lag_ratio(0.1)` (via `AnimationGroup`).
- **`.animate()`** — manim's beloved API. `mobject_id.animate()` returns an
  `AnimBuilder<M>` that records method calls (`shift`, `rotate`, `set_color`, any
  `&mut self` mobject method via a recorded closure) and becomes a
  `MobjectTransform` animation from the pre-state to the post-state. Same
  caveat as manim: it interpolates start→end states, not the path of methods.

## The animation catalog (parity)

Ported by CE module, mechanical once the trait is in place:

- **creation**: `Create`, `Uncreate`, `DrawBorderThenFill`, `Write`, `Unwrite`,
  `AddTextLetterByLetter`, `ShowIncreasingSubsets`, `ShowSubmobjectsOneByOne`,
  `SpiralIn`
- **transform**: `Transform`, `ReplacementTransform`, `TransformFromCopy`,
  `ClockwiseTransform`, `CounterclockwiseTransform`, `MoveToTarget`,
  `ApplyMethod`(subsumed by `.animate()`), `ApplyPointwiseFunction`,
  `ApplyMatrix`, `ApplyFunction`, `FadeTransform`, `ScaleInPlace`,
  `ShrinkToCenter`, `Restore`, `Swap`, `CyclicReplace`, `TransformMatchingShapes`,
  `TransformMatchingTex`
- **fading**: `FadeIn`, `FadeOut` (with shift/scale/target_position options)
- **indication**: `Indicate`, `Flash`, `FocusOn`, `Circumscribe`, `Wiggle`,
  `ApplyWave`, `ShowPassingFlash`
- **growing**: `GrowFromCenter`, `GrowFromPoint`, `GrowFromEdge`, `GrowArrow`,
  `SpinInFromNothing`
- **movement**: `Homotopy`, `ComplexHomotopy`, `PhaseFlow`, `MoveAlongPath`
- **rotation**: `Rotate`, `Rotating`
- **composition**: `AnimationGroup`, `Succession`, `LaggedStart`, `LaggedStartMap`
- **numbers**: `ChangingDecimal`, `ChangeDecimalToValue`
- **updaters**: `UpdateFromFunc`, `UpdateFromAlphaFunc`, `MaintainPositionRelativeTo`
- **changing**: `AnimatedBoundary`, `TracedPath`
- **speedmodifier**: `ChangeSpeed`

`Transform` correctness hinges on `VMobject::align_points` (insert curves so
both paths have equal counts, match subpath ordering) — implemented in
`manim-math::path` and property-tested (aligning preserves shape to ε).

## Timeline & real-time playback

`Timeline` is an explicit schedule: a sequence of `Segment`s (play-group or
wait), each with start time and duration.

```
tick(dt):
  t += dt
  for anim in active(t): anim.interpolate(scene, rate_fn(local_alpha(t)))
  start/finish crossings call begin()/finish() in order
  run updaters(dt)
```

Two clocks drive the same tick:

- **`RealtimePlayer`** — wall clock (winit/rAF frame callbacks). Variable dt;
  supports pause, seek (re-begin from nearest segment boundary), and scrubbing.
- **`OfflineRenderer`** — fixed dt = 1/fps. Deterministic; each tick yields a
  frame → PNG sequence / video. Golden tests use this.

Because animations snapshot state in `begin()` and are pure in `alpha`,
seeking backward within a segment is exact; seeking across segments replays
segment boundaries (fast: no rendering).

`Scene` in user code is a builder over a `Timeline`; "construct()" parity:

```rust
struct SquareToCircle;
impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let sq = scene.add(Square::new().fill(BLUE, 0.7));
        let c  = Circle::new().fill(RED, 0.7);
        scene.play(TransformInto::new(sq, c))?;
        scene.wait(1.0);
        Ok(())
    }
}
```

`construct` runs **once**, eagerly building the timeline (cheap: no rendering).
Playback then consumes the timeline at leisure — this is what makes scrubbing,
re-rendering, and embedding in UI (Dioxus) natural, and it's a deliberate
improvement over manim CE's render-as-you-construct coupling. Interactive
scenes (updaters reacting to live input) bypass the prebuilt timeline with
`scene.play_live()` in the Dioxus layer.
