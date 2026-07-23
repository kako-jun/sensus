//! 色覚特性フィルタ。
//!
//! Machado 2009 行列ベースの 2 色覚 3 種 + 全色盲 + 四色覚を実装する。
//! `apply_machado_matrix` は色覚 3 種専用ヘルパーのため本モジュールに置く。

use super::*;
use crate::Result;
use image::{DynamicImage, RgbaImage};

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// #165 以降、実際の severity 解決は [`PROTANOMALY_TABLE`] (11 段) を使う。
/// この const は Machado 2009 の severity=1.0 値を独立に保持し続ける
/// regression anchor（[`PROTANOMALY_TABLE`] の末尾要素と一致することを
/// `protanomaly_table_severity1_matches_legacy_const` で固定する）。
/// 実処理では参照されないため `#[allow(dead_code)]`。
#[allow(dead_code)]
const PROTANOPIA: [[f32; 3]; 3] = [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
];

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典・regression anchor としての位置づけは上記 [`PROTANOPIA`] と同じ
/// （[`DEUTERANOMALY_TABLE`] の末尾要素との一致を固定する）。
#[allow(dead_code)]
const DEUTERANOPIA: [[f32; 3]; 3] = [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
];

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典・regression anchor としての位置づけは上記 [`PROTANOPIA`] と同じ
/// （[`TRITANOMALY_TABLE`] の末尾要素との一致を固定する）。
#[allow(dead_code)]
const TRITANOPIA: [[f32; 3]; 3] = [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
];

/// Protanomaly (1 型色覚異常) の severity 別 11 段行列テーブル。
///
/// `table[i]` は severity `i as f32 / 10.0`（0.0..=1.0, 0.1 刻み）に対応する
/// `linear sRGB → simulated linear sRGB` の 3×3 行列。`table[0]` は単位行列
/// （severity=0.0 で完全 identity）、`table[10]` は上記 [`PROTANOPIA`] と同値
/// （severity=1.0 の完全 dichromacy）。
///
/// 出典: Machado, Oliveira, Fernandes (2009) "A Physiologically-based Model
/// for Simulation of Color Vision Deficiency", IEEE TVCG,
/// DOI: 10.1109/TVCG.2009.113。VIP-Sim (myRecolour.cs) の
/// `T_Protanomaly` と照合済み（詳細は `docs/adr/matrix-provenance.md`）。
pub(crate) const PROTANOMALY_TABLE: [[[f32; 3]; 3]; 11] = [
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    [
        [0.856167, 0.182038, -0.038205],
        [0.029342, 0.955115, 0.015544],
        [-0.00288, -0.001563, 1.004443],
    ],
    [
        [0.734766, 0.334872, -0.069637],
        [0.05184, 0.919198, 0.028963],
        [-0.004928, -0.004209, 1.009137],
    ],
    [
        [0.630323, 0.465641, -0.095964],
        [0.069181, 0.890046, 0.040773],
        [-0.006308, -0.007724, 1.014032],
    ],
    [
        [0.539009, 0.579343, -0.118352],
        [0.082546, 0.866121, 0.051332],
        [-0.007136, -0.011959, 1.019095],
    ],
    [
        [0.458064, 0.679578, -0.137642],
        [0.092785, 0.846313, 0.060902],
        [-0.007494, -0.016807, 1.024301],
    ],
    [
        [0.38545, 0.769005, -0.154455],
        [0.100526, 0.829802, 0.069673],
        [-0.007442, -0.02219, 1.029632],
    ],
    [
        [0.319627, 0.849633, -0.169261],
        [0.106241, 0.815969, 0.07779],
        [-0.007025, -0.028051, 1.035076],
    ],
    [
        [0.259411, 0.923008, -0.18242],
        [0.110296, 0.80434, 0.085364],
        [-0.006276, -0.034346, 1.040622],
    ],
    [
        [0.203876, 0.990338, -0.194214],
        [0.112975, 0.794542, 0.092483],
        [-0.005222, -0.041043, 1.046265],
    ],
    [
        [0.152286, 1.052583, -0.204868],
        [0.114503, 0.786281, 0.099216],
        [-0.003882, -0.048116, 1.051998],
    ],
];

/// Deuteranomaly (2 型色覚異常) の severity 別 11 段行列テーブル。
///
/// 構造・出典は [`PROTANOMALY_TABLE`] と同じ（`T_Deuteranomaly`）。
/// `table[10]` は [`DEUTERANOPIA`] と同値。
pub(crate) const DEUTERANOMALY_TABLE: [[[f32; 3]; 3]; 11] = [
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    [
        [0.866435, 0.177704, -0.044139],
        [0.049567, 0.939063, 0.01137],
        [-0.003453, 0.007233, 0.99622],
    ],
    [
        [0.760729, 0.319078, -0.079807],
        [0.090568, 0.889315, 0.020117],
        [-0.006027, 0.013325, 0.992702],
    ],
    [
        [0.675425, 0.43385, -0.109275],
        [0.125303, 0.847755, 0.026942],
        [-0.00795, 0.018572, 0.989378],
    ],
    [
        [0.605511, 0.52856, -0.134071],
        [0.155318, 0.812366, 0.032316],
        [-0.009376, 0.023176, 0.9862],
    ],
    [
        [0.547494, 0.607765, -0.155259],
        [0.181692, 0.781742, 0.036566],
        [-0.01041, 0.027275, 0.983136],
    ],
    [
        [0.498864, 0.674741, -0.173604],
        [0.205199, 0.754872, 0.039929],
        [-0.011131, 0.030969, 0.980162],
    ],
    [
        [0.457771, 0.731899, -0.18967],
        [0.226409, 0.731012, 0.042579],
        [-0.011595, 0.034333, 0.977261],
    ],
    [
        [0.422823, 0.781057, -0.203881],
        [0.245752, 0.709602, 0.044646],
        [-0.011843, 0.037423, 0.974421],
    ],
    [
        [0.392952, 0.82361, -0.216562],
        [0.263559, 0.69021, 0.046232],
        [-0.01191, 0.040281, 0.97163],
    ],
    [
        [0.367322, 0.860646, -0.227968],
        [0.280085, 0.672501, 0.047413],
        [-0.01182, 0.04294, 0.968881],
    ],
];

/// Tritanomaly (3 型色覚異常) の severity 別 11 段行列テーブル。
///
/// 構造・出典は [`PROTANOMALY_TABLE`] と同じ（`T_Tritanomaly`）。
/// `table[10]` は [`TRITANOPIA`] と同値。
pub(crate) const TRITANOMALY_TABLE: [[[f32; 3]; 3]; 11] = [
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    [
        [0.92667, 0.092514, -0.019184],
        [0.021191, 0.964503, 0.014306],
        [0.008437, 0.054813, 0.93675],
    ],
    [
        [0.89572, 0.13333, -0.02905],
        [0.029997, 0.9454, 0.024603],
        [0.013027, 0.104707, 0.882266],
    ],
    [
        [0.905871, 0.127791, -0.033662],
        [0.026856, 0.941251, 0.031893],
        [0.01341, 0.148296, 0.838294],
    ],
    [
        [0.948035, 0.08949, -0.037526],
        [0.014364, 0.946792, 0.038844],
        [0.010853, 0.193991, 0.795156],
    ],
    [
        [1.017277, 0.027029, -0.044306],
        [-0.006113, 0.958479, 0.047634],
        [0.006379, 0.248708, 0.744913],
    ],
    [
        [1.104996, -0.046633, -0.058363],
        [-0.032137, 0.971635, 0.060503],
        [0.001336, 0.317922, 0.680742],
    ],
    [
        [1.193214, -0.109812, -0.083402],
        [-0.058496, 0.97941, 0.079086],
        [-0.002346, 0.403492, 0.598854],
    ],
    [
        [1.257728, -0.139648, -0.118081],
        [-0.078003, 0.975409, 0.102594],
        [-0.003316, 0.501214, 0.502102],
    ],
    [
        [1.278864, -0.125333, -0.153531],
        [-0.084748, 0.957674, 0.127074],
        [-0.000989, 0.601151, 0.399838],
    ],
    [
        [1.255528, -0.076749, -0.178779],
        [-0.078411, 0.930809, 0.147602],
        [0.004733, 0.691367, 0.3039],
    ],
];

/// severity テーブル（11 段, index=0..=10, severity=index/10）から
/// `strength` に対応する 3x3 行列を解決する。
///
/// VIP-Sim (`myRecolour.cs`) と同方式: `scaled = strength * 10`、下側
/// `floor` と上側 `floor+1` の 2 エントリ間を **行列要素空間**で線形補間する
/// （行列積は要素に対して線形なので、要素空間の lerp と「両端を個別に適用して
/// 結果を lerp」は数学的に等価）。
///
/// `strength` が 0.1 刻みのグリッド上に厳密一致する場合（0.0, 0.1, …, 1.0）は
/// 補間を行わず該当エントリを**そのまま**返す。これにより severity=0.5 は
/// `table[5]` と、severity=1.0 は `table[10]` と浮動小数の丸め誤差なしに
/// 一致する。
///
/// `strength` は呼び出し元で 0.0..=1.0 に正規化済みであることを期待するが、
/// 範囲外が来ても defensive に clamp する。**NaN は `table[0]`（単位行列 /
/// identity）を返す**（`f32::clamp` は NaN を素通しするため、単純な
/// `.clamp(0.0, 1.0)` だけでは NaN が `scaled`/`frac` に伝播し補間結果が
/// NaN になってしまう。呼び出し元の [`normalize_strength`] と同じ
/// 「NaN→identity」の流儀をここでも独立に守る）。
pub(crate) fn resolve_severity_matrix(table: &[[[f32; 3]; 3]; 11], strength: f32) -> [[f32; 3]; 3] {
    if strength.is_nan() {
        return table[0];
    }
    let strength = strength.clamp(0.0, 1.0);
    let scaled = strength * 10.0;
    let i0 = (scaled.floor() as usize).min(10);
    let frac = scaled - i0 as f32;

    if frac <= 0.0 || i0 >= 10 {
        return table[i0];
    }

    let i1 = i0 + 1;
    let mut out = [[0.0f32; 3]; 3];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (col, out_cell) in out_row.iter_mut().enumerate() {
            *out_cell = lerp(table[i0][row][col], table[i1][row][col], frac);
        }
    }
    out
}

/// BT.709 / sRGB photopic luminance 係数（CIE Y）。
pub(crate) const LUMA_R: f32 = 0.2126;

pub(crate) const LUMA_G: f32 = 0.7152;

pub(crate) const LUMA_B: f32 = 0.0722;

/// Protanopia (1 型 2 色覚, L 錐体欠損 / 赤盲) シミュレーション。
///
/// `strength` を Machado 2009 severity (0.0..=1.0) として扱い、範囲外は clamp する。
/// `0.0` は元画像と同一、`1.0` で完全 dichromacy。
pub fn protanopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &PROTANOMALY_TABLE, strength)
}

/// Deuteranopia (2 型 2 色覚, M 錐体欠損 / 緑盲) シミュレーション。
///
/// `strength` の意味は [`protanopia`] と同じ。
pub fn deuteranopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &DEUTERANOMALY_TABLE, strength)
}

/// Tritanopia (3 型 2 色覚, S 錐体欠損 / 青盲) シミュレーション。
///
/// `strength` の意味は [`protanopia`] と同じ。
pub fn tritanopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &TRITANOMALY_TABLE, strength)
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

/// severity テーブルから strength に対応する行列を解決し、linear sRGB 上で
/// 3x3 行列を直接適用する内部実装（#165）。
///
/// 行列は LMS 空間のものではなく、Machado 2009 がプリ計算した
/// linear sRGB → simulated linear sRGB の per-severity 行列
/// （[`resolve_severity_matrix`] 参照）。テーブル自体が severity=0.0 で
/// 単位行列・severity=1.0 で完全 dichromacy 行列を持つため、旧実装のような
/// 「severity=1.0 行列 + strength blend」という追加のブレンド段は不要
/// （テーブル補間が実数演算としては同じ効果を担う。ADR-0008 参照）。
///
/// **注意（実数と f32 演算の違い）**: strength=1.0 での出力は旧実装
/// （`n = v + (matrix·v - v) * 1.0`）と代数的には同一だが、f32 加減算は
/// 非結合的なため bit 単位では稀に ±1 LSB 差が出る（256^3 全数実測:
/// protanopia 28/16,777,216・deuteranopia 11/16,777,216・tritanopia
/// 6/16,777,216 ピクセルで発生、いずれも最大 1 LSB。
/// `tests/color_severity1_full_sweep.rs` 参照）。旧実装の除去でむしろ
/// 丸め回数が1回減っており、精度が悪化したわけではない。
fn apply_machado_matrix(
    img: DynamicImage,
    table: &[[[f32; 3]; 3]; 11],
    strength: f32,
) -> Result<DynamicImage> {
    // strength を正規化（NaN→0 / clamp 0..1）。CPU 全段共通の正規化を使う（#113）。
    let strength = normalize_strength(strength);
    let mut rgba: RgbaImage = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let matrix = resolve_severity_matrix(table, strength);

    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);

        let nr = matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b;
        let ng = matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b;
        let nb = matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b;

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
/// ## アルゴリズム（Machado 2009 LMS 変換使用）
///
/// 1. linear sRGB に変換（gamma 解除）
/// 2. linear sRGB → LMS（Machado 2009 の変換行列）
/// 3. M（緑錐体）と L（赤錐体）の差分 `delta = M - L` を抽出
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

    // Machado 2009 linear sRGB → LMS 変換行列
    // 出典: Machado, Oliveira, Fernandes 2009, Equation 1 / Table 1
    // (Hunt-Pointer-Estévez の D65 白色点正規化版)
    const SRGB_TO_LMS: [[f32; 3]; 3] = [
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

        // linear sRGB → LMS
        let l_cone = SRGB_TO_LMS[0][0] * r + SRGB_TO_LMS[0][1] * g + SRGB_TO_LMS[0][2] * b;
        let m_cone = SRGB_TO_LMS[1][0] * r + SRGB_TO_LMS[1][1] * g + SRGB_TO_LMS[1][2] * b;

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

// ---------------------------------------------------------------
// #165: severity テーブルの regression anchor / 補間ロジックの単体テスト
// ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// テーブル transcription の正しさを固定する regression anchor。
    ///
    /// [`PROTANOMALY_TABLE`] / [`DEUTERANOMALY_TABLE`] / [`TRITANOMALY_TABLE`] は
    /// `myRecolour_reference.cs` (VIP-Sim) から今回新規に書き写した値であり、
    /// 末尾要素 (`table[10]`, severity=1.0) は Machado 2009 の severity=1.0 値と
    /// 完全一致するはずである（VIP-Sim も同じ Machado 2009 公開値を使用）。
    /// 既存の [`PROTANOPIA`] / [`DEUTERANOPIA`] / [`TRITANOPIA`] は独立に
    /// 書かれた regression anchor なので、ここでの一致確認はトートロジーではない。
    /// 不一致ならテーブル transcription にミスがある（この場合は書き換えず停止する）。
    #[test]
    fn protanomaly_table_severity1_matches_legacy_const() {
        assert_eq!(
            PROTANOMALY_TABLE[10], PROTANOPIA,
            "PROTANOMALY_TABLE[10] (severity=1.0) は既存 PROTANOPIA const と\
             全要素一致するはず"
        );
    }

    #[test]
    fn deuteranomaly_table_severity1_matches_legacy_const() {
        assert_eq!(
            DEUTERANOMALY_TABLE[10], DEUTERANOPIA,
            "DEUTERANOMALY_TABLE[10] (severity=1.0) は既存 DEUTERANOPIA const と\
             全要素一致するはず"
        );
    }

    #[test]
    fn tritanomaly_table_severity1_matches_legacy_const() {
        assert_eq!(
            TRITANOMALY_TABLE[10], TRITANOPIA,
            "TRITANOMALY_TABLE[10] (severity=1.0) は既存 TRITANOPIA const と\
             全要素一致するはず"
        );
    }

    /// severity=0.0 / 1.0 の境界: グリッド上は補間を経由せず該当エントリを
    /// そのまま返す（浮動小数の丸め誤差なしに一致させるための設計）。
    #[test]
    fn resolve_severity_matrix_grid_points_are_exact() {
        assert_eq!(
            resolve_severity_matrix(&PROTANOMALY_TABLE, 0.0),
            PROTANOMALY_TABLE[0]
        );
        assert_eq!(
            resolve_severity_matrix(&PROTANOMALY_TABLE, 0.5),
            PROTANOMALY_TABLE[5]
        );
        assert_eq!(
            resolve_severity_matrix(&PROTANOMALY_TABLE, 1.0),
            PROTANOMALY_TABLE[10]
        );
    }

    /// 非グリッド点 (0.1 刻みでない strength) はテーブル[i0]/[i1] の
    /// 要素空間 lerp になる。
    #[test]
    fn resolve_severity_matrix_interpolates_between_grid_points() {
        let resolved = resolve_severity_matrix(&TRITANOMALY_TABLE, 0.25);
        for (row, resolved_row) in resolved.iter().enumerate() {
            for (col, &resolved_cell) in resolved_row.iter().enumerate() {
                let expected = lerp(
                    TRITANOMALY_TABLE[2][row][col],
                    TRITANOMALY_TABLE[3][row][col],
                    0.5,
                );
                assert!(
                    (resolved_cell - expected).abs() < 1e-6,
                    "row={row} col={col}: got {resolved_cell} expected {expected}"
                );
            }
        }
    }

    /// 範囲外 strength は defensive に clamp される（呼び出し元の正規化に
    /// 依存しない防御的挙動）。
    #[test]
    fn resolve_severity_matrix_clamps_out_of_range_strength() {
        assert_eq!(
            resolve_severity_matrix(&DEUTERANOMALY_TABLE, -1.0),
            DEUTERANOMALY_TABLE[0]
        );
        assert_eq!(
            resolve_severity_matrix(&DEUTERANOMALY_TABLE, 2.0),
            DEUTERANOMALY_TABLE[10]
        );
    }

    /// #165 レビュー S1: `f32::clamp` は NaN を素通しするため、明示的な
    /// `is_nan()` チェックがないと NaN が `scaled`/`frac` へ伝播し、
    /// 補間結果が NaN 行列になってしまう。identity (`table[0]`) を返すこと
    /// を固定する。
    #[test]
    fn resolve_severity_matrix_nan_strength_is_identity() {
        let resolved = resolve_severity_matrix(&PROTANOMALY_TABLE, f32::NAN);
        assert_eq!(resolved, PROTANOMALY_TABLE[0]);
        for row in resolved {
            for cell in row {
                assert!(!cell.is_nan(), "resolved matrix must not contain NaN");
            }
        }
    }
}
