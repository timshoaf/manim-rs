//! The bundled font database and shared [`cosmic_text::FontSystem`].
//!
//! For determinism (identical layout on every platform, including wasm) we embed
//! DejaVu Sans and load it explicitly into a `fontdb` database with **no** system
//! font discovery by default. System fonts are an opt-in
//! ([`Text::with_system_fonts`](crate::Text::with_system_fonts), native only).
//!
//! DejaVu Sans is distributed under the Bitstream Vera / DejaVu license (a
//! permissive, MIT-like free license); see `assets/DejaVu-LICENSE.txt`.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use cosmic_text::fontdb;
use cosmic_text::FontSystem;

/// The default bundled font family name.
pub const DEFAULT_FONT: &str = "DejaVu Sans";

/// The bundled monospace font family (for `<tt>` markup and [`Code`](crate::Code)).
pub const MONO_FONT: &str = "DejaVu Sans Mono";

/// Embedded DejaVu Sans regular.
pub(crate) static DEJAVU_REGULAR: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
/// Embedded DejaVu Sans bold.
pub(crate) static DEJAVU_BOLD: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");
/// Embedded DejaVu Sans oblique (italic).
pub(crate) static DEJAVU_OBLIQUE: &[u8] = include_bytes!("../assets/DejaVuSans-Oblique.ttf");
/// Embedded DejaVu Sans bold oblique.
pub(crate) static DEJAVU_BOLD_OBLIQUE: &[u8] =
    include_bytes!("../assets/DejaVuSans-BoldOblique.ttf");
/// Embedded DejaVu Sans Mono regular.
pub(crate) static DEJAVU_MONO: &[u8] = include_bytes!("../assets/DejaVuSansMono.ttf");
/// Embedded DejaVu Sans Mono bold.
pub(crate) static DEJAVU_MONO_BOLD: &[u8] = include_bytes!("../assets/DejaVuSansMono-Bold.ttf");

/// The embedded DejaVu faces (sans + mono) as `fontdb` sources.
fn embedded_sources() -> Vec<fontdb::Source> {
    [
        DEJAVU_REGULAR,
        DEJAVU_BOLD,
        DEJAVU_OBLIQUE,
        DEJAVU_BOLD_OBLIQUE,
        DEJAVU_MONO,
        DEJAVU_MONO_BOLD,
    ]
    .into_iter()
    .map(|bytes| fontdb::Source::Binary(Arc::new(bytes.to_vec())))
    .collect()
}

/// The process-wide font system, built once from the embedded faces only.
static SHARED: OnceLock<Mutex<FontSystem>> = OnceLock::new();

/// Locks the shared, embedded-only font system (no system fonts).
pub(crate) fn shared() -> MutexGuard<'static, FontSystem> {
    SHARED
        .get_or_init(|| Mutex::new(FontSystem::new_with_fonts(embedded_sources())))
        .lock()
        .expect("font system mutex poisoned")
}

/// Builds a fresh font system that also includes the platform's system fonts.
///
/// On wasm (no filesystem font discovery) this is identical to the embedded-only
/// system.
pub(crate) fn with_system() -> FontSystem {
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut fs = FontSystem::new_with_fonts(embedded_sources());
    #[cfg(not(target_arch = "wasm32"))]
    fs.db_mut().load_system_fonts();
    fs
}
