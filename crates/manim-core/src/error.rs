//! Error types for `manim-core`.

use thiserror::Error;

/// The error type for scene/animation operations.
///
/// ```
/// use manim_core::error::CoreError;
/// let e = CoreError::EmptyPlay;
/// assert_eq!(e.to_string(), "play() was called with no animations");
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A handle referred to a mobject that is no longer in the scene.
    #[error("stale mobject handle: the mobject was removed from the scene")]
    StaleHandle,

    /// A handle referred to a mobject of a different concrete type.
    #[error("mobject handle had the wrong type")]
    TypeMismatch,

    /// [`play`](crate::scene::Scene::play) was called with no animations.
    #[error("play() was called with no animations")]
    EmptyPlay,

    /// A [`SceneBuilder::construct`](crate::scene::SceneBuilder::construct)
    /// implementation failed with a custom message.
    #[error("scene construction failed: {0}")]
    Construct(String),

    /// A text/typesetting operation failed (e.g. manim-text's `MathError`).
    ///
    /// The originating error is preserved as the [`source`](std::error::Error::source),
    /// so callers can downcast to recover it. This lets `manim-text`'s fallible
    /// constructors and label helpers return [`CoreError`] so they compose with
    /// `?` inside a `construct` that returns [`Result`].
    #[error("text/typesetting failed: {0}")]
    Text(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl CoreError {
    /// Wraps a foreign text/typesetting error (e.g. `manim-text`'s `MathError`)
    /// as a [`CoreError::Text`]. Orphan rules block a `From` impl in the text
    /// crate, so its APIs call this at the boundary.
    ///
    /// ```
    /// use manim_core::error::CoreError;
    /// let e = CoreError::text("bad latex");
    /// assert!(e.to_string().contains("bad latex"));
    /// ```
    pub fn text(err: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        CoreError::Text(err.into())
    }
}

/// The crate result type.
///
/// ```
/// use manim_core::error::{CoreError, Result};
/// fn f() -> Result<i32> { Ok(1) }
/// assert!(f().is_ok());
/// # let _ = CoreError::EmptyPlay;
/// ```
pub type Result<T> = std::result::Result<T, CoreError>;
