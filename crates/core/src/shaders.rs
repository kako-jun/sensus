//! GLSL ES 3.00 シェーダソース API。
//! CPU 実装との正本一元化のため、sensus-core がシェーダ文字列と uniform 計算を提供する。
//!
//! # 設計
//!
//! - シェーダ文字列は `include_str!` でバイナリに埋め込む。
//! - uniform 計算は `vision.rs` の CPU 実装と完全に同じ定数・式を使う。
//! - strength は 0.0..=1.0 の範囲を前提とし、範囲外の値は呼び出し元で clamp すること。

// vision.rs の定数と同じ値（radius_px 計算の共通化）
const MYOPIA_MAX_RADIUS_RATIO: f32 = 0.023;
const HYPEROPIA_MAX_RADIUS_RATIO: f32 = 0.015;
const PRESBYOPIA_MAX_RADIUS_RATIO: f32 = 0.011;
const ASTIGMATISM_MAX_RADIUS_RATIO: f32 = 0.011;

/// Machado 2009 severity = 1.0 行列（行優先: row0col0, row0col1, row0col2, ...）。
/// vision.rs の PROTANOPIA 定数と同じ値。
pub const PROTANOPIA_MATRIX: [f32; 9] = [
    0.152286, 1.052583, -0.204868,
    0.114503, 0.786281,  0.099216,
   -0.003882, -0.048116,  1.051998,
];

/// Machado 2009 severity = 1.0 行列（行優先）。
/// vision.rs の DEUTERANOPIA 定数と同じ値。
pub const DEUTERANOPIA_MATRIX: [f32; 9] = [
    0.367322, 0.860646, -0.227968,
    0.280085, 0.672501,  0.047413,
   -0.011820, 0.042940,  0.968881,
];

/// Machado 2009 severity = 1.0 行列（行優先）。
/// vision.rs の TRITANOPIA 定数と同じ値。
pub const TRITANOPIA_MATRIX: [f32; 9] = [
    1.255528, -0.076749, -0.178779,
   -0.078411,  0.930809,  0.147602,
    0.004733,  0.691367,  0.303900,
];

// ---------------------------------------------------------------------------
// シェーダ文字列取得
// ---------------------------------------------------------------------------

/// protanopia.frag の GLSL ES 3.00 ソースを返す。
pub fn protanopia_glsl() -> &'static str {
    include_str!("shaders/protanopia.frag")
}

/// deuteranopia.frag の GLSL ES 3.00 ソースを返す。
pub fn deuteranopia_glsl() -> &'static str {
    include_str!("shaders/deuteranopia.frag")
}

/// tritanopia.frag の GLSL ES 3.00 ソースを返す。
pub fn tritanopia_glsl() -> &'static str {
    include_str!("shaders/tritanopia.frag")
}

/// achromatopsia.frag の GLSL ES 3.00 ソースを返す。
pub fn achromatopsia_glsl() -> &'static str {
    include_str!("shaders/achromatopsia.frag")
}

/// myopia.frag の GLSL ES 3.00 ソースを返す。
pub fn myopia_glsl() -> &'static str {
    include_str!("shaders/myopia.frag")
}

/// hyperopia.frag の GLSL ES 3.00 ソースを返す。
pub fn hyperopia_glsl() -> &'static str {
    include_str!("shaders/hyperopia.frag")
}

/// presbyopia.frag の GLSL ES 3.00 ソースを返す。
pub fn presbyopia_glsl() -> &'static str {
    include_str!("shaders/presbyopia.frag")
}

/// astigmatism.frag の GLSL ES 3.00 ソースを返す。
pub fn astigmatism_glsl() -> &'static str {
    include_str!("shaders/astigmatism.frag")
}

// ---------------------------------------------------------------------------
// uniform 構造体
// ---------------------------------------------------------------------------

/// 色覚フィルタ（Machado LMS 行列）の uniform。
#[derive(Debug, Clone)]
pub struct ColorMatrixUniforms {
    /// severity (0.0..=1.0)
    pub strength: f32,
    /// 3x3 行列（行優先）
    pub matrix: [f32; 9],
}

/// 全色盲フィルタの uniform。
#[derive(Debug, Clone)]
pub struct LumaUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
    /// BT.709 R 係数
    pub r_weight: f32,
    /// BT.709 G 係数
    pub g_weight: f32,
    /// BT.709 B 係数
    pub b_weight: f32,
}

/// disk blur フィルタ（myopia / hyperopia / presbyopia）の uniform。
#[derive(Debug, Clone)]
pub struct BlurUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
    /// ぼかし半径（ピクセル単位）。CPU 実装と同じ式で計算。
    pub radius_px: f32,
}

/// 乱視フィルタの uniform。
#[derive(Debug, Clone)]
pub struct AstigmatismUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
    /// ぼかし半径（ピクセル単位）
    pub radius_px: f32,
    /// 軸角度（度数法）
    pub axis_deg: f32,
}

// ---------------------------------------------------------------------------
// uniform 計算
// ---------------------------------------------------------------------------

/// protanopia の uniform を計算する。
pub fn protanopia_uniforms(strength: f32) -> ColorMatrixUniforms {
    ColorMatrixUniforms {
        strength,
        matrix: PROTANOPIA_MATRIX,
    }
}

/// deuteranopia の uniform を計算する。
pub fn deuteranopia_uniforms(strength: f32) -> ColorMatrixUniforms {
    ColorMatrixUniforms {
        strength,
        matrix: DEUTERANOPIA_MATRIX,
    }
}

/// tritanopia の uniform を計算する。
pub fn tritanopia_uniforms(strength: f32) -> ColorMatrixUniforms {
    ColorMatrixUniforms {
        strength,
        matrix: TRITANOPIA_MATRIX,
    }
}

/// achromatopsia の uniform を計算する。
pub fn achromatopsia_uniforms(strength: f32) -> LumaUniforms {
    LumaUniforms {
        strength,
        r_weight: 0.2126,
        g_weight: 0.7152,
        b_weight: 0.0722,
    }
}

/// myopia の uniform を計算する。
///
/// `image_min_dim`: 画像の `min(width, height)`（ピクセル）。
pub fn myopia_uniforms(strength: f32, image_min_dim: u32) -> BlurUniforms {
    let radius_px = strength.clamp(0.0, 1.0) * MYOPIA_MAX_RADIUS_RATIO * image_min_dim as f32;
    BlurUniforms { strength, radius_px }
}

/// hyperopia の uniform を計算する。
///
/// `image_min_dim`: 画像の `min(width, height)`（ピクセル）。
pub fn hyperopia_uniforms(strength: f32, image_min_dim: u32) -> BlurUniforms {
    let radius_px = strength.clamp(0.0, 1.0) * HYPEROPIA_MAX_RADIUS_RATIO * image_min_dim as f32;
    BlurUniforms { strength, radius_px }
}

/// presbyopia の uniform を計算する。
///
/// `image_min_dim`: 画像の `min(width, height)`（ピクセル）。
pub fn presbyopia_uniforms(strength: f32, image_min_dim: u32) -> BlurUniforms {
    let radius_px = strength.clamp(0.0, 1.0) * PRESBYOPIA_MAX_RADIUS_RATIO * image_min_dim as f32;
    BlurUniforms { strength, radius_px }
}

/// astigmatism の uniform を計算する。
///
/// `image_min_dim`: 画像の `min(width, height)`（ピクセル）。
/// `axis_deg`: 軸角度（度数法。0°=水平, 90°=垂直）。
pub fn astigmatism_uniforms(strength: f32, image_min_dim: u32, axis_deg: f32) -> AstigmatismUniforms {
    let radius_px =
        strength.clamp(0.0, 1.0) * ASTIGMATISM_MAX_RADIUS_RATIO * image_min_dim as f32;
    AstigmatismUniforms {
        strength,
        radius_px,
        axis_deg,
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protanopia_glsl_is_not_empty() {
        assert!(!protanopia_glsl().is_empty());
    }

    #[test]
    fn deuteranopia_glsl_is_not_empty() {
        assert!(!deuteranopia_glsl().is_empty());
    }

    #[test]
    fn tritanopia_glsl_is_not_empty() {
        assert!(!tritanopia_glsl().is_empty());
    }

    #[test]
    fn achromatopsia_glsl_is_not_empty() {
        assert!(!achromatopsia_glsl().is_empty());
    }

    #[test]
    fn myopia_glsl_is_not_empty() {
        assert!(!myopia_glsl().is_empty());
    }

    #[test]
    fn hyperopia_glsl_is_not_empty() {
        assert!(!hyperopia_glsl().is_empty());
    }

    #[test]
    fn presbyopia_glsl_is_not_empty() {
        assert!(!presbyopia_glsl().is_empty());
    }

    #[test]
    fn astigmatism_glsl_is_not_empty() {
        assert!(!astigmatism_glsl().is_empty());
    }

    #[test]
    fn protanopia_uniforms_has_correct_matrix() {
        let u = protanopia_uniforms(0.0);
        assert_eq!(u.strength, 0.0);
        assert_eq!(u.matrix, PROTANOPIA_MATRIX);
    }

    #[test]
    fn deuteranopia_uniforms_has_correct_matrix() {
        let u = deuteranopia_uniforms(1.0);
        assert_eq!(u.matrix, DEUTERANOPIA_MATRIX);
    }

    #[test]
    fn tritanopia_uniforms_has_correct_matrix() {
        let u = tritanopia_uniforms(1.0);
        assert_eq!(u.matrix, TRITANOPIA_MATRIX);
    }

    #[test]
    fn achromatopsia_uniforms_bt709_weights() {
        let u = achromatopsia_uniforms(1.0);
        assert!((u.r_weight - 0.2126).abs() < 1e-6);
        assert!((u.g_weight - 0.7152).abs() < 1e-6);
        assert!((u.b_weight - 0.0722).abs() < 1e-6);
    }

    #[test]
    fn myopia_uniforms_strength_zero_has_zero_radius() {
        let u = myopia_uniforms(0.0, 1000);
        assert!(u.radius_px < 0.001, "strength=0 のとき radius はゼロに近い");
    }

    #[test]
    fn myopia_uniforms_strength_one_correct_radius() {
        let u = myopia_uniforms(1.0, 1000);
        let expected = MYOPIA_MAX_RADIUS_RATIO * 1000.0;
        assert!((u.radius_px - expected).abs() < 1e-4);
    }

    #[test]
    fn hyperopia_uniforms_strength_zero_has_zero_radius() {
        let u = hyperopia_uniforms(0.0, 1000);
        assert!(u.radius_px < 0.001);
    }

    #[test]
    fn presbyopia_uniforms_strength_zero_has_zero_radius() {
        let u = presbyopia_uniforms(0.0, 1000);
        assert!(u.radius_px < 0.001);
    }

    #[test]
    fn astigmatism_uniforms_preserves_axis() {
        let u = astigmatism_uniforms(1.0, 1000, 45.0);
        assert_eq!(u.axis_deg, 45.0);
    }

    #[test]
    fn astigmatism_uniforms_strength_zero_has_zero_radius() {
        let u = astigmatism_uniforms(0.0, 1000, 90.0);
        assert!(u.radius_px < 0.001);
    }
}
