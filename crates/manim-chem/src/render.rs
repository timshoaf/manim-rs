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
    let atom_instances: Vec<Instance> = mol
        .atoms
        .iter()
        .map(|a| {
            let (color, cov, _) = element_info(&a.element);
            sphere_instance(a.pos, BALL_AND_STICK_RADIUS_SCALE * cov, color)
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
            let (color, _, vdw) = element_info(&a.element);
            sphere_instance(a.pos, vdw, color)
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
    let radii: Vec<f32> = mol
        .atoms
        .iter()
        .map(|a| element_info(&a.element).1)
        .collect();
    let mut bonds = Vec::new();
    for i in 0..mol.atoms.len() {
        for j in (i + 1)..mol.atoms.len() {
            let d = (mol.atoms[i].pos - mol.atoms[j].pos).length();
            if d > 0.4 && d < 1.3 * (radii[i] + radii[j]) {
                bonds.push(Bond::new(i, j, 1));
            }
        }
    }
    bonds
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
    let mut out = mol.clone();
    if out.bonds.is_empty() {
        out.bonds = perceive_bonds(mol);
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
        let mesh = Isosurface::new(field.clone(), signed_level)
            .region(min, max)
            .resolution(ORBITAL_RESOLUTION)
            .mesh();
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
