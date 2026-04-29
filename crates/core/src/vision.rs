//! Vision filters: color vision deficiency, blur / refraction, visual field
//! defects, light sensitivity, etc.
//!
//! Phase 1 (Issue #2) で色覚特性フィルタ（protanopia / deuteranopia /
//! tritanopia / achromatopsia）から実装する。

use crate::Result;
use image::DynamicImage;

/// Protanopia (red-blind) simulation. Implemented in #2.
pub fn protanopia(_img: DynamicImage, _strength: f32) -> Result<DynamicImage> {
    unimplemented!("vision::protanopia is not implemented yet (#2)")
}

/// Deuteranopia (green-blind) simulation. Implemented in #2.
pub fn deuteranopia(_img: DynamicImage, _strength: f32) -> Result<DynamicImage> {
    unimplemented!("vision::deuteranopia is not implemented yet (#2)")
}

/// Tritanopia (blue-blind) simulation. Implemented in #2.
pub fn tritanopia(_img: DynamicImage, _strength: f32) -> Result<DynamicImage> {
    unimplemented!("vision::tritanopia is not implemented yet (#2)")
}

/// Achromatopsia (total color blindness) simulation. Implemented in #2.
pub fn achromatopsia(_img: DynamicImage, _strength: f32) -> Result<DynamicImage> {
    unimplemented!("vision::achromatopsia is not implemented yet (#2)")
}
