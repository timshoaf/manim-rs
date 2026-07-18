//! Build-time asset harness for the documentation site.
//!
//! Every scientific gallery example is included here as a module via `#[path]`
//! (the same trick `manim`'s `gallery_smoke` test uses), so the harness links
//! the *real* example sources rather than a drifting copy. Each example exposes
//! a `pub struct Demo` implementing `SceneBuilder`; its own `fn main` is dead
//! code in this context.
//!
//! The [`manifest`] pairs each example with a domain and a [`Kind`] — a single
//! PNG still at a time code, or a short MP4 clip over a frame span. Assets are
//! written to `site/src/assets/<domain>/<example>.{png,mp4}` and are **never**
//! committed; the site build regenerates them.
//!
//! ```sh
//! # everything, at the default 1280x720
//! cargo run -p render-examples --release
//! # one domain, or one example
//! cargo run -p render-examples --release -- --domain quantum
//! cargo run -p render-examples --release -- --only bloch_gates
//! # what would be rendered, without touching the GPU
//! cargo run -p render-examples -- --list
//! ```
//!
//! `REQUIRE_GPU=1` turns a missing GPU adapter into a hard failure instead of a
//! clean skip — CI sets it, backed by mesa lavapipe, so the asset job cannot
//! pass by rendering nothing.

mod render;

use std::path::PathBuf;

use manim_core::config::Config;
use manim_core::prelude::SceneBuilder;

/// Includes an example's source as a module. Relative to `src/`.
macro_rules! include_example {
    ($module:ident, $file:literal) => {
        #[allow(dead_code, unused_imports)]
        #[path = $file]
        mod $module;
    };
}

// -- manim-sci ---------------------------------------------------------------
include_example!(
    conformal_square,
    "../../../crates/manim-sci/examples/conformal_square.rs"
);
include_example!(
    torus_curvature,
    "../../../crates/manim-sci/examples/torus_curvature.rs"
);
include_example!(
    dipole_field,
    "../../../crates/manim-sci/examples/dipole_field.rs"
);
include_example!(
    symplectic_vs_rk4,
    "../../../crates/manim-sci/examples/symplectic_vs_rk4.rs"
);
include_example!(
    kepler_orbits,
    "../../../crates/manim-sci/examples/kepler_orbits.rs"
);
include_example!(
    domain_coloring_gallery,
    "../../../crates/manim-sci/examples/domain_coloring_gallery.rs"
);
include_example!(
    heatmap_contours,
    "../../../crates/manim-sci/examples/heatmap_contours.rs"
);
include_example!(
    mobius_flow,
    "../../../crates/manim-sci/examples/mobius_flow.rs"
);
include_example!(
    geodesic_race,
    "../../../crates/manim-sci/examples/geodesic_race.rs"
);
include_example!(
    trefoil_tube,
    "../../../crates/manim-sci/examples/trefoil_tube.rs"
);
include_example!(
    stream_ribbons,
    "../../../crates/manim-sci/examples/stream_ribbons.rs"
);
include_example!(
    tensor_glyph_field,
    "../../../crates/manim-sci/examples/tensor_glyph_field.rs"
);
// -- manim-quantum -----------------------------------------------------------
include_example!(
    wavepacket_barrier,
    "../../../crates/manim-quantum/examples/wavepacket_barrier.rs"
);
include_example!(
    hydrogen_orbitals,
    "../../../crates/manim-quantum/examples/hydrogen_orbitals.rs"
);
include_example!(
    bloch_gates,
    "../../../crates/manim-quantum/examples/bloch_gates.rs"
);
// -- manim-chem --------------------------------------------------------------
include_example!(caffeine, "../../../crates/manim-chem/examples/caffeine.rs");
include_example!(
    nacl_lattice,
    "../../../crates/manim-chem/examples/nacl_lattice.rs"
);
include_example!(
    orbital_isosurface,
    "../../../crates/manim-chem/examples/orbital_isosurface.rs"
);
// -- manim-nn ----------------------------------------------------------------
include_example!(
    transformer_block,
    "../../../crates/manim-nn/examples/transformer_block.rs"
);
include_example!(
    loss_landscape_descent,
    "../../../crates/manim-nn/examples/loss_landscape_descent.rs"
);

/// What asset an entry produces.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// One PNG at (the frame nearest) `t` seconds. `f32::INFINITY` means "the
    /// final frame", which is the usual choice for a scene whose payoff is its
    /// finished construction.
    Still {
        /// Time code in seconds.
        t: f32,
    },
    /// An MP4 over `[t0, t1]` seconds at `fps`. Capped at [`MAX_CLIP_SECS`].
    Clip {
        /// Start of the window, seconds.
        t0: f32,
        /// End of the window, seconds.
        t1: f32,
        /// Frame rate the scene is sampled and encoded at.
        fps: u32,
    },
}

/// One row of the manifest.
pub struct Entry {
    /// File stem of the example — also the asset's basename.
    pub name: &'static str,
    /// Site section; becomes the asset subdirectory.
    pub domain: &'static str,
    /// Still or clip.
    pub kind: Kind,
    /// Constructs the example's scene builder.
    pub builder: fn() -> Box<dyn SceneBuilder>,
}

/// Clips are teasers, not lectures — the site embeds them inline.
pub const MAX_CLIP_SECS: f32 = 6.0;

/// Output resolution for every asset (720p).
const SIZE: (u32, u32) = (1280, 720);

/// Shorthand for a manifest row.
macro_rules! entry {
    ($module:ident, $domain:literal, $kind:expr) => {
        Entry {
            name: stringify!($module),
            domain: $domain,
            kind: $kind,
            builder: || Box::new($module::Demo),
        }
    };
}

/// A six-second clip at 30 fps from the top of the timeline — the default shape.
const fn clip6() -> Kind {
    Kind::Clip {
        t0: 0.0,
        t1: 6.0,
        fps: 30,
    }
}

/// The final frame — for scenes whose payoff is the completed construction.
const fn final_still() -> Kind {
    Kind::Still { t: f32::INFINITY }
}

/// Every example the site publishes, grouped by domain.
///
/// Domains map to the scientific-extensions milestones: `fields` (S0),
/// `materials` (S1), `deformations` (S2), `surfaces` (S4), `quantum` (S5),
/// `chem` (S6), `nn` (S7), `volumetrics` (S8).
pub fn manifest() -> Vec<Entry> {
    vec![
        // fields — S0
        entry!(symplectic_vs_rk4, "fields", clip6()),
        entry!(kepler_orbits, "fields", clip6()),
        // materials — S1
        entry!(domain_coloring_gallery, "materials", final_still()),
        entry!(heatmap_contours, "materials", final_still()),
        // deformations — S2
        entry!(conformal_square, "deformations", clip6()),
        entry!(mobius_flow, "deformations", clip6()),
        // surfaces — S4
        entry!(torus_curvature, "surfaces", final_still()),
        entry!(geodesic_race, "surfaces", clip6()),
        entry!(trefoil_tube, "surfaces", clip6()),
        // quantum — S5
        entry!(wavepacket_barrier, "quantum", clip6()),
        entry!(hydrogen_orbitals, "quantum", final_still()),
        entry!(bloch_gates, "quantum", clip6()),
        // chem — S6
        entry!(caffeine, "chem", clip6()),
        entry!(nacl_lattice, "chem", final_still()),
        entry!(orbital_isosurface, "chem", final_still()),
        // nn — S7
        entry!(transformer_block, "nn", clip6()),
        entry!(loss_landscape_descent, "nn", clip6()),
        // volumetrics — S8
        entry!(dipole_field, "volumetrics", clip6()),
        entry!(stream_ribbons, "volumetrics", clip6()),
        entry!(tensor_glyph_field, "volumetrics", final_still()),
    ]
}

/// Parsed command line.
struct Args {
    out: PathBuf,
    only: Vec<String>,
    domain: Option<String>,
    list: bool,
    stills_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        out: PathBuf::from("site/src/assets"),
        only: Vec::new(),
        domain: None,
        list: false,
        stills_only: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => args.out = it.next().ok_or("--out needs a path")?.into(),
            "--only" => args.only.push(it.next().ok_or("--only needs a name")?),
            "--domain" => args.domain = Some(it.next().ok_or("--domain needs a name")?),
            "--list" => args.list = true,
            "--stills-only" => args.stills_only = true,
            "-h" | "--help" => {
                println!(
                    "render-examples — renders gallery assets for the docs site\n\n\
                     --out <dir>     output root (default: site/src/assets)\n\
                     --only <name>   render just this example (repeatable)\n\
                     --domain <name> render just this domain\n\
                     --stills-only   skip clips (no ffmpeg needed)\n\
                     --list          print the manifest and exit\n"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(args)
}

fn main() -> std::process::ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("render-examples: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let selected: Vec<Entry> = manifest()
        .into_iter()
        .filter(|e| args.only.is_empty() || args.only.iter().any(|n| n == e.name))
        .filter(|e| args.domain.as_ref().is_none_or(|d| d == e.domain))
        .filter(|e| !(args.stills_only && matches!(e.kind, Kind::Clip { .. })))
        .collect();

    if args.list {
        for e in &selected {
            println!("{:<14} {:<26} {:?}", e.domain, e.name, e.kind);
        }
        println!("\n{} entries", selected.len());
        return std::process::ExitCode::SUCCESS;
    }

    if selected.is_empty() {
        eprintln!("render-examples: no manifest entries matched");
        return std::process::ExitCode::FAILURE;
    }

    // The renderer only depends on the pixel size, so one instance serves every
    // entry — and its tessellation/mesh caches are reused across all of them.
    // Frame rate lives in the per-entry scene config, not the renderer.
    let base = Config {
        pixel_width: SIZE.0,
        pixel_height: SIZE.1,
        fps: 30,
        ..Config::medium()
    };
    let Some(mut renderer) = render::try_renderer(&base) else {
        // No adapter and REQUIRE_GPU unset: a clean, honest skip.
        eprintln!(
            "render-examples: skipped {} entries (no GPU)",
            selected.len()
        );
        return std::process::ExitCode::SUCCESS;
    };

    let mut failures: Vec<String> = Vec::new();
    let mut rendered = 0usize;

    for e in &selected {
        let fps = match e.kind {
            Kind::Still { .. } => 30,
            Kind::Clip { fps, .. } => fps,
        };
        let config = Config {
            fps,
            ..base.clone()
        };

        let builder = (e.builder)();
        eprintln!("{}/{}", e.domain, e.name);

        let dir = args.out.join(e.domain);
        let result = match e.kind {
            Kind::Still { t } => render::still(
                builder.as_ref(),
                &config,
                t,
                &dir.join(format!("{}.png", e.name)),
                &mut renderer,
            ),
            Kind::Clip { t0, t1, fps: _ } => {
                let t1 = t1.min(t0 + MAX_CLIP_SECS);
                render::clip(
                    builder.as_ref(),
                    &config,
                    t0,
                    t1,
                    &dir.join(format!("{}.mp4", e.name)),
                    &mut renderer,
                )
            }
        };

        match result {
            Ok(()) => rendered += 1,
            Err(err) => {
                eprintln!("  FAILED: {err}");
                failures.push(format!("{}/{}: {err}", e.domain, e.name));
            }
        }
    }

    eprintln!("\nrendered {rendered}/{} entries", selected.len());
    if failures.is_empty() {
        std::process::ExitCode::SUCCESS
    } else {
        eprintln!("failures:");
        for f in &failures {
            eprintln!("  {f}");
        }
        std::process::ExitCode::FAILURE
    }
}
