//! Molecular render builders: ball-and-stick, space-filling, and wireframe
//! models plus orbital isosurfaces, all on the GPU-instanced mesh pipeline.
//!
//! Every atom cloud is **one** [`InstancedMesh`] (one draw call, whatever the
//! atom count) and every bond set is another: per-atom CPK color and per-element
//! radius ride the instance buffer, so a 10k-atom model is still two draws.
//!
//! # Coordinates and units
//!
//! Positions and radii are in ångström, straight from the [`Molecule`]. Radii
//! come from the [`element`](crate::element) table (covalent for bonds/sticks,
//! van-der-Waals for space-filling). Unknown element symbols fall back to a
//! neutral grey with generic radii so a model never silently loses atoms.

use glam::{Mat4, Quat, Vec3};

use manim_core::geometry::VGroup;
use manim_core::mesh::{Instance, InstancedMesh, Mesh, MeshMaterial, TriMesh};
use manim_core::mobject::MobjectId;
use manim_core::prelude::{Color, BLUE, RED, WHITE};
use manim_core::scene_state::SceneState;
use manim_sci::isosurface::Isosurface;

use crate::molecule::{Atom, Bond, Molecule};

/// Ball-and-stick sphere radius as a fraction of the element covalent radius.
///
/// Full covalent radii make neighbouring spheres touch (that is what a covalent
/// radius *is*), hiding the bonds; `0.3` shrinks them to legible balls while
/// keeping their *relative* sizes correct.
pub const BALL_AND_STICK_RADIUS_SCALE: f32 = 0.3;

/// Ball-and-stick bond cylinder radius (ångström).
pub const BOND_RADIUS: f32 = 0.1;

/// Wireframe bond cylinder radius (ångström) — thinner than [`BOND_RADIUS`].
pub const WIREFRAME_BOND_RADIUS: f32 = 0.05;

/// Centre-to-centre offset (ångström) between the parallel cylinders of a
/// double or triple bond.
pub const MULTI_BOND_SEPARATION: f32 = 0.18;

/// Latitude/longitude bands of an atom sphere. Coarse on purpose — instancing
/// spends its budget on instance count, not per-atom tessellation.
const ATOM_RINGS: usize = 12;
/// Longitude divisions of an atom sphere (twice [`ATOM_RINGS`]).
const ATOM_SEGMENTS: usize = 24;
/// Divisions around a bond cylinder.
const BOND_SEGMENTS: usize = 12;

/// Marching-cubes cells per axis for [`molecular_orbital_isosurface`].
const ORBITAL_RESOLUTION: usize = 48;
/// Half-width (ångström) of the default cubic sampling box for orbitals.
///
/// [`crate::cube::CubeData`] only guarantees a
/// [`ScalarField`](manim_fields::field::ScalarField); it carries no bounds in
/// the API this crate relies on, so orbitals are sampled over this symmetric box
/// about the origin — ample for a small-molecule MO. Widen it here if a lobe is
/// clipped.
const ORBITAL_HALF_EXTENT: f64 = 6.0;

/// Colour and radii used when an element symbol is not in the table.
const FALLBACK: (Color, f32, f32) = (Color::from_rgb(0.5, 0.5, 0.5), 0.7, 1.6);

/// Which per-element radius sizes an atom sphere.
///
/// Ball-and-stick defaults to [`Covalent`](Self::Covalent), which is right for
/// molecules but actively misleading for an ionic solid: covalent radii make
/// Na *bigger* than Cl, while in rock salt the chloride **anion** is much the
/// larger of the two. Size ionic crystals with [`Ionic`](Self::Ionic).
///
/// ```
/// use manim_chem::molecule::Atom;
/// use manim_chem::render::RadiusSource;
/// use glam::Vec3;
///
/// let na = Atom::new("Na", Vec3::ZERO);
/// let cl = Atom::new("Cl", Vec3::X);
/// // Covalent radii say sodium is larger…
/// assert!(RadiusSource::Covalent.radius_for(&na) > RadiusSource::Covalent.radius_for(&cl));
/// // …ionic radii, correctly for a salt, say chloride is.
/// assert!(RadiusSource::Ionic.radius_for(&cl) > RadiusSource::Ionic.radius_for(&na));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadiusSource {
    /// Single-bond covalent radius (Cordero 2008). The default: correct for
    /// molecules, wrong for salts.
    #[default]
    Covalent,
    /// Van-der-Waals radius (Bondi 1964 / Alvarez 2013) — what
    /// [`space_filling`] uses.
    VdW,
    /// Shannon effective ionic radius at CN 6, for the atom's
    /// [`charge`](crate::molecule::Atom::charge) or the element's common
    /// oxidation state. Falls back to [`Covalent`](Self::Covalent) for elements
    /// with no ion in the table (noble gases, hydrogen) or an unlisted charge,
    /// so a mixed structure never loses an atom.
    Ionic,
}

impl RadiusSource {
    /// The radius (ångström) this source assigns to `atom`.
    ///
    /// Unknown element symbols get the generic fallback radii rather than zero.
    ///
    /// ```
    /// use manim_chem::molecule::Atom;
    /// use manim_chem::render::RadiusSource;
    /// use glam::Vec3;
    /// // Argon has no common ion, so Ionic falls back to the covalent radius.
    /// let ar = Atom::new("Ar", Vec3::ZERO);
    /// assert_eq!(
    ///     RadiusSource::Ionic.radius_for(&ar),
    ///     RadiusSource::Covalent.radius_for(&ar),
    /// );
    /// ```
    pub fn radius_for(&self, atom: &Atom) -> f32 {
        let (_, covalent, vdw) = element_info(&atom.element);
        match self {
            Self::Covalent => covalent,
            Self::VdW => vdw,
            Self::Ionic => crate::element::ionic_radius(&atom.element, atom.charge)
                // An explicit but unlisted charge still deserves *an* ion radius
                // if the element has one; only then fall back to covalent.
                .or_else(|| crate::element::ionic_radius(&atom.element, None))
                .unwrap_or(covalent),
        }
    }
}

/// Electronegativity difference (Pauling) above which
/// [`BondRule::UnlikeOnly`] treats a pair as an ionic contact worth drawing.
///
/// `1.7` is the classic Pauling cutoff for ~50% ionic character. Na–Cl is 2.23
/// and passes; C–H is 0.35 and does not.
pub const IONIC_ELECTRONEGATIVITY_THRESHOLD: f32 = 1.7;

/// How [`perceive_bonds_with`] decides which atom pairs are bonded.
///
/// ```
/// use manim_chem::lattice::nacl;
/// use manim_chem::render::{perceive_bonds_with, BondRule};
///
/// let crystal = nacl().replicate(1, 1, 1);
/// let covalent = perceive_bonds_with(&crystal, BondRule::CovalentHeuristic);
/// let ionic = perceive_bonds_with(&crystal, BondRule::UnlikeOnly);
/// // The covalent heuristic also "bonds" Na to Na; the ionic rule does not.
/// assert!(ionic.len() < covalent.len());
/// assert!(perceive_bonds_with(&crystal, BondRule::Explicit).is_empty());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BondRule {
    /// The distance heuristic: bond any pair closer than `1.3 ×` the sum of
    /// their covalent radii. The default; right for molecules.
    #[default]
    CovalentHeuristic,
    /// The distance heuristic **plus** an unlike-ions filter: the two atoms must
    /// be different elements whose Pauling electronegativity difference exceeds
    /// [`IONIC_ELECTRONEGATIVITY_THRESHOLD`].
    ///
    /// This is what keeps an ionic lattice from hairballing. In rock salt the
    /// second-nearest neighbours are Na–Na and Cl–Cl at `a/√2 ≈ 3.99 Å`, and
    /// the covalent criterion happily bonds the Na pairs (sodium's covalent
    /// radius is large); the resulting cat's-cradle obscures the structure.
    /// Requiring unlike, strongly-polarized partners leaves only the Na–Cl
    /// octahedral contacts that make the rock-salt motif legible.
    UnlikeOnly,
    /// Perceive nothing — keep whatever bonds the molecule already carries.
    /// Use when connectivity came from the source file, or for a structure
    /// (a metal, an extended solid) where no distance rule is honest.
    Explicit,
}

/// Grey stick colour for bonds.
const BOND_COLOR: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Looks up `(cpk_color, covalent_radius, vdw_radius)` for `symbol`, falling
/// back to a neutral grey with generic radii for unknown elements.
fn element_info(symbol: &str) -> (Color, f32, f32) {
    match crate::element::data(symbol) {
        Some(d) => (d.cpk_color, d.covalent_radius, d.vdw_radius),
        None => FALLBACK,
    }
}

/// A unit sphere scaled to `radius`, placed at `pos`, tinted `color`.
fn sphere_instance(pos: Vec3, radius: f32, color: Color) -> Instance {
    Instance::new(
        Mat4::from_scale_rotation_translation(Vec3::splat(radius), Quat::IDENTITY, pos),
        color,
    )
}

/// A unit `+Z` cylinder scaled to `radius` and length `|q - p|`, rotated onto
/// `q - p`, and translated to the midpoint; `None` for a zero-length bond.
fn cylinder_instance(p: Vec3, q: Vec3, radius: f32, color: Color) -> Option<Instance> {
    let axis = q - p;
    let length = axis.length();
    if length <= 1e-9 {
        return None;
    }
    Some(Instance::new(
        Mat4::from_scale_rotation_translation(
            Vec3::new(radius, radius, length),
            Quat::from_rotation_arc(Vec3::Z, axis / length),
            (p + q) * 0.5,
        ),
        color,
    ))
}

/// The `(start, end)` endpoint pairs a bond of `order` draws.
///
/// A single bond is one axial cylinder; a double is two, a triple is three,
/// offset perpendicular to the bond by [`MULTI_BOND_SEPARATION`]. The offset
/// direction is any stable perpendicular to the bond axis (`axis × X`, or
/// `axis × Y` when the axis is near-parallel to `X`), so it is deterministic for
/// a given bond.
fn bond_cylinders(p: Vec3, q: Vec3, order: u8, sep: f32) -> Vec<(Vec3, Vec3)> {
    let axis = q - p;
    let dir = if axis.length() > 1e-9 {
        axis.normalize()
    } else {
        Vec3::Z
    };
    let reference = if dir.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let perp = dir.cross(reference).normalize_or_zero();
    match order {
        2 => {
            let o = perp * (sep * 0.5);
            vec![(p - o, q - o), (p + o, q + o)]
        }
        n if n >= 3 => {
            let o = perp * sep;
            vec![(p - o, q - o), (p, q), (p + o, q + o)]
        }
        // 0 and 1 both draw a single axial stick.
        _ => vec![(p, q)],
    }
}

/// Collects every bond cylinder instance for `mol` at `radius`.
fn bond_instances(mol: &Molecule, radius: f32) -> Vec<Instance> {
    let mut instances = Vec::new();
    for bond in &mol.bonds {
        let (Some(a), Some(b)) = (mol.atoms.get(bond.a), mol.atoms.get(bond.b)) else {
            continue;
        };
        for (p, q) in bond_cylinders(a.pos, b.pos, bond.order, MULTI_BOND_SEPARATION) {
            if let Some(inst) = cylinder_instance(p, q, radius, BOND_COLOR) {
                instances.push(inst);
            }
        }
    }
    instances
}

/// The shading material shared by every atom/bond cloud (per-instance colour
/// multiplies this white base).
fn cloud_material() -> MeshMaterial {
    MeshMaterial::new(WHITE).with_lighting(0.3, 0.7, 0.4)
}

/// Builds a **ball-and-stick** model of `mol`: spheres at
/// [`BALL_AND_STICK_RADIUS_SCALE`]`×` each element covalent radius, CPK-coloured,
/// with bonds drawn as [`BOND_RADIUS`] cylinders (double/triple bonds as 2/3
/// parallel offset cylinders). The atom cloud and the bond cloud are wrapped in
/// a [`VGroup`] so the whole model transforms as one.
///
/// ```
/// use manim_chem::molecule::{Atom, Bond, Molecule};
/// use manim_chem::render;
/// use manim_core::scene_state::SceneState;
/// use glam::Vec3;
///
/// let mol = Molecule {
///     atoms: vec![Atom::new("C", Vec3::ZERO), Atom::new("O", 1.2 * Vec3::X)],
///     bonds: vec![Bond::new(0, 1, 2)],
/// };
/// let mut scene = SceneState::new();
/// let model = render::ball_and_stick(&mut scene, &mol);
/// // The group holds the atom mesh and the bond mesh (group + 2 children).
/// assert_eq!(scene.family(model).len(), 3);
/// ```
pub fn ball_and_stick(scene: &mut SceneState, mol: &Molecule) -> MobjectId<VGroup> {
    ball_and_stick_sized(scene, mol, RadiusSource::Covalent)
}

/// [`ball_and_stick`] with an explicit [`RadiusSource`] for the atom spheres.
///
/// Use [`RadiusSource::Ionic`] for a salt, where covalent radii would draw the
/// cation larger than the anion.
///
/// ```
/// use manim_chem::lattice::nacl;
/// use manim_chem::render::{ball_and_stick_sized, RadiusSource};
/// use manim_core::scene_state::SceneState;
///
/// let crystal = nacl().replicate(1, 1, 1);
/// let mut scene = SceneState::new();
/// let model = ball_and_stick_sized(&mut scene, &crystal, RadiusSource::Ionic);
/// assert_eq!(scene.family(model).len(), 3); // group + atoms + bonds
/// ```
pub fn ball_and_stick_sized(
    scene: &mut SceneState,
    mol: &Molecule,
    radius: RadiusSource,
) -> MobjectId<VGroup> {
    let atom_instances: Vec<Instance> = mol
        .atoms
        .iter()
        .map(|a| {
            let (color, _, _) = element_info(&a.element);
            sphere_instance(
                a.pos,
                BALL_AND_STICK_RADIUS_SCALE * radius.radius_for(a),
                color,
            )
        })
        .collect();
    let atoms = scene.add(
        InstancedMesh::new(
            TriMesh::uv_sphere(ATOM_RINGS, ATOM_SEGMENTS),
            atom_instances,
        )
        .with_material(cloud_material()),
    );

    let bonds = scene.add(
        InstancedMesh::new(
            TriMesh::cylinder(BOND_SEGMENTS),
            bond_instances(mol, BOND_RADIUS),
        )
        .with_material(cloud_material()),
    );

    VGroup::of(scene, [atoms.erase(), bonds.erase()])
}

/// Builds a **space-filling** (CPK) model of `mol`: one sphere per atom at its
/// full van-der-Waals radius, CPK-coloured, with no bonds.
///
/// ```
/// use manim_chem::molecule::{Atom, Molecule};
/// use manim_chem::render;
/// use manim_core::scene_state::SceneState;
/// use glam::Vec3;
///
/// let mol = Molecule {
///     atoms: vec![Atom::new("C", Vec3::ZERO), Atom::new("H", Vec3::X)],
///     bonds: vec![],
/// };
/// let mut scene = SceneState::new();
/// let cloud = render::space_filling(&mut scene, &mol);
/// assert_eq!(scene[cloud].instances().len(), 2);
/// ```
pub fn space_filling(scene: &mut SceneState, mol: &Molecule) -> MobjectId<InstancedMesh> {
    let instances: Vec<Instance> = mol
        .atoms
        .iter()
        .map(|a| {
            let (color, _, _) = element_info(&a.element);
            sphere_instance(a.pos, RadiusSource::VdW.radius_for(a), color)
        })
        .collect();
    scene.add(
        InstancedMesh::new(TriMesh::uv_sphere(ATOM_RINGS, ATOM_SEGMENTS), instances)
            .with_material(cloud_material()),
    )
}

/// Builds a **wireframe** model of `mol`: only the bonds, as thin
/// [`WIREFRAME_BOND_RADIUS`] cylinders, with no atom spheres.
///
/// ```
/// use manim_chem::molecule::{Atom, Bond, Molecule};
/// use manim_chem::render;
/// use manim_core::scene_state::SceneState;
/// use glam::Vec3;
///
/// let mol = Molecule {
///     atoms: vec![Atom::new("C", Vec3::ZERO), Atom::new("C", 1.5 * Vec3::X)],
///     bonds: vec![Bond::new(0, 1, 1)],
/// };
/// let mut scene = SceneState::new();
/// let wire = render::wireframe(&mut scene, &mol);
/// assert_eq!(scene[wire].instances().len(), 1);
/// ```
pub fn wireframe(scene: &mut SceneState, mol: &Molecule) -> MobjectId<InstancedMesh> {
    scene.add(
        InstancedMesh::new(
            TriMesh::cylinder(BOND_SEGMENTS),
            bond_instances(mol, WIREFRAME_BOND_RADIUS),
        )
        .with_material(cloud_material()),
    )
}

/// Infers bonds from interatomic distances alone.
///
/// Atoms `i` and `j` are bonded (order 1) when their separation `d` satisfies
/// `0.4 Å < d < 1.3 × (r_cov(i) + r_cov(j))`: the upper bound is the classic
/// distance criterion (a modest tolerance over the sum of covalent radii), the
/// lower bound rejects coincident atoms.
///
/// **Limits:** this is purely geometric. It assigns every bond order 1 — it
/// does **not** detect double/triple bonds, aromaticity, or hypervalency — and a
/// badly distorted or non-equilibrium geometry can over- or under-bond. For
/// exact connectivity, read bonds from the source file instead.
///
/// ```
/// use manim_chem::molecule::{Atom, Molecule};
/// use manim_chem::render::perceive_bonds;
/// use glam::Vec3;
///
/// // Water at ~real geometry: two O–H bonds, no H–H bond.
/// let mol = Molecule {
///     atoms: vec![
///         Atom::new("O", Vec3::ZERO),
///         Atom::new("H", Vec3::new(0.758, 0.587, 0.0)),
///         Atom::new("H", Vec3::new(-0.758, 0.587, 0.0)),
///     ],
///     bonds: vec![],
/// };
/// assert_eq!(perceive_bonds(&mol).len(), 2);
/// ```
pub fn perceive_bonds(mol: &Molecule) -> Vec<Bond> {
    perceive_bonds_with(mol, BondRule::CovalentHeuristic)
}

/// Infers bonds under an explicit [`BondRule`].
///
/// [`perceive_bonds`] is this with [`BondRule::CovalentHeuristic`]; reach for
/// [`BondRule::UnlikeOnly`] on an ionic lattice.
///
/// **Limits:** every rule here is geometric plus, at most, a two-element
/// electronegativity test. All bonds come out order 1 — no double/triple bonds,
/// no aromaticity, no hypervalency. [`BondRule::UnlikeOnly`] is a *legibility*
/// heuristic, not a bonding theory: it will also suppress genuine like-atom
/// bonds (the Si–Si framework of a silicate, a metal–metal bond), and its
/// electronegativity cutoff is a convention with no sharp physical meaning.
/// When connectivity matters, read bonds from the source file and use
/// [`BondRule::Explicit`].
///
/// ```
/// use manim_chem::molecule::{Atom, Molecule};
/// use manim_chem::render::{perceive_bonds_with, BondRule};
/// use glam::Vec3;
///
/// // Two sodiums at a distance the covalent heuristic would bond.
/// let mol = Molecule {
///     atoms: vec![Atom::new("Na", Vec3::ZERO), Atom::new("Na", 3.5 * Vec3::X)],
///     bonds: vec![],
/// };
/// assert_eq!(perceive_bonds_with(&mol, BondRule::CovalentHeuristic).len(), 1);
/// assert_eq!(perceive_bonds_with(&mol, BondRule::UnlikeOnly).len(), 0);
/// ```
pub fn perceive_bonds_with(mol: &Molecule, rule: BondRule) -> Vec<Bond> {
    if rule == BondRule::Explicit {
        return Vec::new();
    }
    let radii: Vec<f32> = mol
        .atoms
        .iter()
        .map(|a| element_info(&a.element).1)
        .collect();
    let mut bonds = Vec::new();
    for i in 0..mol.atoms.len() {
        for j in (i + 1)..mol.atoms.len() {
            let d = (mol.atoms[i].pos - mol.atoms[j].pos).length();
            if !(d > 0.4 && d < 1.3 * (radii[i] + radii[j])) {
                continue;
            }
            if rule == BondRule::UnlikeOnly && !is_unlike_ion_pair(&mol.atoms[i], &mol.atoms[j]) {
                continue;
            }
            bonds.push(Bond::new(i, j, 1));
        }
    }
    bonds
}

/// Whether `a` and `b` are different elements polarized enough to count as an
/// ionic contact (see [`IONIC_ELECTRONEGATIVITY_THRESHOLD`]).
///
/// An element with no tabulated electronegativity can never qualify, so an
/// exotic pair is left unbonded rather than bonded on a guess.
fn is_unlike_ion_pair(a: &Atom, b: &Atom) -> bool {
    if a.element.eq_ignore_ascii_case(&b.element) {
        return false;
    }
    let (Some(ea), Some(eb)) = (
        crate::element::electronegativity(&a.element),
        crate::element::electronegativity(&b.element),
    ) else {
        return false;
    };
    (ea - eb).abs() > IONIC_ELECTRONEGATIVITY_THRESHOLD
}

/// Returns `mol` with its `bonds` filled by [`perceive_bonds`] when it has none;
/// molecules that already carry bonds are returned unchanged.
///
/// ```
/// use manim_chem::molecule::{Atom, Molecule};
/// use manim_chem::render::with_perceived_bonds;
/// use glam::Vec3;
///
/// let bare = Molecule {
///     atoms: vec![Atom::new("O", Vec3::ZERO), Atom::new("H", Vec3::new(0.96, 0.0, 0.0))],
///     bonds: vec![],
/// };
/// assert_eq!(with_perceived_bonds(&bare).bonds.len(), 1);
/// ```
pub fn with_perceived_bonds(mol: &Molecule) -> Molecule {
    with_perceived_bonds_using(mol, BondRule::CovalentHeuristic)
}

/// [`with_perceived_bonds`] under an explicit [`BondRule`]: fills empty `bonds`
/// with [`perceive_bonds_with`], leaving already-bonded molecules alone.
///
/// ```
/// use manim_chem::lattice::nacl;
/// use manim_chem::render::{with_perceived_bonds_using, BondRule};
///
/// let crystal = nacl().replicate(1, 1, 1);
/// let bonded = with_perceived_bonds_using(&crystal, BondRule::UnlikeOnly);
/// // Only unlike Na–Cl contacts survive.
/// for b in &bonded.bonds {
///     assert_ne!(bonded.atoms[b.a].element, bonded.atoms[b.b].element);
/// }
/// ```
pub fn with_perceived_bonds_using(mol: &Molecule, rule: BondRule) -> Molecule {
    let mut out = mol.clone();
    if out.bonds.is_empty() {
        out.bonds = perceive_bonds_with(mol, rule);
    }
    out
}

/// Builds a signed molecular-orbital isosurface pair from `cube`: one surface at
/// `+level` (blue, the positive lobe) and one at `−level` (red, the negative
/// lobe), grouped in a [`VGroup`].
///
/// The field is sampled by marching cubes over a fixed symmetric box at a fixed
/// resolution (internal constants). `level` should be a positive iso-value; its
/// sign is applied for you (the ± lobes are colored separately).
pub fn molecular_orbital_isosurface(
    scene: &mut SceneState,
    cube: &crate::cube::CubeData,
    level: f64,
) -> MobjectId<VGroup> {
    let level = level.abs();
    let field = cube.to_scalar_field();
    let min = [-ORBITAL_HALF_EXTENT; 3];
    let max = [ORBITAL_HALF_EXTENT; 3];

    let lobe = |scene: &mut SceneState, signed_level: f64, color: Color| {
        let mut surface = Isosurface::new(field.clone(), signed_level)
            .region(min, max)
            .resolution(ORBITAL_RESOLUTION);
        // The +lobe is the ψ > +level region, which marching cubes classifies as
        // "outside", leaving its +∇ψ normals pointing inward — unflipped it is
        // lit from behind and renders flat. The −lobe is genuinely the
        // below-level region and is already oriented outward.
        if signed_level > 0.0 {
            surface = surface.flip_normals();
        }
        let mesh = surface.mesh();
        scene.add(Mesh::new(mesh).with_material(MeshMaterial::new(color).with_opacity(0.6)))
    };

    let positive = lobe(scene, level, BLUE);
    let negative = lobe(scene, -level, RED);
    VGroup::of(scene, [positive.erase(), negative.erase()])
}

/// Interpolates between two conformers/geometries along a reaction coordinate
/// `t ∈ [0, 1]`, atom-for-atom by index: at `t = 0` the result equals `mol_a`,
/// at `t = 1` it equals `mol_b`, and in between each atom sits at the linear
/// blend of its two positions.
///
/// # Why position-lerp and not [`MorphMesh`](manim_core::mesh::MorphMesh)
///
/// `MorphMesh` tweens **one** [`Mesh`]'s vertex buffer between two meshes of the
/// *same topology*. A ball-and-stick model is an [`InstancedMesh`] whose atoms
/// are independent instances, not one welded mesh, so `MorphMesh` does not
/// apply. Interpolating the atom *positions* and re-running
/// [`ball_and_stick`] instead moves each atom rigidly and rebuilds the bonds —
/// the chemically meaningful motion. Element identity and bonds are taken from
/// `mol_a` (conformers of one molecule share them).
///
/// Mismatched atom counts are handled gracefully: the shared leading atoms are
/// blended and any surplus atoms of the longer molecule are appended unchanged.
///
/// ```
/// use manim_chem::molecule::{Atom, Molecule};
/// use manim_chem::render::reaction_coordinate;
/// use glam::Vec3;
///
/// let a = Molecule { atoms: vec![Atom::new("H", Vec3::ZERO)], bonds: vec![] };
/// let b = Molecule { atoms: vec![Atom::new("H", 2.0 * Vec3::X)], bonds: vec![] };
/// assert_eq!(reaction_coordinate(&a, &b, 0.0), a);
/// assert_eq!(reaction_coordinate(&a, &b, 1.0), b);
/// assert_eq!(reaction_coordinate(&a, &b, 0.5).atoms[0].pos, Vec3::X);
/// ```
pub fn reaction_coordinate(mol_a: &Molecule, mol_b: &Molecule, t: f64) -> Molecule {
    let t = t.clamp(0.0, 1.0) as f32;
    let shared = mol_a.atoms.len().min(mol_b.atoms.len());

    let mut atoms: Vec<Atom> = (0..shared)
        .map(|i| {
            Atom::new(
                mol_a.atoms[i].element.clone(),
                mol_a.atoms[i].pos.lerp(mol_b.atoms[i].pos, t),
            )
        })
        .collect();

    // Append any surplus atoms of the longer molecule, untouched.
    let longer = if mol_a.atoms.len() >= mol_b.atoms.len() {
        mol_a
    } else {
        mol_b
    };
    atoms.extend(longer.atoms[shared..].iter().cloned());

    Molecule {
        atoms,
        bonds: mol_a.bonds.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// Water at its real bent geometry (O–H ≈ 0.96 Å, angle ≈ 104.5°).
    fn water() -> Molecule {
        Molecule {
            atoms: vec![
                Atom::new("O", Vec3::ZERO),
                Atom::new("H", Vec3::new(0.758, 0.587, 0.0)),
                Atom::new("H", Vec3::new(-0.758, 0.587, 0.0)),
            ],
            bonds: vec![],
        }
    }

    /// Benzene: a planar C6 ring at 1.39 Å with radial C–H at 1.09 Å.
    fn benzene() -> Molecule {
        let mut atoms = Vec::new();
        // Ring circumradius for a hexagon of side 1.39 Å.
        let ring_r = 1.39 / (2.0 * (std::f32::consts::PI / 6.0).sin());
        let ch_r = ring_r + 1.09;
        for i in 0..6 {
            let a = i as f32 / 6.0 * std::f32::consts::TAU;
            atoms.push(Atom::new(
                "C",
                Vec3::new(ring_r * a.cos(), ring_r * a.sin(), 0.0),
            ));
        }
        for i in 0..6 {
            let a = i as f32 / 6.0 * std::f32::consts::TAU;
            atoms.push(Atom::new(
                "H",
                Vec3::new(ch_r * a.cos(), ch_r * a.sin(), 0.0),
            ));
        }
        Molecule {
            atoms,
            bonds: vec![],
        }
    }

    #[test]
    fn ball_and_stick_groups_atom_and_bond_meshes() {
        let mol = Molecule {
            atoms: vec![Atom::new("C", Vec3::ZERO), Atom::new("O", 1.2 * Vec3::X)],
            bonds: vec![Bond::new(0, 1, 1)],
        };
        let mut scene = SceneState::new();
        let group = ball_and_stick(&mut scene, &mol);
        // group + atom mesh + bond mesh => at least two mesh children.
        assert!(scene.family(group).len() >= 3);
    }

    #[test]
    fn perceive_bonds_water_finds_two() {
        assert_eq!(perceive_bonds(&water()).len(), 2);
    }

    #[test]
    fn perceive_bonds_benzene_finds_twelve() {
        // 6 ring C–C + 6 radial C–H.
        assert_eq!(perceive_bonds(&benzene()).len(), 12);
    }

    /// The FE-142a headline: rock salt must draw Cl⁻ bigger than Na⁺.
    #[test]
    fn ionic_sizing_makes_chloride_outsize_sodium() {
        let crystal = crate::lattice::nacl().replicate(1, 1, 1);
        let na = crystal.atoms.iter().find(|a| a.element == "Na").unwrap();
        let cl = crystal.atoms.iter().find(|a| a.element == "Cl").unwrap();

        assert!(RadiusSource::Ionic.radius_for(cl) > RadiusSource::Ionic.radius_for(na));
        // …and the covalent default is what got it backwards.
        assert!(RadiusSource::Covalent.radius_for(na) > RadiusSource::Covalent.radius_for(cl));
    }

    /// `replicate` must carry basis charges into every tiled cell, or ionic
    /// sizing silently degrades to the common-charge default.
    #[test]
    fn replicate_preserves_formal_charges() {
        let crystal = crate::lattice::nacl().replicate(2, 2, 2);
        assert_eq!(crystal.atoms.len(), 64);
        for atom in &crystal.atoms {
            let want = if atom.element == "Na" { 1 } else { -1 };
            assert_eq!(atom.charge, Some(want), "{}", atom.element);
        }
    }

    #[test]
    fn unlike_only_drops_like_contacts_in_rock_salt() {
        let crystal = crate::lattice::nacl().replicate(2, 2, 2);
        let covalent = perceive_bonds_with(&crystal, BondRule::CovalentHeuristic);
        let ionic = perceive_bonds_with(&crystal, BondRule::UnlikeOnly);

        assert!(
            ionic.len() < covalent.len(),
            "unlike-only {} should be below covalent {}",
            ionic.len(),
            covalent.len()
        );
        // Every surviving bond joins unlike elements.
        for b in &ionic {
            assert_ne!(crystal.atoms[b.a].element, crystal.atoms[b.b].element);
        }
        // The covalent heuristic really does produce like-element bonds here —
        // that is the hairball this rule exists to cut.
        assert!(covalent
            .iter()
            .any(|b| crystal.atoms[b.a].element == crystal.atoms[b.b].element));
    }

    /// The ionic rule must not disturb ordinary molecules' perception… and it
    /// must be honest that it *does* suppress genuine covalent bonds.
    #[test]
    fn unlike_only_suppresses_covalent_molecules() {
        // Water's O–H difference is 1.24 — below the 1.7 ionic cutoff.
        assert_eq!(perceive_bonds_with(&water(), BondRule::UnlikeOnly).len(), 0);
        // Benzene's C–C and C–H likewise vanish; documented as a known limit.
        assert_eq!(
            perceive_bonds_with(&benzene(), BondRule::UnlikeOnly).len(),
            0
        );
        // The default rule is untouched.
        assert_eq!(perceive_bonds(&water()).len(), 2);
    }

    #[test]
    fn explicit_rule_perceives_nothing() {
        assert!(perceive_bonds_with(&water(), BondRule::Explicit).is_empty());
        // …and leaves an already-bonded molecule alone.
        let bonded = with_perceived_bonds_using(&water(), BondRule::Explicit);
        assert!(bonded.bonds.is_empty());
    }

    #[test]
    fn explicit_atom_charge_overrides_the_common_state() {
        use crate::molecule::Atom;
        let default_fe = Atom::new("Fe", Vec3::ZERO); // Fe(III) by default
        let fe_two = Atom::new("Fe", Vec3::ZERO).with_charge(2);
        assert!(
            RadiusSource::Ionic.radius_for(&fe_two) > RadiusSource::Ionic.radius_for(&default_fe)
        );
    }

    #[test]
    fn reaction_coordinate_endpoints_and_midpoint() {
        let a = Molecule {
            atoms: vec![
                Atom::new("H", Vec3::ZERO),
                Atom::new("O", Vec3::new(0.0, 1.0, 0.0)),
            ],
            bonds: vec![Bond::new(0, 1, 1)],
        };
        let b = Molecule {
            atoms: vec![
                Atom::new("H", 2.0 * Vec3::X),
                Atom::new("O", Vec3::new(0.0, 3.0, 0.0)),
            ],
            bonds: vec![Bond::new(0, 1, 1)],
        };
        assert_eq!(reaction_coordinate(&a, &b, 0.0), a);
        assert_eq!(reaction_coordinate(&a, &b, 1.0), b);
        let mid = reaction_coordinate(&a, &b, 0.5);
        assert_eq!(mid.atoms[0].pos, Vec3::X);
        assert_eq!(mid.atoms[1].pos, Vec3::new(0.0, 2.0, 0.0));
    }
}
