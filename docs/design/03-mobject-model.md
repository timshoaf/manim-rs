# The Mobject Model

This is the single most important design decision in the port. Manim's Python
mobjects form a mutable object graph: mobjects own submobject lists, animations
hold references to mobjects, updaters close over them, and everything mutates
everything. Transliterating that to Rust means `Rc<RefCell<dyn Mobject>>`
everywhere — runtime borrow panics, no Send, unidiomatic. We do not do that.

## Arena scene graph with typed handles

The `Scene` owns all mobjects in a slotmap arena. Users hold lightweight,
`Copy`, *typed* handles:

```rust
pub struct MobjectId<M: Mobject = AnyMobject>(Key, PhantomData<M>);

let circle: MobjectId<Circle> = scene.add(Circle::new().radius(1.0).color(BLUE));
scene[circle].set_fill(RED, 0.5);        // Index/IndexMut sugar for scene.get_mut()
```

- Handles are cheap to copy into animations, updaters, and closures — no borrow
  fights, no lifetimes in user code.
- `MobjectId<M>` derefs to typed access (`scene[circle].radius()`), and erases to
  `MobjectId<AnyMobject>` for heterogeneous collections (`VGroup`).
- Hierarchy (submobjects) lives in the arena as parent/children key lists, like
  manim's `submobjects`, giving `family()` traversal, group transforms, and
  z-index ordering.
- Removal is generational: stale handles are detected, not UB.

## The trait stack

```rust
/// Anything placeable in a scene.
pub trait Mobject: 'static {
    fn data(&self) -> &MobjectData;          // transform, style, children — shared struct
    fn data_mut(&mut self) -> &mut MobjectData;
    fn draw(&self, ctx: &mut DrawContext);   // contribute DrawItems to the display list
    fn bounding_box(&self) -> BoundingBox;
    fn as_vmobject(&self) -> Option<&VMobject> { None }
    fn interpolate_with(&mut self, a: &dyn Mobject, b: &dyn Mobject, t: f32);
}
```

`MobjectData` holds what *every* mobject has (matching manim's `Mobject` attrs):
points/transform, color, opacity, z_index, name, children. Concrete mobjects
(Circle, Line, Text…) are plain structs embedding a `VMobject` (path + style)
plus their semantic parameters — so `Circle` remembers its radius and can offer
`point_at_angle()`, exactly as manim's subclasses carry semantics over raw points.

### VMobject

The vectorized workhorse, port of manim's `VMobject`:

- geometry: `Path` (cubic bezier subpaths) from `manim-math`
- style: fill `Color`+opacity (or gradient), stroke `Color`+width+opacity,
  background stroke, cap/join, `dash_pattern`
- the full manim point-manipulation API: `set_points_as_corners`,
  `set_points_smoothly`, `add_cubic_bezier_curve`, `point_from_proportion`,
  `get_subcurve`, `align_points` (for transforms), `insert_n_curves`,
  `apply_function`, `become`
- family-aware transforms: `shift`, `scale`, `rotate`, `flip`, `stretch`,
  `move_to`, `align_to`, `next_to`, `to_edge`, `to_corner`, `arrange`,
  `arrange_in_grid`, `center`, width/height/depth getters+setters

All positional methods are **builder-compatible** (`self -> Self` before adding
to the scene, `&mut self` after), so both styles read naturally:

```rust
// declarative construction
let sq = Square::new().side_length(2.0).fill(BLUE, 0.5).shift(2.0 * RIGHT);
// imperative mutation post-add
scene[sq_id].rotate(PI / 4.0);
```

This is achieved with a single blanket impl over `Mobject` (methods defined once
on the trait via `MobjectData`, not per-type).

## Groups

`VGroup` is a mobject whose draw is its children's draws; `group![a, b, c]`
macro mirrors `VGroup(a, b, c)`. Group transforms apply to the family via the
arena. Indexing parity: `group.get(i)`, iteration, `add`/`remove`.

## Updaters

```rust
scene.add_updater(dot, |dot: &mut Dot, ctx: UpdaterCtx| {
    dot.move_to(ctx.scene.get(other).get_center() + UP);
});
```

Updaters are `FnMut(&mut M, UpdaterCtx)` stored in the scene keyed by handle;
`UpdaterCtx` exposes `dt`, elapsed time, and read access to other mobjects
(split-borrow safe because the arena hands out the target disjointly).
`always_redraw` is a mobject variant whose contents are rebuilt by closure each
frame. `ValueTracker` is a plain mobject holding an `f32` — identical to manim.

## Why not ECS (bevy_ecs) or Rc<RefCell>?

- `Rc<RefCell>`: runtime borrow panics leak into user code; not Send; not wasm-worker-able; unidiomatic.
- Full ECS: overkill — we have one component tree and ordered draw; ECS
  scheduling adds conceptual weight without wins at manim's entity counts, and
  hurts the "reads like manim" goal.
- slotmap arena: O(1) stable keys, generational safety, trivially serializable,
  zero unsafe, and keeps `Scene` a value you can clone for tests.
