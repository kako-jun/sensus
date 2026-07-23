//! GLSL ES 3.00 シェーダソース API。
//! CPU 実装との正本一元化のため、sensus-core がシェーダ文字列と uniform 計算を提供する。
//!
//! # 設計
//!
//! - シェーダ文字列は `include_str!` でバイナリに埋め込む。
//! - uniform 計算は `vision/` の CPU 実装と完全に同じ定数・式を使う。
//! - strength は 0.0..=1.0 の範囲を前提とし、範囲外の値は呼び出し元で clamp すること。

// vision/refraction.rs の定数と同じ値（radius_px 計算の共通化）
const MYOPIA_MAX_RADIUS_RATIO: f32 = 0.023;
const HYPEROPIA_MAX_RADIUS_RATIO: f32 = 0.015;
const PRESBYOPIA_MAX_RADIUS_RATIO: f32 = 0.011;
const ASTIGMATISM_MAX_RADIUS_RATIO: f32 = 0.011;
// vision/light.rs の PHOTOPHOBIA_BLOOM_RADIUS_RATIO と同じ値（bloom 半径 = ratio * min(W,H) * strength）
const PHOTOPHOBIA_BLOOM_RADIUS_RATIO: f32 = 0.05;
// vision/fatigue.rs eye_strain の disk blur 半径係数（半径 = strength * 1.5 px、画像サイズ非依存）
const EYE_STRAIN_BLUR_RADIUS_PX_PER_STRENGTH: f32 = 1.5;

/// Machado 2009 severity = 1.0 行列（行優先: row0col0, row0col1, row0col2, ...）。
/// vision/color.rs の PROTANOPIA 定数と同じ値。
pub const PROTANOPIA_MATRIX: [f32; 9] = [
    0.152286, 1.052583, -0.204868, 0.114503, 0.786281, 0.099216, -0.003882, -0.048116, 1.051998,
];

/// Machado 2009 severity = 1.0 行列（行優先）。
/// vision/color.rs の DEUTERANOPIA 定数と同じ値。
pub const DEUTERANOPIA_MATRIX: [f32; 9] = [
    0.367322, 0.860646, -0.227968, 0.280085, 0.672501, 0.047413, -0.011820, 0.042940, 0.968881,
];

/// Machado 2009 severity = 1.0 行列（行優先）。
/// vision/color.rs の TRITANOPIA 定数と同じ値。
pub const TRITANOPIA_MATRIX: [f32; 9] = [
    1.255528, -0.076749, -0.178779, -0.078411, 0.930809, 0.147602, 0.004733, 0.691367, 0.303900,
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

/// diplopia.frag の GLSL ES 3.00 ソースを返す。
pub fn diplopia_glsl() -> &'static str {
    include_str!("shaders/diplopia.frag")
}

/// nystagmus.frag の GLSL ES 3.00 ソースを返す。
pub fn nystagmus_glsl() -> &'static str {
    include_str!("shaders/nystagmus.frag")
}

/// starbursts.frag の GLSL ES 3.00 ソースを返す。
pub fn starbursts_glsl() -> &'static str {
    include_str!("shaders/starbursts.frag")
}

/// eye_strain.frag の GLSL ES 3.00 ソースを返す。
pub fn eye_strain_glsl() -> &'static str {
    include_str!("shaders/eye_strain.frag")
}

/// eye_strain フィルタの uniform。
///
/// CPU 実装 `vision::eye_strain` は contrast+vignette の後に
/// 半径 `strength * 1.5 px`（画像サイズ非依存）の disk blur を適用する。
/// シェーダはこの半径を `uRadiusPx` として受け取り、texel size で正規化する。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeStrainUniforms {
    /// strength (0.0..=1.0)。eye_strain.frag の `uStrength` に渡す。
    pub strength: f32,
    /// disk blur 半径（ピクセル単位）= strength * 1.5。
    /// eye_strain.frag の `uRadiusPx` に渡す。
    pub radius_px: f32,
    /// テクセルサイズ vec2(1.0/width, 1.0/height)。
    /// eye_strain.frag の `uTexelSize` に渡す。
    pub texel_size: [f32; 2],
}

/// eye_strain の uniform を返す。
///
/// `width`, `height`: 画像の幅・高さ（ピクセル）。texel size の算出に使う。
/// blur 半径は画像サイズ非依存（`strength * 1.5 px`）だが、シェーダ内で
/// テクスチャ座標へ変換するため texel size が必要。
pub fn eye_strain_uniforms(strength: f32, width: u32, height: u32) -> EyeStrainUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let radius_px = strength.clamp(0.0, 1.0) * EYE_STRAIN_BLUR_RADIUS_PX_PER_STRENGTH;
    EyeStrainUniforms {
        strength,
        radius_px,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
    }
}

/// dry_eye.frag の GLSL ES 3.00 ソースを返す。
pub fn dry_eye_glsl() -> &'static str {
    include_str!("shaders/dry_eye.frag")
}

/// dry_eye フィルタの uniform。
///
/// CPU 実装 `vision::dry_eye` は 32×32 ピクセルタイルごとに seed=42 の 32bit
/// spatial hash でノイズ値を決め、半径 `noise * strength * 3px` の等方 disk blur を
/// linear sRGB 空間で適用する（#99 で CPU/GLSL のノイズモデルを統一）。
/// シェーダはタイル座標とピクセル座標の算出に `uTexelSize` を必要とする。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DryEyeUniforms {
    /// strength (0.0..=1.0)。dry_eye.frag の `uStrength` に渡す。
    pub strength: f32,
    /// テクセルサイズ vec2(1.0/width, 1.0/height)。dry_eye.frag の `uTexelSize` に渡す。
    pub texel_size: [f32; 2],
}

/// dry_eye の uniform を返す。
///
/// `width`, `height`: 画像の幅・高さ（ピクセル）。タイル座標・disk 半径の
/// テクスチャ座標変換に使う texel size を算出する。
pub fn dry_eye_uniforms(strength: f32, width: u32, height: u32) -> DryEyeUniforms {
    let strength = crate::vision::normalize_strength(strength);
    DryEyeUniforms {
        strength,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
    }
}

/// contrast_sensitivity.frag の GLSL ES 3.00 ソースを返す。
pub fn contrast_sensitivity_glsl() -> &'static str {
    include_str!("shaders/contrast_sensitivity.frag")
}

/// contrast_sensitivity の uniform を返す。
pub fn contrast_sensitivity_uniforms(strength: f32) -> SimpleStrengthUniforms {
    let strength = crate::vision::normalize_strength(strength);
    SimpleStrengthUniforms { strength }
}

/// detail_loss.frag の GLSL ES 3.00 ソースを返す。
pub fn detail_loss_glsl() -> &'static str {
    include_str!("shaders/detail_loss.frag")
}

/// detail_loss の uniform を返す。
pub fn detail_loss_uniforms(strength: f32) -> SimpleStrengthUniforms {
    let strength = crate::vision::normalize_strength(strength);
    SimpleStrengthUniforms { strength }
}

/// teichopsia.frag の GLSL ES 3.00 ソースを返す。
pub fn teichopsia_glsl() -> &'static str {
    include_str!("shaders/teichopsia.frag")
}

/// teichopsia の uniform を返す。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正に使用（S-3: 楕円化防止）。
pub fn teichopsia_uniforms(strength: f32, width: u32, height: u32) -> TeichopsiaUniforms {
    let strength = crate::vision::normalize_strength(strength);
    TeichopsiaUniforms {
        strength,
        aspect: width as f32 / height as f32,
    }
}

/// flickering_stars.frag の GLSL ES 3.00 ソースを返す。
pub fn flickering_stars_glsl() -> &'static str {
    include_str!("shaders/flickering_stars.frag")
}

/// flickering_stars の uniform を返す。
///
/// `seed`: CPU 実装の u64 seed の下位 32bit を u32 として渡す（M-3）。
pub fn flickering_stars_uniforms(strength: f32, seed: u64) -> FlickeringStarsUniforms {
    let strength = crate::vision::normalize_strength(strength);
    FlickeringStarsUniforms {
        strength,
        seed: seed as u32,
        // CPU vision::flickering_stars と同じ点数算出（strength*200 の切り捨て）。
        count: (strength.clamp(0.0, 1.0) * 200.0) as i32,
    }
}

/// photophobia.frag の GLSL ES 3.00 ソースを返す。
pub fn photophobia_glsl() -> &'static str {
    include_str!("shaders/photophobia.frag")
}

/// nyctalopia.frag の GLSL ES 3.00 ソースを返す。
pub fn nyctalopia_glsl() -> &'static str {
    include_str!("shaders/nyctalopia.frag")
}

/// cataract.frag の GLSL ES 3.00 ソースを返す。
pub fn cataract_glsl() -> &'static str {
    include_str!("shaders/cataract.frag")
}

/// nyctalopia / cataract の共通 uniform（strength のみ）。
/// （photophobia は bloom 半径が必要なため `PhotophobiaUniforms` を使う。）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimpleStrengthUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
}

/// teichopsia フィルタの uniform。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeichopsiaUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
    /// アスペクト比（width / height）。楕円化防止のための aspect 補正（S-3）。
    pub aspect: f32,
}

/// flickering_stars フィルタの uniform。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlickeringStarsUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
    /// ランダムシード（CPU u64 の下位 32bit、M-3）
    pub seed: u32,
    /// 光点数 = `(strength * 200) as usize`（#134）。
    /// CPU の点数と完全に一致させるため float から再計算せず int として渡す。
    pub count: i32,
}

/// photophobia フィルタの uniform。
///
/// bloom 半径は画像サイズ依存（CPU 実装と同じ式）なので strength のみでは表せない。
/// disk blur フィルタ（myopia 等）と同様に `radius_px` と `texel_size` を持つ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotophobiaUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
    /// bloom 半径（ピクセル単位）= strength * 0.05 * min(width, height)。
    /// photophobia.frag の `uRadiusPx` に渡す。
    pub radius_px: f32,
    /// テクセルサイズ vec2(1.0/width, 1.0/height)。
    /// photophobia.frag の `uTexelSize` に渡す。
    pub texel_size: [f32; 2],
}

/// photophobia の uniform を返す。
///
/// `width`, `height`: 画像の幅・高さ（ピクセル）。bloom 半径と texel size の算出に使う。
pub fn photophobia_uniforms(strength: f32, width: u32, height: u32) -> PhotophobiaUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let min_dim = width.min(height) as f32;
    let radius_px = strength.clamp(0.0, 1.0) * PHOTOPHOBIA_BLOOM_RADIUS_RATIO * min_dim;
    PhotophobiaUniforms {
        strength,
        radius_px,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
    }
}

/// nyctalopia の uniform を返す。
pub fn nyctalopia_uniforms(strength: f32) -> SimpleStrengthUniforms {
    let strength = crate::vision::normalize_strength(strength);
    SimpleStrengthUniforms { strength }
}

/// cataract フィルタの uniform。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CataractUniforms {
    /// strength (0.0..=1.0)
    pub strength: f32,
    /// ランダムシード（CPU u64 の下位 32bit）。
    /// cataract.frag の `uniform uint uSeed` に渡す。
    pub seed: u32,
}

/// cataract の uniform を返す。
///
/// `seed`: CPU 実装 `vision::cataract` に渡す u64 シードと同じ値を渡すこと。
pub fn cataract_uniforms(strength: f32, seed: u64) -> CataractUniforms {
    let strength = crate::vision::normalize_strength(strength);
    CataractUniforms {
        strength,
        seed: seed as u32,
    }
}

/// glaucoma.frag の GLSL ES 3.00 ソースを返す。
pub fn glaucoma_glsl() -> &'static str {
    include_str!("shaders/glaucoma.frag")
}

/// macular_degeneration.frag の GLSL ES 3.00 ソースを返す。
pub fn macular_degeneration_glsl() -> &'static str {
    include_str!("shaders/macular_degeneration.frag")
}

/// hemianopia.frag の GLSL ES 3.00 ソースを返す。
pub fn hemianopia_glsl() -> &'static str {
    include_str!("shaders/hemianopia.frag")
}

/// tunnel_vision.frag の GLSL ES 3.00 ソースを返す。
pub fn tunnel_vision_glsl() -> &'static str {
    include_str!("shaders/tunnel_vision.frag")
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
    /// **ぼかし方向**の軸角度（度数法）。
    /// `astigmatism_uniforms()` が入力の「シャープ方向」から +90° した値を設定済み。
    /// シェーダ (`astigmatism.frag`) の `uAxisDeg` に直接渡す。
    pub axis_deg: f32,
}

// ---------------------------------------------------------------------------
// uniform 計算
// ---------------------------------------------------------------------------

/// protanopia の uniform を計算する。
pub fn protanopia_uniforms(strength: f32) -> ColorMatrixUniforms {
    let strength = crate::vision::normalize_strength(strength);
    ColorMatrixUniforms {
        strength,
        matrix: PROTANOPIA_MATRIX,
    }
}

/// deuteranopia の uniform を計算する。
pub fn deuteranopia_uniforms(strength: f32) -> ColorMatrixUniforms {
    let strength = crate::vision::normalize_strength(strength);
    ColorMatrixUniforms {
        strength,
        matrix: DEUTERANOPIA_MATRIX,
    }
}

/// tritanopia の uniform を計算する。
pub fn tritanopia_uniforms(strength: f32) -> ColorMatrixUniforms {
    let strength = crate::vision::normalize_strength(strength);
    ColorMatrixUniforms {
        strength,
        matrix: TRITANOPIA_MATRIX,
    }
}

/// achromatopsia の uniform を計算する。
pub fn achromatopsia_uniforms(strength: f32) -> LumaUniforms {
    let strength = crate::vision::normalize_strength(strength);
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
    let strength = crate::vision::normalize_strength(strength);
    let radius_px = strength.clamp(0.0, 1.0) * MYOPIA_MAX_RADIUS_RATIO * image_min_dim as f32;
    BlurUniforms {
        strength,
        radius_px,
    }
}

/// hyperopia の uniform を計算する。
///
/// `image_min_dim`: 画像の `min(width, height)`（ピクセル）。
pub fn hyperopia_uniforms(strength: f32, image_min_dim: u32) -> BlurUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let radius_px = strength.clamp(0.0, 1.0) * HYPEROPIA_MAX_RADIUS_RATIO * image_min_dim as f32;
    BlurUniforms {
        strength,
        radius_px,
    }
}

/// presbyopia の uniform を計算する。
///
/// `image_min_dim`: 画像の `min(width, height)`（ピクセル）。
pub fn presbyopia_uniforms(strength: f32, image_min_dim: u32) -> BlurUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let radius_px = strength.clamp(0.0, 1.0) * PRESBYOPIA_MAX_RADIUS_RATIO * image_min_dim as f32;
    BlurUniforms {
        strength,
        radius_px,
    }
}

/// astigmatism の uniform を計算する。
///
/// `image_min_dim`: 画像の `min(width, height)`（ピクセル）。
/// `axis_deg`: **シャープ方向**の軸角度（度数法。0°=水平, 90°=垂直）。
///   vision::astigmatism() と同じ規約で、**ぼかし方向 = axis_deg + 90°**。
///   シェーダ (`astigmatism.frag`) の `uAxisDeg` uniform にはぼかし方向を渡す。
///   呼び出し元は vision/refraction.rs と同じ「シャープ方向」で渡せばよい。
pub fn astigmatism_uniforms(
    strength: f32,
    image_min_dim: u32,
    axis_deg: f32,
) -> AstigmatismUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let radius_px = strength.clamp(0.0, 1.0) * ASTIGMATISM_MAX_RADIUS_RATIO * image_min_dim as f32;
    // vision/refraction.rs と同じ規約: axis_deg はシャープ方向。ぼかし方向は +90°。
    // normalize_axis_deg で NaN→90°フォールバック・rem_euclid(180.0) 正規化してから
    // +90° する（Issue #169: 正規化なしで NaN を素通しすると cos/sin=NaN で全 tap
    // 不採用 = 黒画像になり、CPU 側の 90° フォールバックと分岐してしまう）。
    let blur_axis_deg = crate::vision::normalize_axis_deg(axis_deg) + 90.0;
    AstigmatismUniforms {
        strength,
        radius_px,
        axis_deg: blur_axis_deg,
    }
}

/// 複視フィルタの uniform。
#[derive(Debug, Clone)]
pub struct DiplopiaUniforms {
    pub strength: f32,
    /// dx（テクセル単位 = dx_px / width）
    pub offset_x_texel: f32,
    /// dy（テクセル単位 = dy_px / height）
    pub offset_y_texel: f32,
    pub ghost_strength: f32,
}

/// 眼振フィルタの uniform。
#[derive(Debug, Clone)]
pub struct NystagmusUniforms {
    pub strength: f32,
    pub radius_px: f32,
    pub direction_deg: f32,
}

/// スターバーストフィルタの uniform。
#[derive(Debug, Clone)]
pub struct StarburstsUniforms {
    pub strength: f32,
    pub threshold: f32,
    pub dispersion: f32,
    /// レイ本数（CPU `num_rays` と同値）。starbursts.frag の `uNumRays` に渡す。
    pub num_rays: f32,
    /// レイ長（ピクセル, CPU `ray_length_px` と同値）。`uRayLengthPx` に渡す。
    pub ray_length_px: f32,
    /// テクセルサイズ vec2(1.0/width, 1.0/height)。`uTexelSize` に渡す。
    pub texel_size: [f32; 2],
}

/// diplopia の uniform を計算する。
pub fn diplopia_uniforms(
    strength: f32,
    offset_x_px: f32,
    offset_y_px: f32,
    ghost_strength: f32,
    width: u32,
    height: u32,
) -> DiplopiaUniforms {
    let strength = crate::vision::normalize_strength(strength);
    DiplopiaUniforms {
        strength,
        offset_x_texel: offset_x_px / width as f32,
        offset_y_texel: offset_y_px / height as f32,
        ghost_strength,
    }
}

/// nystagmus の uniform を計算する。
pub fn nystagmus_uniforms(
    strength: f32,
    amplitude: f32,
    direction_deg: f32,
    image_min_dim: u32,
) -> NystagmusUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let radius_px = amplitude.clamp(0.0, 1.0) * strength.clamp(0.0, 1.0) * image_min_dim as f32;
    NystagmusUniforms {
        strength,
        radius_px,
        direction_deg,
    }
}

/// starbursts の uniform を計算する。
///
/// `num_rays` / `ray_length_ratio` は CPU `vision::starbursts` と同じ意味。
/// `ray_length_px` は CPU と同様 `(ray_length_ratio.clamp(0,1) * min(W,H)) as u32` で算出し、
/// f32 へ変換して GLSL に渡す（GLSL は float seed/整数 uniform を避ける方針）。
/// gather 型の starbursts.frag はレイの逆方向サンプリングに texel size を必要とする。
pub fn starbursts_uniforms(
    strength: f32,
    threshold: f32,
    dispersion: f32,
    num_rays: u32,
    ray_length_ratio: f32,
    width: u32,
    height: u32,
) -> StarburstsUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let min_dim = width.min(height) as f32;
    let ray_length_px = (ray_length_ratio.clamp(0.0, 1.0) * min_dim) as u32;
    StarburstsUniforms {
        strength,
        threshold,
        dispersion,
        num_rays: num_rays as f32,
        ray_length_px: ray_length_px as f32,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
    }
}

/// 視野欠損（vignette 系）フィルタの uniform。
/// tunnel_vision / macular_degeneration 共通。
#[derive(Debug, Clone)]
pub struct FieldOfVisionUniforms {
    pub strength: f32,
    /// アスペクト比（width / height）。GLSL シェーダの `uAspect` uniform に渡す。
    /// 距離計算で UV 空間を aspect 補正して Rust 実装（pixel 座標）と一致させる。
    pub aspect: f32,
}

/// glaucoma 専用の uniform。Vignette に加えて弧状暗点モード（`uMode`）を持つ。
#[derive(Debug, Clone)]
pub struct GlaucomaUniforms {
    pub strength: f32,
    /// アスペクト比（width / height）。GLSL シェーダの `uAspect` uniform に渡す。
    pub aspect: f32,
    /// glaucoma.frag の `uMode` uniform。
    /// 0=Vignette, 1=ArcuateSuperior, 2=ArcuateInferior, 3=Biarcuate。
    pub mode: i32,
}

/// 半盲フィルタの uniform。
#[derive(Debug, Clone)]
pub struct HemianopiaUniforms {
    pub strength: f32,
    /// side: GLSL 内部値。1.0 = 右欠損, -1.0 = 左欠損。
    /// 公開 API (vision::hemianopia) の side とは規約が異なる:
    /// 公開 API は 0.0=左欠損, 1.0=右欠損 で渡し、シェーダ内で変換する。
    pub side: f32,
}

/// glaucoma の uniform を計算する。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正に使用。
/// `mode`: 暗点モード（[`crate::vision::GlaucomaMode`]）。`uMode` uniform に渡す。
pub fn glaucoma_uniforms(
    strength: f32,
    width: u32,
    height: u32,
    mode: crate::vision::GlaucomaMode,
) -> GlaucomaUniforms {
    let strength = crate::vision::normalize_strength(strength);
    GlaucomaUniforms {
        strength,
        aspect: width as f32 / height as f32,
        mode: mode.to_glsl_mode(),
    }
}

/// macular_degeneration の uniform を計算する。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正に使用。
pub fn macular_degeneration_uniforms(
    strength: f32,
    width: u32,
    height: u32,
) -> FieldOfVisionUniforms {
    let strength = crate::vision::normalize_strength(strength);
    FieldOfVisionUniforms {
        strength,
        aspect: width as f32 / height as f32,
    }
}

/// tunnel_vision の uniform を計算する。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正に使用。
pub fn tunnel_vision_uniforms(strength: f32, width: u32, height: u32) -> FieldOfVisionUniforms {
    let strength = crate::vision::normalize_strength(strength);
    FieldOfVisionUniforms {
        strength,
        aspect: width as f32 / height as f32,
    }
}

/// hemianopia の uniform を計算する。
///
/// `side`: GLSL 内部値。1.0 = 右欠損, -1.0 = 左欠損。
/// 公開 API (`vision::hemianopia`) の side とは規約が異なる:
/// 公開 API は 0.0=左欠損, 1.0=右欠損 で渡し、シェーダ内で変換する。
pub fn hemianopia_uniforms(strength: f32, side: f32) -> HemianopiaUniforms {
    let strength = crate::vision::normalize_strength(strength);
    HemianopiaUniforms { strength, side }
}

// ---------------------------------------------------------------------------
// tetrachromacy / vertigo / bppv_rotation / vestibular_neuritis / floaters (#48)
// ---------------------------------------------------------------------------

/// tetrachromacy.frag の GLSL ES 3.00 ソースを返す。
pub fn tetrachromacy_glsl() -> &'static str {
    include_str!("shaders/tetrachromacy.frag")
}

/// vestibular_neuritis.frag の GLSL ES 3.00 ソースを返す。
pub fn vestibular_neuritis_glsl() -> &'static str {
    include_str!("shaders/vestibular_neuritis.frag")
}

/// vertigo.frag の GLSL ES 3.00 ソースを返す。
pub fn vertigo_glsl() -> &'static str {
    include_str!("shaders/vertigo.frag")
}

/// bppv_rotation.frag の GLSL ES 3.00 ソースを返す。
pub fn bppv_rotation_glsl() -> &'static str {
    include_str!("shaders/bppv_rotation.frag")
}

/// floaters.frag の GLSL ES 3.00 ソースを返す。
pub fn floaters_glsl() -> &'static str {
    include_str!("shaders/floaters.frag")
}

/// tetrachromacy フィルタの uniform。
#[derive(Debug, Clone)]
pub struct TetrachromacyUniforms {
    pub strength: f32,
}

/// vestibular_neuritis フィルタの uniform。
///
/// ### CPU/GLSL シフト定義の対応関係
/// - CPU: `shift_x = strength * 0.05 * width` ピクセル（`vision::vestibular_neuritis` 参照）
/// - GLSL: `shift_texel = strength * 0.05`（テクセル単位）= `shift_x / width` と等価
/// - blur 方式: CPU は `ellipse_blur`（1D box フィルタ）、GLSL は固定 16-tap 水平ブラー。
///   両者は手法が異なるが PSNR ≥ 30 dB で等価と確認済み。
#[derive(Debug, Clone)]
pub struct VestibularNeuritisUniforms {
    pub strength: f32,
    /// 水平 blur 半径（ピクセル単位）= strength * 0.04 * width
    pub radius_px: f32,
    /// 水平シフト（テクセル単位）= strength * 0.05
    pub shift_texel: f32,
    /// テクセルサイズ (1/width, 1/height)
    pub texel_size: [f32; 2],
}

/// vertigo フィルタの uniform。
#[derive(Debug, Clone)]
pub struct VertigoUniforms {
    pub strength: f32,
    pub time: f32,
    /// アスペクト比（width / height）。GLSL の `uAspect`。回転を
    /// ピクセル比例空間で行い、非正方形でも CPU 実装と一致させる。
    pub aspect: f32,
    /// disk blur 半径（ピクセル単位）。GLSL の `uRadiusPx`。
    /// `strength * 0.015 * min(width, height)`。
    pub radius_px: f32,
    /// テクセルサイズ vec2(1/width, 1/height)。GLSL の `uTexelSize`。
    pub texel_size: [f32; 2],
}

/// bppv_rotation フィルタの uniform。
#[derive(Debug, Clone)]
pub struct BppvRotationUniforms {
    pub strength: f32,
    pub time: f32,
    /// アスペクト比（width / height）。GLSL の `uAspect`。回転を
    /// ピクセル比例空間で行い、非正方形でも CPU 実装と一致させる。
    pub aspect: f32,
}

/// floaters フィルタの uniform（#134, 方針 B）。
///
/// マスク（blob+strand+blur）は [`crate::vision::floaters_mask`] で生成して `uMask`
/// テクスチャとして渡すため、uniform は strength のみ。density/seed/gaze はマスク生成側に渡す。
#[derive(Debug, Clone)]
pub struct FloatersUniforms {
    pub strength: f32,
}

/// tetrachromacy の uniform を計算する。
pub fn tetrachromacy_uniforms(strength: f32) -> TetrachromacyUniforms {
    let strength = crate::vision::normalize_strength(strength);
    TetrachromacyUniforms { strength }
}

/// vestibular_neuritis の uniform を計算する。
pub fn vestibular_neuritis_uniforms(
    strength: f32,
    width: u32,
    height: u32,
) -> VestibularNeuritisUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let radius_px = strength.clamp(0.0, 1.0) * 0.04 * width as f32;
    let shift_texel = strength.clamp(0.0, 1.0) * 0.05;
    VestibularNeuritisUniforms {
        strength,
        radius_px,
        shift_texel,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
    }
}

/// vertigo の uniform を計算する。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正と disk blur
/// 半径（`strength * 0.015 * min(width, height)`）の算出に使う。
pub fn vertigo_uniforms(strength: f32, time: f32, width: u32, height: u32) -> VertigoUniforms {
    let strength = crate::vision::normalize_strength(strength);
    let min_dim = width.min(height) as f32;
    // CPU 実装 (vision::normalize_strength) は NaN を 0 (identity) として扱う。
    // clamp 単体は NaN を NaN のまま返すため、ここでも NaN→0 に正規化して
    // radius_px が NaN にならないようにする。
    let strength_norm = if strength.is_nan() {
        0.0
    } else {
        strength.clamp(0.0, 1.0)
    };
    VertigoUniforms {
        strength,
        time,
        aspect: width as f32 / height as f32,
        radius_px: strength_norm * 0.015 * min_dim,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
    }
}

/// bppv_rotation の uniform を計算する。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正に使う。
pub fn bppv_rotation_uniforms(
    strength: f32,
    time: f32,
    width: u32,
    height: u32,
) -> BppvRotationUniforms {
    let strength = crate::vision::normalize_strength(strength);
    BppvRotationUniforms {
        strength,
        time,
        aspect: width as f32 / height as f32,
    }
}

/// floaters の uniform を計算する。
///
/// マスクは [`crate::vision::floaters_mask`] で別途生成し `uMask` テクスチャで渡すこと。
pub fn floaters_uniforms(strength: f32) -> FloatersUniforms {
    let strength = crate::vision::normalize_strength(strength);
    FloatersUniforms { strength }
}

// ---------------------------------------------------------------------------
// Metamorphopsia (#55)
// ---------------------------------------------------------------------------

/// metamorphopsia.frag の GLSL ソースコードを返す。
pub fn metamorphopsia_glsl() -> &'static str {
    include_str!("shaders/metamorphopsia.frag")
}

/// Metamorphopsia シェーダーの uniform 値。
///
/// CPU 実装 `vision::metamorphopsia` と同一のノイズモデル（#99 で統一）。
/// グリッド頂点ごとの変位を 32bit 整数 spatial hash（`seed` + 頂点座標）で生成し、
/// `metamorphopsia.frag` は `uTexelSize` から解像度を復元してグリッド頂点座標を
/// CPU と同じ整数ピクセル基準で計算する。
#[derive(Debug, Clone, PartialEq)]
pub struct MetamorphopsiaUniforms {
    /// 歪み強度（0.0..=1.0）
    pub strength: f32,
    /// 空間周波数（グリッド分割数）
    pub freq: f32,
    /// 32bit spatial hash シード（u64 シードの下位 32bit。uint で精度損失なく渡す）
    pub seed: u32,
    /// テクセルサイズ (1/width, 1/height)
    pub texel_size: [f32; 2],
}

/// metamorphopsia の uniform を計算する。
///
/// `freq`: 空間周波数（グリッド分割数）。
/// `seed`: LCG シード。
/// `width`, `height`: 画像サイズ（ピクセル）。
pub fn metamorphopsia_uniforms(
    strength: f32,
    freq: f32,
    seed: u64,
    width: u32,
    height: u32,
) -> MetamorphopsiaUniforms {
    let strength = crate::vision::normalize_strength(strength);
    MetamorphopsiaUniforms {
        strength,
        freq,
        seed: seed as u32,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
    }
}

/// depth_aware_blur.frag の GLSL ES 3.00 ソースを返す（#107）。
///
/// 深度マップを第 2 テクスチャ（`uDepth`）として渡す単一パスシェーダ。
/// `uDepth` の `.r` を深度（明=近、暗=遠）として読む（CPU の `to_luma8` 相当の
/// grayscale 深度を host 側で渡すこと）。CPU 実装 `vision::depth_aware_blur` は
/// 8 段階ビン box blur の多パス方式なので、本シェーダ（Fibonacci 16 tap disk）とは
/// アルゴリズムが異なり bit/PSNR 等価ではない。効果（ピント面鮮明・離れるほどぼける）で担保する。
pub fn depth_aware_blur_glsl() -> &'static str {
    include_str!("shaders/depth_aware_blur.frag")
}

/// depth_aware_blur フィルタの uniform（#107）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthAwareBlurUniforms {
    /// ピント深度（0.0..=1.0）。depth_aware_blur.frag の `uFocusDepth`。
    pub focus_depth: f32,
    /// 最大ぼけ半径（ピクセル単位）= `max_radius_ratio * min(width, height)`。`uMaxRadiusPx`。
    pub max_radius_px: f32,
    /// ぼけ種別。0=Myopia(遠方ボケ), 1=Hyperopia(近方ボケ), 2=DepthOfField(両側)。`uKind`。
    pub kind: i32,
    /// テクセルサイズ vec2(1.0/width, 1.0/height)。`uTexelSize`。
    pub texel_size: [f32; 2],
}

/// depth_aware_blur の uniform を返す。
///
/// `focus_depth`: ピント深度（0.0..=1.0）。
/// `max_radius_ratio`: 最大ぼけ半径（min(W,H) 比）。CPU `vision::depth_aware_blur` に
///   渡す値と同じものを渡すこと（CLI は `strength * 0.023`）。
/// `kind`: [`crate::vision::DepthBlurKind`]。
pub fn depth_aware_blur_uniforms(
    focus_depth: f32,
    max_radius_ratio: f32,
    kind: crate::vision::DepthBlurKind,
    width: u32,
    height: u32,
) -> DepthAwareBlurUniforms {
    use crate::vision::DepthBlurKind;
    let min_dim = width.min(height) as f32;
    let kind_i = match kind {
        DepthBlurKind::Myopia => 0,
        DepthBlurKind::Hyperopia => 1,
        DepthBlurKind::DepthOfField => 2,
    };
    DepthAwareBlurUniforms {
        focus_depth,
        max_radius_px: max_radius_ratio * min_dim,
        kind: kind_i,
        texel_size: [1.0 / width as f32, 1.0 / height as f32],
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
    fn astigmatism_uniforms_blur_axis_is_sharp_plus_90() {
        // vision/refraction.rs 規約: axis_deg はシャープ方向。ぼかし方向 = axis_deg + 90°
        let u = astigmatism_uniforms(1.0, 1000, 45.0);
        assert!(
            (u.axis_deg - 135.0).abs() < 1e-4,
            "45° シャープ → 135° ぼかし"
        );
        let u2 = astigmatism_uniforms(1.0, 1000, 90.0);
        assert!(
            (u2.axis_deg - 180.0).abs() < 1e-4,
            "90° シャープ → 180° ぼかし"
        );
    }

    #[test]
    fn astigmatism_uniforms_strength_zero_has_zero_radius() {
        let u = astigmatism_uniforms(0.0, 1000, 90.0);
        assert!(u.radius_px < 0.001);
    }

    #[test]
    fn astigmatism_uniforms_nan_axis_matches_cpu_normalization() {
        // Issue #169: NaN 軸は CPU 側 (vision::refraction::normalize_axis_deg) と同じ
        // 90° フォールバックを経て +90° されるべき（= 180.0）。正規化せず NaN を
        // 素通しすると GLSL 側は cos/sin=NaN で黒画像になり CPU (90° フォールバック)
        // と分岐してしまう。
        let u = astigmatism_uniforms(1.0, 1000, f32::NAN);
        assert!(
            (u.axis_deg - 180.0).abs() < 1e-4,
            "NaN axis should fall back to 90° sharp → 180° blur, got {}",
            u.axis_deg
        );
    }

    #[test]
    fn astigmatism_uniforms_matches_cpu_normalize_axis_deg() {
        // GLSL uniform 側の実効軸 (正規化後 - 90°) が CPU 側の
        // crate::vision::normalize_axis_deg と byte-exact で一致することを、
        // NaN / 負値 / 360° 超の代表値で直接クロスチェックする。
        for axis in [f32::NAN, -45.0, 0.0, 90.0, 179.9, 360.0, 405.0] {
            let u = astigmatism_uniforms(1.0, 1000, axis);
            let expected_cpu_norm = crate::vision::normalize_axis_deg(axis);
            assert_eq!(
                u.axis_deg - 90.0,
                expected_cpu_norm,
                "axis_deg={axis}: GLSL effective axis must equal CPU normalize_axis_deg()"
            );
        }
    }

    #[test]
    fn astigmatism_uniforms_axis_is_180_periodic() {
        // 負値 / 360° 超も CPU 側 rem_euclid(180.0) と同じ結果になること。
        let u_neg = astigmatism_uniforms(1.0, 1000, -45.0); // -45 rem_euclid 180 = 135
        assert!(
            (u_neg.axis_deg - 225.0).abs() < 1e-4,
            "axis=-45° should normalize to 135° sharp → 225° blur, got {}",
            u_neg.axis_deg
        );

        let u_over = astigmatism_uniforms(1.0, 1000, 405.0); // 405 rem_euclid 180 = 45
        assert!(
            (u_over.axis_deg - 135.0).abs() < 1e-4,
            "axis=405° should normalize to 45° sharp → 135° blur, got {}",
            u_over.axis_deg
        );
    }

    #[test]
    fn hyperopia_uniforms_strength_one_correct_radius() {
        let u = hyperopia_uniforms(1.0, 800);
        let expected = HYPEROPIA_MAX_RADIUS_RATIO * 800.0;
        assert!((u.radius_px - expected).abs() < 1e-4);
    }

    #[test]
    fn presbyopia_uniforms_strength_one_correct_radius() {
        let u = presbyopia_uniforms(1.0, 800);
        let expected = PRESBYOPIA_MAX_RADIUS_RATIO * 800.0;
        assert!((u.radius_px - expected).abs() < 1e-4);
    }

    #[test]
    fn astigmatism_uniforms_radius_matches_formula() {
        let s = 0.75_f32;
        let dim = 800_u32;
        let u = astigmatism_uniforms(s, dim, 0.0);
        let expected = s * ASTIGMATISM_MAX_RADIUS_RATIO * 800.0;
        assert!((u.radius_px - expected).abs() < 1e-4);
    }

    #[test]
    fn glaucoma_glsl_is_not_empty() {
        assert!(!glaucoma_glsl().is_empty());
    }

    #[test]
    fn macular_degeneration_glsl_is_not_empty() {
        assert!(!macular_degeneration_glsl().is_empty());
    }

    #[test]
    fn hemianopia_glsl_is_not_empty() {
        assert!(!hemianopia_glsl().is_empty());
    }

    #[test]
    fn tunnel_vision_glsl_is_not_empty() {
        assert!(!tunnel_vision_glsl().is_empty());
    }

    #[test]
    fn glaucoma_uniforms_strength_one() {
        let u = glaucoma_uniforms(1.0, 32, 32, crate::vision::GlaucomaMode::Vignette);
        assert_eq!(u.strength, 1.0);
        assert!((u.aspect - 1.0).abs() < 1e-6, "正方形は aspect=1.0");
        assert_eq!(u.mode, 0, "Vignette は uMode=0");
    }

    #[test]
    fn glaucoma_uniforms_mode_mapping() {
        use crate::vision::GlaucomaMode;
        assert_eq!(
            glaucoma_uniforms(1.0, 32, 32, GlaucomaMode::Vignette).mode,
            0
        );
        assert_eq!(
            glaucoma_uniforms(1.0, 32, 32, GlaucomaMode::ArcuateSuperior).mode,
            1
        );
        assert_eq!(
            glaucoma_uniforms(1.0, 32, 32, GlaucomaMode::ArcuateInferior).mode,
            2
        );
        assert_eq!(
            glaucoma_uniforms(1.0, 32, 32, GlaucomaMode::Biarcuate).mode,
            3
        );
    }

    #[test]
    fn macular_degeneration_uniforms_strength_one() {
        let u = macular_degeneration_uniforms(1.0, 64, 32);
        assert_eq!(u.strength, 1.0);
        assert!((u.aspect - 2.0).abs() < 1e-6, "横長は aspect=2.0");
    }

    #[test]
    fn tunnel_vision_uniforms_strength_one() {
        let u = tunnel_vision_uniforms(1.0, 32, 32);
        assert_eq!(u.strength, 1.0);
    }

    #[test]
    fn hemianopia_uniforms_side_values() {
        let u = hemianopia_uniforms(1.0, 1.0);
        assert_eq!(u.side, 1.0);
        let u2 = hemianopia_uniforms(1.0, -1.0);
        assert_eq!(u2.side, -1.0);
    }

    #[test]
    fn depth_aware_blur_glsl_is_not_empty() {
        assert!(depth_aware_blur_glsl().contains("uDepth"));
        assert!(depth_aware_blur_glsl().contains("void main"));
    }

    #[test]
    fn floaters_glsl_samples_mask_texture() {
        // #134 方針 B: floaters.frag は uMask テクスチャを参照する
        assert!(floaters_glsl().contains("uMask"));
        assert_eq!(floaters_uniforms(0.7).strength, 0.7);
    }

    #[test]
    fn uniforms_normalize_strength_like_cpu() {
        // #120: uniforms は CPU と同じく strength を 0..1 clamp / NaN→0 する。
        // 範囲外・NaN を渡す将来の呼び出し元で CPU(uStrength) と乖離しないこと。
        assert_eq!(eye_strain_uniforms(2.0, 32, 32).strength, 1.0);
        assert_eq!(eye_strain_uniforms(-1.0, 32, 32).strength, 0.0);
        assert_eq!(eye_strain_uniforms(f32::NAN, 32, 32).strength, 0.0);
        assert_eq!(photophobia_uniforms(5.0, 32, 32).strength, 1.0);
        assert_eq!(cataract_uniforms(f32::NAN, 0).strength, 0.0);
        assert_eq!(protanopia_uniforms(1.5).strength, 1.0);
        assert_eq!(flickering_stars_uniforms(2.0, 0).strength, 1.0);
        // 有効範囲は素通し
        assert_eq!(eye_strain_uniforms(0.5, 32, 32).strength, 0.5);
    }

    #[test]
    fn depth_aware_blur_uniforms_kind_and_radius() {
        use crate::vision::DepthBlurKind;
        let u = depth_aware_blur_uniforms(0.5, 0.023, DepthBlurKind::Myopia, 100, 200);
        assert_eq!(u.kind, 0);
        assert_eq!(u.focus_depth, 0.5);
        // max_radius_px = ratio * min(w,h) = 0.023 * 100
        assert!((u.max_radius_px - 2.3).abs() < 1e-5);
        assert!((u.texel_size[0] - 0.01).abs() < 1e-6);
        assert_eq!(
            depth_aware_blur_uniforms(0.5, 0.023, DepthBlurKind::Hyperopia, 100, 200).kind,
            1
        );
        assert_eq!(
            depth_aware_blur_uniforms(0.5, 0.023, DepthBlurKind::DepthOfField, 100, 200).kind,
            2
        );
    }
}
