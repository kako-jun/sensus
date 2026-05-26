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
// vision.rs の PHOTOPHOBIA_BLOOM_RADIUS_RATIO と同じ値（bloom 半径 = ratio * min(W,H) * strength）
const PHOTOPHOBIA_BLOOM_RADIUS_RATIO: f32 = 0.05;

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

/// dry_eye.frag の GLSL ES 3.00 ソースを返す。
pub fn dry_eye_glsl() -> &'static str {
    include_str!("shaders/dry_eye.frag")
}

/// contrast_sensitivity.frag の GLSL ES 3.00 ソースを返す。
pub fn contrast_sensitivity_glsl() -> &'static str {
    include_str!("shaders/contrast_sensitivity.frag")
}

/// contrast_sensitivity の uniform を返す。
pub fn contrast_sensitivity_uniforms(strength: f32) -> SimpleStrengthUniforms {
    SimpleStrengthUniforms { strength }
}

/// detail_loss.frag の GLSL ES 3.00 ソースを返す。
pub fn detail_loss_glsl() -> &'static str {
    include_str!("shaders/detail_loss.frag")
}

/// detail_loss の uniform を返す。
pub fn detail_loss_uniforms(strength: f32) -> SimpleStrengthUniforms {
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
    FlickeringStarsUniforms {
        strength,
        seed: seed as u32,
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
/// `axis_deg`: **シャープ方向**の軸角度（度数法。0°=水平, 90°=垂直）。
///   vision::astigmatism() と同じ規約で、**ぼかし方向 = axis_deg + 90°**。
///   シェーダ (`astigmatism.frag`) の `uAxisDeg` uniform にはぼかし方向を渡す。
///   呼び出し元は vision.rs と同じ「シャープ方向」で渡せばよい。
pub fn astigmatism_uniforms(strength: f32, image_min_dim: u32, axis_deg: f32) -> AstigmatismUniforms {
    let radius_px =
        strength.clamp(0.0, 1.0) * ASTIGMATISM_MAX_RADIUS_RATIO * image_min_dim as f32;
    // vision.rs と同じ規約: axis_deg はシャープ方向。ぼかし方向は +90°。
    let blur_axis_deg = axis_deg + 90.0;
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
    let radius_px = amplitude.clamp(0.0, 1.0) * strength.clamp(0.0, 1.0) * image_min_dim as f32;
    NystagmusUniforms {
        strength,
        radius_px,
        direction_deg,
    }
}

/// starbursts の uniform を計算する。
pub fn starbursts_uniforms(strength: f32, threshold: f32, dispersion: f32) -> StarburstsUniforms {
    StarburstsUniforms { strength, threshold, dispersion }
}

/// 視野欠損（vignette 系）フィルタの uniform。
/// glaucoma / tunnel_vision / macular_degeneration 共通。
#[derive(Debug, Clone)]
pub struct FieldOfVisionUniforms {
    pub strength: f32,
    /// アスペクト比（width / height）。GLSL シェーダの `uAspect` uniform に渡す。
    /// 距離計算で UV 空間を aspect 補正して Rust 実装（pixel 座標）と一致させる。
    pub aspect: f32,
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
pub fn glaucoma_uniforms(strength: f32, width: u32, height: u32) -> FieldOfVisionUniforms {
    FieldOfVisionUniforms {
        strength,
        aspect: width as f32 / height as f32,
    }
}

/// macular_degeneration の uniform を計算する。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正に使用。
pub fn macular_degeneration_uniforms(strength: f32, width: u32, height: u32) -> FieldOfVisionUniforms {
    FieldOfVisionUniforms {
        strength,
        aspect: width as f32 / height as f32,
    }
}

/// tunnel_vision の uniform を計算する。
///
/// `width`, `height`: 画像サイズ（ピクセル）。aspect 補正に使用。
pub fn tunnel_vision_uniforms(strength: f32, width: u32, height: u32) -> FieldOfVisionUniforms {
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
}

/// bppv_rotation フィルタの uniform。
#[derive(Debug, Clone)]
pub struct BppvRotationUniforms {
    pub strength: f32,
    pub time: f32,
}

/// floaters フィルタの uniform。
#[derive(Debug, Clone)]
pub struct FloatersUniforms {
    pub strength: f32,
    pub seed: u32,
}

/// tetrachromacy の uniform を計算する。
pub fn tetrachromacy_uniforms(strength: f32) -> TetrachromacyUniforms {
    TetrachromacyUniforms { strength }
}

/// vestibular_neuritis の uniform を計算する。
pub fn vestibular_neuritis_uniforms(strength: f32, width: u32, height: u32) -> VestibularNeuritisUniforms {
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
pub fn vertigo_uniforms(strength: f32, time: f32) -> VertigoUniforms {
    VertigoUniforms { strength, time }
}

/// bppv_rotation の uniform を計算する。
pub fn bppv_rotation_uniforms(strength: f32, time: f32) -> BppvRotationUniforms {
    BppvRotationUniforms { strength, time }
}

/// floaters の uniform を計算する。
pub fn floaters_uniforms(strength: f32, seed: u64) -> FloatersUniforms {
    FloatersUniforms {
        strength,
        seed: seed as u32,
    }
}

// ---------------------------------------------------------------------------
// Metamorphopsia (#55)
// ---------------------------------------------------------------------------

/// metamorphopsia.frag の GLSL ソースコードを返す。
pub fn metamorphopsia_glsl() -> &'static str {
    include_str!("shaders/metamorphopsia.frag")
}

/// Metamorphopsia シェーダーの uniform 値。
#[derive(Debug, Clone, PartialEq)]
pub struct MetamorphopsiaUniforms {
    /// 歪み強度（0.0..=1.0）
    pub strength: f32,
    /// 空間周波数（グリッド分割数）
    pub freq: f32,
    /// LCG シード（uint で精度損失なく渡す）
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
    MetamorphopsiaUniforms {
        strength,
        freq,
        seed: seed as u32,
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
        // vision.rs 規約: axis_deg はシャープ方向。ぼかし方向 = axis_deg + 90°
        let u = astigmatism_uniforms(1.0, 1000, 45.0);
        assert!((u.axis_deg - 135.0).abs() < 1e-4, "45° シャープ → 135° ぼかし");
        let u2 = astigmatism_uniforms(1.0, 1000, 90.0);
        assert!((u2.axis_deg - 180.0).abs() < 1e-4, "90° シャープ → 180° ぼかし");
    }

    #[test]
    fn astigmatism_uniforms_strength_zero_has_zero_radius() {
        let u = astigmatism_uniforms(0.0, 1000, 90.0);
        assert!(u.radius_px < 0.001);
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
        let u = glaucoma_uniforms(1.0, 32, 32);
        assert_eq!(u.strength, 1.0);
        assert!((u.aspect - 1.0).abs() < 1e-6, "正方形は aspect=1.0");
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
}
