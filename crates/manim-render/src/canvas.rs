//! Browser canvas rendering (`wasm32` + `web` feature).
//!
//! [`CanvasSurface`] wraps a wgpu surface created from an
//! [`HtmlCanvasElement`] and draws a
//! [`DisplayList`] into it, letterboxed to the scene's aspect. It is the wasm
//! analogue of the native offscreen/preview renderers: same tessellation and
//! [`Pipeline`], a surface target instead of a
//! texture or window.
//!
//! Construction is async ([`GpuContext`](crate::renderer::GpuContext) can't
//! block on the web); drive it from a `requestAnimationFrame` loop via
//! `wasm_bindgen_futures`.
//!
//! ```no_run
//! # #[cfg(all(target_arch = "wasm32", feature = "web"))]
//! # async fn go(canvas: web_sys::HtmlCanvasElement, list: &manim_core::display::DisplayList) {
//! use manim_core::config::Config;
//! use manim_render::canvas::CanvasSurface;
//!
//! let mut surface = CanvasSurface::new(canvas, &Config::low()).await.unwrap();
//! surface.render(list).unwrap();
//! # }
//! ```

use manim_color::Color;
use manim_core::config::Config;
use manim_core::display::DisplayList;
use manim_core::scene::Frame;
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

use crate::camera::Camera2D;
use crate::layout::letterbox;
use crate::mesh_pipeline::{
    create_depth_view, MeshBufferCache, MeshGlobals, MeshPipeline, SceneLight,
};
use crate::renderer::{Pipeline, RenderError, SAMPLE_COUNT};
use crate::tessellate::TessellationCache;

/// Installs [`console_error_panic_hook`] so Rust panics surface in the browser
/// console with a readable message and stack. Call once at startup.
///
/// ```no_run
/// manim_render::canvas::set_panic_hook();
/// ```
pub fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

/// A wgpu surface bound to an HTML `<canvas>`, rendering display lists in the
/// browser.
///
/// Holds its own device/queue/pipeline plus a [`TessellationCache`], so a
/// `requestAnimationFrame` loop can call [`render`](Self::render) each frame and
/// only re-tessellate changed mobjects.
pub struct CanvasSurface {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: Pipeline,
    /// The shared image + material `FrameOp` draw path — identical to what the
    /// offscreen `TextureTarget` runs, so browser output matches it.
    ops: crate::ops::OpsRenderer,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Magnifying-camera uniform + identity (border) uniform for a zoom window.
    zoom_uniform: wgpu::Buffer,
    zoom_bind_group: wgpu::BindGroup,
    border_uniform: wgpu::Buffer,
    border_bind_group: wgpu::BindGroup,
    msaa_view: wgpu::TextureView,
    /// The depth-tested vector pipeline for [`DrawItem::z_test`] items and its
    /// [`Camera2D::mesh_view_proj`] uniform. Idle until a display list carries
    /// z-tested items.
    ///
    /// [`DrawItem::z_test`]: manim_core::display::DrawItem::z_test
    ztest_pipeline: Pipeline,
    ztest_uniform: wgpu::Buffer,
    ztest_bind_group: wgpu::BindGroup,
    /// The depth-tested mesh pass. Everything here stays idle — the depth
    /// texture is allocated but never attached — until a display list actually
    /// carries meshes.
    mesh_pipeline: MeshPipeline,
    mesh_cache: MeshBufferCache,
    depth_view: wgpu::TextureView,
    mesh_globals: wgpu::Buffer,
    mesh_globals_bind_group: wgpu::BindGroup,
    light: SceneLight,
    cache: TessellationCache,
    camera: Camera2D,
    aspect: f32,
    background: Color,
    /// The active zoom window (from the last `render_frame`), or `None`.
    zoom_window: Option<manim_core::camera::ZoomWindow>,
}

/// A wgpu adapter + device + queue shared across many [`CanvasSurface`]s on one
/// page.
///
/// Requesting a device is the expensive part of surface creation and browsers
/// cap how many GPU devices exist at once — a textbook page with a dozen
/// [`Figure`](../../manim_dioxus/index.html)s must not create a dozen of them.
/// Build one `SharedGpu` (async, once), then create each surface synchronously
/// with [`CanvasSurface::with_shared`]. wgpu's `Device`/`Queue` are
/// reference-counted, so the clones each surface holds all point at the same GPU
/// device and submit to the same queue.
///
/// Clone is cheap (it clones the reference-counted handles).
#[derive(Clone)]
pub struct SharedGpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl SharedGpu {
    /// Requests an adapter and device not tied to any single canvas.
    ///
    /// Uses the same WebGL2-downlevel limits as [`CanvasSurface::new`], so the
    /// shared device runs on both the WebGPU and WebGL backends.
    ///
    /// # Errors
    ///
    /// [`RenderError::NoAdapter`] / [`RenderError::NoDevice`] if the browser
    /// cannot provide a GPU adapter or device.
    pub async fn new() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("manim-render shared device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::NoDevice(e.to_string()))?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// The shared device — e.g. to register a device-loss callback or poll it.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The shared queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

impl CanvasSurface {
    /// Creates a surface from `canvas`, sized and colored by `config`.
    ///
    /// Requests a browser adapter compatible with the canvas and a device with
    /// WebGL2-downlevel limits (so it runs on both the WebGPU and WebGL
    /// backends). Uses an sRGB surface format when available so the pipeline's
    /// linear colors encode correctly.
    ///
    /// # Errors
    ///
    /// [`RenderError::NoAdapter`] / [`RenderError::NoDevice`] if the browser
    /// cannot provide a GPU surface, adapter, or device.
    pub async fn new(canvas: HtmlCanvasElement, config: &Config) -> Result<Self, RenderError> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| RenderError::NoAdapter(format!("create canvas surface: {e}")))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("manim-render canvas device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::NoDevice(e.to_string()))?;

        Self::assemble(&adapter, device, queue, surface, width, height, config)
    }

    /// Builds a surface on an existing [`SharedGpu`], reusing its device/queue
    /// instead of requesting new ones.
    ///
    /// This is the [`Figure`](../../manim_dioxus/index.html) path: a page with
    /// many small canvases creates one [`SharedGpu`] and calls this per canvas,
    /// so all of them submit to a single device. Synchronous — no adapter/device
    /// request — so a figure can mount without awaiting.
    ///
    /// # Errors
    ///
    /// [`RenderError::NoAdapter`] if a surface cannot be created for `canvas`.
    pub fn with_shared(
        gpu: &SharedGpu,
        canvas: HtmlCanvasElement,
        config: &Config,
    ) -> Result<Self, RenderError> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let surface = gpu
            .instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| RenderError::NoAdapter(format!("create canvas surface: {e}")))?;
        Self::assemble(
            &gpu.adapter,
            gpu.device.clone(),
            gpu.queue.clone(),
            surface,
            width,
            height,
            config,
        )
    }

    /// Builds all format-dependent GPU state (surface config, pipelines, caches)
    /// shared by [`new`](Self::new) and [`with_shared`](Self::with_shared).
    #[allow(clippy::too_many_arguments)]
    fn assemble(
        adapter: &wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        config: &Config,
    ) -> Result<Self, RenderError> {
        let caps = surface.get_capabilities(adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let pipeline = Pipeline::new(&device, format);
        let ops = crate::ops::OpsRenderer::new(
            &device,
            &pipeline.bind_group_layout,
            format,
            SAMPLE_COUNT,
        );
        let ztest_pipeline = Pipeline::new_depth_tested(&device, format);
        let (ztest_uniform, ztest_bind_group) =
            make_camera_bind_group(&device, &ztest_pipeline, "canvas z-test camera");
        let (uniform, bind_group) = make_camera_bind_group(&device, &pipeline, "canvas camera");
        let (zoom_uniform, zoom_bind_group) =
            make_camera_bind_group(&device, &pipeline, "canvas zoom camera");
        let (border_uniform, border_bind_group) =
            make_camera_bind_group(&device, &pipeline, "canvas border");
        let msaa_view = make_msaa(&device, format, width, height);

        let mesh_pipeline = MeshPipeline::new(&device, format, SAMPLE_COUNT);
        let (mesh_globals, mesh_globals_bind_group) =
            mesh_pipeline.make_globals(&device, "canvas mesh globals");
        let depth_view = create_depth_view(&device, width, height, SAMPLE_COUNT);

        Ok(Self {
            surface,
            device,
            queue,
            surface_config,
            pipeline,
            ops,
            uniform,
            bind_group,
            zoom_uniform,
            zoom_bind_group,
            border_uniform,
            border_bind_group,
            msaa_view,
            ztest_pipeline,
            ztest_uniform,
            ztest_bind_group,
            mesh_pipeline,
            mesh_cache: MeshBufferCache::new(),
            depth_view,
            mesh_globals,
            mesh_globals_bind_group,
            light: SceneLight::default(),
            cache: TessellationCache::new(),
            camera: Camera2D::from(config),
            aspect: config.frame_width / config.frame_height,
            background: config.background_color,
            zoom_window: None,
        })
    }

    /// Reconfigures the surface (and the MSAA + depth targets) for a new canvas
    /// size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        self.msaa_view = make_msaa(&self.device, self.surface_config.format, width, height);
        self.depth_view = create_depth_view(&self.device, width, height, SAMPLE_COUNT);
    }

    /// Mutable access to the camera (pan/zoom/roll between frames).
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
    }

    /// The directional light the mesh pass shades with.
    pub fn light(&self) -> SceneLight {
        self.light
    }

    /// Sets the directional light the mesh pass shades with.
    pub fn set_light(&mut self, light: SceneLight) {
        self.light = light;
    }

    /// Converts a pointer position in element (CSS) pixels to scene
    /// coordinates, inverting the CSS scaling of the backing store, the
    /// letterbox fit and the camera projection.
    ///
    /// `client_x`/`client_y` are relative to the canvas element's top-left, and
    /// `elem_w`/`elem_h` are its **displayed** (CSS) size. The displayed box need
    /// not share the backing store's aspect — a canvas styled to fill a box of a
    /// different shape is stretched per axis by the browser, so the inverse maps
    /// through the backing size rather than assuming a uniform fit (this is what
    /// keeps drag tracking under the finger on a phone). Returns `None` for a
    /// degenerate (zero-sized) element or fit. See
    /// [`layout::element_to_scene`](crate::layout::element_to_scene).
    pub fn client_to_scene(
        &self,
        client_x: f32,
        client_y: f32,
        elem_w: f32,
        elem_h: f32,
    ) -> Option<glam::Vec3> {
        crate::layout::element_to_scene(
            client_x,
            client_y,
            elem_w,
            elem_h,
            self.surface_config.width as f32,
            self.surface_config.height as f32,
            self.aspect,
            self.camera.view_proj(),
        )
    }

    /// Tessellates and draws `list` into the canvas, letterboxed with
    /// background-color bars.
    ///
    /// A list carrying [`meshes`](DisplayList::meshes) runs the depth-tested
    /// mesh pass first and composites the vector content over it; a list without
    /// them draws exactly as it always has. Vector items opting into
    /// [`z_test`](manim_core::display::DrawItem::z_test) draw depth-tested
    /// between the two, so meshes occlude them; note a zoom-window inset omits
    /// them (like it omits meshes — the inset never re-runs the depth passes).
    ///
    /// A zoom-window inset re-records the **full** `FrameOp` stream under the
    /// magnifying camera, so images and material quads appear inside the
    /// magnifier exactly as they do in the main frame (FE-143b).
    ///
    /// # Errors
    ///
    /// [`RenderError::Readback`] if the swapchain texture cannot be acquired for
    /// a reason other than a recoverable lost/outdated surface (which is
    /// reconfigured and skipped).
    pub fn render(&mut self, list: &DisplayList) -> Result<(), RenderError> {
        // Only the depth-tested half is needed as a raw mesh; the plain half
        // rides the `FrameOp` stream below (including through the zoom inset).
        let (_, ztest_mesh) = self.cache.tessellate_split(list);
        // The full z-ordered FrameOp stream (vector batches + image + material
        // quads) drives the plain pass, so images/materials render in-browser
        // exactly as offscreen. (The zoom-inset path still draws vectors only.)
        let frame_ops = self.cache.tessellate_ops(list);
        self.ops
            .prepare(&self.device, &self.queue, list.arena(), frame_ops.iter());
        let ops_gpu = self.ops.build_ops(&self.device, list.arena(), &frame_ops);
        let mesh_frame = if list.meshes().is_empty() {
            // Skipping the pass must not strand the last mesh scene's buffers.
            self.mesh_cache.clear();
            crate::mesh_pipeline::MeshFrame::default()
        } else {
            self.queue.write_buffer(
                &self.mesh_globals,
                0,
                bytemuck::bytes_of(&MeshGlobals::new(&self.camera, self.light)),
            );
            self.mesh_cache.prepare(
                &self.device,
                &self.queue,
                &self.mesh_pipeline,
                list.arena(),
                list.meshes(),
                &self.camera,
            )
        };

        let view_proj = self.camera.view_proj().to_cols_array_2d();
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::cast_slice(&view_proj));

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            Err(e) => return Err(RenderError::Readback(e.to_string())),
        };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let (out_w, out_h) = (self.surface_config.width, self.surface_config.height);
        let vp = letterbox(out_w as f32, out_h as f32, self.aspect);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("canvas encoder"),
            });

        // The mesh pass clears color + depth over the whole surface, then the
        // vector pass loads its result instead of clearing. Skipped entirely
        // when there are no meshes, so a 2-D frame is unchanged.
        let drew_meshes = !mesh_frame.is_empty();
        if drew_meshes {
            let bg = self.background.premultiplied();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas mesh pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: bg[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if vp.w > 0.0 && vp.h > 0.0 {
                pass.set_viewport(vp.x, vp.y, vp.w, vp.h, 0.0, 1.0);
            }
            mesh_frame.record(
                &mut pass,
                &self.mesh_pipeline,
                &self.mesh_globals_bind_group,
            );
        }

        // Depth-tested vector content, between the mesh pass and the plain
        // vector draw — the wasm mirror of the native z-test pass. Clears what
        // the mesh pass didn't (whole-attachment clears; scissors don't apply).
        let drew_ztest = !ztest_mesh.is_empty();
        if drew_ztest {
            self.queue.write_buffer(
                &self.ztest_uniform,
                0,
                bytemuck::cast_slice(&self.camera.mesh_view_proj().to_cols_array_2d()),
            );
            let bg = self.background.premultiplied();
            let color_load = if drew_meshes {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color {
                    r: bg[0] as f64,
                    g: bg[1] as f64,
                    b: bg[2] as f64,
                    a: bg[3] as f64,
                })
            };
            let depth_load = if drew_meshes {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(1.0)
            };
            let vb = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("canvas z-test vertices"),
                    contents: bytemuck::cast_slice(&ztest_mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let ib = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("canvas z-test indices"),
                    contents: bytemuck::cast_slice(&ztest_mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas z-test vector pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if vp.w > 0.0 && vp.h > 0.0 {
                pass.set_viewport(vp.x, vp.y, vp.w, vp.h, 0.0, 1.0);
            }
            pass.set_pipeline(&self.ztest_pipeline.pipeline);
            pass.set_bind_group(0, &self.ztest_bind_group, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..ztest_mesh.indices.len() as u32, 0, 0..1);
        }

        if let Some(zw) = self.zoom_window {
            // A magnifying inset: second pass with a zoom camera + border.
            let inset =
                crate::layout::inset_viewport(vp, zw.inset_x, zw.inset_y, zw.inset_w, zw.inset_h);
            let (zw_w, zw_h) = crate::layout::zoom_frame_size(zw.region_width, inset.w, inset.h);
            let zoom_cam = Camera2D {
                frame_center: zw.region_center,
                frame_width: zw_w,
                frame_height: zw_h,
                rotation: 0.0,
                three_d: None,
            };
            self.queue.write_buffer(
                &self.zoom_uniform,
                0,
                bytemuck::cast_slice(&zoom_cam.view_proj().to_cols_array_2d()),
            );
            self.queue.write_buffer(
                &self.border_uniform,
                0,
                bytemuck::cast_slice(&glam::Mat4::IDENTITY.to_cols_array_2d()),
            );
            let border = crate::renderer::border_mesh_ndc(
                inset,
                out_w,
                out_h,
                zw.border_width,
                zw.border_color,
            );
            let border_gpu = self.ops.build_ops(
                &self.device,
                list.arena(),
                &[crate::tessellate::FrameOp::Vector(border)],
            );

            // Scissor rectangle in surface pixels, clamped to bounds.
            let sx = inset.x.max(0.0).round() as u32;
            let sy = inset.y.max(0.0).round() as u32;
            let sw = (inset.w.round() as u32).min(out_w.saturating_sub(sx));
            let sh = (inset.h.round() as u32).min(out_h.saturating_sub(sy));

            let bg = self.background.premultiplied();
            let load = if drew_meshes || drew_ztest {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color {
                    r: bg[0] as f64,
                    g: bg[1] as f64,
                    b: bg[2] as f64,
                    a: bg[3] as f64,
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas zoom ops pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_view,
                    resolve_target: Some(&target),
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // The main draw, over the letterboxed base viewport.
            if vp.w > 0.0 && vp.h > 0.0 {
                pass.set_viewport(vp.x, vp.y, vp.w, vp.h, 0.0, 1.0);
            }
            self.ops.record(
                &mut pass,
                &ops_gpu,
                &self.bind_group,
                &self.pipeline.pipeline,
            );
            // The magnifier: the *same* op stream re-recorded under the zoom
            // camera, scissored to the inset. Routing the full stream (not just
            // the vector batches) is what keeps images and material quads
            // visible inside the inset — FE-143b.
            if sw > 0 && sh > 0 {
                pass.set_viewport(inset.x, inset.y, inset.w, inset.h, 0.0, 1.0);
                pass.set_scissor_rect(sx, sy, sw, sh);
                self.ops.record(
                    &mut pass,
                    &ops_gpu,
                    &self.zoom_bind_group,
                    &self.pipeline.pipeline,
                );
                // Reset to the full surface for the border.
                pass.set_viewport(0.0, 0.0, out_w as f32, out_h as f32, 0.0, 1.0);
                pass.set_scissor_rect(0, 0, out_w, out_h);
                self.ops.record(
                    &mut pass,
                    &border_gpu,
                    &self.border_bind_group,
                    &self.pipeline.pipeline,
                );
            }
        } else {
            // The final pass: the full FrameOp stream (vector + image + material),
            // resolving MSAA → swapchain. Loads what the mesh/z-test passes drew,
            // else clears to the background.
            let bg = self.background.premultiplied();
            let load = if drew_meshes || drew_ztest {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color {
                    r: bg[0] as f64,
                    g: bg[1] as f64,
                    b: bg[2] as f64,
                    a: bg[3] as f64,
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas ops pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_view,
                    resolve_target: Some(&target),
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if vp.w > 0.0 && vp.h > 0.0 {
                pass.set_viewport(vp.x, vp.y, vp.w, vp.h, 0.0, 1.0);
            }
            self.ops.record(
                &mut pass,
                &ops_gpu,
                &self.bind_group,
                &self.pipeline.pipeline,
            );
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Renders a [`Frame`], following its camera (center/zoom/rotation) and
    /// background — the web analogue of
    /// `OffscreenRenderer::render_frame` (native).
    ///
    /// Adopts the frame's camera and background and adapts the tessellation
    /// tolerance to the zoom, then draws. Use this to follow an animated camera
    /// in the browser.
    ///
    /// # Errors
    ///
    /// As [`render`](Self::render).
    pub fn render_frame(&mut self, frame: &Frame) -> Result<(), RenderError> {
        self.camera = Camera2D::from(&frame.camera);
        self.background = frame.camera.background;
        self.zoom_window = frame.camera.zoom_window;
        self.cache.set_zoom(frame.camera.height);
        self.render(&frame.display_list)
    }
}

/// Creates a camera uniform buffer and its `@group(0)` bind group.
fn make_camera_bind_group(
    device: &wgpu::Device,
    pipeline: &Pipeline,
    label: &str,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &pipeline.bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    (uniform, bind_group)
}

/// Creates a multisampled color texture view for the canvas surface.
fn make_msaa(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}
