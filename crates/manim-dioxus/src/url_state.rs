//! Shareable figure state in the URL fragment (FE-147).
//!
//! A reader who drags a figure into an interesting configuration should be able
//! to *send* it. This module is the serialization half: a compact, stable,
//! human-legible grammar for "these figures, in these states", with pure
//! [`UrlState::encode`] / [`UrlState::decode`] round-tripping. The browser half
//! (read the fragment on mount, rewrite it when a drag settles) is a thin
//! wrapper in [`crate::url`].
//!
//! # Grammar
//!
//! ```text
//! fragment := entry (";" entry)*
//! entry    := key "=" field ("," field)*
//! field    := name ":" number ("," number)*
//! key,name := [A-Za-z0-9_-]+
//! number   := decimal, at most 4 fractional digits, trailing zeros trimmed
//! ```
//!
//! A field's values continue until the next `,`-separated chunk that contains a
//! `:` — numbers never do, so a multi-value field needs no extra delimiter and a
//! point reads as `z0:-1,0.6`:
//!
//! ```text
//! #fig1=phase:0.5,z0:-1,0.6,z1:1,-0.5;fig2=zoom:2.5
//! ```
//!
//! Decoding is **total and lenient**: unknown keys, unparsable numbers, and
//! malformed entries are skipped rather than failing the page, because a
//! truncated or hand-edited link must still open. Encoding is canonical, so
//! `decode(encode(x))` is a fixed point.

use manim_core::prelude::Point;

/// The most fractional digits an encoded value keeps. Four is ~0.1 mm at
/// textbook scene scale — below the width of the handles being placed — and
/// keeps a shareable link short.
const DECIMALS: usize = 4;

/// One figure's state: an ordered list of named value groups.
///
/// A scalar parameter is a one-value field; a handle position is a two- (or
/// three-) value field. Order is preserved so a link's text is stable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FigureState {
    /// The figure's key in the fragment (`fig1` in `#fig1=…`).
    pub key: String,
    fields: Vec<(String, Vec<f32>)>,
}

impl FigureState {
    /// An empty state for the figure named `key`.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            fields: Vec::new(),
        }
    }

    /// The fields, in order.
    pub fn fields(&self) -> &[(String, Vec<f32>)] {
        &self.fields
    }

    /// Whether this figure carries no fields.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// The values of `name`, if present.
    pub fn values(&self, name: &str) -> Option<&[f32]> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_slice())
    }

    /// Sets (or replaces) `name`'s values. Invalid names — anything outside
    /// `[A-Za-z0-9_-]` — are rejected, so a caller cannot mint an unparsable
    /// link by accident.
    pub fn set_values(&mut self, name: &str, values: impl Into<Vec<f32>>) -> &mut Self {
        if !is_valid_name(name) {
            return self;
        }
        let values = values.into();
        match self.fields.iter_mut().find(|(n, _)| n == name) {
            Some((_, slot)) => *slot = values,
            None => self.fields.push((name.to_string(), values)),
        }
        self
    }

    /// The scalar value of `name` (its first value), if present.
    pub fn scalar(&self, name: &str) -> Option<f32> {
        self.values(name)?.first().copied()
    }

    /// Sets a scalar field.
    pub fn set_scalar(&mut self, name: &str, v: f32) -> &mut Self {
        self.set_values(name, vec![v])
    }

    /// The point value of `name`: `x,y` (z defaults to 0) or `x,y,z`.
    pub fn point(&self, name: &str) -> Option<Point> {
        match self.values(name)? {
            [x, y] => Some(Point::new(*x, *y, 0.0)),
            [x, y, z] => Some(Point::new(*x, *y, *z)),
            _ => None,
        }
    }

    /// Sets a point field, dropping a zero z (the 2-D case, which is most of
    /// them, then costs two numbers instead of three).
    pub fn set_point(&mut self, name: &str, p: Point) -> &mut Self {
        if p.z == 0.0 {
            self.set_values(name, vec![p.x, p.y])
        } else {
            self.set_values(name, vec![p.x, p.y, p.z])
        }
    }

    /// Reads an indexed run of points (`z0`, `z1`, …) written by
    /// [`set_points`](Self::set_points).
    pub fn points(&self, prefix: &str) -> Vec<Point> {
        let mut out = Vec::new();
        while let Some(p) = self.point(&format!("{prefix}{}", out.len())) {
            out.push(p);
        }
        out
    }

    /// Writes handle positions as an indexed run of point fields.
    pub fn set_points(&mut self, prefix: &str, points: &[Point]) -> &mut Self {
        for (i, p) in points.iter().enumerate() {
            self.set_point(&format!("{prefix}{i}"), *p);
        }
        self
    }

    /// This figure's entry text (`key=field,…`), without the separator.
    fn encode(&self) -> String {
        let body = self
            .fields
            .iter()
            .map(|(name, values)| {
                let nums: Vec<String> = values.iter().map(|v| encode_number(*v)).collect();
                format!("{name}:{}", nums.join(","))
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{}={}", self.key, body)
    }

    /// Parses one entry, or `None` if it is not `key=…` with a valid key.
    fn decode(entry: &str) -> Option<Self> {
        let (key, body) = entry.split_once('=')?;
        if !is_valid_name(key) {
            return None;
        }
        let mut out = Self::new(key);
        let mut current: Option<(String, Vec<f32>)> = None;
        for chunk in body.split(',') {
            match chunk.split_once(':') {
                Some((name, first)) => {
                    if let Some(f) = current.take() {
                        out.fields.push(f);
                    }
                    if !is_valid_name(name) {
                        continue; // skip a malformed field, keep the rest
                    }
                    let mut values = Vec::new();
                    if let Some(v) = decode_number(first) {
                        values.push(v);
                    }
                    current = Some((name.to_string(), values));
                }
                // A bare number continues the field in progress; a stray one
                // before any field (or an unparsable token) is dropped.
                None => {
                    if let (Some((_, values)), Some(v)) = (current.as_mut(), decode_number(chunk)) {
                        values.push(v);
                    }
                }
            }
        }
        if let Some(f) = current {
            out.fields.push(f);
        }
        Some(out)
    }
}

/// A whole page's worth of figure states — what lives in the URL fragment.
///
/// ```
/// use manim_dioxus::url_state::{FigureState, UrlState};
/// let mut s = UrlState::new();
/// let mut f = FigureState::new("fig1");
/// f.set_scalar("phase", 0.5);
/// f.set_point("z0", manim_core::prelude::Point::new(-1.0, 0.6, 0.0));
/// s.upsert(f);
/// assert_eq!(s.encode(), "fig1=phase:0.5,z0:-1,0.6");
/// // ...and it round-trips.
/// let back = UrlState::decode(&s.encode());
/// assert_eq!(back.figure("fig1").unwrap().scalar("phase"), Some(0.5));
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UrlState {
    figures: Vec<FigureState>,
}

impl UrlState {
    /// An empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing is stored (so the driver can clear the fragment instead
    /// of writing a bare `#`).
    pub fn is_empty(&self) -> bool {
        self.figures.iter().all(FigureState::is_empty)
    }

    /// The stored figures, in order.
    pub fn figures(&self) -> &[FigureState] {
        &self.figures
    }

    /// The state for `key`, if present.
    pub fn figure(&self, key: &str) -> Option<&FigureState> {
        self.figures.iter().find(|f| f.key == key)
    }

    /// The mutable state for `key`, inserting an empty one if new.
    pub fn figure_mut(&mut self, key: &str) -> &mut FigureState {
        if let Some(i) = self.figures.iter().position(|f| f.key == key) {
            return &mut self.figures[i];
        }
        self.figures.push(FigureState::new(key));
        self.figures.last_mut().expect("just pushed")
    }

    /// Inserts or replaces a figure's state.
    pub fn upsert(&mut self, state: FigureState) {
        match self.figures.iter_mut().find(|f| f.key == state.key) {
            Some(slot) => *slot = state,
            None => self.figures.push(state),
        }
    }

    /// The fragment text, without a leading `#`.
    pub fn encode(&self) -> String {
        self.figures
            .iter()
            .filter(|f| !f.is_empty())
            .map(FigureState::encode)
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Parses a fragment (with or without a leading `#`). Never fails: garbage
    /// entries are skipped so a truncated link still restores what it can.
    pub fn decode(fragment: &str) -> Self {
        let body = fragment.strip_prefix('#').unwrap_or(fragment);
        Self {
            figures: body
                .split(';')
                .filter(|e| !e.trim().is_empty())
                .filter_map(FigureState::decode)
                .filter(|f| !f.is_empty())
                .collect(),
        }
    }
}

/// Whether `s` is a legal key/field name.
fn is_valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Formats a value at [`DECIMALS`] places with trailing zeros (and a trailing
/// point) trimmed, so `0.5` stays `0.5` and `-1.0` shortens to `-1`.
/// Non-finite values encode as `0` — a link must never carry `NaN`.
fn encode_number(v: f32) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let s = format!("{v:.DECIMALS$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    // `-0` and `` (from "0.0000") both mean zero.
    match s {
        "" | "-" | "-0" => "0".to_string(),
        other => other.to_string(),
    }
}

/// Parses one encoded number, rejecting non-finite text (`inf`, `NaN`) which
/// would otherwise poison a camera or a handle position.
fn decode_number(s: &str) -> Option<f32> {
    let v: f32 = s.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UrlState {
        let mut s = UrlState::new();
        let f = s.figure_mut("fig1");
        f.set_scalar("phase", 0.5);
        f.set_point("z0", Point::new(-1.0, 0.6, 0.0));
        f.set_point("z1", Point::new(1.0, -0.5, 0.0));
        s.figure_mut("fig2").set_scalar("zoom", 2.5);
        s
    }

    #[test]
    fn encodes_the_documented_grammar() {
        assert_eq!(
            sample().encode(),
            "fig1=phase:0.5,z0:-1,0.6,z1:1,-0.5;fig2=zoom:2.5"
        );
    }

    #[test]
    fn round_trips_exactly() {
        let s = sample();
        let back = UrlState::decode(&s.encode());
        assert_eq!(back, s);
        // ...and encoding is a fixed point, so a link never churns.
        assert_eq!(back.encode(), s.encode());
    }

    #[test]
    fn round_trips_through_a_leading_hash() {
        let s = sample();
        let back = UrlState::decode(&format!("#{}", s.encode()));
        assert_eq!(back, s);
    }

    #[test]
    fn round_trips_arbitrary_values_within_the_encoded_precision() {
        let mut s = UrlState::new();
        let vals = [0.0, -0.0, 1.0, -3.14179, 1234.5678, 1e-5, -42.0];
        let f = s.figure_mut("f");
        for (i, v) in vals.iter().enumerate() {
            f.set_scalar(&format!("v{i}"), *v);
        }
        let back = UrlState::decode(&s.encode());
        let g = back.figure("f").expect("figure survived");
        for (i, v) in vals.iter().enumerate() {
            let got = g.scalar(&format!("v{i}")).expect("value survived");
            assert!((got - v).abs() <= 5e-5, "v{i}: {got} vs {v}");
        }
    }

    #[test]
    fn points_carry_z_only_when_it_is_nonzero() {
        let mut f = FigureState::new("f");
        f.set_point("a", Point::new(1.0, 2.0, 0.0));
        f.set_point("b", Point::new(1.0, 2.0, 3.0));
        assert_eq!(f.encode(), "f=a:1,2,b:1,2,3");
        let back = FigureState::decode(&f.encode()).unwrap();
        assert_eq!(back.point("a"), Some(Point::new(1.0, 2.0, 0.0)));
        assert_eq!(back.point("b"), Some(Point::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn indexed_point_runs_round_trip() {
        let pts = vec![
            Point::new(-1.0, 0.6, 0.0),
            Point::new(1.0, -0.5, 0.0),
            Point::new(0.25, 1.25, 0.0),
        ];
        let mut s = UrlState::new();
        s.figure_mut("vca").set_points("z", &pts);
        let back = UrlState::decode(&s.encode());
        assert_eq!(back.figure("vca").unwrap().points("z"), pts);
    }

    #[test]
    fn decoding_garbage_never_panics_and_keeps_what_it_can() {
        for junk in [
            "",
            "#",
            ";;;",
            "no-equals",
            "=novalue",
            "fig1=",
            "fig1=phase",
            "fig1=phase:",
            "fig1=phase:abc",
            "fig1=:0.5",
            "fig1=phase:NaN",
            "fig1=phase:inf",
            "fig1=0.5,0.6",
            "bad key=phase:1",
        ] {
            let s = UrlState::decode(junk);
            assert!(
                s.encode().is_empty() || !s.encode().contains("abc"),
                "{junk}"
            );
        }
        // A good field beside a bad one survives.
        let s = UrlState::decode("fig1=bad!:1,phase:0.5;fig2=zoom:x,pan:1,2");
        assert_eq!(s.figure("fig1").unwrap().scalar("phase"), Some(0.5));
        assert_eq!(
            s.figure("fig2").unwrap().point("pan"),
            Some(Point::new(1.0, 2.0, 0.0))
        );
        assert_eq!(s.figure("fig2").unwrap().values("zoom"), Some(&[][..]));
    }

    #[test]
    fn a_later_write_replaces_an_earlier_one_in_place() {
        let mut s = sample();
        s.figure_mut("fig1").set_scalar("phase", -1.25);
        assert_eq!(s.figure("fig1").unwrap().scalar("phase"), Some(-1.25));
        // Order is stable: `phase` stays first, no duplicate entry appears.
        assert!(
            s.encode().starts_with("fig1=phase:-1.25,z0:"),
            "{}",
            s.encode()
        );
    }

    #[test]
    fn empty_figures_are_omitted_from_the_link() {
        let mut s = UrlState::new();
        s.figure_mut("empty");
        assert!(s.is_empty());
        assert_eq!(s.encode(), "");
        s.figure_mut("real").set_scalar("v", 1.0);
        assert_eq!(s.encode(), "real=v:1");
    }

    #[test]
    fn invalid_names_are_refused_at_the_writing_end() {
        let mut f = FigureState::new("f");
        f.set_scalar("has:colon", 1.0);
        f.set_scalar("has,comma", 1.0);
        f.set_scalar("", 1.0);
        assert!(f.is_empty(), "{:?}", f.fields());
    }

    #[test]
    fn non_finite_values_encode_as_zero() {
        let mut f = FigureState::new("f");
        f.set_scalar("a", f32::NAN);
        f.set_scalar("b", f32::INFINITY);
        assert_eq!(f.encode(), "f=a:0,b:0");
    }

    #[test]
    fn upsert_replaces_a_whole_figure() {
        let mut s = sample();
        let mut fresh = FigureState::new("fig1");
        fresh.set_scalar("phase", 0.0);
        s.upsert(fresh);
        assert_eq!(s.figure("fig1").unwrap().fields().len(), 1);
        assert_eq!(s.figures().len(), 2, "the other figure is untouched");
    }
}
