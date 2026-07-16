//! Golden-image tests for the depth-tested mesh pass (FE-125…FE-128).
//!
//! These are the acceptance scenes of `docs/design/12-mesh-pipeline.md` §9: a
//! self-occluding shaded saddle (M1), a deterministic instanced grid (M2),
//! translucent-over-opaque geometry (M3), and a mid-morph frame plus a
//! heightfield wave (M4). They follow the same idiom as `golden.rs` — render at
//! 427×240, compare to a checked-in PNG with a per-channel + fraction-of-pixels
//! tolerance, `BLESS=1` to reseed — and skip cleanly with no GPU adapter unless
//! `REQUIRE_GPU=1` is set.
//!
//! The counterpart guarantee lives in `golden.rs`: every scene there has an
//! empty mesh channel, never runs this pass, and must stay byte-identical.

#![cfg(not(target_arch = "wasm32"))]

use glam::{Mat4, Vec3};
use manim_color::{Color, BLUE, GREEN, ORANGE, RED, TEAL, WHITE, YELLOW};
use manim_core::camera::ThreeDParams;
use manim_core::config::Config;
use manim_core::mesh::{
    HeightField, Instance, InstancedMesh, Mesh, MeshMaterial, MorphSurface, Shading, Surface3D,
    TriMesh,
};
use manim_core::mobject::Buildable;
use manim_core::scene::Scene;
use manim_core::scene_state::SceneState;
use manim_render::golden::assert_golden;
use manim_render::renderer::OffscreenRenderer;

/// The test render size: half of `-ql`, same frame dimensions — matching
/// `golden.rs`.
fn test_config() -> Config {
    Config {
        pixel_width: 427,
        pixel_height: 240,
        ..Config::default()
    }
}

/// Builds a renderer, or returns `None` (with a warning) if no GPU is
/// available. `REQUIRE_GPU=1` turns a missing adapter into a hard failure, so
/// the golden job cannot pass by not running.
fn try_renderer() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new(&test_config()) {
        Ok(r) => {
            let info = r.context().adapter_info();
            eprintln!(
                "manim-render mesh golden tests: {:?} backend, adapter {:?}",
                info.backend, info.name
            );
            Some(r)
        }
        Err(e) => {
            if std::env::var("REQUIRE_GPU").is_ok_and(|v| v != "0" && !v.is_empty()) {
                panic!(
                    "REQUIRE_GPU is set but no GPU adapter is available ({e}); \
                     install a software rasterizer (e.g. mesa lavapipe) or unset REQUIRE_GPU"
                );
            }
            eprintln!("SKIP mesh golden tests: no GPU adapter available ({e})");
            None
        }
    }
}

/// The standard three-quarter orbit these scenes are composed for — the same
/// angles `golden.rs` uses for its 3-D scenes.
fn orbit_camera(renderer: &mut OffscreenRenderer) {
    let deg = std::f32::consts::PI / 180.0;
    renderer.camera_mut().three_d = Some(ThreeDParams {
        phi: 75.0 * deg,
        theta: 30.0 * deg,
        ..ThreeDParams::default()
    });
}

/// The fraction of pixels that are not the pure black background — a cheap
/// "something rendered, and it isn't the whole frame" check.
fn covered_fraction(img: &image::RgbaImage) -> f64 {
    let lit = img.pixels().filter(|p| p.0[0..3] != [0, 0, 0]).count();
    lit as f64 / (img.width() as f64 * img.height() as f64)
}

/// M1: a saddle `z = (x² - y²)/2` shaded per pixel and occluding itself.
///
/// This is the scene the old project-and-sort path cannot draw: the near lobes
/// rise in front of the far ones, and only a depth buffer resolves which wins
/// per pixel. The checkerboard makes the surface's own folds legible.
#[test]
fn saddle_surface_self_occlusion() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    scene.add(
        Surface3D::new(
            |u, v| Vec3::new(u as f32, v as f32, ((u * u - v * v) / 2.0) as f32),
            (-2.0, 2.0),
            (-2.0, 2.0),
        )
        // 20 cells is smooth enough to read as a saddle while keeping the
        // checkerboard squares several pixels wide at 427×240 — at 48 they are
        // ~3 px and MSAA averages them into flat color.
        .with_resolution(20, 20)
        .with_checkerboard(Some([BLUE, TEAL]))
        .with_material(MeshMaterial::new(WHITE).with_lighting(0.35, 0.75, 0.35)),
    );
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();

    // The saddle must fill a real part of the frame without covering it.
    let covered = covered_fraction(&img);
    assert!(
        (0.08..0.85).contains(&covered),
        "saddle covers {covered:.3} of the frame"
    );
    assert_golden("mesh_saddle_self_occlusion", &img);
}

/// M1: the same saddle with `Shading::Flat`, which the fragment shader derives
/// from screen-space derivatives rather than a separate faceted mesh.
#[test]
fn saddle_surface_flat_shaded() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    scene.add(
        Surface3D::new(
            |u, v| Vec3::new(u as f32, v as f32, ((u * u - v * v) / 2.0) as f32),
            (-2.0, 2.0),
            (-2.0, 2.0),
        )
        // A coarse grid so the faceting is unmistakable.
        .with_resolution(10, 10)
        .with_checkerboard(None)
        .with_material(
            MeshMaterial::new(ORANGE)
                .with_shading(Shading::Flat)
                .with_lighting(0.2, 0.85, 0.25),
        ),
    );
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("mesh_saddle_flat_shaded", &img);
}

/// M2: a deterministic instanced scene — a 4×4×2 lattice of spheres wired by
/// cylinder bonds along each row. Two `InstancedMesh`es, so two draw calls.
#[test]
fn instanced_spheres_and_cylinders() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();

    // Atom centers on a fixed lattice — no randomness, so the golden is stable.
    let mut centers = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..2 {
                centers.push(Vec3::new(
                    i as f32 - 1.5,
                    j as f32 - 1.5,
                    k as f32 * 1.2 - 0.6,
                ));
            }
        }
    }
    // Bonds along each row of the lattice.
    let mut bonds = Vec::new();
    for w in centers.windows(2) {
        if (w[0] - w[1]).length() < 1.3 {
            bonds.push((w[0], w[1]));
        }
    }
    assert!(!bonds.is_empty(), "the lattice should produce bonds");

    scene.add(InstancedMesh::spheres(&centers, 0.28).with_material(MeshMaterial::new(RED)));
    scene.add(InstancedMesh::cylinders(&bonds, 0.07).with_material(MeshMaterial::new(WHITE)));
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();

    let covered = covered_fraction(&img);
    assert!(
        (0.05..0.7).contains(&covered),
        "lattice covers {covered:.3} of the frame"
    );
    assert_golden("mesh_instanced_lattice", &img);
}

/// M2: per-instance tints reach the shader — the same base mesh drawn in three
/// colors from one instance buffer.
#[test]
fn instanced_per_instance_tint() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let tints: [Color; 3] = [RED, GREEN, BLUE];
    let instances = tints
        .iter()
        .enumerate()
        .map(|(i, c)| {
            Instance::new(
                Mat4::from_translation(Vec3::new(i as f32 * 1.8 - 1.8, 0.0, 0.0)),
                *c,
            )
        })
        .collect();
    scene.add(
        InstancedMesh::new(TriMesh::uv_sphere(24, 48), instances)
            // A white base color, so all the color on screen is per-instance.
            .with_material(MeshMaterial::new(WHITE)),
    );
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("mesh_instanced_tints", &img);
}

/// M3: a translucent surface cutting through an opaque sphere.
///
/// The sphere is opaque and depth-writes; the plane is translucent, draws after
/// it with a read-only depth test, and so is occluded by the sphere's near half
/// while still showing the sphere through itself. That combination is what the
/// two-queue split buys.
#[test]
fn translucent_plane_through_opaque_sphere() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    scene.add(
        Mesh::new(TriMesh::uv_sphere(32, 64))
            .with_transform(Mat4::from_scale(Vec3::splat(1.2)))
            .with_material(MeshMaterial::new(RED).with_lighting(0.3, 0.75, 0.4)),
    );
    // A large plane through the sphere's equator, tilted so it reads in 3-D.
    scene.add(
        Mesh::new(TriMesh::grid(16, 16))
            .with_transform(
                Mat4::from_translation(Vec3::ZERO)
                    * Mat4::from_rotation_x(0.35)
                    * Mat4::from_scale(Vec3::splat(4.5)),
            )
            .with_material(
                MeshMaterial::new(YELLOW)
                    .with_opacity(0.45)
                    .with_lighting(0.5, 0.5, 0.1),
            ),
    );
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert_golden("mesh_translucent_over_opaque", &img);
}

/// The regression this whole design turns on: 2-D vector content composites
/// *over* the mesh pass, and a mesh scene still clears to the background.
#[test]
fn vector_content_draws_over_meshes() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    scene.add(Mesh::sphere().with_material(MeshMaterial::new(BLUE)));
    // A HUD label-ish square, fixed in frame: it must not be depth-tested away.
    scene.add(
        manim_core::geometry::Square::new()
            .with_fill(WHITE, 1.0)
            .with_scale(0.5),
    );
    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();

    // The frame corner is still pure background: the mesh pass cleared it.
    assert_eq!(img.get_pixel(0, 0).0, [0, 0, 0, 255]);
    // The square's center is white — it painted over the sphere, not under it.
    let center = img.get_pixel(img.width() / 2, img.height() / 2).0;
    assert!(
        center[0] > 200 && center[1] > 200 && center[2] > 200,
        "center pixel {center:?} should be the white square drawn over the mesh"
    );
    assert_golden("mesh_vector_over_mesh", &img);
}

/// A mesh renders under the plain 2-D (orthographic) camera too — the mesh pass
/// is not gated on `Camera2D::is_3d`.
#[test]
fn mesh_under_2d_camera() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    scene.add(
        Mesh::new(TriMesh::uv_sphere(32, 64))
            .with_transform(Mat4::from_scale(Vec3::splat(2.0)))
            .with_material(MeshMaterial::new(GREEN)),
    );
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert!(
        covered_fraction(&img) > 0.05,
        "the sphere should be visible"
    );
    assert_golden("mesh_ortho_camera", &img);
}

/// M4: a static height field — a radial standing wave `sin(5r)/(1+2r)` — shaded
/// from normals the vertex shader derives from the height texture itself.
///
/// The mesh uploaded here is *flat*; every bump on screen is GPU displacement,
/// and the shading proves the finite-difference normals are right (a wrong
/// normal reads as a flat-lit sheet, not a lit wave).
#[test]
fn heightfield_standing_wave() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let field = HeightField::from_fn(64, 64, (5.0, 5.0), |x, y| {
        let r = (x * x + y * y).sqrt();
        (r * 5.0).sin() / (1.0 + 2.0 * r)
    })
    .with_material(MeshMaterial::new(TEAL).with_lighting(0.25, 0.8, 0.45));
    // The grid the renderer receives really is flat.
    assert!(field.mesh().positions.iter().all(|p| p.z == 0.0));
    scene.add(field);

    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();
    assert!(covered_fraction(&img) > 0.03, "the wave should be visible");
    assert_golden("mesh_heightfield_wave", &img);
}

/// Two independently-built scenes, rendered through one renderer, must not share
/// GPU buffers — even though their first mobjects have identical
/// `(source, generation)`. The [`DisplayList::arena`] stamp is what separates
/// them.
///
/// This is a regression test for a real bug: before the stamp existed, this
/// exact shape produced two byte-identical frames, because the second scene was
/// silently served the first scene's uploaded geometry.
#[test]
fn independent_scenes_do_not_share_gpu_buffers() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    orbit_camera(&mut renderer);

    // Two separate scenes whose height fields differ only in their heights —
    // no builder call touches either one, so their ids and generations collide.
    let mut flat_scene = SceneState::new();
    flat_scene.add(HeightField::from_fn(32, 32, (4.0, 4.0), |_, _| 0.0));
    let mut wavy_scene = SceneState::new();
    wavy_scene.add(HeightField::from_fn(32, 32, (4.0, 4.0), |x, y| {
        (x + y).sin()
    }));

    let (flat_dl, wavy_dl) = (flat_scene.display_list(), wavy_scene.display_list());
    // Precondition: the identities really do collide.
    assert_eq!(flat_dl.meshes()[0].source, wavy_dl.meshes()[0].source);
    assert_eq!(
        flat_dl.meshes()[0].generation,
        wavy_dl.meshes()[0].generation
    );
    assert_ne!(flat_dl.arena(), wavy_dl.arena());

    let flat = renderer.render_display_list(&flat_dl).unwrap();
    let wavy = renderer.render_display_list(&wavy_dl).unwrap();
    let differing = manim_render::golden::pixel_diff_fraction(&flat, &wavy, 3);
    assert!(
        differing > 0.02,
        "the two scenes rendered {:.3}% different — the second was served the \
         first scene's cached buffers",
        differing * 100.0
    );
}

/// A snapshot replay must keep hitting the mesh cache: seeking clones the
/// snapshot, and a clone keeps its arena stamp, so unchanged geometry must not
/// re-upload.
#[test]
fn snapshot_replays_do_not_reupload() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    orbit_camera(&mut renderer);
    let mut scene = SceneState::new();
    scene.add(Mesh::sphere().with_material(MeshMaterial::new(TEAL)));
    renderer.render_display_list(&scene.display_list()).unwrap();
    let after_first = renderer.mesh_cache().geometry_uploads();

    // Replay the same state through clones, as `Timeline::state_at` does.
    for _ in 0..6 {
        renderer
            .render_display_list(&scene.clone().display_list())
            .unwrap();
    }
    assert_eq!(
        renderer.mesh_cache().geometry_uploads(),
        after_first,
        "replaying a snapshot re-uploaded geometry"
    );
    assert_eq!(
        renderer.mesh_cache().len(),
        1,
        "and must not grow the cache"
    );
}

/// M4: displacement is real geometry, not a shading trick — a displaced field
/// must not look like the flat grid it was uploaded as.
#[test]
fn heightfield_displacement_changes_the_image() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    orbit_camera(&mut renderer);
    let mut scene = SceneState::new();
    let f = scene.add(
        HeightField::from_fn(32, 32, (4.0, 4.0), |_, _| 0.0).with_material(MeshMaterial::new(TEAL)),
    );
    let flat = renderer.render_display_list(&scene.display_list()).unwrap();

    scene.get_mut(f).update_heights(|x, y| (x + y).sin());
    let ridged = renderer.render_display_list(&scene.display_list()).unwrap();

    let differing = manim_render::golden::pixel_diff_fraction(&flat, &ridged, 3);
    assert!(
        differing > 0.05,
        "displacement changed only {:.3}% of pixels — the height texture is not \
         reaching the vertex shader",
        differing * 100.0
    );
}

/// M4: a `MorphSurface` caught mid-tween — a sheet halfway into a saddle.
///
/// Rendered through the ordinary frame path (`frames_with_camera`), so this also
/// covers the mesh channel surviving the timeline: each frame's display list
/// carries a freshly-lerped `TriMesh` with a bumped generation.
#[test]
fn morph_surface_midframe() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = Scene::new(test_config());
    // The orbit must live on the *scene* camera: `render_frame` follows the
    // frame's camera, so anything set on the renderer would be overwritten.
    let deg = std::f32::consts::PI / 180.0;
    scene.set_camera_orientation(75.0 * deg, 30.0 * deg);
    let sheet = scene.add(
        Surface3D::new(
            |u, v| Vec3::new(u as f32, v as f32, 0.0),
            (-1.5, 1.5),
            (-1.5, 1.5),
        )
        .with_resolution(20, 20)
        .with_checkerboard(Some([BLUE, TEAL]))
        .with_material(MeshMaterial::new(WHITE).with_lighting(0.35, 0.75, 0.35)),
    );
    // Bend it into a saddle — the homeomorphism animation of design doc §3.
    scene
        .play(MorphSurface::new(
            sheet,
            |u, v| Vec3::new(u as f32, v as f32, ((u * u - v * v) / 1.5) as f32),
            (-1.5, 1.5),
            (-1.5, 1.5),
        ))
        .unwrap();

    let frames: Vec<_> = scene.frames_with_camera().collect();
    let mid = &frames[frames.len() / 2];
    assert_eq!(mid.display_list.meshes().len(), 1);
    // Mid-morph: bent, but not yet the full saddle.
    let zs: Vec<f32> = mid.display_list.meshes()[0]
        .mesh
        .positions
        .iter()
        .map(|p| p.z.abs())
        .collect();
    let peak = zs.iter().cloned().fold(0.0_f32, f32::max);
    assert!(
        (0.05..1.4).contains(&peak),
        "mid-morph peak height {peak} should be partway to the saddle"
    );

    assert!(
        mid.camera.three_d.is_some(),
        "the frame must carry the 3-D orbit for render_frame to follow"
    );
    let img = renderer.render_frame(mid).unwrap();
    assert!(
        covered_fraction(&img) > 0.02,
        "the surface should be visible"
    );
    assert_golden("mesh_morph_surface_mid", &img);
}

/// The FE-128 caching contract, end to end on the GPU: N height updates re-upload
/// N textures and exactly *one* grid, and the cache never accumulates entries.
///
/// This is the memory-growth smoke test — an unbounded cache or a per-frame grid
/// re-upload both show up here as a counter that tracks N.
#[test]
fn animating_heights_reuploads_only_the_texture() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    const FRAMES: u64 = 12;
    let mut scene = SceneState::new();
    let f = scene.add(
        HeightField::from_fn(32, 32, (4.0, 4.0), |_, _| 0.0).with_material(MeshMaterial::new(TEAL)),
    );
    orbit_camera(&mut renderer);

    for n in 0..FRAMES {
        let t = n as f32 * 0.3;
        scene
            .get_mut(f)
            .update_heights(|x, y| ((x * x + y * y).sqrt() * 3.0 - t).sin() * 0.4);
        renderer.render_display_list(&scene.display_list()).unwrap();
    }

    let cache = renderer.mesh_cache();
    // The grid uploaded once and never again …
    assert_eq!(
        cache.geometry_uploads(),
        1,
        "the flat grid must upload once, not once per frame"
    );
    // … while each frame's heights did reach the GPU.
    assert_eq!(cache.height_uploads(), FRAMES);
    // And one mobject means one cache entry, forever.
    assert_eq!(cache.len(), 1);
}

/// Cache eviction: a mobject that leaves the display list drops its GPU
/// resources rather than lingering.
#[test]
fn a_vanished_height_field_is_evicted() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    let f = scene.add(HeightField::from_fn(16, 16, (2.0, 2.0), |x, _| x));
    orbit_camera(&mut renderer);
    renderer.render_display_list(&scene.display_list()).unwrap();
    assert_eq!(renderer.mesh_cache().len(), 1);

    scene.remove(f.erase());
    renderer.render_display_list(&scene.display_list()).unwrap();
    assert_eq!(
        renderer.mesh_cache().len(),
        0,
        "the entry should be evicted"
    );
}

/// M2 acceptance: 10k instanced spheres, rendered offscreen for a handful of
/// frames.
///
/// Ignored by default — it is a throughput probe, not a correctness check, and
/// on a software rasterizer (the CI adapter) it is far too slow to gate on. Run
/// it deliberately:
///
/// ```text
/// cargo test -p manim-render --release --test mesh_golden -- --ignored --nocapture
/// ```
#[test]
#[ignore = "perf smoke: slow on software adapters; run explicitly with --ignored"]
fn perf_smoke_10k_instances() {
    let Some(mut renderer) = try_renderer() else {
        return;
    };
    const N: usize = 10_000;
    const FRAMES: usize = 20;

    // A deterministic 3-D lattice of 10k atoms (a 22×22×21 block, trimmed).
    let side = 22;
    let centers: Vec<Vec3> = (0..N)
        .map(|i| {
            let x = (i % side) as f32;
            let y = ((i / side) % side) as f32;
            let z = (i / (side * side)) as f32;
            Vec3::new(x - side as f32 / 2.0, y - side as f32 / 2.0, z - 10.0) * 0.35
        })
        .collect();
    assert_eq!(centers.len(), N);

    let mut scene = SceneState::new();
    scene.add(InstancedMesh::spheres(&centers, 0.12).with_material(MeshMaterial::new(TEAL)));
    orbit_camera(&mut renderer);
    let list = scene.display_list();
    assert_eq!(list.meshes()[0].instances.as_ref().unwrap().len(), N);

    // One warm-up frame uploads the buffers; the timed frames should hit the
    // cache and re-upload nothing.
    renderer.render_display_list(&list).unwrap();

    let start = std::time::Instant::now();
    for _ in 0..FRAMES {
        renderer.render_display_list(&list).unwrap();
    }
    let per_frame = start.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64;
    eprintln!(
        "perf: {N} instanced spheres ({} tris each), {FRAMES} frames at {:.1} ms/frame on {:?}",
        TriMesh::uv_sphere(
            manim_core::mesh::DEFAULT_ATOM_RINGS,
            manim_core::mesh::DEFAULT_ATOM_RINGS * 2
        )
        .n_triangles(),
        per_frame,
        renderer.context().adapter_info().name,
    );
}

/// GH #3 / FE-131: base-plane contour circles opting into `z_test` are
/// occluded by a surface floating between them and the camera, while an
/// ordinary (non-`z_test`) annotation still paints over everything.
#[test]
fn ztest_curves_hidden_under_floating_surface() {
    use manim_core::geometry::Circle;
    use manim_core::mobject::MobjectExt;

    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut scene = SceneState::new();
    // A flat opaque plate floating above the base plane.
    scene.add(
        Surface3D::new(
            |u, v| Vec3::new(u as f32, v as f32, 0.5),
            (-1.6, 1.6),
            (-1.6, 1.6),
        )
        .with_resolution(4, 4)
        .with_checkerboard(None)
        .with_material(MeshMaterial::new(ORANGE).with_lighting(0.35, 0.75, 0.2)),
    );
    // Base-plane "contour" circles, depth-tested: hidden where the plate sits
    // between them and the camera.
    for (r, color) in [(0.9, GREEN), (1.8, YELLOW), (2.7, BLUE)] {
        let mut contour = Circle::new().with_scale(r).with_stroke(color, 4.0, 1.0);
        contour.set_z_test(true);
        scene.add(contour);
    }
    // A plain annotation circle: draws over the plate like any 2-D content.
    scene.add(Circle::new().with_scale(0.4).with_stroke(RED, 4.0, 1.0));

    orbit_camera(&mut renderer);
    let img = renderer.render_display_list(&scene.display_list()).unwrap();

    // The same scene with the depth test off must differ — the contours would
    // paint over the plate.
    let mut plain = SceneState::new();
    plain.add(
        Surface3D::new(
            |u, v| Vec3::new(u as f32, v as f32, 0.5),
            (-1.6, 1.6),
            (-1.6, 1.6),
        )
        .with_resolution(4, 4)
        .with_checkerboard(None)
        .with_material(MeshMaterial::new(ORANGE).with_lighting(0.35, 0.75, 0.2)),
    );
    for (r, color) in [(0.9, GREEN), (1.8, YELLOW), (2.7, BLUE)] {
        plain.add(Circle::new().with_scale(r).with_stroke(color, 4.0, 1.0));
    }
    plain.add(Circle::new().with_scale(0.4).with_stroke(RED, 4.0, 1.0));
    let img_plain = renderer.render_display_list(&plain.display_list()).unwrap();
    assert_ne!(
        img.as_raw(),
        img_plain.as_raw(),
        "z_test had no effect: contours are not occluded by the plate"
    );

    assert_golden("mesh_ztest_contours_under_plate", &img);
}

/// Without any meshes in the scene, a `z_test` item renders exactly like a
/// plain one: the pass clears depth to 1.0, every fragment wins, and
/// `mesh_view_proj` keeps x/y bit-identical to the ortho camera.
#[test]
fn ztest_without_meshes_is_invisible() {
    use manim_core::geometry::Circle;
    use manim_core::mobject::MobjectExt;

    let Some(mut renderer) = try_renderer() else {
        return;
    };
    let mut with_flag = SceneState::new();
    let mut c = Circle::new().with_scale(1.5).with_stroke(WHITE, 4.0, 1.0);
    c.set_z_test(true);
    with_flag.add(c);
    let img_flag = renderer
        .render_display_list(&with_flag.display_list())
        .unwrap();

    let mut without = SceneState::new();
    without.add(Circle::new().with_scale(1.5).with_stroke(WHITE, 4.0, 1.0));
    let img_plain = renderer
        .render_display_list(&without.display_list())
        .unwrap();

    assert_eq!(
        img_flag.as_raw(),
        img_plain.as_raw(),
        "a z_test item in a mesh-less 2-D scene must render byte-identically"
    );
}
