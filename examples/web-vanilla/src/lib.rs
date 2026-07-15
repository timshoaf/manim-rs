//! Vanilla-wasm manim demo: `SquareToCircle` rendered onto an HTML `<canvas>`.
//!
//! Builds the scene, precomputes its frames' display lists, creates a
//! [`CanvasSurface`] from the page's `#manim-canvas` element, and steps through
//! the frames in a `requestAnimationFrame` loop. No framework — just
//! `wasm-bindgen` + `web-sys`, mirroring what `manim-dioxus` will do inside a
//! component.

use std::cell::RefCell;
use std::rc::Rc;

use manim_core::animations::{Create, FadeOut, TransformInto};
use manim_core::prelude::*;
use manim_core::scene::Frame;
use manim_render::CanvasSurface;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// The canonical square→circle scene.
struct SquareToCircle;

impl SceneBuilder for SquareToCircle {
    fn construct(&self, scene: &mut Scene) -> manim_core::error::Result<()> {
        let square = scene.add(
            Square::new()
                .with_fill(BLUE, 0.7)
                .with_stroke(WHITE, 4.0, 1.0),
        );
        scene.play(Create::new(square))?;
        scene.play(square.animate().rotate(PI / 4.0))?;
        scene.play(TransformInto::new(
            square,
            Circle::new()
                .with_fill(RED, 0.7)
                .with_stroke(WHITE, 4.0, 1.0),
        ))?;
        scene.wait(0.5);
        scene.play(FadeOut::new(square).shift(DOWN))?;
        Ok(())
    }
}

/// wasm entry point (invoked automatically on module load).
#[wasm_bindgen(start)]
pub fn start() {
    manim_render::canvas::set_panic_hook();
    wasm_bindgen_futures::spawn_local(run());
}

/// Sets up the canvas surface and drives the animation loop.
async fn run() {
    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");
    let canvas = document
        .get_element_by_id("manim-canvas")
        .expect("missing #manim-canvas element")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("#manim-canvas is not a <canvas>");

    let config = Config::low();
    let mut scene = Scene::build(&SquareToCircle, config.clone()).expect("build scene");
    // frames_with_camera so the loop follows any camera motion in the scene.
    let frames: Vec<Frame> = scene.frames_with_camera().collect();
    let mut surface = CanvasSurface::new(canvas, &config)
        .await
        .expect("create canvas surface");

    // Classic wasm rAF loop: a self-referential Closure kept alive via Rc.
    let callback = Rc::new(RefCell::new(None));
    let handle = callback.clone();
    let mut index = 0usize;
    *handle.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let frame = &frames[index % frames.len()];
        if let Err(e) = surface.render_frame(frame) {
            web_sys::console::error_1(&format!("render error: {e}").into());
            return;
        }
        index += 1;
        request_animation_frame(callback.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));
    request_animation_frame(handle.borrow().as_ref().unwrap());
}

/// Schedules `f` on the next animation frame.
fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .expect("no window")
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("requestAnimationFrame failed");
}
