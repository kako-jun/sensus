//! 色覚特性フィルタ。
//!
//! Machado 2009 行列ベースの 2 色覚 3 種 + 全色盲 + 四色覚を実装する。
//! `apply_machado_matrix` は色覚 3 種専用ヘルパーのため本モジュールに置く。

use super::*;
use crate::Result;
use image::{DynamicImage, RgbaImage};

const PROTANOPIA: [[f32; 3]; 3] = [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
];

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典: 上記 [`PROTANOPIA`] と同じ。
const DEUTERANOPIA: [[f32; 3]; 3] = [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
];

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典: 上記 [`PROTANOPIA`] と同じ。
const TRITANOPIA: [[f32; 3]; 3] = [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
];

/// BT.709 / sRGB photopic luminance 係数（CIE Y）。
pub(crate) const LUMA_R: f32 = 0.2126;

pub(crate) const LUMA_G: f32 = 0.7152;

pub(crate) const LUMA_B: f32 = 0.0722;

/// Protanopia (1 型 2 色覚, L 錐体欠損 / 赤盲) シミュレーション。
///
/// `strength` を Machado 2009 severity (0.0..=1.0) として扱い、範囲外は clamp する。
/// `0.0` は元画像と同一、`1.0` で完全 dichromacy。
pub fn protanopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &PROTANOPIA, strength)
}

/// Deuteranopia (2 型 2 色覚, M 錐体欠損 / 緑盲) シミュレーション。
///
/// `strength` の意味は [`protanopia`] と同じ。
pub fn deuteranopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &DEUTERANOPIA, strength)
}

/// Tritanopia (3 型 2 色覚, S 錐体欠損 / 青盲) シミュレーション。
///
/// `strength` の意味は [`protanopia`] と同じ。
pub fn tritanopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &TRITANOPIA, strength)
}

/// Achromatopsia (全色盲) シミュレーション。
///
/// LMS 経路ではなく、BT.709 photopic luminance によるグレースケール化を行う。
/// `strength = 1.0` で完全グレースケール (R == G == B)。`strength = 0.0` で原画像。
/// 中間値は linear sRGB 空間で線形補間。
pub fn achromatopsia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    // strength を正規化（NaN→0 / clamp 0..1）。CPU 全段共通の正規化を使う（#113）。
    let strength = normalize_strength(strength);
    let mut rgba = img.to_rgba8();

    // strength == 0.0 のショートカット（元画像と完全一致を保証）。
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);

        let y = LUMA_R * r + LUMA_G * g + LUMA_B * b;

        // linear 空間で原色 → 完全グレースケールへブレンド
        let nr = r + (y - r) * strength;
        let ng = g + (y - g) * strength;
        let nb = b + (y - b) * strength;

        px[0] = pack_u8(linear_to_srgb(nr));
        px[1] = pack_u8(linear_to_srgb(ng));
        px[2] = pack_u8(linear_to_srgb(nb));
        // alpha (px[3]) はそのまま保持
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

/// linear sRGB 上で 3x3 行列を掛けたシミュレーション結果と原色を、
/// strength で linear blend する内部実装。
///
/// 行列は LMS 空間のものではなく、Machado 2009 がプリ計算した
/// linear sRGB → simulated linear sRGB の severity = 1.0 行列。
fn apply_machado_matrix(
    img: DynamicImage,
    matrix: &[[f32; 3]; 3],
    strength: f32,
) -> Result<DynamicImage> {
    // strength を正規化（NaN→0 / clamp 0..1）。CPU 全段共通の正規化を使う（#113）。
    let strength = normalize_strength(strength);
    let mut rgba: RgbaImage = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);

        let sr = matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b;
        let sg = matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b;
        let sb = matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b;

        // strength で linear blend（0.0 = 原色, 1.0 = 完全 dichromacy）
        let nr = r + (sr - r) * strength;
        let ng = g + (sg - g) * strength;
        let nb = b + (sb - b) * strength;

        px[0] = pack_u8(linear_to_srgb(nr));
        px[1] = pack_u8(linear_to_srgb(ng));
        px[2] = pack_u8(linear_to_srgb(nb));
        // alpha はそのまま
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

// ---------------------------------------------------------------
// Phase 1+: 四色型色覚 (Issue #3) — tetrachromacy
// ---------------------------------------------------------------

/// 四色型色覚（Tetrachromacy）可視化。
///
/// RGB 画像は分光情報を失っているため完全な四色型シミュレーションは不可能。
/// メタメリズムベースのアルゴリズムで、L/M 錐体の差分が小さい領域（メタメリック
/// ペア候補）を検出し、その領域の Cb/Cr 色差を追加誇張する。
/// 全領域には赤-緑 opponent channel の基本誇張も適用する。
///
/// 本フィルタは測色的忠実度を主張しない可視化演出であり、メタメリックペア候補の
/// 検出に使う L/M 値も真の錐体刺激値ではなくヒューリスティックな代理量である
/// （詳細は `HPE_LMS_HEURISTIC` の doc コメントと
/// `docs/adr/matrix-provenance.md` の Heuristic matrices 節を参照）。
///
/// ## アルゴリズム（Hunt-Pointer-Estévez 行列を linear RGB に直接流用するヒューリスティック）
///
/// 1. linear sRGB に変換（gamma 解除）
/// 2. linear sRGB → 疑似 LMS（Hunt-Pointer-Estévez の XYZ→LMS 変換行列を、
///    sRGB→XYZ を挟まず linear RGB に直接適用するヒューリスティック）
/// 3. M（緑錐体相当）と L（赤錐体相当）の差分 `delta = M - L` を抽出
/// 4. `|delta| < 0.05` の領域 = メタメリックペア候補
/// 5. そのような領域で Cb/Cr（色差）を `strength * 2.0` 倍に誇張
/// 6. 全領域: 赤-緑 opponent channel を基本誇張（strength でスケール）
/// 7. clamp(0.0, 1.0) して linear → sRGB に戻す
/// 8. alpha は保持
///
/// `strength = 0.0` は元画像と完全一致。`strength = 1.0` で最大誇張。
pub fn tetrachromacy(img: DynamicImage, strength: f32) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let mut rgba = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    // Hunt-Pointer-Estévez (HPE) の CIE XYZ→LMS 変換行列（D65 白色点正規化版）。
    //
    // この行列は Machado, Oliveira & Fernandes (2009) の Table 1 には存在しない
    // （同 Table 1 が公開するのは severity=1.0 の CVD 行列 3 種のみ。
    // `docs/adr/matrix-provenance.md` §1 参照）。かつ本来 HPE 行列は CIE XYZ
    // 入力を前提とするが、ここでは sRGB→XYZ 変換を挟まず linear RGB に直接
    // 適用しているためヒューリスティックであり、測色的な LMS 値ではない。
    //
    // tetrachromacy はメタメリック検出の可視化演出であり、測色的忠実度を
    // 主張しない。出典・スコープの詳細は
    // `docs/adr/matrix-provenance.md` の "Heuristic matrices" 節を参照（#170）。
    const HPE_LMS_HEURISTIC: [[f32; 3]; 3] = [
        [0.4002, 0.7076, -0.0808],
        [-0.2263, 1.1653, 0.0457],
        [0.0000, 0.0000, 0.9182],
    ];

    // 基本赤-緑誇張係数（全領域に適用）
    const K_RG: f32 = 0.5;

    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);

        // linear RGB → 疑似 LMS（HPE ヒューリスティック、上記 doc コメント参照）
        let l_cone =
            HPE_LMS_HEURISTIC[0][0] * r + HPE_LMS_HEURISTIC[0][1] * g + HPE_LMS_HEURISTIC[0][2] * b;
        let m_cone =
            HPE_LMS_HEURISTIC[1][0] * r + HPE_LMS_HEURISTIC[1][1] * g + HPE_LMS_HEURISTIC[1][2] * b;

        // M と L の差分（メタメリズム指標）
        let delta = m_cone - l_cone;

        // 全領域: 赤-緑 opponent channel 誇張（既存テスト互換）
        let rg = r - g;
        let mut nr = r + strength * rg * K_RG;
        let mut ng = g - strength * rg * K_RG;
        let mut nb = b;

        // |delta| < 0.05 のメタメリックペア候補領域: Cb/Cr をさらに誇張
        if delta.abs() < 0.05 {
            let y = LUMA_R * r + LUMA_G * g + LUMA_B * b;
            let cb = b - y;
            let cr = r - y;
            let scale = strength * 2.0;
            nr = (y + cr * scale).clamp(0.0, 1.0);
            ng = y.clamp(0.0, 1.0);
            nb = (y + cb * scale).clamp(0.0, 1.0);
        }

        px[0] = pack_u8(linear_to_srgb(nr.clamp(0.0, 1.0)));
        px[1] = pack_u8(linear_to_srgb(ng.clamp(0.0, 1.0)));
        px[2] = pack_u8(linear_to_srgb(nb.clamp(0.0, 1.0)));
        // alpha (px[3]) はそのまま保持
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}
