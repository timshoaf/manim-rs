# Testing & Documentation Strategy

The bar: **at or above manim CE**. CE has ~good test coverage with
graphical unit tests; we exceed it with types, properties, and enforced docs.

## Layers

1. **Doctests — every public item.** `#![deny(missing_docs)]` +
   `#![deny(rustdoc::broken_intra_doc_links)]` on every published crate.
   Every public type/fn/method gets a doc comment with a *runnable* minimal
   example (compiled & executed by `cargo test`). This single policy delivers
   the "working minimal examples for every library component" requirement.
   Doc examples that need a GPU are `no_run`; everything else executes.

2. **Unit tests** — colocated `#[cfg(test)]` modules. Numeric assertions via
   `approx::assert_relative_eq!`.

3. **Property tests (`proptest`)** — math invariants:
   - bezier split/join round-trips; `partial(0,1)` = identity
   - `align_points(a, b)` preserves both shapes to ε and equalizes counts
   - rate functions: endpoint & range contracts
   - color: hex/HSV round-trips; interpolate endpoints
   - space ops: rotation matrices orthonormal; angle functions vs. glam

4. **Core integration tests** — build scenes headlessly, assert on
   `DisplayList` output and timeline state at sampled times (no GPU needed):
   "after `play(Create(circle))` at alpha 0.5, the display list contains a
   partial path of proportion 0.5". Fast, deterministic, the workhorse layer.

5. **Golden-image tests (`manim-render`)** — offscreen wgpu at 854×480,
   perceptual diff vs. checked-in PNGs (tolerance: per-channel δ≤3 on ≥99.5%
   of pixels). `BLESS=1` to regenerate. Runs headless in CI on lavapipe.
   Mirrors CE's `frames_comparison` tests; we port their scene set.

6. **Example gallery as tests** — every `examples/*.rs` compiles in CI and the
   offline ones render one frame successfully (smoke).

## CI (GitHub Actions, added when repo gets a remote)

`fmt --check` → `clippy -D warnings` → `test --workspace` (includes doctests)
→ golden tests (lavapipe) → `wasm32-unknown-unknown` check of `manim` +
`manim-dioxus` → `cargo doc -D warnings`.

## Documentation deliverables

- rustdoc for all crates (the API reference; doc-front-page = quickstart).
- `docs/design/*` (these) as the architecture book.
- `README.md` with rendered example GIF, quickstart, CE-migration cheatsheet
  (`self.play(sq.animate.shift(UP))` → `scene.play(sq.animate().shift(UP))`).
- Per-module `//!` overviews with a table linking each item to its manim CE
  equivalent — the living parity map.
