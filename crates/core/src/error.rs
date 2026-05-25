//! Error types shared across `sensus-core`.
//!
//! All public entry points return [`crate::Result`], which wraps these
//! variants. CLI / GUI consumers map them to user-facing messages.

use thiserror::Error;

/// Errors produced by `sensus-core` filters and pipeline operations.
#[derive(Debug, Error)]
pub enum Error {
    /// The requested filter is declared but not yet implemented.
    /// Phase 1〜3 で順次解消される。
    #[error("filter {0:?} is not implemented yet")]
    NotImplemented(crate::Filter),

    /// Underlying [`image`] crate error (decode / encode / format).
    #[error("image processing error: {0}")]
    Image(#[from] image::ImageError),

    /// A pipeline step failed. `step` はゼロ起算インデックス、`filter` はフィルタ名。
    #[error("pipeline step {step} ({filter}) failed: {source}")]
    Pipeline {
        step: usize,
        filter: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// MPO バイト列に2枚目の JPEG (SOI `FFD8`) が見つからなかった。
    #[error("invalid MPO: second JPEG (SOI) not found after EOI")]
    InvalidMpo,

    /// 左右画像のサイズが一致しない。
    #[error("size mismatch: left {left_w}x{left_h}, right {right_w}x{right_h}")]
    SizeMismatch {
        left_w: u32,
        left_h: u32,
        right_w: u32,
        right_h: u32,
    },
}
