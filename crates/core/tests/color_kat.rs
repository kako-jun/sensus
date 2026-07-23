//! Known Answer Tests (KAT) — 色変換の数値的正しさを **出典値** に対して固定する。
//!
//! このリポは色変換アルゴリズムの正本であり、下流 (universal-experience) が
//! flutter_rust_bridge 経由で消費する。既存の検証は片肺だった:
//!
//! - `shader_equivalence.rs` は CPU↔GLSL の **自己整合** (PSNR≥30dB) を見るが、
//!   両者が同じ式から同時に壊れたら検出できない。
//! - `vision.rs` の unit test には散発的な実値テストが数本ある
//!   (`*_matches_machado_2009` 系 4 本)が、boundary (strength=0 で identity /
//!   clamp / NaN / alpha 保持) が中心で、全色覚 × 複数入力色の体系的な網羅は無い。
//!
//! 本ファイルは「出典由来の期待 RGB と一致するか」を体系化した KAT を足す。
//! 既存の散発的な実値テストに対し、本ファイルは
//!   (1) 出典をコードから独立に書き写したリテラルで検証し、
//!   (2) 全色覚 (protanopia / deuteranopia / tritanopia / achromatopsia) ×
//!       複数入力色を体系的に網羅し、
//!   (3) nyctalopia の閉形式も固定する、
//! 点で体系化している。**既存の boundary / 実値 test には一切触らない**。
//!
//! # 検出できるドリフトの大きさ (誇張しない)
//!
//! KAT の価値は「コードの行列定数が変わったら落ちる」ことにあるが、本テストは
//! **8bit 量子化出力 (u8) に対する検証** である。したがって捕捉できるのは:
//!
//! - 丸め後の出力 u8 を動かす大きさの行列ドリフト。golden アンカーは厳密一致
//!   (`assert_eq!`) なので、出力を 1/255 でも動かすドリフトを捕捉する。
//!   非飽和の中間色を含めることで、おおむね係数 0.001〜0.004 以上の変化なら
//!   どこかの色で出力 u8 に表れる (飽和チャンネルはドリフトを隠すため、
//!   検出感度は入力色に依存する)。
//! - 出力 u8 を変えない sub-u8 の浮動小数ドリフト (係数 4〜6 桁目の微小変化で
//!   全色の丸め結果が不変なもの) は、**設計上検出対象外**。u8 量子化の本質的限界。
//!
//! # トートロジー回避 (最重要)
//!
//! - **コードの `PROTANOPIA` / `DEUTERANOPIA` / `TRITANOPIA` const を import /
//!   参照しない**。自己参照は常に緑になり無意味。
//! - 期待値は公開出典から独立に導く:
//!   1. このファイル内に **出典行列を新規リテラルとして書き写す**
//!      (DOI コメント付き。`vision.rs` の const とは物理的に別のコピー)。
//!   2. ガンマ往復・行列積・blend・pack を **このファイル内で再実装** した参照
//!      パイプラインで期待値を計算する (`vision.rs` の private fn を使わない)。
//!   3. 主要ケースはさらに **オフライン計算済みの u8 リテラル golden 値** でも
//!      厳密一致で固定する (パイプライン退行も捕捉)。
//!
//! # 出典
//!
//! Machado, Oliveira, Fernandes (2009)
//! "A Physiologically-based Model for Simulation of Color Vision Deficiency"
//! IEEE TVCG, DOI: 10.1109/TVCG.2009.113
//! <https://doi.org/10.1109/TVCG.2009.113>
//! 著者ページ / DaltonLens 公開値の severity=1.0 行列
//! (linear sRGB → simulated linear sRGB) を用いる。

use image::{DynamicImage, Rgba, RgbaImage};
use sensus_core::vision::{achromatopsia, deuteranopia, nyctalopia, protanopia, tritanopia};

// =====================================================================
// 出典行列リテラル (DOI: 10.1109/TVCG.2009.113, severity=1.0)
//
// ⚠️ これは出典から **独立に書き写した** コピーであり、vision.rs の const とは
// 物理的に別物。これを vision.rs から import すると自己参照 (トートロジー) に
// なるため、絶対にしない。コードの const がドリフトして丸め後の出力 u8 を動かせば、
// ここを基準に計算した期待値と実出力が乖離して KAT が落ちる ——それが本テストの
// 存在意義 (sub-u8 の微小ドリフトは u8 量子化により検出対象外)。
// =====================================================================

/// Protanopia (1 型 2 色覚) severity=1.0 行列 — Machado 2009。
const SRC_PROTANOPIA: [[f64; 3]; 3] = [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
];

/// Deuteranopia (2 型 2 色覚) severity=1.0 行列 — Machado 2009。
const SRC_DEUTERANOPIA: [[f64; 3]; 3] = [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
];

/// Tritanopia (3 型 2 色覚) severity=1.0 行列 — Machado 2009。
const SRC_TRITANOPIA: [[f64; 3]; 3] = [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
];

// =====================================================================
// 出典行列リテラル (#165, per-severity 11 段テーブルの中間エントリ)
//
// Machado 2009 / VIP-Sim (myRecolour.cs, T_Protanomaly / T_Deuteranomaly /
// T_Tritanomaly) の severity 別行列テーブルのうち、中間 strength の cross-check
// に使うエントリだけを **独立に書き写す**。上記 severity=1.0 リテラルと同じ
// トートロジー回避の方針: `vision::color::PROTANOMALY_TABLE` 等を import しない。
// =====================================================================

/// Protanomaly severity=0.5 (テーブル index=5) 行列。
const SRC_PROTANOMALY_SEV_0_5: [[f64; 3]; 3] = [
    [0.458064, 0.679578, -0.137642],
    [0.092785, 0.846313, 0.060902],
    [-0.007494, -0.016807, 1.024301],
];

/// Deuteranomaly severity=0.5 (テーブル index=5) 行列。
const SRC_DEUTERANOMALY_SEV_0_5: [[f64; 3]; 3] = [
    [0.547494, 0.607765, -0.155259],
    [0.181692, 0.781742, 0.036566],
    [-0.010410, 0.027275, 0.983136],
];

/// Tritanomaly severity=0.2 (テーブル index=2) 行列。severity=0.25 (非グリッド点)
/// の補間元の下側エントリ。
const SRC_TRITANOMALY_SEV_0_2: [[f64; 3]; 3] = [
    [0.895720, 0.133330, -0.029050],
    [0.029997, 0.945400, 0.024603],
    [0.013027, 0.104707, 0.882266],
];

/// Tritanomaly severity=0.3 (テーブル index=3) 行列。severity=0.25 (非グリッド点)
/// の補間元の上側エントリ。
const SRC_TRITANOMALY_SEV_0_3: [[f64; 3]; 3] = [
    [0.905871, 0.127791, -0.033662],
    [0.026856, 0.941251, 0.031893],
    [0.013410, 0.148296, 0.838294],
];

/// BT.709 / sRGB photopic luminance 係数 (CIE Y)。achromatopsia と nyctalopia で
/// 使う。ITU-R BT.709 / sRGB 規格値であり、vision.rs の `LUMA_*` とは独立に
/// このファイルへ書き写している。
const BT709: [f64; 3] = [0.2126, 0.7152, 0.0722];

// =====================================================================
// 参照パイプライン (このファイル内で再実装。vision.rs の private fn を使わない)
//
// 仕様 (Issue #156 / vision.rs docstring):
//   1. r,g,b = srgb_to_linear(channel/255)  (標準 sRGB ガンマ解除)
//   2. 行列積 (clamp なし) で simulated 値を得る
//   3. blend: nr = r + (sr-r)*strength
//   4. pack_u8(linear_to_srgb(nr)): NaN→0 / [0,1] clamp / *255 round
// =====================================================================

/// 標準 sRGB ガンマ解除 (sRGB 0..=1 → linear)。
fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// 標準 sRGB ガンマ適用 (linear → sRGB 0..=1)。
fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// [0,1] clamp して 8bit に丸める。NaN は 0。
fn pack_u8(c: f64) -> u8 {
    if c.is_nan() {
        0
    } else {
        (c.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

/// 出典行列 + 参照パイプラインで 1 ピクセルの期待 u8 を計算する。
fn reference_matrix_pixel(m: &[[f64; 3]; 3], rgb: [u8; 3], strength: f64) -> [u8; 3] {
    let r = srgb_to_linear(rgb[0] as f64 / 255.0);
    let g = srgb_to_linear(rgb[1] as f64 / 255.0);
    let b = srgb_to_linear(rgb[2] as f64 / 255.0);

    let sr = m[0][0] * r + m[0][1] * g + m[0][2] * b;
    let sg = m[1][0] * r + m[1][1] * g + m[1][2] * b;
    let sb = m[2][0] * r + m[2][1] * g + m[2][2] * b;

    let nr = r + (sr - r) * strength;
    let ng = g + (sg - g) * strength;
    let nb = b + (sb - b) * strength;

    [
        pack_u8(linear_to_srgb(nr)),
        pack_u8(linear_to_srgb(ng)),
        pack_u8(linear_to_srgb(nb)),
    ]
}

/// #165: severity テーブルが**既に解決済みの行列**を、blend なしで直接適用して
/// 1 ピクセルの期待 u8 を計算する。severity=0.5 のようなグリッド点では、
/// テーブルの該当エントリをそのまま `m` として渡す（`vision::color::
/// apply_machado_matrix` は resolve 済み行列を直接適用するだけで、旧実装の
/// ような追加の strength blend 段を持たない）。
fn reference_matrix_pixel_direct(m: &[[f64; 3]; 3], rgb: [u8; 3]) -> [u8; 3] {
    let r = srgb_to_linear(rgb[0] as f64 / 255.0);
    let g = srgb_to_linear(rgb[1] as f64 / 255.0);
    let b = srgb_to_linear(rgb[2] as f64 / 255.0);

    let nr = m[0][0] * r + m[0][1] * g + m[0][2] * b;
    let ng = m[1][0] * r + m[1][1] * g + m[1][2] * b;
    let nb = m[2][0] * r + m[2][1] * g + m[2][2] * b;

    [
        pack_u8(linear_to_srgb(nr)),
        pack_u8(linear_to_srgb(ng)),
        pack_u8(linear_to_srgb(nb)),
    ]
}

/// #165: 2 つの行列を要素空間で線形補間する。`vision::color::
/// resolve_severity_matrix` と**独立に**再実装したもの（`w=0.5` は中点）。
/// `crates/core/src/vision/color.rs` の実装を import せず、このファイル内で
/// 完結させることでトートロジーを避ける。
fn lerp_matrix_f64(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3], w: f64) -> [[f64; 3]; 3] {
    let mut out = [[0.0f64; 3]; 3];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (col, out_cell) in out_row.iter_mut().enumerate() {
            *out_cell = a[row][col] + (b[row][col] - a[row][col]) * w;
        }
    }
    out
}

/// achromatopsia の参照ピクセル: 行列の代わりに BT.709 luminance で grayscale blend。
fn reference_achromatopsia_pixel(rgb: [u8; 3], strength: f64) -> [u8; 3] {
    let r = srgb_to_linear(rgb[0] as f64 / 255.0);
    let g = srgb_to_linear(rgb[1] as f64 / 255.0);
    let b = srgb_to_linear(rgb[2] as f64 / 255.0);

    let y = BT709[0] * r + BT709[1] * g + BT709[2] * b;

    let nr = r + (y - r) * strength;
    let ng = g + (y - g) * strength;
    let nb = b + (y - b) * strength;

    [
        pack_u8(linear_to_srgb(nr)),
        pack_u8(linear_to_srgb(ng)),
        pack_u8(linear_to_srgb(nb)),
    ]
}

// =====================================================================
// テスト用ヘルパー
// =====================================================================

/// 単一色の 1x1 RGBA 画像 (alpha=255) を作る。
fn pixel_image(rgb: [u8; 3]) -> DynamicImage {
    let mut img = RgbaImage::new(1, 1);
    img.put_pixel(0, 0, Rgba([rgb[0], rgb[1], rgb[2], 255]));
    DynamicImage::ImageRgba8(img)
}

/// 単一色 + 指定 alpha の 1x1 RGBA 画像を作る。
fn pixel_image_with_alpha(rgb: [u8; 3], alpha: u8) -> DynamicImage {
    let mut img = RgbaImage::new(1, 1);
    img.put_pixel(0, 0, Rgba([rgb[0], rgb[1], rgb[2], alpha]));
    DynamicImage::ImageRgba8(img)
}

/// フィルタ適用後の (0,0) ピクセルを [r,g,b,a] で読む。
fn out_pixel(img: DynamicImage) -> [u8; 4] {
    img.to_rgba8().get_pixel(0, 0).0
}

/// strength のみを取る色覚フィルタの関数ポインタ型 (clippy::type_complexity 回避)。
type StrengthFilter = fn(DynamicImage, f32) -> sensus_core::Result<DynamicImage>;

/// チャンネルごとに ±tol u8 の一致を assert する。
fn assert_close(actual: [u8; 3], expected: [u8; 3], tol: i32, ctx: &str) {
    for ch in 0..3 {
        let diff = (actual[ch] as i32 - expected[ch] as i32).abs();
        assert!(
            diff <= tol,
            "{ctx}: channel {ch} expected {} got {} (diff {diff} > tol {tol}); actual={actual:?} expected={expected:?}",
            expected[ch],
            actual[ch],
        );
    }
}

/// golden アンカー用: u8 出力が **厳密一致** することを assert する。
/// ±1 を許さないことで、丸め後の出力を 1/255 でも動かすドリフトを捕捉する
/// (最大感度)。
fn assert_exact(actual: [u8; 3], expected: [u8; 3], ctx: &str) {
    assert_eq!(actual, expected, "{ctx}");
}

/// 入力の代表 5 + 補助 4 色。
const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
const CYAN: [u8; 3] = [0, 255, 255];
const MAGENTA: [u8; 3] = [255, 0, 255];
const YELLOW: [u8; 3] = [255, 255, 0];
const GRAY128: [u8; 3] = [128, 128, 128];

/// チャンネルが飽和 (0/255) しにくい非飽和中間色。飽和チャンネルは行列ドリフトを
/// 隠す (clamp で吸収される) ので、こうした中間色を回すと小さな係数変化が
/// 出力 u8 に表れて検出感度が上がる。
const WARM_MID: [u8; 3] = [180, 120, 60];
const COOL_MID: [u8; 3] = [90, 140, 200];

/// cross-check で回す入力色 (代表 5 + 補助 4 + 非飽和中間 2 = 11 色)。
/// 飽和色は不変条件・clamp 経路を、非飽和中間色は係数ドリフトの検出力を担う。
const COLORS: &[[u8; 3]] = &[
    RED, GREEN, BLUE, WHITE, BLACK, CYAN, MAGENTA, YELLOW, GRAY128, WARM_MID, COOL_MID,
];

/// f32 丸め境界を吸収する許容差。
const TOL: i32 = 1;

// =====================================================================
// 1. golden u8 アンカー (完全ハードコードリテラル / 少数精鋭)
//
// オフライン (Python, f64 参照パイプライン) で計算した u8 を直接書く。
// 出典行列が壊れても、pack/gamma パイプラインが退行しても落ちる。
// =====================================================================

#[test]
fn golden_protanopia_severity1() {
    // 出典行列 SRC_PROTANOPIA を f64 参照パイプラインで通したオフライン値。
    // red(255,0,0): linear R のみが赤行 [0.152286,1.052583,-0.204868] で混色され、
    //   sr=0.152286*1.0=0.152..→l2s≈0.428→109、sg=0.114503→l2s≈0.373→95、
    //   sb=-0.003882<0→clamp 0→0。アンカー (109,95,0) と一致する。
    assert_exact(
        out_pixel(protanopia(pixel_image(RED), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [109, 95, 0],
        "protanopia red",
    );
    // green(0,255,0): sr=1.052583→飽和→255、sg=0.786281→229、sb=-0.048116→0。
    assert_exact(
        out_pixel(protanopia(pixel_image(GREEN), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [255, 229, 0],
        "protanopia green",
    );
    // blue(0,0,255): sr=-0.204868→0、sg=0.099216→89、sb=1.051998→飽和→255。
    assert_exact(
        out_pixel(protanopia(pixel_image(BLUE), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 89, 255],
        "protanopia blue",
    );
    // white: 行和がほぼ 1 (≈1.000001/1.0/1.0) → 不変条件 ≈白。
    assert_exact(
        out_pixel(protanopia(pixel_image(WHITE), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [255, 255, 255],
        "protanopia white",
    );
    // black: 全 0 → 不変条件 黒。
    assert_exact(
        out_pixel(protanopia(pixel_image(BLACK), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 0, 0],
        "protanopia black",
    );
}

#[test]
fn golden_deuteranopia_severity1() {
    // 出典行列 SRC_DEUTERANOPIA を f64 参照パイプラインで通したオフライン値。
    assert_exact(
        out_pixel(deuteranopia(pixel_image(RED), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [163, 144, 0],
        "deuteranopia red",
    );
    assert_exact(
        out_pixel(deuteranopia(pixel_image(GREEN), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [239, 214, 58],
        "deuteranopia green",
    );
    assert_exact(
        out_pixel(deuteranopia(pixel_image(BLUE), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 61, 251],
        "deuteranopia blue",
    );
    assert_exact(
        out_pixel(deuteranopia(pixel_image(WHITE), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [255, 255, 255],
        "deuteranopia white",
    );
    assert_exact(
        out_pixel(deuteranopia(pixel_image(BLACK), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 0, 0],
        "deuteranopia black",
    );
}

#[test]
fn golden_tritanopia_severity1() {
    // 出典行列 SRC_TRITANOPIA を f64 参照パイプラインで通したオフライン値。
    assert_exact(
        out_pixel(tritanopia(pixel_image(RED), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [255, 0, 15],
        "tritanopia red",
    );
    assert_exact(
        out_pixel(tritanopia(pixel_image(GREEN), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 247, 217],
        "tritanopia green",
    );
    assert_exact(
        out_pixel(tritanopia(pixel_image(BLUE), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 107, 150],
        "tritanopia blue",
    );
    assert_exact(
        out_pixel(tritanopia(pixel_image(WHITE), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [255, 255, 255],
        "tritanopia white",
    );
    assert_exact(
        out_pixel(tritanopia(pixel_image(BLACK), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 0, 0],
        "tritanopia black",
    );
}

#[test]
fn golden_achromatopsia_strength1() {
    // strength=1.0 で完全グレースケール (R==G==B)。BT.709 luminance を
    // linear で取り l2s した値。red/green/blue の luma 差が見える。
    let red = out_pixel(achromatopsia(pixel_image(RED), 1.0).unwrap());
    assert_exact(
        red[..3].try_into().unwrap(),
        [127, 127, 127],
        "achromatopsia red",
    );
    assert_eq!(red[0], red[1], "achromatopsia red: R==G");
    assert_eq!(red[1], red[2], "achromatopsia red: G==B");

    let green = out_pixel(achromatopsia(pixel_image(GREEN), 1.0).unwrap());
    assert_exact(
        green[..3].try_into().unwrap(),
        [220, 220, 220],
        "achromatopsia green",
    );
    assert_eq!(green[0], green[1], "achromatopsia green: R==G");
    assert_eq!(green[1], green[2], "achromatopsia green: G==B");

    let blue = out_pixel(achromatopsia(pixel_image(BLUE), 1.0).unwrap());
    assert_exact(
        blue[..3].try_into().unwrap(),
        [76, 76, 76],
        "achromatopsia blue",
    );
    assert_eq!(blue[0], blue[1], "achromatopsia blue: R==G");
    assert_eq!(blue[1], blue[2], "achromatopsia blue: G==B");

    // 不変条件: 白→白、黒→黒 (grayscale でも変わらない)。
    assert_exact(
        out_pixel(achromatopsia(pixel_image(WHITE), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [255, 255, 255],
        "achromatopsia white",
    );
    assert_exact(
        out_pixel(achromatopsia(pixel_image(BLACK), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 0, 0],
        "achromatopsia black",
    );
}

/// 白→ほぼ白 / 黒→黒 の不変条件を全色覚フィルタで確認する。
#[test]
fn invariant_white_black_all_filters() {
    let filters: [(&str, StrengthFilter); 4] = [
        ("protanopia", protanopia),
        ("deuteranopia", deuteranopia),
        ("tritanopia", tritanopia),
        ("achromatopsia", achromatopsia),
    ];
    for (name, f) in filters {
        let white = out_pixel(f(pixel_image(WHITE), 1.0).unwrap());
        assert_exact(
            white[..3].try_into().unwrap(),
            [255, 255, 255],
            &format!("{name} white invariant"),
        );
        let black = out_pixel(f(pixel_image(BLACK), 1.0).unwrap());
        assert_exact(
            black[..3].try_into().unwrap(),
            [0, 0, 0],
            &format!("{name} black invariant"),
        );
    }
}

// =====================================================================
// 2. 出典行列 cross-check (網羅 / トートロジー回避の本体)
//
// このファイル内に書き写した出典行列リテラル + ファイル内参照パイプラインで
// 期待値を計算し、実出力と ±1 u8 で一致を検証する。複数の入力色で回す。
// ±1 許容のため、丸め後 u8 を動かす大きさの係数ドリフトを捕捉する（sub-u8 の
// 微小ドリフトは設計上対象外。1/255 単位の感度は golden の厳密一致が担う）。
// 非飽和の中間色を含めることで、飽和チャンネルに隠れない検出力を確保する。
// =====================================================================

/// 全 cross-check 色を出典行列で検証する共通ルーチン。
fn cross_check_matrix(name: &str, src: &[[f64; 3]; 3], apply: StrengthFilter) {
    for &c in COLORS {
        let expected = reference_matrix_pixel(src, c, 1.0);
        let actual = out_pixel(apply(pixel_image(c), 1.0).unwrap());
        assert_close(
            actual[..3].try_into().unwrap(),
            expected,
            TOL,
            &format!("{name} cross-check color {c:?}"),
        );
    }
}

#[test]
fn cross_check_protanopia() {
    cross_check_matrix("protanopia", &SRC_PROTANOPIA, protanopia);
}

#[test]
fn cross_check_deuteranopia() {
    cross_check_matrix("deuteranopia", &SRC_DEUTERANOPIA, deuteranopia);
}

#[test]
fn cross_check_tritanopia() {
    cross_check_matrix("tritanopia", &SRC_TRITANOPIA, tritanopia);
}

#[test]
fn cross_check_achromatopsia() {
    for &c in COLORS {
        let expected = reference_achromatopsia_pixel(c, 1.0);
        let actual = out_pixel(achromatopsia(pixel_image(c), 1.0).unwrap());
        assert_close(
            actual[..3].try_into().unwrap(),
            expected,
            TOL,
            &format!("achromatopsia cross-check color {c:?}"),
        );
    }
}

/// 中間 strength (severity=0.5) の cross-check 共通ルーチン。
///
/// #165 以降、protanopia / deuteranopia / tritanopia の severity 解決は
/// 「severity=1.0 行列 + strength blend」(ADR-0002, superseded) ではなく
/// 「per-severity 11 段テーブル (0.1 刻み) から strength に対応する行列を解決し、
/// **直接適用**」(ADR-0008) に変わった。severity=0.5 はテーブルのグリッド点
/// (index=5) に厳密一致するため、補間すら経由せずテーブル該当エントリを
/// そのまま適用した結果になる。したがって期待値は
/// `reference_matrix_pixel_direct(sev0.5 行列, color)` で計算する
/// (`reference_matrix_pixel` の strength blend 版とは異なる関数)。
fn cross_check_mid_strength(name: &str, sev_0_5: &[[f64; 3]; 3], apply: StrengthFilter) {
    for &c in COLORS {
        let expected = reference_matrix_pixel_direct(sev_0_5, c);
        let actual = out_pixel(apply(pixel_image(c), 0.5).unwrap());
        assert_close(
            actual[..3].try_into().unwrap(),
            expected,
            TOL,
            &format!("{name} s=0.5 cross-check color {c:?}"),
        );
    }
}

#[test]
fn cross_check_protanopia_mid_strength() {
    cross_check_mid_strength("protanopia", &SRC_PROTANOMALY_SEV_0_5, protanopia);
}

#[test]
fn cross_check_deuteranopia_mid_strength() {
    cross_check_mid_strength("deuteranopia", &SRC_DEUTERANOMALY_SEV_0_5, deuteranopia);
}

/// #165: 非グリッド点 (severity=0.25) の cross-check。
///
/// severity=0.25 → `index = 0.25 * 10 = 2.5` → `i0=2, i1=3, frac=0.5`
/// (`vision::color::resolve_severity_matrix` の式)。このテストは同じ式を
/// このファイル内で独立に再実装した [`lerp_matrix_f64`] で
/// `SRC_TRITANOMALY_SEV_0_2` と `SRC_TRITANOMALY_SEV_0_3` を中点補間し、
/// その行列を [`reference_matrix_pixel_direct`] で直接適用した結果を期待値とする。
#[test]
fn cross_check_tritanopia_quarter_strength_interpolated() {
    let interpolated = lerp_matrix_f64(&SRC_TRITANOMALY_SEV_0_2, &SRC_TRITANOMALY_SEV_0_3, 0.5);
    for &c in COLORS {
        let expected = reference_matrix_pixel_direct(&interpolated, c);
        let actual = out_pixel(tritanopia(pixel_image(c), 0.25).unwrap());
        assert_close(
            actual[..3].try_into().unwrap(),
            expected,
            TOL,
            &format!("tritanopia s=0.25 cross-check color {c:?}"),
        );
    }
}

/// alpha が色覚変換で保持されることを KAT 文脈でも確認する
/// (boundary test とは別に、出典 KAT パイプライン全体での保持を固定)。
#[test]
fn alpha_preserved_through_color_filters() {
    for alpha in [0u8, 128, 200, 255] {
        let out = out_pixel(protanopia(pixel_image_with_alpha(RED, alpha), 1.0).unwrap());
        assert_eq!(out[3], alpha, "protanopia must preserve alpha={alpha}");
        let out = out_pixel(achromatopsia(pixel_image_with_alpha(GREEN, alpha), 1.0).unwrap());
        assert_eq!(out[3], alpha, "achromatopsia must preserve alpha={alpha}");
    }
}

// =====================================================================
// 3. 非色覚フィルタの実値検証 (roadmap 指定: 光系の代表)
//
// 選んだフィルタ: nyctalopia (夜盲)。
// 理由: per-pixel の閉形式変換で **空間カーネルを持たない** ため、入力が一様で
//   なくても・1x1 でも閉形式で期待値が出る (空間ブラー系の no-op 条件すら不要)。
//   roadmap の「光・透明度 (#6) 系」の代表でもある。
//
// 係数は docs/overview.md と vision.rs docstring から **独立に** 書き写す:
//   - photopic luma: BT.709 = (0.2126, 0.7152, 0.0722)
//   - scotopic luma: Vos (1978) 近似 = 0.0610 R + 0.3751 G + 0.6038 B
//     出典: Vos (1978) "Colorimetric and photometric properties of a 2°
//     fundamental observer" Color Research & Application 3(3): 125-128
//   - dark_factor = 1 - s*0.7 ; desat = s*0.8
//   - Purkinje shift: R *= (1 - s*0.2) ; B *= (1 + s*0.1)
// =====================================================================

/// nyctalopia の参照ピクセル (閉形式、このファイル内で独立に再実装)。
fn reference_nyctalopia_pixel(rgb: [u8; 3], s: f64) -> [u8; 3] {
    let r = srgb_to_linear(rgb[0] as f64 / 255.0);
    let g = srgb_to_linear(rgb[1] as f64 / 255.0);
    let b = srgb_to_linear(rgb[2] as f64 / 255.0);

    let dark_factor = 1.0 - s * 0.7;
    let desat = s * 0.8;

    let y_phot = BT709[0] * r + BT709[1] * g + BT709[2] * b;
    // Vos (1978) scotopic luminance 近似 (独立に書き写し)。
    let y_scot = 0.0610 * r + 0.3751 * g + 0.6038 * b;
    let y = y_phot + (y_scot - y_phot) * s;

    let dr = r + (y - r) * desat;
    let dg = g + (y - g) * desat;
    let db = b + (y - b) * desat;

    // Purkinje shift: 青チャネル微増・赤チャネル微減。
    let pr = dr * (1.0 - s * 0.2);
    let pb = db * (1.0 + s * 0.1);

    let fr = pr * dark_factor;
    let fg = dg * dark_factor;
    let fb = pb * dark_factor;

    [
        pack_u8(linear_to_srgb(fr)),
        pack_u8(linear_to_srgb(fg)),
        pack_u8(linear_to_srgb(fb)),
    ]
}

#[test]
fn closed_form_nyctalopia() {
    // golden アンカー (オフライン f64 で計算したハードコード値)。
    // mid-gray 128, s=1.0: dark=0.3, desat=0.8。grayscale なので y_phot≈y_scot で
    //   ほぼ R/G/B が luma に寄り、Purkinje で R 微減・B 微増、最後に ×0.3。
    assert_close(
        out_pixel(nyctalopia(pixel_image(GRAY128), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [65, 73, 77],
        TOL,
        "nyctalopia gray128 s=1.0 golden",
    );
    // mid-gray 128, s=0.5。
    assert_close(
        out_pixel(nyctalopia(pixel_image(GRAY128), 0.5).unwrap())[..3]
            .try_into()
            .unwrap(),
        [100, 105, 108],
        TOL,
        "nyctalopia gray128 s=0.5 golden",
    );

    // 閉形式 cross-check: 複数の入力色 × strength で参照式と一致を確認する。
    let colors = [RED, GREEN, BLUE, WHITE, GRAY128, [200, 100, 50]];
    for c in colors {
        for &s in &[0.3_f64, 0.5, 1.0] {
            let expected = reference_nyctalopia_pixel(c, s);
            let actual = out_pixel(nyctalopia(pixel_image(c), s as f32).unwrap());
            assert_close(
                actual[..3].try_into().unwrap(),
                expected,
                TOL,
                &format!("nyctalopia closed-form color {c:?} s={s}"),
            );
        }
    }

    // 不変条件: 黒 → 黒 (全係数が乗算で 0 を保つ)。
    assert_close(
        out_pixel(nyctalopia(pixel_image(BLACK), 1.0).unwrap())[..3]
            .try_into()
            .unwrap(),
        [0, 0, 0],
        TOL,
        "nyctalopia black invariant",
    );
}
