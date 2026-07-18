# The manim-rs documentation site

An [mdBook](https://rust-lang.github.io/mdBook/) site deployed to GitHub Pages
by [`.github/workflows/pages.yml`](../.github/workflows/pages.yml).

## Layout of the deployed site

```
/            the book (this directory)
/api/        cargo doc --no-deps --workspace, plus a generated landing page
/demos/      examples/dioxus-app compiled to wasm by `dx`
```

## The single-source-of-truth rule

**Every code listing in this book is an `{{#include}}` of a real example file
under `crates/*/examples/`.** There is no copied-and-pasted code anywhere in
`src/`, and there must not be — a listing that drifts from the file it claims to
show is worse than no listing.

Likewise, **every figure is rendered from that same example** by the harness in
[`tools/render-examples`](../tools/render-examples), into
`site/src/assets/<domain>/<example>.{png,mp4}`.

`<domain>` is the **chapter/milestone** name, not the crate name — `fields`,
`materials`, `deformations`, `surfaces`, `quantum`, `chem`, `nn`,
`volumetrics`. The manifest in
[`tools/render-examples/src/main.rs`](../tools/render-examples/src/main.rs) is
the authority; chapter asset paths must match it exactly.

Each manifest entry is a `Still` **or** a `Clip`, and emits only one file:

| Kind | Emits | Markup to use |
|---|---|---|
| `Still` | `<example>.png` | `<img src="assets/<domain>/<example>.png" alt="…">` |
| `Clip` | `<example>.mp4` | `<video controls loop muted playsinline src="assets/<domain>/<example>.mp4">` |

**There are no poster images for clips.** Writing `poster="…png"` on a clip's
`<video>` points at a file the harness never produces — `check-assets.sh` will
catch it, but it is easier not to write it.

Assets are **gitignored and never committed** (see [`.gitignore`](.gitignore)).
The repository stays text-only; media is a build product.

## Building locally

```sh
cargo install mdbook --locked --version 0.4.52

# 1. Get some media. Either render the real thing (needs a GPU + ffmpeg):
cargo run -p render-examples --release
#    …or generate captioned stand-ins to check layout (needs ffmpeg):
./site/scripts/placeholder-assets.sh

# 2. Build, or serve with live reload at http://localhost:3000
mdbook build site
mdbook serve site
```

The book builds fine **without** any assets — mdBook does not care that a
`<video src>` is missing. That tolerance is what lets the `site` job in
`ci.yml` validate every branch without a GPU. The deploy path is stricter:
`check-assets.sh` runs between the harness and `mdbook build` and fails if any
referenced figure is absent.

## Scripts

| Script | Purpose |
|---|---|
| `scripts/check-assets.sh` | Fails if any `assets/…` referenced by `src/*.md` is missing, **or** if placeholder media is present under `CI`. Deploy gate. |
| `scripts/placeholder-assets.sh` | Generates captioned placeholder media locally. Development aid only; drops a `.placeholders` marker. |
| `scripts/api-index.sh` | Writes `/api/index.html` — `cargo doc --no-deps` over a workspace emits no top-level index. |
| `scripts/demos-unavailable.html` | Served at `/demos/` when the wasm build fails, so links stay honest instead of 404ing. |

## Adding a chapter or an example

1. Write the example at `crates/<kit>/examples/<name>.rs`, exposing
   `pub struct Demo` (the harness entry point) and gated behind
   `required-features = ["render-examples"]`.
2. Add it to the render harness manifest so its media is produced.
3. In the chapter, add a `## <title>` section with prose, a `<figure
   class="manim-figure">` block pointing at
   `assets/<domain>/<name>.{png,mp4}`, a `.source-note` div linking to the file
   on GitHub, and an mdBook include of the example path. Copy the shape from
   `src/surfaces.md`.
4. New chapters also need an entry in `src/SUMMARY.md` — `create-missing` is
   `false`, so a typo is a build failure rather than a blank generated page.

### Placeholders are dangerous — that is why they are marked

`placeholder-assets.sh` exists so the site can be laid out without a GPU, but a
placeholder is a *file that exists*, so it satisfies every existence check while
being a captioned colour card. This bit us once: a partial harness run left the
stills real and the clips as placeholders, and nothing downstream noticed —
the site looked complete and shipped colour cards for every animation.

So the script now writes `src/assets/.placeholders`, and `check-assets.sh`
treats that marker as **fatal when `CI` is set** and a loud warning locally. If
you ever add another way to produce stand-in media, mark it the same way.

Cheap manual sanity check — real clips are 6 s at 30 fps (≈180 frames) and tens
to hundreds of KB; a placeholder is 90 frames and ~12 KB:

```sh
ffprobe -v error -select_streams v -count_frames \
        -show_entries stream=nb_read_frames -of csv=p=0 \
        site/src/assets/fields/symplectic_vs_rk4.mp4
```

### Two mdBook footguns

- **`{{#include}}` is processed inside HTML comments.** You cannot "comment out"
  an include as a TODO; mdBook rewrites it anyway and a placeholder path fails
  the build. Describe the directive in prose instead — the existing TODO blocks
  in `fields.md` and `materials.md` show the form.
- **A broken include is an `[ERROR]` log line, but mdBook still exits 0.** The
  `site` job in `ci.yml` greps the output for `[ERROR]` rather than trusting the
  exit code. Do not "simplify" that away.

## Chapters

| File | Covers |
|---|---|
| `intro.md` | The constructivist-textbook thesis, crate layering |
| `getting-started.md` | Dependency, first scene, rendering, preview |
| `fields.md` | `manim-fields`: AD, fields, `SpaceMap`, integrators, PDE |
| `materials.md` | Per-pixel materials, domain coloring, `MaterialQuad` |
| `deformations.md` | `ApplyMap` vs `ApplyFunction`, `DeformationGrid`, complex analysis |
| `surfaces.md` | Curvature, geodesics, parallel transport, tubes, isosurfaces |
| `quantum.md` | Split-step evolution, eigenstates, Bloch sphere, tunneling |
| `chemistry.md` | Instanced molecules, parsers, lattices, orbitals |
| `neural-nets.md` | Compute-graph layout, heatmaps, loss landscapes |
| `volumetrics.md` | Stream tubes/ribbons, tensor glyphs, clouds, flux |
| `interactive.md` | `Figure`, `use_parameter`, `DragHandle`, `OrbitControls` |
| `reference.md` | Links out to `/api/` and `/demos/` |
