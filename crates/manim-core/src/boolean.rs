//! Boolean set operations on the fill regions of two vectorized mobjects:
//! [`Union`], [`Difference`], [`Intersection`], [`Exclusion`], and [`Cutout`].
//! Port of manim CE's `manim.mobject.geometry.boolean_ops`.
//!
//! # Approach
//!
//! manim CE delegates to `skia-pathops`, which keeps Bézier smoothness. We match
//! that with [`flo_curves`](https://crates.io/crates/flo_curves) (pure-Rust,
//! Apache-2.0, wasm-compatible): each input's cubic contours are handed to
//! `flo_curves`' path arithmetic (`path_add` / `path_sub` / `path_intersect`)
//! and the **curved** result is converted straight back to cubic subpaths. Two
//! overlapping circles' union stays a handful of smooth arcs, not a fine polygon.
//! All contours of an input participate, so holes composite correctly.
//!
//! Because `flo_curves` treats every edge as exterior (no winding rule), result
//! contours are re-oriented by nesting depth (`orient_even_odd`) so holes wind
//! opposite their surrounding ring — correct under both non-zero and even-odd
//! fill.
//!
//! ## Fallback: Greiner–Hormann polyline clip
//!
//! For inputs that break the curve path (degeneracies that make `flo_curves`
//! panic or return nothing where a result exists), we fall back to a
//! **polygon clip** via the Greiner–Hormann algorithm: each outline is flattened
//! to a polygon (cubics subdivided until flat to `1e-3` scene units) and clipped,
//! yielding a polyline approximation (correct regions, faceted curves). GH
//! assumes *transversal* intersections; near-degenerate crossings (a parameter
//! within `1e-7` of an edge endpoint) trigger a perturb-and-retry that resolves
//! common axis-aligned cases (two squares sharing a vertex) but is not a general
//! exact-predicate solution. In practice the curve route handles these directly
//! and the fallback rarely fires.

use flo_curves::bezier::path::{path_add, path_intersect, path_sub, SimpleBezierPath};
use flo_curves::Coord2;
use manim_color::WHITE;
use manim_math::bezier::CubicBezier;
use manim_math::path::{Path, SubPath};
use manim_math::Point;

use crate::geometry::VMobject;
use crate::mobject::Mobject;
use crate::style::Style;

/// Flatness tolerance (scene units) for subdividing cubics into line segments.
const FLATTEN_TOL: f64 = 1e-3;
/// A crossing whose edge parameter is within this of `0`/`1` is treated as
/// degenerate and triggers a perturb-and-retry.
const DEGEN_EPS: f64 = 1e-7;
/// Per-retry positional nudge applied to the clip polygon; the x/y ratio is
/// deliberately irrational-ish so a nudge never re-aligns to an axis.
const PERTURB: (f64, f64) = (1.0e-6, 1.7e-6);
/// Maximum perturb-and-retry attempts before proceeding with possibly-degenerate
/// input.
const MAX_RETRIES: usize = 4;

/// The four set operations the clipper supports (Cutout is expressed via
/// [`Op::Difference`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Union,
    Intersection,
    Difference,
}

/// A 2D polygon as a ring of points (no repeated closing vertex).
type Poly = Vec<[f64; 2]>;

// ---------------------------------------------------------------------------
// Curve-preserving path arithmetic (primary route: flo_curves).
// ---------------------------------------------------------------------------

/// Fit accuracy (scene units) for `flo_curves` path arithmetic — well under the
/// 0.01 deviation budget so preserved arcs stay faithful without over-splitting.
const FIT_ACCURACY: f64 = 1e-3;

fn to_coord(p: Point) -> Coord2 {
    Coord2(p.x as f64, p.y as f64)
}
fn to_point(c: &Coord2) -> Point {
    Point::new(c.0 as f32, c.1 as f32, 0.0)
}

/// Converts one subpath of cubics into a `flo_curves` path (start + hull triples).
fn subpath_to_flo(sp: &SubPath) -> Option<SimpleBezierPath> {
    if sp.curves.is_empty() {
        return None;
    }
    let start = to_coord(sp.curves[0].p0);
    let segs = sp
        .curves
        .iter()
        .map(|c| (to_coord(c.p1), to_coord(c.p2), to_coord(c.p3)))
        .collect();
    Some((start, segs))
}

/// Every drawable contour of a mobject as flo paths (multi-contour aware, so
/// holes survive — unlike the single-contour polyline route).
fn mobject_to_flo(m: &dyn Mobject) -> Vec<SimpleBezierPath> {
    m.data()
        .path
        .subpaths
        .iter()
        .filter_map(subpath_to_flo)
        .collect()
}

/// Converts a flo result path back into our (always-closed) cubic subpath.
fn flo_to_subpath((start, segs): &SimpleBezierPath) -> SubPath {
    let mut prev = to_point(start);
    let mut curves = Vec::with_capacity(segs.len());
    for (c1, c2, end) in segs {
        let e = to_point(end);
        curves.push(CubicBezier::new(prev, to_point(c1), to_point(c2), e));
        prev = e;
    }
    SubPath {
        curves,
        closed: true,
    }
}

/// Runs one flo path-arithmetic op, catching panics from pathological input so
/// the caller can fall back. Returns `None` only on panic — a legitimately empty
/// result is `Some(vec![])`.
fn flo_op(a: &[SimpleBezierPath], b: &[SimpleBezierPath], op: Op) -> Option<Vec<SimpleBezierPath>> {
    let a = a.to_vec();
    let b = b.to_vec();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op {
        Op::Union => path_add::<SimpleBezierPath>(&a, &b, FIT_ACCURACY),
        Op::Intersection => path_intersect::<SimpleBezierPath>(&a, &b, FIT_ACCURACY),
        Op::Difference => path_sub::<SimpleBezierPath>(&a, &b, FIT_ACCURACY),
    }))
    .ok()
}

/// Result contours of `op`, preferring the curve-preserving flo route and
/// falling back to the Greiner–Hormann polyline clip when flo panics or comes up
/// empty on inputs the polyline route can still resolve.
fn op_subpaths(a: &dyn Mobject, b: &dyn Mobject, op: Op) -> Vec<SubPath> {
    let fa = mobject_to_flo(a);
    let fb = mobject_to_flo(b);
    if let Some(res) = flo_op(&fa, &fb, op) {
        if !res.is_empty() {
            return orient_even_odd(res.iter().map(flo_to_subpath).collect());
        }
    }
    // Fallback (also correctly empty for genuinely empty results).
    polys_to_subpaths(run(a, b, op))
}

/// Reverses a subpath's direction (flips its winding) by reversing the curve
/// order and each cubic's control points.
fn reverse_subpath(sp: &SubPath) -> SubPath {
    let curves = sp
        .curves
        .iter()
        .rev()
        .map(|c| CubicBezier::new(c.p3, c.p2, c.p1, c.p0))
        .collect();
    SubPath {
        curves,
        closed: sp.closed,
    }
}

/// A point just inside a contour (a vertex nudged toward the centroid), used for
/// nesting tests without landing exactly on the boundary.
fn interior_point(poly: &Poly) -> [f64; 2] {
    let n = poly.len() as f64;
    let cx = poly.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = poly.iter().map(|p| p[1]).sum::<f64>() / n;
    let v = poly[0];
    [v[0] + 0.01 * (cx - v[0]), v[1] + 0.01 * (cy - v[1])]
}

/// `flo_curves` treats every edge as exterior and does not orient holes. Re-wind
/// each result contour by its nesting depth (even = filled/CCW, odd = hole/CW) so
/// holes render correctly under both non-zero and even-odd fill rules.
fn orient_even_odd(subpaths: Vec<SubPath>) -> Vec<SubPath> {
    if subpaths.len() <= 1 {
        return subpaths;
    }
    let polys: Vec<Poly> = subpaths.iter().map(flatten_subpath).collect();
    let reps: Vec<[f64; 2]> = polys.iter().map(interior_point).collect();
    subpaths
        .into_iter()
        .enumerate()
        .map(|(i, sp)| {
            let depth = (0..polys.len())
                .filter(|&j| j != i && point_in_poly(reps[i], &polys[j]))
                .count();
            let want_ccw = depth % 2 == 0;
            let is_ccw = poly_area(&polys[i]) > 0.0;
            if is_ccw == want_ccw {
                sp
            } else {
                reverse_subpath(&sp)
            }
        })
        .collect()
}

/// Closes GH polygon rings back into polyline subpaths (the fallback geometry).
fn polys_to_subpaths(polys: Vec<Poly>) -> Vec<SubPath> {
    polys
        .into_iter()
        .filter(|p| p.len() >= 3)
        .map(|poly| {
            let mut corners: Vec<Point> = poly
                .iter()
                .map(|p| Point::new(p[0] as f32, p[1] as f32, 0.0))
                .collect();
            corners.push(corners[0]);
            let mut sp = SubPath::from_corners(&corners);
            sp.closed = true;
            sp
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Public mobjects.
// ---------------------------------------------------------------------------

/// The union of two mobjects' fill regions (everything inside *either*). Port of
/// manim CE's `Union`.
///
/// ```
/// use manim_core::boolean::Union;
/// use manim_core::geometry::Square;
/// use manim_core::mobject::{Mobject, MobjectExt};
/// use manim_math::RIGHT;
/// let a = Square::new();
/// let mut b = Square::new();
/// b.shift(RIGHT); // overlapping unit squares, centers 1 apart
/// let u = Union::new(&a, &b);
/// // The union spans both squares: width 3 (from -1 to 2).
/// assert!((u.bounding_box().width() - 3.0).abs() < 1e-2);
/// ```
pub struct Union;

impl Union {
    /// Builds the union of `a` and `b` as a filled [`VMobject`] (inherits `a`'s
    /// style).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(a: &dyn Mobject, b: &dyn Mobject) -> VMobject {
        build_from_subpaths(a, op_subpaths(a, b, Op::Union))
    }
}

/// The difference `a − b`: everything inside `a` but not `b`. Port of manim CE's
/// `Difference`.
///
/// ```
/// use manim_core::boolean::Difference;
/// use manim_core::geometry::Square;
/// use manim_core::mobject::{Mobject, MobjectExt};
/// use manim_math::RIGHT;
/// let a = Square::new();
/// let mut b = Square::new();
/// b.shift(0.5 * RIGHT);
/// let d = Difference::new(&a, &b);
/// // Removing the right half leaves the left part of `a`.
/// assert!(d.bounding_box().width() > 0.0);
/// ```
pub struct Difference;

impl Difference {
    /// Builds `a − b` as a filled [`VMobject`] (inherits `a`'s style).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(a: &dyn Mobject, b: &dyn Mobject) -> VMobject {
        build_from_subpaths(a, op_subpaths(a, b, Op::Difference))
    }
}

/// The intersection of two mobjects' fill regions (everything inside *both*).
/// Port of manim CE's `Intersection`.
///
/// ```
/// use manim_core::boolean::Intersection;
/// use manim_core::geometry::Square;
/// use manim_core::mobject::{Mobject, MobjectExt};
/// use manim_math::RIGHT;
/// let a = Square::new();       // [-1,1]^2
/// let mut b = Square::new();
/// b.shift(RIGHT);              // [0,2]x[-1,1]
/// let i = Intersection::new(&a, &b);
/// // Overlap is the strip x in [0,1] — width 1.
/// assert!((i.bounding_box().width() - 1.0).abs() < 1e-2);
/// ```
pub struct Intersection;

impl Intersection {
    /// Builds the intersection of `a` and `b` as a filled [`VMobject`] (inherits
    /// `a`'s style).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(a: &dyn Mobject, b: &dyn Mobject) -> VMobject {
        build_from_subpaths(a, op_subpaths(a, b, Op::Intersection))
    }
}

/// The exclusion (symmetric difference) of two mobjects: inside exactly one of
/// them. Port of manim CE's `Exclusion`. Computed as `(a − b) ∪ (b − a)`.
///
/// ```
/// use manim_core::boolean::Exclusion;
/// use manim_core::geometry::Square;
/// use manim_core::mobject::{Mobject, MobjectExt};
/// use manim_math::RIGHT;
/// let a = Square::new();
/// let mut b = Square::new();
/// b.shift(RIGHT);
/// let x = Exclusion::new(&a, &b);
/// // The overlap is carved out, so the outline spans both squares.
/// assert!((x.bounding_box().width() - 3.0).abs() < 1e-2);
/// ```
pub struct Exclusion;

impl Exclusion {
    /// Builds the symmetric difference of `a` and `b` as a filled [`VMobject`]
    /// (inherits `a`'s style).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(a: &dyn Mobject, b: &dyn Mobject) -> VMobject {
        let mut subpaths = op_subpaths(a, b, Op::Difference);
        subpaths.extend(op_subpaths(b, a, Op::Difference));
        build_from_subpaths(a, subpaths)
    }
}

/// A shape with holes punched out. Port of manim CE's `Cutout`: the result is
/// `main` minus the union of every `hole`. Computed by folding pairwise
/// differences over the holes.
///
/// ```
/// use manim_core::boolean::Cutout;
/// use manim_core::geometry::{Circle, Square};
/// use manim_core::mobject::{Mobject, MobjectExt};
/// let mut main = Square::new();
/// main.scale(2.0); // [-2,2]^2
/// let hole = Circle::new();             // unit circle at origin
/// let holes: Vec<&dyn Mobject> = vec![&hole];
/// let c = Cutout::new(&main, &holes);
/// // Outer extent is unchanged by the interior hole.
/// assert!((c.bounding_box().width() - 4.0).abs() < 1e-1);
/// ```
pub struct Cutout;

impl Cutout {
    /// Builds `main` with each mobject in `holes` subtracted, as a filled
    /// [`VMobject`] (inherits `main`'s style).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(main: &dyn Mobject, holes: &[&dyn Mobject]) -> VMobject {
        // Curve-preserving route: fold flo `path_sub` over each hole.
        let mut cur = mobject_to_flo(main);
        let mut flo_ok = true;
        for hole in holes {
            match flo_op(&cur, &mobject_to_flo(*hole), Op::Difference) {
                Some(next) => cur = next,
                None => {
                    flo_ok = false;
                    break;
                }
            }
        }
        if flo_ok && !cur.is_empty() {
            let subpaths = orient_even_odd(cur.iter().map(flo_to_subpath).collect());
            return build_from_subpaths(main, subpaths);
        }

        // Fallback: Greiner–Hormann polyline cutout.
        let mut polys = vec![representative_poly(main)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        for hole in holes {
            let hp = match representative_poly(*hole) {
                Some(p) => p,
                None => continue,
            };
            let mut next = Vec::new();
            for subj in &polys {
                next.extend(clip(subj, &hp, Op::Difference));
            }
            polys = next;
        }
        build_from_subpaths(main, polys_to_subpaths(polys))
    }
}

// ---------------------------------------------------------------------------
// Driver: mobjects -> polygons -> clip -> polygons.
// ---------------------------------------------------------------------------

/// Runs `op` on the representative contours of `a` and `b`, returning result
/// polygons. Missing contours degrade gracefully (empty input polygon).
fn run(a: &dyn Mobject, b: &dyn Mobject, op: Op) -> Vec<Poly> {
    match (representative_poly(a), representative_poly(b)) {
        (Some(pa), Some(pb)) => clip(&pa, &pb, op),
        (Some(pa), None) => {
            if op == Op::Intersection {
                Vec::new()
            } else {
                vec![pa]
            }
        }
        (None, Some(pb)) => {
            if op == Op::Union {
                vec![pb]
            } else {
                Vec::new()
            }
        }
        (None, None) => Vec::new(),
    }
}

/// The largest-area contour of a mobject, flattened to a polygon.
fn representative_poly(m: &dyn Mobject) -> Option<Poly> {
    path_to_polys(&m.data().path)
        .into_iter()
        .max_by(|x, y| poly_area(x).abs().total_cmp(&poly_area(y).abs()))
        .filter(|p| p.len() >= 3)
}

/// Flattens every subpath of `path` to a polygon.
fn path_to_polys(path: &Path) -> Vec<Poly> {
    path.subpaths
        .iter()
        .filter_map(|sp| {
            let poly = flatten_subpath(sp);
            (poly.len() >= 3).then_some(poly)
        })
        .collect()
}

/// Flattens one subpath's cubics into a polygon ring (drops the repeated closing
/// vertex).
fn flatten_subpath(sp: &SubPath) -> Poly {
    let mut pts: Poly = Vec::new();
    for c in &sp.curves {
        let start = [c.p0.x as f64, c.p0.y as f64];
        if pts.last().map(|p| dist2(*p, start) > 1e-18).unwrap_or(true) {
            pts.push(start);
        }
        flatten_cubic(c, 0, &mut pts);
    }
    // Drop a closing vertex that duplicates the start.
    if pts.len() >= 2 && dist2(pts[0], *pts.last().unwrap()) <= 1e-18 {
        pts.pop();
    }
    pts
}

/// Recursively subdivides a cubic until flat to [`FLATTEN_TOL`], pushing segment
/// endpoints (excluding the start, which the caller already pushed).
fn flatten_cubic(c: &CubicBezier, depth: u32, out: &mut Poly) {
    let p0 = [c.p0.x as f64, c.p0.y as f64];
    let p1 = [c.p1.x as f64, c.p1.y as f64];
    let p2 = [c.p2.x as f64, c.p2.y as f64];
    let p3 = [c.p3.x as f64, c.p3.y as f64];
    // Flatness: max control-point deviation from the chord p0->p3.
    let d1 = point_line_dist(p1, p0, p3);
    let d2 = point_line_dist(p2, p0, p3);
    if depth >= 18 || d1.max(d2) <= FLATTEN_TOL {
        out.push(p3);
        return;
    }
    let (l, r) = c.split(0.5);
    flatten_cubic(&l, depth + 1, out);
    flatten_cubic(&r, depth + 1, out);
}

// ---------------------------------------------------------------------------
// Greiner–Hormann polygon clipping.
// ---------------------------------------------------------------------------

/// A node in the interleaved subject/clip vertex rings.
#[derive(Clone)]
struct Node {
    p: [f64; 2],
    next: usize,
    prev: usize,
    neighbour: Option<usize>,
    intersect: bool,
    entry: bool,
    visited: bool,
}

/// Clips `subject` against `clip` under `op`, returning the result contours.
///
/// Robustness: near-degenerate crossings trigger a perturb-and-retry of the clip
/// polygon (see the [module docs](self)).
fn clip(subject: &Poly, clip_poly: &Poly, op: Op) -> Vec<Poly> {
    // Normalize both inputs to CCW so the asymmetric Difference flip is
    // orientation-independent (input mobjects may wind either way).
    let subject = ensure_ccw(subject.clone());
    let mut clip_poly = ensure_ccw(clip_poly.clone());
    for attempt in 0..=MAX_RETRIES {
        match try_clip(&subject, &clip_poly, op) {
            Ok(result) => return result,
            Err(Degenerate) => {
                let k = (attempt + 1) as f64;
                for v in &mut clip_poly {
                    v[0] += PERTURB.0 * k;
                    v[1] += PERTURB.1 * k;
                }
            }
        }
    }
    // Give up on exactness; take whatever the last attempt produces.
    try_clip(&subject, &clip_poly, op).unwrap_or_default()
}

/// Returns `poly` wound counter-clockwise (positive signed area).
fn ensure_ccw(mut poly: Poly) -> Poly {
    if poly_area(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

/// Signals a degenerate crossing that warrants a perturb-and-retry.
struct Degenerate;

/// One clipping attempt; `Err(Degenerate)` if a near-endpoint crossing is seen.
fn try_clip(subject: &Poly, clip_poly: &Poly, op: Op) -> Result<Vec<Poly>, Degenerate> {
    let mut arena: Vec<Node> = Vec::new();
    let subj_orig = build_ring(subject, &mut arena);
    let clip_orig = build_ring(clip_poly, &mut arena);

    // Phase 1: find intersections between original edges and splice them in.
    let mut had_intersection = false;
    // Per original directed edge, the (alpha, node) intersections to insert.
    let mut subj_ins: Vec<Vec<(f64, usize)>> = vec![Vec::new(); subj_orig.len()];
    let mut clip_ins: Vec<Vec<(f64, usize)>> = vec![Vec::new(); clip_orig.len()];

    for (si, &su) in subj_orig.iter().enumerate() {
        let sv = subj_orig[(si + 1) % subj_orig.len()];
        let a0 = arena[su].p;
        let a1 = arena[sv].p;
        for (ci, &cu) in clip_orig.iter().enumerate() {
            let cv = clip_orig[(ci + 1) % clip_orig.len()];
            let b0 = arena[cu].p;
            let b1 = arena[cv].p;
            if let Some((ta, tb, pt)) = segment_intersection(a0, a1, b0, b1)? {
                had_intersection = true;
                let sn = new_intersection(&mut arena, pt);
                let cn = new_intersection(&mut arena, pt);
                arena[sn].neighbour = Some(cn);
                arena[cn].neighbour = Some(sn);
                subj_ins[si].push((ta, sn));
                clip_ins[ci].push((tb, cn));
            }
        }
    }

    if !had_intersection {
        return Ok(no_intersection_case(subject, clip_poly, op));
    }

    splice(&mut arena, &subj_orig, &mut subj_ins);
    splice(&mut arena, &clip_orig, &mut clip_ins);

    // Phase 2: mark entry/exit on each ring relative to the other polygon.
    mark_entries(&mut arena, subj_orig[0], clip_poly);
    mark_entries(&mut arena, clip_orig[0], subject);

    // Apply the operation as flips of the intersection-marking.
    match op {
        Op::Intersection => {}
        Op::Union => flip_all(&mut arena),
        // A − B keeps A's exterior-of-B arcs and B's interior-of-A arcs: flip the
        // subject ring's entry sense (flipping the clip ring would yield B − A).
        Op::Difference => flip_ring(&mut arena, subj_orig[0]),
    }

    // Phase 3: trace result contours.
    Ok(trace(&mut arena))
}

/// Appends a ring of vertices for `poly` to `arena`, returning the original
/// vertex indices in order.
fn build_ring(poly: &Poly, arena: &mut Vec<Node>) -> Vec<usize> {
    let base = arena.len();
    let n = poly.len();
    let mut ids = Vec::with_capacity(n);
    for (i, p) in poly.iter().enumerate() {
        arena.push(Node {
            p: *p,
            next: base + (i + 1) % n,
            prev: base + (i + n - 1) % n,
            neighbour: None,
            intersect: false,
            entry: false,
            visited: false,
        });
        ids.push(base + i);
    }
    ids
}

/// Pushes a fresh intersection node (not yet linked into a ring).
fn new_intersection(arena: &mut Vec<Node>, p: [f64; 2]) -> usize {
    let idx = arena.len();
    arena.push(Node {
        p,
        next: idx,
        prev: idx,
        neighbour: None,
        intersect: true,
        entry: false,
        visited: false,
    });
    idx
}

/// Splices each edge's intersection nodes (sorted by alpha) between its original
/// endpoints.
fn splice(arena: &mut [Node], orig: &[usize], ins: &mut [Vec<(f64, usize)>]) {
    for (ei, list) in ins.iter_mut().enumerate() {
        if list.is_empty() {
            continue;
        }
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        let u = orig[ei];
        let v = orig[(ei + 1) % orig.len()];
        let mut prev = u;
        for &(_, node) in list.iter() {
            arena[prev].next = node;
            arena[node].prev = prev;
            prev = node;
        }
        arena[prev].next = v;
        arena[v].prev = prev;
    }
}

/// Marks entry/exit flags around the ring starting at `start`, relative to
/// `other`. Uses the intersection convention: a crossing from outside `other`
/// into it is an entry.
fn mark_entries(arena: &mut [Node], start: usize, other: &Poly) {
    let mut inside = point_in_poly(arena[start].p, other);
    let mut cur = start;
    loop {
        if arena[cur].intersect {
            arena[cur].entry = !inside;
            inside = !inside;
        }
        cur = arena[cur].next;
        if cur == start {
            break;
        }
    }
}

/// Flips entry flags on every intersection node (used for union).
fn flip_all(arena: &mut [Node]) {
    for n in arena.iter_mut() {
        if n.intersect {
            n.entry = !n.entry;
        }
    }
}

/// Flips entry flags on the ring reachable from `start` (used for difference,
/// applied to the clip ring).
fn flip_ring(arena: &mut [Node], start: usize) {
    let mut cur = start;
    loop {
        if arena[cur].intersect {
            arena[cur].entry = !arena[cur].entry;
        }
        cur = arena[cur].next;
        if cur == start {
            break;
        }
    }
}

/// Walks the marked graph, emitting one polygon per closed traversal.
fn trace(arena: &mut [Node]) -> Vec<Poly> {
    let mut result = Vec::new();
    let n = arena.len();
    for start in 0..n {
        if !arena[start].intersect || arena[start].visited {
            continue;
        }
        let mut poly: Poly = Vec::new();
        let mut cur = start;
        loop {
            arena[cur].visited = true;
            if let Some(nb) = arena[cur].neighbour {
                arena[nb].visited = true;
            }
            let forward = arena[cur].entry;
            loop {
                cur = if forward {
                    arena[cur].next
                } else {
                    arena[cur].prev
                };
                poly.push(arena[cur].p);
                if arena[cur].intersect {
                    break;
                }
            }
            match arena[cur].neighbour {
                Some(nb) => cur = nb,
                None => break,
            }
            if cur == start || arena[cur].visited {
                break;
            }
        }
        if poly.len() >= 3 {
            result.push(poly);
        }
    }
    result
}

/// Handles operations when the polygons do not cross: pure containment /
/// disjointness.
fn no_intersection_case(subject: &Poly, clip_poly: &Poly, op: Op) -> Vec<Poly> {
    let s_in_c = point_in_poly(subject[0], clip_poly);
    let c_in_s = point_in_poly(clip_poly[0], subject);
    match op {
        Op::Intersection => {
            if s_in_c {
                vec![subject.clone()]
            } else if c_in_s {
                vec![clip_poly.clone()]
            } else {
                Vec::new()
            }
        }
        Op::Union => {
            if s_in_c {
                vec![clip_poly.clone()]
            } else if c_in_s {
                vec![subject.clone()]
            } else {
                vec![subject.clone(), clip_poly.clone()]
            }
        }
        Op::Difference => {
            if s_in_c {
                // Subject entirely removed.
                Vec::new()
            } else if c_in_s {
                // A hole: subject outline plus the clip as a reversed inner ring.
                let mut hole = clip_poly.clone();
                hole.reverse();
                vec![subject.clone(), hole]
            } else {
                vec![subject.clone()]
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry primitives (f64).
// ---------------------------------------------------------------------------

/// Proper segment intersection of `a0a1` and `b0b1`, returning
/// `(t_a, t_b, point)` for parameters strictly inside both segments, `None` for
/// no crossing, `Err(Degenerate)` when a crossing lands on an endpoint.
#[allow(clippy::type_complexity)]
fn segment_intersection(
    a0: [f64; 2],
    a1: [f64; 2],
    b0: [f64; 2],
    b1: [f64; 2],
) -> Result<Option<(f64, f64, [f64; 2])>, Degenerate> {
    let r = [a1[0] - a0[0], a1[1] - a0[1]];
    let s = [b1[0] - b0[0], b1[1] - b0[1]];
    let denom = cross(r, s);
    let qp = [b0[0] - a0[0], b0[1] - a0[1]];
    if denom.abs() < 1e-12 {
        // Parallel; treat any collinear overlap as degenerate, ignore otherwise.
        if cross(qp, r).abs() < 1e-9 && overlaps_collinear(a0, a1, b0, b1) {
            return Err(Degenerate);
        }
        return Ok(None);
    }
    let t = cross(qp, s) / denom;
    let u = cross(qp, r) / denom;
    let bounds = -DEGEN_EPS..=1.0 + DEGEN_EPS;
    if !bounds.contains(&t) || !bounds.contains(&u) {
        return Ok(None);
    }
    let interior = DEGEN_EPS..=1.0 - DEGEN_EPS;
    if !interior.contains(&t) || !interior.contains(&u) {
        return Err(Degenerate);
    }
    let pt = [a0[0] + t * r[0], a0[1] + t * r[1]];
    Ok(Some((t, u, pt)))
}

/// Whether two collinear segments share more than a point.
fn overlaps_collinear(a0: [f64; 2], a1: [f64; 2], b0: [f64; 2], b1: [f64; 2]) -> bool {
    let d = [a1[0] - a0[0], a1[1] - a0[1]];
    let proj = |p: [f64; 2]| (p[0] - a0[0]) * d[0] + (p[1] - a0[1]) * d[1];
    let len2 = d[0] * d[0] + d[1] * d[1];
    if len2 < 1e-18 {
        return false;
    }
    let (tb0, tb1) = (proj(b0) / len2, proj(b1) / len2);
    let (lo, hi) = (tb0.min(tb1), tb0.max(tb1));
    hi > 1e-9 && lo < 1.0 - 1e-9
}

/// Even-odd ray-cast point-in-polygon test.
fn point_in_poly(p: [f64; 2], poly: &Poly) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Signed shoelace area of a polygon (positive for CCW).
fn poly_area(poly: &Poly) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut j = n - 1;
    for i in 0..n {
        sum += (poly[j][0] + poly[i][0]) * (poly[j][1] - poly[i][1]);
        j = i;
    }
    sum / 2.0
}

/// 2D cross product `a × b`.
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// Squared distance between two points.
fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1]];
    d[0] * d[0] + d[1] * d[1]
}

/// Perpendicular distance from `p` to the line through `a` and `b`.
fn point_line_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len < 1e-12 {
        return dist2(p, a).sqrt();
    }
    (cross([p[0] - a[0], p[1] - a[1]], [d[0] / len, d[1] / len])).abs()
}

// ---------------------------------------------------------------------------
// Result construction.
// ---------------------------------------------------------------------------

/// Builds a filled [`VMobject`] from result polygons, inheriting `src`'s style
/// (falling back to a white fill when `src` has none).
fn build_from_subpaths(src: &dyn Mobject, subpaths: Vec<SubPath>) -> VMobject {
    let mut style = src.data().style.clone();
    if style.fill_color.is_none() && style.stroke_color.is_none() {
        style = Style::filled(WHITE);
    }
    VMobject::new(Path { subpaths }, style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Circle, Square};
    use crate::mobject::MobjectExt;
    use manim_math::{RIGHT, UP};

    /// The turn angle (degrees) between consecutive curves' tangents at each ring
    /// join of a closed subpath.
    fn join_angles(sp: &SubPath) -> Vec<f64> {
        let m = sp.curves.len();
        (0..m)
            .map(|i| {
                let cin = &sp.curves[i];
                let cout = &sp.curves[(i + 1) % m];
                let ti = cin.p3 - cin.p2;
                let to = cout.p1 - cout.p0;
                let dot = (ti.x * to.x + ti.y * to.y) as f64;
                let mag = ((ti.x * ti.x + ti.y * ti.y).sqrt() * (to.x * to.x + to.y * to.y).sqrt())
                    as f64;
                if mag > 1e-12 {
                    (dot / mag).clamp(-1.0, 1.0).acos().to_degrees()
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// Total absolute area of a set of result polygons.
    fn total_area(polys: &[Poly]) -> f64 {
        polys.iter().map(|p| poly_area(p).abs()).sum()
    }

    fn square_poly(cx: f64, cy: f64, half: f64) -> Poly {
        vec![
            [cx - half, cy - half],
            [cx + half, cy - half],
            [cx + half, cy + half],
            [cx - half, cy + half],
        ]
    }

    #[test]
    fn union_of_circles_is_curve_preserving_and_accurate() {
        let a = Circle::new();
        let mut b = Circle::new();
        b.shift(RIGHT);
        let u = Union::new(&a, &b);
        let path = &u.data().path;

        // Curve-preserving: a handful of cubic arcs, NOT the hundreds of tiny
        // segments the old polyline flattening produced.
        let n_curves: usize = path.subpaths.iter().map(|s| s.curves.len()).sum();
        assert!(n_curves < 40, "expected few cubic arcs, got {n_curves}");

        // Deviation from the analytic union boundary (unit circles at 0 / RIGHT):
        // every boundary point lies on whichever circle it is outside the other of.
        let mut max_dev = 0.0_f64;
        for sp in &path.subpaths {
            for c in &sp.curves {
                for k in 0..=16 {
                    let p = c.eval(k as f32 / 16.0);
                    let da = ((p.x as f64).powi(2) + (p.y as f64).powi(2)).sqrt();
                    let db = (((p.x as f64) - 1.0).powi(2) + (p.y as f64).powi(2)).sqrt();
                    max_dev = max_dev.max((da - 1.0).abs().min((db - 1.0).abs()));
                }
            }
        }
        assert!(
            max_dev < 0.01,
            "union boundary deviates {max_dev} from the true arcs"
        );
    }

    #[test]
    fn union_of_circles_arcs_stay_smooth() {
        let a = Circle::new();
        let mut b = Circle::new();
        b.shift(RIGHT);
        let u = Union::new(&a, &b);
        let sp = &u.data().path.subpaths[0];
        let angles = join_angles(sp);
        let m = angles.len();

        // Two circles cross at exactly two points (legitimate kinks); every other
        // join follows a single circle arc and must be tangent-continuous — no
        // spurious corner where the arc should be smooth.
        let corners = angles.iter().filter(|a| **a > 15.0).count();
        let smooth = angles.iter().filter(|a| **a < 1.0).count();
        assert!(
            corners <= 2,
            "expected ≤2 crossing corners, got {corners} ({angles:?})"
        );
        assert!(
            smooth >= m / 2,
            "arc joins should be mostly smooth, got {smooth}/{m} ({angles:?})"
        );
    }

    #[test]
    fn degenerate_shared_edge_union_area() {
        // Two unit squares sharing the edge x=1 — collinear edges, a GH
        // degeneracy. The curve route (or its GH fallback) must still be correct.
        let a = Square::new(); // [-1,1]^2, area 4
        let mut b = Square::new();
        b.shift(2.0 * RIGHT); // [1,3]x[-1,1]
        let u = Union::new(&a, &b);
        let area = total_area(&path_to_polys(&u.data().path));
        assert!(
            (area - 8.0).abs() < 1e-1,
            "shared-edge union area={area}, want 8"
        );
        assert!((u.bounding_box().width() - 4.0).abs() < 1e-1);
    }

    #[test]
    fn degenerate_shared_vertex_union_area() {
        // Two unit squares touching only at the corner (1,1) — vertex incidence.
        let a = Square::new();
        let mut b = Square::new();
        b.shift(2.0 * RIGHT + 2.0 * UP);
        let u = Union::new(&a, &b);
        let area = total_area(&path_to_polys(&u.data().path));
        assert!(
            (area - 8.0).abs() < 2e-1,
            "vertex-touch union area={area}, want 8"
        );
    }

    #[test]
    fn union_area_identity() {
        // area(A∪B) = area(A) + area(B) − area(A∩B).
        let a = square_poly(0.0, 0.0, 1.0);
        let b = square_poly(1.0, 1.0, 1.0);
        let union = total_area(&clip(&a, &b, Op::Union));
        let inter = total_area(&clip(&a, &b, Op::Intersection));
        assert!(
            (union - (poly_area(&a).abs() + poly_area(&b).abs() - inter)).abs() < 1e-3,
            "union={union} inter={inter}"
        );
    }

    #[test]
    fn intersection_is_overlap_rect() {
        // A=[-1,1]^2, B=[0,2]^2 → overlap [0,1]^2 area 1.
        let a = square_poly(0.0, 0.0, 1.0);
        let b = square_poly(1.0, 1.0, 1.0);
        let inter = total_area(&clip(&a, &b, Op::Intersection));
        assert!((inter - 1.0).abs() < 1e-3, "inter={inter}");
    }

    #[test]
    fn difference_area_and_bbox() {
        // area(A−B) = area(A) − area(A∩B); bbox unchanged (L-shape).
        let a = square_poly(0.0, 0.0, 1.0);
        let b = square_poly(1.0, 1.0, 1.0);
        let diff = clip(&a, &b, Op::Difference);
        let da = total_area(&diff);
        let inter = total_area(&clip(&a, &b, Op::Intersection));
        assert!(
            (da - (poly_area(&a).abs() - inter)).abs() < 1e-3,
            "diff={da}"
        );
        // Direction check: A−B reaches A's far corner (min x ≈ -1); B−A would
        // not (its min x is 0). Guards against flipping the wrong ring.
        let min_x = diff
            .iter()
            .flatten()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min);
        assert!(min_x < -0.9, "A−B should span to x≈-1, got min_x={min_x}");
    }

    #[test]
    fn non_overlapping_difference_is_original() {
        let a = square_poly(0.0, 0.0, 1.0);
        let b = square_poly(5.0, 5.0, 1.0);
        let diff = clip(&a, &b, Op::Difference);
        assert_eq!(diff.len(), 1);
        assert!((total_area(&diff) - poly_area(&a).abs()).abs() < 1e-6);
    }

    #[test]
    fn exclusion_is_union_minus_intersection() {
        let a = Square::new();
        let mut b = Square::new();
        b.shift(RIGHT);
        let x = Exclusion::new(&a, &b);
        // Symmetric difference of two unit squares overlapping by a 1x2 strip:
        // total area = 4 + 4 − 2·(overlap 2) = 4. (each square area 4, overlap 2)
        let polys = path_to_polys(&x.data().path);
        assert!(
            (total_area(&polys) - 4.0).abs() < 1e-1,
            "area={}",
            total_area(&polys)
        );
    }

    #[test]
    fn cutout_has_hole() {
        let mut main = Square::new();
        main.scale(2.0); // area 16
        let hole = Square::new(); // area 4, centered inside
        let holes: Vec<&dyn Mobject> = vec![&hole];
        let c = Cutout::new(&main, &holes);
        let polys = path_to_polys(&c.data().path);
        // Two contours: outer ring + reversed inner ring (a hole).
        assert_eq!(polys.len(), 2, "expected outer + hole contours");
        // The hole is wound opposite the outer ring, so the *signed* area sum is
        // the net filled area: 16 − 4 = 12.
        let net: f64 = polys.iter().map(poly_area).sum::<f64>().abs();
        assert!((net - 12.0).abs() < 1e-1, "net={net}");
    }
}
