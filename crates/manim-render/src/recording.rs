//! Browser capture-to-video: record a `<canvas>` to a downloadable video file.
//!
//! `CanvasRecorder` (wasm + `web` only) wraps the browser's
//! `canvas.captureStream()` + `MediaRecorder` pair. Start it, drive your render loop as usual, stop it,
//! and you get an object URL pointing at the recorded video — hand it to an
//! `<a download>` or a `<video src>`.
//!
//! ```no_run
//! # #[cfg(all(target_arch = "wasm32", feature = "web"))]
//! # async fn go(canvas: &web_sys::HtmlCanvasElement) -> Result<(), manim_render::recording::RecordError> {
//! use manim_render::recording::{CanvasRecorder, RecorderOptions};
//!
//! let rec = CanvasRecorder::start(canvas, &RecorderOptions::default())?;
//! // ... drive the rAF render loop for a while ...
//! let url = rec.stop().await?;
//! // `url` is an object URL — set it on an <a download="scene.webm">.
//! # Ok(())
//! # }
//! ```
//!
//! # What the browser actually records
//!
//! `captureStream(fps)` samples the canvas's *presented* frames. It records what
//! the compositor shows, so the output runs at wall-clock speed, not at the
//! scene's nominal frame rate: a render loop that cannot keep up produces a
//! video with dropped/duplicated frames rather than a slowed-down one. For a
//! frame-exact render, use the native `VideoExporter` (ffmpeg) instead — this
//! module is for capturing a live, interactive session.
//!
//! # Browser support
//!
//! Codec support is genuinely uneven, which is why [`RecorderOptions`] carries a
//! *list* of MIME candidates and [`choose_mime`] picks the first the browser
//! admits to supporting:
//!
//! - **Chrome / Edge / Firefox** — WebM works. VP9 (`video/webm;codecs=vp9`) is
//!   the default first choice; VP8 is the fallback for older builds.
//! - **Safari** — the honest caveat. Safari has shipped `MediaRecorder` since
//!   14.1, but historically records **MP4/H.264 only** and reports
//!   `isTypeSupported("video/webm")` as `false`. Recent Safari (17+) added some
//!   WebM support, but it is not something to rely on. The default candidate
//!   list therefore ends with `video/mp4`, so Safari lands there. Two further
//!   caveats worth telling users about: Safari's `isTypeSupported` has
//!   historically over-reported for some codec strings, so a `start()` that
//!   succeeds is not an absolute guarantee of a playable file; and Safari on iOS
//!   may stop the capture stream when the tab is backgrounded, truncating the
//!   recording.
//! - **Anything else / very old browsers** — if no candidate is supported,
//!   `start()` fails with [`RecordError::NoSupportedMime`] rather than silently
//!   producing an unplayable file.
//!
//! The recorded container is whatever [`choose_mime`] picked; read it back from
//! `CanvasRecorder::mime_type` to name the download file correctly.

#[cfg(all(feature = "web", target_arch = "wasm32"))]
use wasm_bindgen::prelude::*;

/// The default MIME candidates, best first.
///
/// VP9 gives the best quality-per-bit of the three; VP8 covers older
/// Chromium/Firefox; `video/mp4` is the Safari landing spot. See the
/// [module docs](self) for the per-browser story.
pub const DEFAULT_MIME_CANDIDATES: &[&str] = &[
    "video/webm;codecs=vp9",
    "video/webm;codecs=vp8",
    "video/webm",
    "video/mp4",
];

/// How to record: which containers to try, and the quality/cadence knobs.
///
/// ```
/// use manim_render::recording::RecorderOptions;
///
/// let opts = RecorderOptions::default();
/// assert_eq!(opts.mime_candidates[0], "video/webm;codecs=vp9");
/// assert_eq!(opts.fps, Some(60.0));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct RecorderOptions {
    /// MIME types to try in order; the first the browser supports wins.
    pub mime_candidates: Vec<String>,
    /// Frames per second to request from `captureStream`. `None` captures a new
    /// frame only when the canvas is painted (the browser's variable-rate mode),
    /// which is the lighter option for a scene that is often static.
    pub fps: Option<f64>,
    /// Target video bitrate in bits per second, or `None` for the browser's
    /// default (typically ~2.5 Mbps, low for detailed line art).
    pub video_bits_per_second: Option<u32>,
    /// If set, the recorder emits a data chunk every this many milliseconds
    /// instead of one chunk at stop. Only matters for streaming the result out
    /// as it is produced; for a plain "record then download" leave it `None`.
    pub timeslice_ms: Option<f64>,
}

impl Default for RecorderOptions {
    fn default() -> Self {
        Self {
            mime_candidates: DEFAULT_MIME_CANDIDATES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            fps: Some(60.0),
            // 8 Mbps: manim frames are large flat areas plus thin high-contrast
            // strokes, and the browser default smears the strokes badly.
            video_bits_per_second: Some(8_000_000),
            timeslice_ms: None,
        }
    }
}

impl RecorderOptions {
    /// Options recording at `fps`, otherwise the defaults.
    ///
    /// ```
    /// use manim_render::recording::RecorderOptions;
    /// assert_eq!(RecorderOptions::at_fps(30.0).fps, Some(30.0));
    /// ```
    pub fn at_fps(fps: f64) -> Self {
        Self {
            fps: Some(fps),
            ..Self::default()
        }
    }

    /// Options with a single forced MIME type — use when you know the target
    /// browser and want a hard failure rather than a silent fallback.
    ///
    /// ```
    /// use manim_render::recording::{choose_mime, RecorderOptions};
    /// let opts = RecorderOptions::with_mime("video/mp4");
    /// assert_eq!(opts.mime_candidates, vec!["video/mp4".to_string()]);
    /// // A browser that only does WebM now finds nothing.
    /// assert_eq!(choose_mime(&opts.mime_candidates, |m| m.starts_with("video/webm")), None);
    /// ```
    pub fn with_mime(mime: impl Into<String>) -> Self {
        Self {
            mime_candidates: vec![mime.into()],
            ..Self::default()
        }
    }

    /// Options with `video_bits_per_second` set.
    ///
    /// ```
    /// use manim_render::recording::RecorderOptions;
    /// assert_eq!(RecorderOptions::default().at_bitrate(2_000_000).video_bits_per_second, Some(2_000_000));
    /// ```
    pub fn at_bitrate(mut self, bps: u32) -> Self {
        self.video_bits_per_second = Some(bps);
        self
    }
}

/// Picks the first candidate in `candidates` that `supported` accepts.
///
/// Split out from the browser call (`MediaRecorder.isTypeSupported`) so the
/// fallback-chain logic is a pure function and testable natively — the wasm side
/// just passes the real predicate.
///
/// ```
/// use manim_render::recording::{choose_mime, DEFAULT_MIME_CANDIDATES};
///
/// let candidates: Vec<String> =
///     DEFAULT_MIME_CANDIDATES.iter().map(|s| s.to_string()).collect();
///
/// // A Chrome-like browser takes VP9.
/// assert_eq!(
///     choose_mime(&candidates, |_| true).as_deref(),
///     Some("video/webm;codecs=vp9")
/// );
/// // A Safari-like browser falls all the way through to MP4.
/// assert_eq!(
///     choose_mime(&candidates, |m| m == "video/mp4").as_deref(),
///     Some("video/mp4")
/// );
/// // A browser supporting none of them yields `None`.
/// assert_eq!(choose_mime(&candidates, |_| false), None);
/// ```
pub fn choose_mime(candidates: &[String], supported: impl Fn(&str) -> bool) -> Option<String> {
    candidates
        .iter()
        .find(|m| supported(m))
        .map(|m| m.to_string())
}

/// The file extension conventionally matching a recorded `mime` — for naming the
/// download.
///
/// ```
/// use manim_render::recording::extension_for_mime;
/// assert_eq!(extension_for_mime("video/webm;codecs=vp9"), "webm");
/// assert_eq!(extension_for_mime("video/mp4"), "mp4");
/// // Unknown containers fall back to the safest generic guess.
/// assert_eq!(extension_for_mime("video/x-matroska"), "webm");
/// ```
pub fn extension_for_mime(mime: &str) -> &'static str {
    let base = mime.split(';').next().unwrap_or("").trim();
    match base {
        "video/mp4" => "mp4",
        _ => "webm",
    }
}

/// Why a recording could not be started or finished.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    /// The browser supports none of the requested MIME types. Carries the list
    /// that was tried, so the message can name them.
    #[error("no supported video MIME type among [{}]; \
             the browser's MediaRecorder rejected every candidate", .0.join(", "))]
    NoSupportedMime(Vec<String>),
    /// `canvas.captureStream()` failed or is unavailable.
    #[error("canvas.captureStream() failed: {0}")]
    CaptureStream(String),
    /// Constructing or driving the `MediaRecorder` failed.
    #[error("MediaRecorder error: {0}")]
    Recorder(String),
    /// The recording produced no data (stopped before any frame was captured,
    /// or the stream was cut short by the browser).
    #[error("recording produced no data: {0}")]
    NoData(String),
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod web {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use web_sys::{Blob, BlobEvent, HtmlCanvasElement, MediaRecorder, MediaRecorderOptions};

    /// An in-progress recording of a `<canvas>`.
    ///
    /// Created by [`start`](Self::start), finished by [`stop`](Self::stop).
    /// Dropping without calling `stop` cancels the recording and discards the
    /// data (the underlying `MediaRecorder` is stopped so the capture stream
    /// does not leak).
    ///
    /// See the [module docs](super) for browser caveats.
    pub struct CanvasRecorder {
        recorder: MediaRecorder,
        chunks: Rc<RefCell<Vec<Blob>>>,
        mime: String,
        // Kept alive for the recorder's lifetime: dropping a `Closure` detaches
        // the JS callback, so the chunk handler must outlive `start`.
        _on_data: Closure<dyn FnMut(BlobEvent)>,
    }

    impl CanvasRecorder {
        /// Starts recording `canvas` under `opts`.
        ///
        /// Picks a MIME type via [`choose_mime`] against the browser's real
        /// `MediaRecorder.isTypeSupported`, opens a capture stream, and begins
        /// collecting chunks immediately.
        ///
        /// # Errors
        ///
        /// [`RecordError::NoSupportedMime`] if the browser supports none of
        /// `opts.mime_candidates`; [`RecordError::CaptureStream`] if the canvas
        /// cannot be captured; [`RecordError::Recorder`] if the `MediaRecorder`
        /// cannot be built or started.
        pub fn start(
            canvas: &HtmlCanvasElement,
            opts: &RecorderOptions,
        ) -> Result<Self, RecordError> {
            let mime = choose_mime(&opts.mime_candidates, MediaRecorder::is_type_supported)
                .ok_or_else(|| RecordError::NoSupportedMime(opts.mime_candidates.clone()))?;

            let stream = match opts.fps {
                Some(fps) => canvas.capture_stream_with_frame_request_rate(fps),
                None => canvas.capture_stream(),
            }
            .map_err(|e| RecordError::CaptureStream(js_msg(&e)))?;

            let mr_opts = MediaRecorderOptions::new();
            mr_opts.set_mime_type(&mime);
            if let Some(bps) = opts.video_bits_per_second {
                mr_opts.set_video_bits_per_second(bps);
            }
            let recorder =
                MediaRecorder::new_with_media_stream_and_media_recorder_options(&stream, &mr_opts)
                    .map_err(|e| RecordError::Recorder(js_msg(&e)))?;

            // Every `dataavailable` chunk is appended; `stop` concatenates them
            // into one Blob. The browser emits a single chunk at stop unless a
            // timeslice was requested.
            let chunks: Rc<RefCell<Vec<Blob>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = Rc::clone(&chunks);
            let on_data = Closure::<dyn FnMut(BlobEvent)>::new(move |ev: BlobEvent| {
                if let Some(blob) = ev.data() {
                    if blob.size() > 0.0 {
                        sink.borrow_mut().push(blob);
                    }
                }
            });
            recorder.set_ondataavailable(Some(on_data.as_ref().unchecked_ref()));

            match opts.timeslice_ms {
                Some(ms) => recorder.start_with_time_slice(ms as i32),
                None => recorder.start(),
            }
            .map_err(|e| RecordError::Recorder(js_msg(&e)))?;

            Ok(Self {
                recorder,
                chunks,
                mime,
                _on_data: on_data,
            })
        }

        /// The MIME type actually being recorded — the candidate that won.
        ///
        /// Pair it with [`extension_for_mime`] to name the download file.
        pub fn mime_type(&self) -> &str {
            &self.mime
        }

        /// Whether the recorder is still running (`MediaRecorder.state` is not
        /// `"inactive"`).
        pub fn is_recording(&self) -> bool {
            self.recorder.state() != web_sys::RecordingState::Inactive
        }

        /// Stops recording and resolves to an **object URL** for the recorded
        /// video.
        ///
        /// The caller owns the URL: revoke it with `URL.revokeObjectURL` once the
        /// download or playback is done, or the blob is pinned in memory for the
        /// page's lifetime.
        ///
        /// # Errors
        ///
        /// [`RecordError::NoData`] if the recording captured nothing (stopped
        /// immediately, or the browser cut the stream); [`RecordError::Recorder`]
        /// if the stop itself or the blob/URL construction fails.
        pub async fn stop(self) -> Result<String, RecordError> {
            // `stop()` flushes a final `dataavailable` *then* fires `stop`, so
            // waiting on `stop` is what guarantees the last chunk has landed.
            let (tx, rx) = futures_oneshot();
            let on_stop = Closure::<dyn FnMut()>::new(tx);
            self.recorder
                .set_onstop(Some(on_stop.as_ref().unchecked_ref()));

            if self.is_recording() {
                self.recorder
                    .stop()
                    .map_err(|e| RecordError::Recorder(js_msg(&e)))?;
                rx.await;
            }
            self.recorder.set_onstop(None);
            self.recorder.set_ondataavailable(None);
            drop(on_stop);

            let parts = js_sys::Array::new();
            for blob in self.chunks.borrow().iter() {
                parts.push(blob);
            }
            if parts.length() == 0 {
                return Err(RecordError::NoData(format!(
                    "MediaRecorder emitted no chunks for {}",
                    self.mime
                )));
            }
            let bag = web_sys::BlobPropertyBag::new();
            bag.set_type(&self.mime);
            let blob = Blob::new_with_blob_sequence_and_options(&parts, &bag)
                .map_err(|e| RecordError::Recorder(js_msg(&e)))?;
            web_sys::Url::create_object_url_with_blob(&blob)
                .map_err(|e| RecordError::Recorder(js_msg(&e)))
        }
    }

    impl Drop for CanvasRecorder {
        fn drop(&mut self) {
            // Cancel rather than leak the capture stream. Errors here are
            // unactionable (the page is tearing down the recorder anyway).
            if self.recorder.state() != web_sys::RecordingState::Inactive {
                let _ = self.recorder.stop();
            }
            self.recorder.set_ondataavailable(None);
            self.recorder.set_onstop(None);
        }
    }

    /// A minimal one-shot channel: a `send` closure and a future that completes
    /// when it is called. Avoids pulling `futures-channel` in for one signal.
    fn futures_oneshot() -> (impl FnMut(), impl std::future::Future<Output = ()>) {
        use std::task::{Poll, Waker};

        #[derive(Default)]
        struct Shared {
            done: bool,
            waker: Option<Waker>,
        }
        let shared = Rc::new(RefCell::new(Shared::default()));
        let tx_shared = Rc::clone(&shared);
        let tx = move || {
            let mut s = tx_shared.borrow_mut();
            s.done = true;
            if let Some(w) = s.waker.take() {
                w.wake();
            }
        };
        let rx = std::future::poll_fn(move |cx| {
            let mut s = shared.borrow_mut();
            if s.done {
                Poll::Ready(())
            } else {
                s.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        });
        (tx, rx)
    }

    /// A readable message out of a `JsValue` error.
    fn js_msg(e: &JsValue) -> String {
        e.as_string()
            .or_else(|| {
                e.dyn_ref::<js_sys::Error>()
                    .map(|err| String::from(err.message()))
            })
            .unwrap_or_else(|| format!("{e:?}"))
    }
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use web::CanvasRecorder;

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Vec<String> {
        DEFAULT_MIME_CANDIDATES
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    #[test]
    fn chrome_like_browser_gets_vp9() {
        // Chrome supports every WebM flavour; the first candidate wins.
        let got = choose_mime(&defaults(), |m| m.starts_with("video/webm"));
        assert_eq!(got.as_deref(), Some("video/webm;codecs=vp9"));
    }

    #[test]
    fn vp8_only_browser_skips_vp9() {
        let got = choose_mime(&defaults(), |m| {
            m == "video/webm;codecs=vp8" || m == "video/webm"
        });
        assert_eq!(got.as_deref(), Some("video/webm;codecs=vp8"));
    }

    #[test]
    fn safari_like_browser_falls_through_to_mp4() {
        // Safari's historical behaviour: isTypeSupported("video/webm") is false.
        let got = choose_mime(&defaults(), |m| m == "video/mp4");
        assert_eq!(got.as_deref(), Some("video/mp4"));
    }

    #[test]
    fn no_supported_mime_yields_none() {
        assert_eq!(choose_mime(&defaults(), |_| false), None);
    }

    #[test]
    fn empty_candidate_list_yields_none() {
        assert_eq!(choose_mime(&[], |_| true), None);
    }

    #[test]
    fn choice_follows_candidate_order_not_preference() {
        // The list is the policy: reversing it reverses the pick.
        let mut reversed = defaults();
        reversed.reverse();
        assert_eq!(
            choose_mime(&reversed, |_| true).as_deref(),
            Some("video/mp4")
        );
    }

    #[test]
    fn extensions_match_containers() {
        assert_eq!(extension_for_mime("video/webm;codecs=vp9"), "webm");
        assert_eq!(extension_for_mime("video/webm;codecs=vp8"), "webm");
        assert_eq!(extension_for_mime("video/webm"), "webm");
        assert_eq!(extension_for_mime("video/mp4"), "mp4");
        assert_eq!(extension_for_mime("video/mp4;codecs=avc1"), "mp4");
        // Whitespace around the base type is tolerated.
        assert_eq!(extension_for_mime("video/mp4 ;codecs=avc1"), "mp4");
    }

    #[test]
    fn default_options_are_the_documented_ones() {
        let o = RecorderOptions::default();
        assert_eq!(o.mime_candidates, defaults());
        assert_eq!(o.fps, Some(60.0));
        assert_eq!(o.video_bits_per_second, Some(8_000_000));
        assert_eq!(o.timeslice_ms, None);
    }

    #[test]
    fn builders_override_only_their_field() {
        let o = RecorderOptions::at_fps(24.0).at_bitrate(1_000_000);
        assert_eq!(o.fps, Some(24.0));
        assert_eq!(o.video_bits_per_second, Some(1_000_000));
        // Everything else stays at the default.
        assert_eq!(o.mime_candidates, defaults());

        let forced = RecorderOptions::with_mime("video/mp4");
        assert_eq!(forced.mime_candidates, vec!["video/mp4".to_string()]);
        assert_eq!(forced.fps, RecorderOptions::default().fps);
    }

    #[test]
    fn no_supported_mime_error_names_the_candidates() {
        let e = RecordError::NoSupportedMime(defaults());
        let msg = e.to_string();
        assert!(msg.contains("video/webm;codecs=vp9"), "{msg}");
        assert!(msg.contains("video/mp4"), "{msg}");
    }
}
