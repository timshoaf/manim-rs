# Contributing to manim_rust

Thanks for your interest! This is an early-stage, test-driven port of
[Manim Community Edition](https://docs.manim.community). The
[`docs/design/`](docs/design/00-vision.md) book is the source of truth for
architecture and API decisions — read the relevant chapter before a substantial
change.

## Toolchain

Rust **stable** (pinned in [`rust-toolchain.toml`](rust-toolchain.toml), which
also installs the `wasm32-unknown-unknown` target and `rustfmt`/`clippy`). The
workspace MSRV is **1.85**. `ffmpeg` on `PATH` is optional — only the video
exporter and its example need it.

## Everyday commands

```sh
cargo test --workspace                              # unit + property + doc tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                     # or --all --check in CI
cargo doc --workspace --no-deps                     # missing_docs is a hard error
```

Every public item must carry a doc comment **with a runnable example**
(`missing_docs = "deny"` workspace-wide); `broken_intra_doc_links` is denied too.

## Golden-image tests

The renderer is verified against checked-in PNGs in
[`crates/manim-render/tests/golden/`](crates/manim-render/tests/golden). They
compare with a per-channel + fraction-of-pixels tolerance.

```sh
cargo test -p manim-render                          # run goldens (+ unit tests)
cargo test -p manim-render --test golden -- --test-threads=1   # avoid GPU contention
BLESS=1 cargo test -p manim-render --test golden    # re-seed after an intentional change
REQUIRE_GPU=1 cargo test -p manim-render            # fail loudly if no GPU adapter (CI)
```

- Goldens **auto-skip** when no GPU adapter is available (headless dev boxes),
  so they never block a local run. CI sets `REQUIRE_GPU=1` with a software
  rasterizer (lavapipe) so a missing adapter is a hard failure, not a silent skip.
- When you `BLESS`, **open the changed PNGs and eyeball them** before committing —
  a golden is only as good as the eyes that seeded it. GPU tests can contend when
  run in parallel across threads/agents; prefer `--test-threads=1` for heavy runs.

## Feature flags

Most crates are lean by default. Optional features:

- `manim-render`/`manim` **`preview`** — native winit realtime window
  (`RealtimePlayer`). Native-only.
- `manim-render`/`manim` **`web`** — browser `CanvasSurface` (wasm32 + web-sys).
- `manim-text`/`manim` **`code`** — syntax-highlighted `Code` mobject (`syntect`).

```sh
cargo build -p manim-render --features preview
cargo test  -p manim-text   --features code
cargo check -p manim-render --target wasm32-unknown-unknown --features web
```

## Before you open a PR

Run the four everyday commands above plus any goldens your change touches, and
confirm the wasm check (`cargo check --target wasm32-unknown-unknown` for the
web-facing crates). CI runs the same gates plus the software-rendered golden job.
