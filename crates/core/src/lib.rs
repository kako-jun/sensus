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
/// Phase 1 (#2) で色覚特性 4 種、Phase 2 (#4) で焦点・屈折 4 種を実装済み。
/// 残りのフィルタは引き続き [`Error::NotImplemented`] を返す。
///
/// `Astigmatism` は軸 90°（with-the-rule）の既定値で適用される。任意の軸を
/// 指定したい場合は [`vision::astigmatism`] を直接呼ぶこと。
pub fn apply(
    filter: Filter,
    img: image::DynamicImage,
    strength: f32,
) -> Result<image::DynamicImage> {
    match filter {
        Filter::Protanopia => vision::protanopia(img, strength),
        Filter::Deuteranopia => vision::deuteranopia(img, strength),
        Filter::Tritanopia => vision::tritanopia(img, strength),
        Filter::Achromatopsia => vision::achromatopsia(img, strength),
        Filter::Myopia => vision::myopia(img, strength),
        Filter::Hyperopia => vision::hyperopia(img, strength),
        Filter::Presbyopia => vision::presbyopia(img, strength),
        Filter::Astigmatism => vision::astigmatism(img, strength, 90.0),
        Filter::Cataract => vision::cataract(img, strength, 0),
        Filter::Photophobia => vision::photophobia(img, strength),
        Filter::NightBlindness => vision::nyctalopia(img, strength),
        Filter::Floaters => vision::floaters(img, strength, 0.5, 0, 0.5, 0.5),
        Filter::Glaucoma => vision::glaucoma(img, strength),
        Filter::MacularDegeneration => vision::macular_degeneration(img, strength),
        Filter::Hemianopia => vision::hemianopia(img, strength, 0.0),
        Filter::TunnelVision => vision::tunnel_vision(img, strength),
        other => Err(Error::NotImplemented(other)),
    }
}
