//! Browser glue for [`url_state`](crate::url_state) (FE-147): read the fragment
//! on mount, rewrite it when an interaction settles.
//!
//! The grammar and the merge logic are pure (and tested natively — the native
//! build keeps the "fragment" in a thread-local, so `update` behaves identically
//! off the browser); only [`read_raw`]/[`write_raw`] differ per target.
//!
//! # Why `replaceState`, and why only on settle
//!
//! Writing the fragment with `location.hash = …` pushes a history entry and can
//! scroll the page to a matching element; `history.replaceState` does neither.
//! And the write happens when a drag *settles*, not per frame: a 60 Hz history
//! rewrite is both wasteful and turns the back button into a scrub bar.

use crate::url_state::{FigureState, UrlState};

/// The current fragment text (without `#`).
pub fn read_raw() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .map(|h| h.trim_start_matches('#').to_string())
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        native_fragment::with(|s| s.clone())
    }
}

/// Replaces the fragment text (without adding a history entry).
pub fn write_raw(fragment: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let path = window
            .location()
            .pathname()
            .unwrap_or_else(|_| String::from("/"));
        let search = window.location().search().unwrap_or_default();
        // An empty fragment writes the bare path, so a reset link is clean
        // rather than a dangling "#".
        let url = if fragment.is_empty() {
            format!("{path}{search}")
        } else {
            format!("{path}{search}#{fragment}")
        };
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        native_fragment::with(|s| *s = fragment.to_string());
    }
}

/// The decoded page state.
pub fn read() -> UrlState {
    UrlState::decode(&read_raw())
}

/// Writes the whole page state, replacing whatever was there.
pub fn write(state: &UrlState) {
    write_raw(&state.encode());
}

/// The state stored for one figure, if any.
pub fn read_figure(key: &str) -> Option<FigureState> {
    read().figure(key).cloned()
}

/// Merges one figure's state into the fragment, leaving every other figure's
/// entry untouched — which is what makes a page of independent figures share
/// one link without racing each other.
///
/// ```
/// use manim_dioxus::url;
/// url::write_raw("figA=v:1");
/// url::update("figB", |f| { f.set_scalar("v", 2.0); });
/// assert_eq!(url::read_raw(), "figA=v:1;figB=v:2");
/// # url::write_raw("");
/// ```
pub fn update(key: &str, edit: impl FnOnce(&mut FigureState)) {
    let mut state = read();
    edit(state.figure_mut(key));
    write(&state);
}

/// Clears the fragment entirely (a "reset the page" affordance).
pub fn clear() {
    write_raw("");
}

/// The native stand-in for `location.hash`, so the merge logic above is exercised
/// by ordinary tests instead of only in a browser.
#[cfg(not(target_arch = "wasm32"))]
mod native_fragment {
    use std::cell::RefCell;

    thread_local! {
        static FRAGMENT: RefCell<String> = const { RefCell::new(String::new()) };
    }

    /// Runs `f` over the stored fragment.
    pub(super) fn with<R>(f: impl FnOnce(&mut String) -> R) -> R {
        FRAGMENT.with(|c| f(&mut c.borrow_mut()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manim_core::prelude::Point;

    #[test]
    fn update_merges_without_disturbing_other_figures() {
        write_raw("");
        update("fig1", |f| {
            f.set_scalar("phase", 0.5);
        });
        update("fig2", |f| {
            f.set_point("z0", Point::new(1.0, -2.0, 0.0));
        });
        assert_eq!(read_raw(), "fig1=phase:0.5;fig2=z0:1,-2");
        // Rewriting one figure leaves the other alone...
        update("fig1", |f| {
            f.set_scalar("phase", -1.0);
        });
        assert_eq!(read_raw(), "fig1=phase:-1;fig2=z0:1,-2");
        // ...and reading back gives the same values.
        let s = read();
        assert_eq!(s.figure("fig1").unwrap().scalar("phase"), Some(-1.0));
        assert_eq!(
            s.figure("fig2").unwrap().point("z0"),
            Some(Point::new(1.0, -2.0, 0.0))
        );
        write_raw("");
    }

    #[test]
    fn a_missing_figure_reads_as_none_and_clear_empties_the_fragment() {
        write_raw("");
        assert!(read_figure("nope").is_none());
        update("fig", |f| {
            f.set_scalar("v", 3.0);
        });
        assert!(read_figure("fig").is_some());
        clear();
        assert_eq!(read_raw(), "");
        assert!(read().is_empty());
    }

    #[test]
    fn a_hand_edited_fragment_still_restores_what_it_can() {
        write_raw("fig1=phase:0.25,garbage;fig2");
        let s = read();
        assert_eq!(s.figure("fig1").unwrap().scalar("phase"), Some(0.25));
        assert!(s.figure("fig2").is_none());
        write_raw("");
    }
}
