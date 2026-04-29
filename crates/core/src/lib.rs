//! sensus-core — sensory perception simulation core.
//!
//! Pure logic library that applies sensory filters (color blindness, blur,
//! visual field defects, hearing loss, etc.) to media buffers. All public
//! entry points take and return [`image::DynamicImage`] so callers can chain
//! filters without committing to a specific pixel format.
//!
//! This crate intentionally has **no I/O** — file reads, file writes,
//! decoding from arbitrary formats, and any subprocess work belongs in the
//! `sensus` CLI crate or in downstream applications (e.g. universal-experience).

pub mod error;
pub mod hearing;
pub mod pipeline;
pub mod vision;

pub use error::Error;

/// Convenience alias for `Result<T, sensus_core::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// All sensory filters planned for sensus.
///
/// Implemented filters return their result via [`apply`]; variants whose
/// implementation has not yet landed return [`Error::NotImplemented`].
/// The enum lives in `sensus-core` so non-CLI consumers (GUI, library
/// users) can refer to filters without pulling in clap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    // vision (Phase 1: color vision deficiency)
    Protanopia,
    Deuteranopia,
    Tritanopia,
    Achromatopsia,
    // vision (Phase 1+: tetrachromacy)
    Tetrachromacy,
    // vision (Phase 2: focus / refraction)
    Myopia,
    Hyperopia,
    Astigmatism,
    Presbyopia,
    // vision (Phase 3: visual field)
    Glaucoma,
    MacularDegeneration,
    Hemianopia,
    TunnelVision,
    // vision (Phase 3: light / transparency)
    Cataract,
    Floaters,
    Photophobia,
    NightBlindness,
}

/// Apply a [`Filter`] to an image at a given strength (`0.0..=1.0`).
///
/// Phase 1〜3 で各フィルタを順次実装する。現状は scaffold (#1) のため、
/// すべてのバリアントが [`Error::NotImplemented`] を返す。
pub fn apply(
    filter: Filter,
    _img: image::DynamicImage,
    _strength: f32,
) -> Result<image::DynamicImage> {
    Err(Error::NotImplemented(filter))
}
