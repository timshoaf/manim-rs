# Migrating from manim CE to `manim_rust`

This guide is for people who know [manim Community Edition](https://docs.manim.community)
and want to write the same scenes in Rust. It was written while porting the CE
example gallery, so it leans on what actually comes up in practice.

- [Mental-model differences](#mental-model-differences)
- [Side-by-side cheatsheet](#side-by-side-cheatsheet)
- [Naming deltas](#naming-deltas)
- [Deliberately different](#deliberately-different)
- [Not yet ported](#not-yet-ported)

---

## Mental-model differences

Four things surprise CE users first. Internalize these and the rest is mechanical.

### 1. Mobjects live in an arena; you hold typed handles

In CE a mobject is a Python object you keep a reference to and mutate directly:

```python
square = Square()
self.add(square)
square.shift(RIGHT)     # mutate the object you're holding
```

In Rust the scene owns every mobject in an arena. `scene.add(..)` returns a
cheap, `Copy` **handle** — `MobjectId<Square>` — not the mobject itself:

```rust
let square = scene.add(Square::new());   // square: MobjectId<Square>
scene.state_mut().shift(square.erase(), RIGHT);  // ask the scene to move it
```

- Handles are `Copy`, so passing them around is free — no borrow-checker fights.
- `id.erase()` turns a typed `MobjectId<M>` into a type-erased `AnyId` (what most
  scene methods and animations accept).
- Read a live mobject with `scene[id]` (panicking) or `scene.state().try_get(id)`.

### 2. `construct()` builds a *timeline*, it does not render

CE renders frames as `construct` runs. Here `construct` runs **once** and eagerly
builds a `Timeline` (a list of play/wait segments, each with a state snapshot).
Rendering, previewing, and scrubbing happen *afterward*, by replaying the
timeline. This is what makes seeking and re-rendering cheap.

A consequence to know: `play(..)` **eagerly applies each animation's end-state**
to the live scene, so the rest of `construct` sees final positions — exactly like
CE. You just don't pay for rendering during construction.

```rust
impl SceneBuilder for Demo {
    fn construct(&self, scene: &mut Scene) -> Result<()> {
        let sq = scene.add(Square::new());
        scene.play(sq.animate().shift(2.0 * RIGHT))?;
        // Here sq is already at x = 2 (end-state applied), as in CE.
        Ok(())
    }
}
```

### 3. Composite mobjects use `add_to(scene)`

Text and math are groups of per-glyph children. In CE `Text("hi")` returns a
ready object. Here the glyphs must be placed into the arena, so text-like
mobjects have an `add_to` step that returns the parent handle:

```rust
let title = Text::new("hi").font_size(48.0).add_to(scene.state_mut());
```

The same pattern (`Type::of(scene, ..)` / `.add_to(scene)`) is used by every
composite that owns children: `Matrix`, `Table`, `LabeledDot`, `BraceLabel`,
`Surface`, and the vector-space helpers.

### 4. Builder (`with_*`) vs mutate (`set_*`) styles

Two ways to configure a mobject, mirroring CE's `__init__` kwargs vs method
calls:

- **Before adding** — consuming builders that return `Self`, chainable:
  `Square::new().with_fill(BLUE, 0.7).with_stroke(WHITE, 4.0, 1.0).with_shift(UP)`.
- **After adding** — `scene[id].set_fill(..)` for a single mobject's own path, or
  `scene.shift(id, ..)` / `scene.scale(id, ..)` for family-aware transforms (a
  group and all its descendants). These take the typed handle directly (no
  `.erase()`) and are also on `Scene` as shortcuts.

---

## Side-by-side cheatsheet

`RIGHT`, `UP`, `BLUE`, `PI`, … come from `use manim::prelude::*;`.

| Operation | manim CE (Python) | `manim_rust` |
|---|---|---|
| Scene skeleton | `class S(Scene): def construct(self):` | `impl SceneBuilder for S { fn construct(&self, scene: &mut Scene) -> Result<()> }` |
| Add a mobject | `self.add(Square())` | `let sq = scene.add(Square::new());` |
| Remove | `self.remove(sq)` | `scene.remove(sq);` |
| Play one anim | `self.play(Create(sq))` | `scene.play(Create::new(sq))?;` |
| Play concurrently | `self.play(Create(a), FadeIn(b))` | `scene.play((Create::new(a), FadeIn::new(b)))?;` |
| Wait | `self.wait(1)` | `scene.wait(1.0);` |
| `.animate` | `self.play(sq.animate.shift(RIGHT))` | `scene.play(sq.animate().shift(RIGHT))?;` |
| Chained `.animate` | `sq.animate.scale(2).set_color(RED)` | `sq.animate().scale(2.0).set_color(RED)` |
| Shift (in place) | `sq.shift(2*RIGHT)` | `scene.shift(sq, 2.0 * RIGHT);` |
| Move to point | `sq.move_to(p)` | `scene.move_to(sq, p);` |
| Scale | `sq.scale(2)` | `scene.scale(sq, 2.0);` |
| Rotate | `sq.rotate(PI/4)` | `scene.rotate(sq, PI / 4.0);` |
| Fill (build) | `Square(fill_color=BLUE, fill_opacity=0.7)` | `Square::new().with_fill(BLUE, 0.7)` |
| Stroke (build) | `Square(stroke_color=WHITE, stroke_width=4)` | `Square::new().with_stroke(WHITE, 4.0, 1.0)` |
| Color (build) | `Dot(color=YELLOW)` | `Dot::new().with_color(YELLOW)` |
| Fill (mutate) | `sq.set_fill(BLUE, 0.7)` | `scene[sq].set_fill(BLUE, 0.7);` |
| Text | `Text("hi", font_size=48)` | `Text::new("hi").font_size(48.0).add_to(scene.state_mut())` |
| Math | `MathTex(r"e^{i\pi}")` | `MathTex::new(r"e^{i\pi}")?.add_to(scene.state_mut())` |
| Write text | `self.play(Write(t))` | `scene.play(Write::new(t))?;` |
| Group | `VGroup(a, b)` | `VGroup::of(&mut scene, [a.erase(), b.erase()])` |
| Axes | `Axes(x_range=[-5,5,1], y_range=[-3,3,1])` | `Axes::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0])` |
| Plot | `axes.plot(lambda x: x**2)` | `axes.plot(\|x\| x * x, None)` |
| Data → point | `axes.c2p(1, 2)` | `axes.c2p(1.0, 2.0)` |
| Axis labels | `axes.get_axis_labels("x", "y")` | `axes.get_axis_labels(scene.state_mut(), "x", "y")?` |
| Number plane | `NumberPlane()` | `NumberPlane::new([-5.0, 5.0, 1.0], [-3.0, 3.0, 1.0])` |
| Transform | `self.play(Transform(a, b))` | `scene.play(Transform::new(a, b))?;` |
| Transform into new | `self.play(Transform(a, Circle()))` | `scene.play(TransformInto::new(a, Circle::new()))?;` |
| Match tex | `TransformMatchingTex(a, b)` | `TransformMatchingTex::new(a, b)` |
| Move along path | `MoveAlongPath(dot, circle)` | `MoveAlongPath::new(dot, circle_path)` |
| Rotating | `Rotating(m, radians=PI)` | `Rotating::new(m).angle(PI)` |
| ValueTracker | `t = ValueTracker(0)` | `let t = scene.add(ValueTracker::new(0.0));` |
| Read tracker | `t.get_value()` | `scene.get(t).get_value()` (or `s.get(t)...` in a closure) |
| Animate tracker | `t.animate.set_value(3)` | `SetValue::new(t, 3.0)` |
| always_redraw | `always_redraw(lambda: Dot(...))` | `scene.always_redraw(move \|s\| Dot::at(...))` |
| Updater | `m.add_updater(lambda m: ...)` | `scene.state_mut().add_updater(m.erase(), \|s, id, ctx\| { .. })` |
| Boolean union | `Union(a, b)` | `Union::new(&a, &b)` |
| Render to file | `manim -ql scene.py S` | `manim::render(&S, Config::low(), "s.mp4")?` |

### Booleans / vector spaces / 3D (newer families)

| Operation | manim CE | `manim_rust` |
|---|---|---|
| Difference | `Difference(a, b)` | `Difference::new(&a, &b)` |
| LinearTransformationScene | subclass it | `let mut lts = LinearTransformationScene::new(&mut scene);` |
| Apply a matrix | `self.apply_matrix([[2,1],[1,1]])` | `lts.apply_matrix(&mut scene, [[2.0, 1.0], [1.0, 1.0]])?` |
| Sphere | `Sphere(radius=1)` | `Sphere::new(1.0).add_to(&mut scene)` |
| 3D axes | `ThreeDAxes()` | `let ax = ThreeDAxes::new(); ax.add_to(&mut scene);` |

---

## Naming deltas

Mechanical renames, mostly forced by Rust conventions (`new` constructors,
`snake_case`, no keyword arguments, keywords need escaping).

| manim CE | `manim_rust` | Why |
|---|---|---|
| `self.play(..)` / `self.add(..)` | `scene.play(..)` / `scene.add(..)` | `scene` is an explicit parameter, not `self` |
| `mob.animate.foo()` | `mob.animate().foo()` | `animate` is a method, not a property |
| `Create(x)` | `Create::new(x)` | Rust has no bare-call constructors |
| `Circle(radius=2)` | `Circle::new().radius(2.0)` | no kwargs; semantic builders instead |
| `VGroup(a, b)` | `VGroup::of(scene, [a, b])` | children live in the arena |
| `Axes(...).plot(f)` | `Axes::new(...).plot(f, None)` | explicit optional x-range arg |
| `t.get_value()` | `scene.get(t).get_value()` | read through the scene |
| Python floats `2` | `2.0` | Rust needs `f32` literals |
| `mob.become(other)` | `scene[m].r#become(&other)` | `become` is a Rust keyword — escape it as `r#become` |
| trait/keyword clashes | escape with `r#` (e.g. `r#type`) | Rust raw identifiers |

> Escaping note: any CE identifier that collides with a Rust keyword (`become`,
> `move`, `type`, `match`, `box`) is either renamed to something descriptive or
> written as a raw identifier `r#keyword`. We prefer a descriptive rename.

---

## Deliberately different

These are not ports — they are places where Rust idioms give a better result.

- **No global config / no `self.camera` mutation.** CE reaches into a process-wide
  `config` and mutates `self.camera` mid-scene. Here a `Config` is passed to
  `Scene::build(&scene, Config::low())` (also `medium()`, `high()`), and the camera
  lives inside the scene state — animate it with camera animations, don't poke a
  global. This makes scenes reproducible and embeddable.

- **Errors are `Result`, not exceptions.** `construct` returns
  `Result<(), CoreError>`. Fallible operations — `MathTex::new`, `get_axis_labels`,
  `play` — return `Result`, so `?` composes naturally. A bad LaTeX string is a
  recoverable `CoreError::Text` (with the underlying `MathError` on its
  `source()`), not a crash.

- **Snapshot timeline → free scrubbing.** Because `construct` builds a timeline of
  state snapshots instead of rendering inline, you get scrubbing, re-rendering at
  a different resolution, and Dioxus/web embedding for free — none of which CE's
  render-as-you-go model supports without re-running the scene.

- **Typed handles instead of object references.** `MobjectId<Square>` keeps the
  concrete type at compile time (so `scene[sq]` returns a `&Square` with its own
  methods) while staying `Copy`. `.erase()` drops to `AnyId` when a call is
  type-agnostic.

- **Builder vs mutate is explicit.** `with_fill` (consumes, returns `Self`, for
  pre-add construction) and `set_fill` (mutates, for a mobject already in the
  scene) are different methods, so it's always clear whether you're building or
  editing.

---

## Not yet ported

See the [parity map](design/10-parity-map.md) for the authoritative status. As of
`0.1.0-dev`, the notable gaps a CE user will hit:

- **Boolean smoothness.** `Union`/`Difference`/`Intersection`/`Exclusion`/`Cutout`
  work but return *polyline* approximations (flattened), not Béziers — CE keeps
  smoothness via skia-pathops, which we have no equivalent for yet.
- **3D rendering is in flight.** 3D *geometry* (`Surface`, `Sphere`, `Cube`,
  `ThreeDAxes`, …) exists and is testable headlessly, but the 3D camera /
  projection / depth-sorting renderer is still landing (FE-107).
- **Some text features.** Sub/superscript substring isolation, `Code`
  (syntax-highlighted) blocks, and monospace/`tt` runs are partial or pending.
- **`ZoomedScene`** and **sound** (`add_sound`) are not implemented.
- **Extra layouts** for graphs (some CE auto-layouts) and **animated vector-field
  flow** are partial.
- **`set_fill_by_value`** on 3D surfaces (per-face value coloring) is deferred.
