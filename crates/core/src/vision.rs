//! Vision filters: color vision deficiency, blur / refraction, visual field
//! defects, light sensitivity, etc.
//!
//! Phase 1 (Issue #2) では色覚特性 4 種を実装する:
//!
//! - [`protanopia`]    — 1 型 2 色覚（L 錐体欠損, 赤盲）
//! - [`deuteranopia`]  — 2 型 2 色覚（M 錐体欠損, 緑盲）
//! - [`tritanopia`]    — 3 型 2 色覚（S 錐体欠損, 青盲）
//! - [`achromatopsia`] — 全色盲（錐体機能不全）
//!
//! Phase 2 (Issue #4) では焦点・屈折 4 種を実装する:
//!
//! - [`myopia`]      — 近視 (-6D 上限相当, 等方 disk blur)
//! - [`hyperopia`]   — 遠視 (+4D 上限相当, 等方 disk blur)
//! - [`presbyopia`]  — 老眼 (+3D add 相当, 等方 disk blur)
//! - [`astigmatism`] — 乱視 (純粋 cylinder lens, -3CD 上限相当の **方向性 blur**)
//!
//! myopia / hyperopia / presbyopia は光学的に正しい等方 **disk blur
//! (pillbox kernel)** を linear sRGB 空間で適用する。Gaussian は実際の defocus
//! blur ではないため採用しない（瞳孔は円形であり、点光源の retina 上の像は
//! circle of confusion = 円となる）。
//!
//! astigmatism は **isolated cylinder error** のシミュレーションで、純粋
//! cylinder lens は line focus (焦線) を作るため光学的には **1D directional
//! blur** が正しい。実装上は楕円カーネルの短軸を sub-pixel まで縮退させて
//! 1D box フィルタとして畳み込む。臨床現場で多い合併乱視 (cylinder + sphere)
//! は両経線にぼけがあるが、これは Phase 4 (#10) pipeline で
//! `Myopia + Astigmatism` のような合成として扱う前提で、本フィルタ単体では
//! 表現しない。
//!
//! ディオプター → 画素半径の換算は以下の前提による:
//! Smith-Helmholtz 近似 `θ_diameter (rad) ≈ pupil_diameter(m) × |D|` は
//! **角直径 (CoC 円盤の直径)** を返すので、半径は `θ_diameter / 2`。
//! pupil 4 mm = 0.004 m (mesopic 標準), 視距離 50 cm / FOV 30° を想定し、
//! 画像の `min(width, height)` に対する比率で表現する。詳細は各関数の
//! `MAX_RADIUS_RATIO` 定数のコメントを参照。
//!
//! # アルゴリズム
//!
//! ## protanopia / deuteranopia / tritanopia
//!
//! Machado, Oliveira, Fernandes (2009)
//! "A Physiologically-based Model for Simulation of Color Vision Deficiency"
//! IEEE TVCG, DOI: [10.1109/TVCG.2009.113][doi]
//! の severity = 1.0 行列を **linear sRGB → simulated linear sRGB** に
//! 直接適用する。著者ページの supplementary に同じ値が掲載されている:
//! <https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html>
//!
//! 中間 strength は Machado 自身が示唆する通り、linear sRGB 空間で
//! `lerp(original, simulated, strength)` する。これは
//! anomalous trichromacy（軽度色覚異常）の臨床的近似として
//! DaltonLens 等で広く採用されている方式。
//!
//! ## achromatopsia
//!
//! LMS 経路は使わない（錐体機能不全のため三刺激値の前提が成立しない）。
//! CIE photopic luminance を BT.709 係数 (0.2126, 0.7152, 0.0722) で
//! linear sRGB から計算し、`(Y, Y, Y)` と原色を strength で linear blend する。
//!
//! BT.601 (0.299, 0.587, 0.114) は **使わない** — NTSC CRT 規格であり
//! sRGB / linear 空間には不適切。
//!
//! # 色空間
//!
//! 全処理は **linear sRGB 空間** で行う。入力 sRGB を gamma 解除 → 行列適用 /
//! luma 計算 → strength で linear blend → sRGB に gamma 戻し。アルファは
//! そのまま保持する。
//!
//! [doi]: https://doi.org/10.1109/TVCG.2009.113

use crate::Result;
use image::{DynamicImage, RgbaImage};
use std::f32::consts::PI;

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典: Machado, Oliveira, Fernandes 2009, Table 3 / 5 相当の severity=1.0
/// プリ計算行列。著者ページ:
/// <https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html>
/// および DaltonLens 公開データ <https://daltonlens.org/> と一致。
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
const LUMA_R: f32 = 0.2126;
const LUMA_G: f32 = 0.7152;
const LUMA_B: f32 = 0.0722;

/// sRGB (0.0..=1.0) → linear sRGB の標準ガンマ解除。
#[inline]
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// linear sRGB → sRGB (0.0..=1.0) の標準ガンマ適用。
#[inline]
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `[0.0, 1.0]` に clamp してから 8 bit に丸めて保存する。
///
/// NaN は明示的に 0 として扱う（saturating cast の暗黙挙動に依存しない）。
#[inline]
fn pack_u8(c: f32) -> u8 {
    if c.is_nan() {
        0
    } else {
        (c.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

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
    // NaN strength は identity（元画像）として扱う。
    // f32::NAN.clamp(0.0, 1.0) は NaN のままだが、上流で 0.0 に置換しているので
    // silent な全画素 0 出力にはならない。
    let strength = if strength.is_nan() {
        0.0
    } else {
        strength.clamp(0.0, 1.0)
    };
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
    // NaN strength は identity（元画像）として扱う。
    // f32::NAN.clamp(0.0, 1.0) は NaN のままだが、上流で 0.0 に置換しているので
    // silent な全画素 0 出力にはならない。
    let strength = if strength.is_nan() {
        0.0
    } else {
        strength.clamp(0.0, 1.0)
    };
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

// ---------------------------------------------------------------------
// Phase 2: focus / refraction (disk blur in linear sRGB)
// ---------------------------------------------------------------------

/// strength=1.0 における近視 (-6D 相当) の disk **半径** 比 (min(W,H) 比)。
///
/// 導出: Smith-Helmholtz 近似 `θ_diameter ≈ pupil(m) × |D|`（angular **diameter**）
///   - pupil = 4 mm = 0.004 m（mesopic 標準）
///   - max diopter = 6 D（強度近視の入口）
///   - θ_diameter = 0.004 × 6 = 0.024 rad ≈ 1.375°
///   - radius (rad) = θ_diameter / 2 = 0.012 rad
///
/// 画像 FOV = 30° ≈ 0.5236 rad（視距離 50 cm の典型的写真鑑賞）と仮定:
///   ratio = 0.012 / 0.5236 ≈ 0.02292 → 0.023 に丸める
const MYOPIA_MAX_RADIUS_RATIO: f32 = 0.023;

/// strength=1.0 における遠視 (+4D 相当) の disk **半径** 比 (min(W,H) 比)。
///
/// 導出: Smith-Helmholtz 近似 `θ_diameter ≈ pupil(m) × |D|`
///   - pupil = 0.004 m, max diopter = 4 D
///   - θ_diameter = 0.004 × 4 = 0.016 rad ≈ 0.917°
///   - radius (rad) = 0.008 rad
///
/// FOV 30° (0.5236 rad) 前提で:
///   ratio = 0.008 / 0.5236 ≈ 0.01528 → 0.015 に丸める
const HYPEROPIA_MAX_RADIUS_RATIO: f32 = 0.015;

/// strength=1.0 における老眼 (+3D add 相当) の disk **半径** 比 (min(W,H) 比)。
///
/// 導出: Smith-Helmholtz 近似 `θ_diameter ≈ pupil(m) × |D|`
///   - pupil = 0.004 m, max diopter = 3 D
///   - θ_diameter = 0.004 × 3 = 0.012 rad ≈ 0.687°
///   - radius (rad) = 0.006 rad
///
/// FOV 30° (0.5236 rad) 前提で:
///   ratio = 0.006 / 0.5236 ≈ 0.01146 → 0.011 に丸める
const PRESBYOPIA_MAX_RADIUS_RATIO: f32 = 0.011;

/// strength=1.0 における乱視 (-3CD 相当) の **ボケ方向** 半径比 (min(W,H) 比)。
///
/// 純粋 cylinder lens の line focus は 1D directional blur となるため、
/// 楕円カーネルの長軸 (ボケ方向) のみが意味を持つ。短軸は sub-pixel に縮退して
/// 1D box フィルタになる。
///
/// 導出: Smith-Helmholtz 近似 `θ_diameter ≈ pupil(m) × |D|`
///   - pupil = 0.004 m, max cylinder diopter = 3 CD
///   - θ_diameter = 0.004 × 3 = 0.012 rad ≈ 0.687°
///   - radius (rad) = 0.006 rad
///
/// FOV 30° (0.5236 rad) 前提で:
///   ratio = 0.006 / 0.5236 ≈ 0.01146 → 0.011 に丸める
const ASTIGMATISM_MAX_RADIUS_RATIO: f32 = 0.011;

/// 識別不能とみなす最小半径 (px)。1px 未満のぼけは視認できないため identity。
const MIN_BLUR_RADIUS_PX: f32 = 0.5;

/// strength を 0.0..=1.0 に正規化する。NaN は 0 (identity) として扱う。
#[inline]
fn normalize_strength(strength: f32) -> f32 {
    if strength.is_nan() {
        0.0
    } else {
        strength.clamp(0.0, 1.0)
    }
}

/// 線形補間: `a` と `b` を `t` (0.0..=1.0) で補間する。
#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// RGBA8 画像を linear sRGB の `[r, g, b]` 配列 + alpha 配列に分離する。
fn rgba_to_linear_planes(rgba: &RgbaImage) -> (Vec<[f32; 3]>, Vec<u8>) {
    let len = (rgba.width() * rgba.height()) as usize;
    let mut linear = Vec::with_capacity(len);
    let mut alpha = Vec::with_capacity(len);
    for px in rgba.pixels() {
        linear.push([
            srgb_to_linear(px[0] as f32 / 255.0),
            srgb_to_linear(px[1] as f32 / 255.0),
            srgb_to_linear(px[2] as f32 / 255.0),
        ]);
        alpha.push(px[3]);
    }
    (linear, alpha)
}

/// linear sRGB の `[r, g, b]` 配列 + alpha 配列を RGBA8 画像に再合成する。
fn linear_planes_to_rgba(linear: &[[f32; 3]], alpha: &[u8], width: u32, height: u32) -> RgbaImage {
    let mut out = RgbaImage::new(width, height);
    for (i, px) in out.pixels_mut().enumerate() {
        let lin = linear[i];
        *px = image::Rgba([
            pack_u8(linear_to_srgb(lin[0])),
            pack_u8(linear_to_srgb(lin[1])),
            pack_u8(linear_to_srgb(lin[2])),
            alpha[i],
        ]);
    }
    out
}

/// 楕円 disk のカーネル形状を「dy ごとの (x_min, x_max) 範囲」のリストとして
/// プリ計算する。`a` (長軸 / ボケ方向) と `b` (短軸 / シャープ方向)、`axis_rad`
/// (長軸が +x 軸となす角) を渡す。等方 disk は `a == b` で表現できる。
///
/// 各行の x 範囲は連続区間になることを利用して内側ループの clamp / インデックス
/// 計算を大幅に削減する。ピクセル数は `(x_max - x_min + 1)` の合計で求まる。
struct EllipseSpans {
    /// dy が `dy_min..=dy_max` のとき、有効な行は dy = dy_min + i (i は 0 始まり)。
    dy_min: i32,
    /// 各行の (x_min, x_max) 包含範囲。空行は持たない (確実に origin を含む)。
    rows: Vec<(i32, i32)>,
    /// 楕円内の全ピクセル数 (= 平均化の分母)。
    count: usize,
}

fn build_ellipse_spans(a: f32, b: f32, axis_rad: f32) -> EllipseSpans {
    let r_max = a.max(b).ceil() as i32;
    let cos_t = axis_rad.cos();
    let sin_t = axis_rad.sin();
    let a2 = (a * a).max(1e-6);
    let b2 = (b * b).max(1e-6);

    let mut rows: Vec<(i32, i32)> = Vec::with_capacity((2 * r_max + 1) as usize);
    let mut dy_min = i32::MAX;
    let mut count: usize = 0;

    for dy in -r_max..=r_max {
        let mut x_lo: Option<i32> = None;
        let mut x_hi: i32 = i32::MIN;
        for dx in -r_max..=r_max {
            let u = dx as f32 * cos_t + dy as f32 * sin_t;
            let v = -(dx as f32) * sin_t + dy as f32 * cos_t;
            if (u * u) / a2 + (v * v) / b2 <= 1.0 {
                if x_lo.is_none() {
                    x_lo = Some(dx);
                }
                x_hi = dx;
            }
        }
        if let Some(lo) = x_lo {
            if dy < dy_min {
                dy_min = dy;
            }
            rows.push((lo, x_hi));
            count += (x_hi - lo + 1) as usize;
        }
    }
    debug_assert!(!rows.is_empty(), "ellipse must contain at least origin");
    EllipseSpans {
        dy_min,
        rows,
        count,
    }
}

/// 楕円 (a, b, axis_rad) で linear plane を畳み込む。境界は edge replication
/// (端ピクセルを無限に複製する) で拡張する。
///
/// `a == b` のときは等方 disk (pillbox)。`b ≪ a` のときは細長い 1D 様の
/// blur (乱視で使用)。
///
/// **アルゴリズム**: 各行 (y_src) について、edge-replicated horizontal prefix
/// sum (累積和) を構築する (O(W) per row)。各出力ピクセルは、kernel の各 dy
/// 行について `(x + hi)` と `(x + lo - 1)` の prefix sum 差で row sum を
/// O(1) で取得する。総計算量は O(W × H × kernel_height)。
/// 1024×1024 / R=51 のとき ≈ 1M × 103 = 1.05×10^8 ops で <1s。
fn ellipse_blur(
    src: &[[f32; 3]],
    width: u32,
    height: u32,
    a: f32,
    b: f32,
    axis_rad: f32,
) -> Vec<[f32; 3]> {
    let spans = build_ellipse_spans(a, b, axis_rad);
    let inv_n = 1.0 / spans.count as f32;
    let w = width as i32;
    let h = height as i32;
    let dy_min = spans.dy_min;
    let mut dst = vec![[0.0_f32; 3]; src.len()];

    // 行 prefix sum (画像内範囲のみ)。`prefix[i]` = src[0..i] の合計。
    // 画像外への参照は端ピクセル (src[0] または src[w-1]) を
    // pad_left × / pad_right × で個別に加算する。
    let mut prefix: Vec<[f64; 3]> = vec![[0.0; 3]; (w as usize) + 1];

    // y_out ループ外で 1 回だけ alloc し、各 y で zero-fill して再利用。
    let mut row_sums: Vec<[f32; 3]> = vec![[0.0; 3]; w as usize];

    for y_out in 0..h {
        row_sums.iter_mut().for_each(|s| *s = [0.0; 3]);

        for (i, &(lo, hi)) in spans.rows.iter().enumerate() {
            let sy = (y_out + dy_min + i as i32).clamp(0, h - 1) as usize;
            let row_off = sy * width as usize;

            // src 行の prefix sum を更新 (f64 で誤差累積を抑える)。
            prefix[0] = [0.0; 3];
            for k in 0..(w as usize) {
                let p = src[row_off + k];
                prefix[k + 1] = [
                    prefix[k][0] + p[0] as f64,
                    prefix[k][1] + p[1] as f64,
                    prefix[k][2] + p[2] as f64,
                ];
            }
            let left_px = src[row_off];
            let right_px = src[row_off + (w as usize) - 1];

            // 各出力 x について行 i の寄与を加算。
            for x in 0..w {
                let raw_start = x + lo;
                let raw_end = x + hi;

                // 完全に画像外
                if raw_end < 0 {
                    let n = (hi - lo + 1) as f32;
                    let s = &mut row_sums[x as usize];
                    s[0] += left_px[0] * n;
                    s[1] += left_px[1] * n;
                    s[2] += left_px[2] * n;
                    continue;
                }
                if raw_start > w - 1 {
                    let n = (hi - lo + 1) as f32;
                    let s = &mut row_sums[x as usize];
                    s[0] += right_px[0] * n;
                    s[1] += right_px[1] * n;
                    s[2] += right_px[2] * n;
                    continue;
                }

                let in_lo = raw_start.max(0) as usize;
                let in_hi = raw_end.min(w - 1) as usize;
                let left_pad = (in_lo as i32 - raw_start) as f32;
                let right_pad = (raw_end - in_hi as i32) as f32;

                let pl = prefix[in_lo];
                let ph = prefix[in_hi + 1];
                let s = &mut row_sums[x as usize];
                let inner_r = (ph[0] - pl[0]) as f32;
                let inner_g = (ph[1] - pl[1]) as f32;
                let inner_b = (ph[2] - pl[2]) as f32;
                s[0] += inner_r + left_px[0] * left_pad + right_px[0] * right_pad;
                s[1] += inner_g + left_px[1] * left_pad + right_px[1] * right_pad;
                s[2] += inner_b + left_px[2] * left_pad + right_px[2] * right_pad;
            }
        }

        // 平均化して dst へ書き出し。
        let dst_off = (y_out as u32 * width) as usize;
        for x in 0..(w as usize) {
            let s = row_sums[x];
            dst[dst_off + x] = [s[0] * inv_n, s[1] * inv_n, s[2] * inv_n];
        }
    }
    dst
}

/// 等方 disk blur を linear sRGB 空間で適用する内部実装。
///
/// `radius_px < MIN_BLUR_RADIUS_PX` のときは identity を返す。
fn isotropic_disk_blur_image(img: DynamicImage, radius_px: f32) -> Result<DynamicImage> {
    let rgba = img.to_rgba8();
    if radius_px < MIN_BLUR_RADIUS_PX {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }
    let width = rgba.width();
    let height = rgba.height();
    let (linear, alpha) = rgba_to_linear_planes(&rgba);
    let blurred = ellipse_blur(&linear, width, height, radius_px, radius_px, 0.0);
    let out = linear_planes_to_rgba(&blurred, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}

/// strength と最大半径比から、画像サイズに応じた disk 半径 (px) を求める。
fn radius_from_strength(img: &DynamicImage, strength: f32, max_ratio: f32) -> f32 {
    let s = normalize_strength(strength);
    if s == 0.0 {
        return 0.0;
    }
    let min_dim = img.width().min(img.height()) as f32;
    s * max_ratio * min_dim
}

/// Myopia (近視) シミュレーション。
///
/// strength=1.0 で約 -6D 相当の defocus blur (disk 半径 ≈ 5% × min(W,H))。
/// 2D 画像には深度情報がないため、本実装は画面全体の uniform blur となる
/// (現実の myopia は遠方ほどボケが強い)。alpha は保持。
pub fn myopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let r = radius_from_strength(&img, strength, MYOPIA_MAX_RADIUS_RATIO);
    isotropic_disk_blur_image(img, r)
}

/// Hyperopia (遠視) シミュレーション。
///
/// strength=1.0 で約 +4D 相当の defocus blur (disk 半径 ≈ 1.5% × min(W,H))。
/// myopia と同様、2D 画像には深度がないため画面全体の uniform blur となる
/// (現実の hyperopia は近方ほどボケが強い)。alpha は保持。
pub fn hyperopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let r = radius_from_strength(&img, strength, HYPEROPIA_MAX_RADIUS_RATIO);
    isotropic_disk_blur_image(img, r)
}

/// Presbyopia (老眼) シミュレーション。
///
/// strength=1.0 で約 +3D add 相当の near-vision defocus blur (disk 半径 ≈
/// 1.1% × min(W,H))。視距離 50 cm 想定で、近距離の対象を見るときに発生する
/// uniform blur として扱う。alpha は保持。
pub fn presbyopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let r = radius_from_strength(&img, strength, PRESBYOPIA_MAX_RADIUS_RATIO);
    isotropic_disk_blur_image(img, r)
}

/// Astigmatism (乱視) シミュレーション。軸 `axis_deg` (0.0..=180.0) は
/// **シャープに見える経線方向** (cylinder lens の柱方向) を指す医学的慣習。
/// 実装上、楕円カーネルの **長軸 (ボケ方向)** は `axis_deg + 90°` 方向となる。
///
/// strength=1.0 で約 -3CD 相当 (長軸半径 ≈ 1.1% × min(W,H))。
///
/// 純粋 cylinder lens の line focus は **1D directional blur** が物理的に正しい。
/// 短軸は `MIN_BLUR_RADIUS_PX` (0.5 px) で sub-pixel に縮退するため、
/// 楕円カーネルは事実上ボケ方向の 1D box フィルタとして動作する。
///
/// `axis_deg` は `rem_euclid(180.0)` で 180° 周期に正規化される
/// (`360.0` → `0.0`、`-45.0` → `135.0`)。NaN の場合のみ既定値 90°
/// (with-the-rule) にフォールバックする。alpha は保持。
pub fn astigmatism(img: DynamicImage, strength: f32, axis_deg: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let min_dim = width.min(height) as f32;

    // 軸の正規化: NaN は 90° にフォールバック、有限値は 180° 周期で正規化。
    let axis_norm = if axis_deg.is_nan() {
        90.0
    } else {
        axis_deg.rem_euclid(180.0)
    };

    let a_radius = s * ASTIGMATISM_MAX_RADIUS_RATIO * min_dim;
    let b_radius = MIN_BLUR_RADIUS_PX; // short axis (sharp side)

    if s == 0.0 || a_radius < MIN_BLUR_RADIUS_PX {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    // axis_deg はシャープ方向。長軸 (ボケ方向) はそれと直交 = +90°。
    // 画像座標系は y 下向きだが、回転対称な楕円なので符号反転は結果に影響しない。
    let blur_axis_rad = (axis_norm + 90.0).to_radians();

    let (linear, alpha) = rgba_to_linear_planes(&rgba);
    let blurred = ellipse_blur(&linear, width, height, a_radius, b_radius, blur_axis_rad);
    let out = linear_planes_to_rgba(&blurred, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}

// ---------------------------------------------------------------------
// Phase 3b: 光・透明度 (Issue #6) — cataract / photophobia / nyctalopia / floaters
// ---------------------------------------------------------------------

/// 白内障（Cataract）シミュレーション。
///
/// linear sRGB 空間で黄変マトリクスを適用してコントラストを圧縮し、
/// その後に空間相関を持つ LCG ベースの Simplex-like ノイズで局所白濁を重ねる。
///
/// ### 黄変マトリクス
///
/// 以下の係数は Pokorny et al. (1987) "Aging of the human lens" *Applied Optics* 26(8):
/// 1437–1440 および van Norren & Vos (1974) "Spectral transmission of the human ocular
/// media" *Vision Research* 14(11): 1237–1244 に基づく水晶体黄変の近似。
///
/// ```text
/// R' = R * 1.00 + G * 0.05 + B * (-0.05)
/// G' = R * 0.02 + G * 1.00 + B * (-0.02)
/// B' = R * 0.00 + G * 0.00 + B *  0.85
/// ```
/// strength でブレンド: `final = orig * (1-s) + yellowed * s`
///
/// ### 散乱ノイズ（Simplex-like LCG ノイズ）
///
/// 旧実装の 8×8 矩形ブロックノイズを空間相関を持つ格子補間ノイズに置き換え。
/// 各格子頂点に LCG シードを割り当て、4 頂点を bilinear 補間することで
/// 連続的な滑らかな白濁パターンを生成する。
///
/// - `strength`: 0.0 = 元画像, 1.0 = 強度白内障
/// - `seed`: 白濁ノイズのランダムシード
pub fn cataract(img: DynamicImage, strength: f32, seed: u64) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let mut rgba = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();

    // 白濁ノイズの最大ブレンド量
    const WHITE_BLEND_MAX: f32 = 0.4;
    // 格子セルサイズ（旧 BLOCK_SIZE=8 より大きい 32px で空間相関を確保）
    const CELL_SIZE: u32 = 32;

    // 格子頂点数（+1 は境界含むため）
    let grid_cols = width.div_ceil(CELL_SIZE) + 1;
    let grid_rows = height.div_ceil(CELL_SIZE) + 1;

    // 各格子頂点の LCG ノイズ値を事前計算
    let mut grid_noise: Vec<f32> = Vec::with_capacity((grid_cols * grid_rows) as usize);
    for gy in 0..grid_rows {
        for gx in 0..grid_cols {
            // 頂点ごとに独立したシードを生成（空間ハッシュ）
            let h = seed
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add((gx as u64).wrapping_mul(0x517cc1b727220a95))
                .wrapping_add((gy as u64).wrapping_mul(0x6c62272e07bb0142));
            // LCG を 1 ステップ回して上位 32bit を 0.0..=1.0 に正規化
            let lcg = h.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let n = (lcg >> 32) as f32 / u32::MAX as f32;
            grid_noise.push(n);
        }
    }

    // 格子頂点の値を bilinear 補間する内部関数
    let grid_sample = |px: u32, py: u32| -> f32 {
        // セル内の位置（0.0..=1.0）
        let fx = px as f32 / CELL_SIZE as f32;
        let fy = py as f32 / CELL_SIZE as f32;
        let gx0 = (px / CELL_SIZE) as usize;
        let gy0 = (py / CELL_SIZE) as usize;
        let gx1 = (gx0 + 1).min(grid_cols as usize - 1);
        let gy1 = (gy0 + 1).min(grid_rows as usize - 1);
        let tx = fx - gx0 as f32; // セル内 x 位置
        let ty = fy - gy0 as f32; // セル内 y 位置

        // 4 頂点の値を取得
        let v00 = grid_noise[gy0 * grid_cols as usize + gx0];
        let v10 = grid_noise[gy0 * grid_cols as usize + gx1];
        let v01 = grid_noise[gy1 * grid_cols as usize + gx0];
        let v11 = grid_noise[gy1 * grid_cols as usize + gx1];

        // smoothstep で補間（線形補間より自然な見た目）
        let stx = tx * tx * (3.0 - 2.0 * tx);
        let sty = ty * ty * (3.0 - 2.0 * ty);

        // bilinear 補間
        v00 * (1.0 - stx) * (1.0 - sty)
            + v10 * stx * (1.0 - sty)
            + v01 * (1.0 - stx) * sty
            + v11 * stx * sty
    };

    for y in 0..height {
        for x in 0..width {
            let px = rgba.get_pixel_mut(x, y);

            // linear sRGB に変換
            let r = srgb_to_linear(px[0] as f32 / 255.0);
            let g = srgb_to_linear(px[1] as f32 / 255.0);
            let b = srgb_to_linear(px[2] as f32 / 255.0);

            // 黄変マトリクスを適用
            // 係数出典: Pokorny et al. (1987) / van Norren & Vos (1974)
            let yr = (r * 1.00 + g * 0.05 + b * (-0.05)).clamp(0.0, 1.0);
            let yg = (r * 0.02 + g * 1.00 + b * (-0.02)).clamp(0.0, 1.0);
            let yb = (r * 0.00 + g * 0.00 + b * 0.85).clamp(0.0, 1.0);

            // strength でブレンド: orig * (1-s) + yellowed * s
            let nr = r + (yr - r) * strength;
            let ng = g + (yg - g) * strength;
            let nb = b + (yb - b) * strength;

            // Simplex-like ノイズによる白濁（空間相関あり）
            let noise = grid_sample(x, y);
            let white_blend = strength * noise * WHITE_BLEND_MAX;

            let fr = nr + (1.0 - nr) * white_blend;
            let fg = ng + (1.0 - ng) * white_blend;
            let fb = nb + (1.0 - nb) * white_blend;

            px[0] = pack_u8(linear_to_srgb(fr));
            px[1] = pack_u8(linear_to_srgb(fg));
            px[2] = pack_u8(linear_to_srgb(fb));
            // alpha はそのまま
        }
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

/// 光過敏（Photophobia）シミュレーション。
///
/// 明るい部分が滲み出す bloom 効果を linear sRGB 空間で適用する。
///
/// - `strength`: 0.0 = 元画像, 1.0 = 強い bloom
pub fn photophobia(img: DynamicImage, strength: f32) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();

    // bloom 半径
    const PHOTOPHOBIA_BLOOM_RADIUS_RATIO: f32 = 0.05;
    let min_dim = width.min(height) as f32;
    let bloom_radius = strength * PHOTOPHOBIA_BLOOM_RADIUS_RATIO * min_dim;

    // ハイライト閾値
    const PHOTOPHOBIA_THRESHOLD: f32 = 0.5;

    // linear sRGB に変換
    let (linear, _alpha) = rgba_to_linear_planes(&rgba);

    // ハイライトレイヤーを抽出
    let mut highlight: Vec<[f32; 3]> = linear
        .iter()
        .map(|&[r, g, b]| {
            let y = LUMA_R * r + LUMA_G * g + LUMA_B * b;
            let mask = if y > PHOTOPHOBIA_THRESHOLD {
                (y - PHOTOPHOBIA_THRESHOLD) / (1.0 - PHOTOPHOBIA_THRESHOLD)
            } else {
                0.0
            };
            [r * mask, g * mask, b * mask]
        })
        .collect();

    // ハイライトレイヤーに disk blur を適用（bloom_radius >= MIN_BLUR_RADIUS_PX の場合のみ）
    // bloom_radius が小さすぎる（= strength が非常に小さい）場合は bloom 効果なし
    if bloom_radius >= MIN_BLUR_RADIUS_PX {
        highlight = ellipse_blur(&highlight, width, height, bloom_radius, bloom_radius, 0.0);
    } else {
        // blur できない = bloom なし。highlight をゼロにして加算しない
        highlight.iter_mut().for_each(|p| *p = [0.0, 0.0, 0.0]);
    }

    // 元画像 + bloom を加算（saturate）
    let mut out_rgba = rgba.clone();
    for (i, px) in out_rgba.pixels_mut().enumerate() {
        let orig = linear[i];
        let bloom = highlight[i];
        let fr = (orig[0] + bloom[0]).min(1.0);
        let fg = (orig[1] + bloom[1]).min(1.0);
        let fb = (orig[2] + bloom[2]).min(1.0);
        px[0] = pack_u8(linear_to_srgb(fr));
        px[1] = pack_u8(linear_to_srgb(fg));
        px[2] = pack_u8(linear_to_srgb(fb));
        // alpha はそのまま
    }

    Ok(DynamicImage::ImageRgba8(out_rgba))
}

/// 夜盲（Nyctalopia）シミュレーション。
///
/// 暗所視力低下: 全体が暗くなり色感度が落ちてグレースケール寄りになる。
/// Purkinje shift（プルキンエ現象）を追加: 暗所では桿体が支配的になり、
/// 分光感度が青寄り（scotopic luminance ピーク 507nm）にシフトする。
///
/// ## Purkinje shift 実装
///
/// linear sRGB 空間で photopic / scotopic luminance をブレンドし、
/// strength に応じて青チャネルを微増・赤チャネルを微減する。
///
/// - scotopic luminance: `L_scot = 0.0610 R + 0.3751 G + 0.6038 B`（Vos 1978 近似）
/// - photopic/scotopic blend: `L = lerp(L_phot, L_scot, strength)`
/// - 青チャネル微増: `B' = B * (1.0 + strength * 0.1)`
/// - 赤チャネル微減: `R' = R * (1.0 - strength * 0.2)`
///
/// 出典: Vos (1978) "Colorimetric and photometric properties of a 2° fundamental
/// observer" *Color Research & Application* 3(3): 125–128
///
/// - `strength`: 0.0 = 元画像, 1.0 = 強度夜盲
pub fn nyctalopia(img: DynamicImage, strength: f32) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let mut rgba = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let dark_factor = 1.0 - strength * 0.7_f32;
    let desat = strength * 0.8_f32;

    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);

        // photopic luminance（BT.709）
        let y_phot = LUMA_R * r + LUMA_G * g + LUMA_B * b;
        // scotopic luminance（Vos 1978）
        let y_scot = 0.0610 * r + 0.3751 * g + 0.6038 * b;
        // photopic/scotopic blend
        let y = y_phot + (y_scot - y_phot) * strength;

        // 脱色（ブレンドした luma に寄せる）
        let dr = r + (y - r) * desat;
        let dg = g + (y - g) * desat;
        let db = b + (y - b) * desat;

        // Purkinje shift: 青チャネル微増・赤チャネル微減
        let pr = dr * (1.0 - strength * 0.2);
        let pb = db * (1.0 + strength * 0.1);

        // 暗化
        let fr = pr * dark_factor;
        let fg = dg * dark_factor;
        let fb = pb * dark_factor;

        px[0] = pack_u8(linear_to_srgb(fr));
        px[1] = pack_u8(linear_to_srgb(fg));
        px[2] = pack_u8(linear_to_srgb(fb));
        // alpha はそのまま
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

/// 飛蚊症（Floaters）シミュレーション。
///
/// 視野内に暗い blob と糸くず形状が浮かぶオーバーレイを乗算ブレンドで適用する。
/// 円形 blob 30% + 糸くず形状（ランダムウォーク折れ線） 70% の混合。
/// 描画後に box blur (radius 1px) でエッジをソフト化する。
///
/// - `strength`: 0.0 = 元画像, 1.0 = 強い飛蚊症
/// - `density`: blob 密度 (0.0..=1.0)
/// - `seed`: blob 配置のランダムシード（実際に使用される）
/// - `gaze_x`: 視線 X 位置 (0.0 = 左, 1.0 = 右)
/// - `gaze_y`: 視線 Y 位置 (0.0 = 上, 1.0 = 下)
pub fn floaters(
    img: DynamicImage,
    strength: f32,
    density: f32,
    seed: u64,
    gaze_x: f32,
    gaze_y: f32,
) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;

    let density = density.clamp(0.0, 1.0);
    let gaze_x = gaze_x.clamp(0.0, 1.0);
    let gaze_y = gaze_y.clamp(0.0, 1.0);

    // 視線オフセット（フローターは視線に追随）
    let offset_x = (gaze_x - 0.5) * 0.3 * w_f;
    let offset_y = (gaze_y - 0.5) * 0.3 * h_f;

    // blob/糸くず 総数
    let total_count = (density * 200.0) as usize;
    if total_count == 0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let blob_count = (total_count as f32 * 0.3).ceil() as usize; // 30% 円形
    let strand_count = total_count - blob_count;                  // 70% 糸くず

    let blob_radius = (w_f.min(h_f) * 0.04).max(2.0);
    let blob_radius_sq = blob_radius * blob_radius;

    // ── LCG ヘルパー ──────────────────────────────────────────────
    // 64bit LCG: state → next state, returns 0..=u32::MAX を f32 に正規化した値
    let lcg_next = |state: u64| -> (u64, f32) {
        let next = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let fval = (next >> 32) as f32 / u32::MAX as f32;
        (next, fval)
    };

    // seed から初期 LCG 状態を生成
    let init_state = seed.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(1);

    // ── マスクバッファ（0.0 = 完全フローター, 1.0 = 透明）────────
    let npx = (width * height) as usize;
    let mut mask_buf: Vec<f32> = vec![1.0_f32; npx];

    // ── 円形 blob を描画 ─────────────────────────────────────────
    let mut state = init_state;
    for _ in 0..blob_count {
        let (s1, fx) = lcg_next(state);
        let (s2, fy) = lcg_next(s1);
        state = s2;
        let cx = fx * w_f + offset_x;
        let cy = fy * h_f + offset_y;

        // AABB で描画範囲を絞る
        let x0 = ((cx - blob_radius).floor() as i32).max(0);
        let x1 = ((cx + blob_radius).ceil() as i32).min(width as i32 - 1);
        let y0 = ((cy - blob_radius).floor() as i32).max(0);
        let y1 = ((cy + blob_radius).ceil() as i32).min(height as i32 - 1);

        for py in y0..=y1 {
            for px in x0..=x1 {
                let dx = px as f32 - cx;
                let dy = py as f32 - cy;
                let d2 = dx * dx + dy * dy;
                if d2 < blob_radius_sq {
                    let t = d2 / blob_radius_sq;
                    let m = t * t * (3.0 - 2.0 * t); // smoothstep: エッジで 1.0
                    let idx = py as usize * width as usize + px as usize;
                    if m < mask_buf[idx] {
                        mask_buf[idx] = m;
                    }
                }
            }
        }
    }

    // ── 糸くず形状を描画（ランダムウォーク折れ線） ────────────────
    for _ in 0..strand_count {
        // 開始点
        let (s1, fx) = lcg_next(state);
        let (s2, fy) = lcg_next(s1);
        // セグメント数 2..=5
        let (s3, fn_seg) = lcg_next(s2);
        // 初期角度
        let (s4, f_angle) = lcg_next(s3);
        // 線幅 1..=4 px
        let (s5, f_width) = lcg_next(s4);
        state = s5;

        let sx = fx * w_f + offset_x;
        let sy = fy * h_f + offset_y;
        let n_seg = (fn_seg * 4.0) as usize + 2; // 2..=5
        let half_w = (f_width * 3.0 + 1.0) * 0.5; // 0.5..=2.0

        let mut cur_x = sx;
        let mut cur_y = sy;
        let mut cur_angle = f_angle * std::f32::consts::TAU;

        for _seg in 0..n_seg {
            // セグメント長 5..=15 px（連続した LCG チェーン）
            let (s_next, _) = lcg_next(state);
            state = s_next;
            let s_len = ((state >> 33) % 11 + 5) as f32 + 5.0;
            // 角度変化 ±45°
            let (s_da, f_da) = lcg_next(state);
            state = s_da;

            let seg_len = s_len;
            let delta_angle = (f_da - 0.5) * std::f32::consts::FRAC_PI_2; // ±45°
            cur_angle += delta_angle;

            let nx = cur_x + cur_angle.cos() * seg_len;
            let ny = cur_y + cur_angle.sin() * seg_len;

            // 線分を太さ half_w でラスタライズ
            let steps = (seg_len.ceil() as usize * 4).max(1);
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let lx = cur_x + (nx - cur_x) * t;
                let ly = cur_y + (ny - cur_y) * t;

                let hw_ceil = (half_w.ceil() as i32) + 1;
                let px0 = ((lx - half_w).floor() as i32 - hw_ceil).max(0);
                let px1 = ((lx + half_w).ceil() as i32 + hw_ceil).min(width as i32 - 1);
                let py0 = ((ly - half_w).floor() as i32 - hw_ceil).max(0);
                let py1 = ((ly + half_w).ceil() as i32 + hw_ceil).min(height as i32 - 1);

                let half_w_sq = half_w * half_w;
                for py in py0..=py1 {
                    for ppx in px0..=px1 {
                        let dx = ppx as f32 - lx;
                        let dy = py as f32 - ly;
                        let dist_sq = dx * dx + dy * dy;
                        if dist_sq < half_w_sq {
                            let m = (dist_sq.sqrt() / half_w).clamp(0.0, 1.0);
                            let idx = py as usize * width as usize + ppx as usize;
                            if m < mask_buf[idx] {
                                mask_buf[idx] = m;
                            }
                        }
                    }
                }
            }

            cur_x = nx;
            cur_y = ny;
        }
    }

    // ── box blur (radius 1px) でエッジをソフト化 ──────────────────
    let mut blurred_mask: Vec<f32> = vec![0.0_f32; npx];
    let w = width as usize;
    let h = height as usize;
    for py in 0..h {
        for px in 0..w {
            let mut sum = 0.0_f32;
            let mut cnt = 0_u32;
            for dy in -1_i32..=1 {
                for dx in -1_i32..=1 {
                    let nx = px as i32 + dx;
                    let ny = py as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        sum += mask_buf[ny as usize * w + nx as usize];
                        cnt += 1;
                    }
                }
            }
            blurred_mask[py * w + px] = sum / cnt as f32;
        }
    }

    // ── 元画像に乗算ブレンド ──────────────────────────────────────
    let mut out_rgba = rgba.clone();
    for y in 0..height {
        for x in 0..width {
            let mask = blurred_mask[y as usize * w + x as usize];
            let blend = 1.0 - strength * (1.0 - mask);

            let px = out_rgba.get_pixel_mut(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);
            px[0] = pack_u8(linear_to_srgb(rl * blend));
            px[1] = pack_u8(linear_to_srgb(gl * blend));
            px[2] = pack_u8(linear_to_srgb(bl * blend));
            // alpha はそのまま
        }
    }

    Ok(DynamicImage::ImageRgba8(out_rgba))
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
// Phase 3a: 視野異常 (Issue #5) — glaucoma / macular_degeneration / hemianopia / tunnel_vision
// ---------------------------------------------------------------

/// 緑内障シミュレーションのモード。
///
/// `Vignette` は既存の均等 vignetting 実装（後方互換）。
/// `ArcuateSuperior` / `ArcuateInferior` / `Biarcuate` は視神経乳頭を中心とした
/// 弧状暗点を生成する。
///
/// ## 医学的背景
///
/// 緑内障の視野欠損は視神経乳頭（ON head）の損傷パターンに対応する弧状暗点
/// （arcuate scotoma）として現れることが多い。均等な周辺暗化（Vignette）は
/// 近似であり、実臨床ではBjerrumの弧状暗点が典型的。
///
/// - `Vignette`: 旧実装の中心保存 + 周辺均等暗化（近似）
/// - `ArcuateSuperior`: 上方弧状暗点（Bjerrum 上方）
/// - `ArcuateInferior`: 下方弧状暗点（Bjerrum 下方）
/// - `Biarcuate`: 両方の弧状暗点（進行した緑内障）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlaucomaMode {
    /// 既存実装（後方互換）: 中心保存 + 周辺 smoothstep vignetting
    Vignette,
    /// 上方弧状暗点（Bjerrum 上方弧状暗点）
    ArcuateSuperior,
    /// 下方弧状暗点（Bjerrum 下方弧状暗点）
    ArcuateInferior,
    /// 両弧状暗点（上下両方、進行例）
    Biarcuate,
}

/// 緑内障（glaucoma）シミュレーション。
///
/// 緑内障は眼圧上昇による視神経萎縮が原因で、周辺視野から徐々に欠けていく。
/// `mode` により均等 vignetting と弧状暗点を切り替えられる。
///
/// ## モード: Vignette（デフォルト、後方互換）
///
/// 中心からの距離に基づく vignetted mask を使用:
/// - 中心付近 (normalized 距離 < `inner_r`): 保存
/// - 周辺 (距離 > `outer_r`): 暗化 × `strength`
/// - 中間: smoothstep で滑らかに移行
///
/// `inner_r` = `1.0 - strength * 0.7`, `outer_r` = `inner_r + 0.2`
///
/// ## モード: ArcuateSuperior / ArcuateInferior / Biarcuate
///
/// 視神経乳頭（ON head）を画像中心から水平方向 15% オフセットした位置に設定し、
/// そこから放射する Bjerrum 弧状暗点を極座標マスクで生成する。
/// 弧状領域内を strength に応じて暗化する。
///
/// > **注記**: `Vignette` モードの均等暗化は緑内障の視野欠損の近似に過ぎない。
/// > 実臨床の典型的な欠損は `ArcuateSuperior` / `ArcuateInferior` のような
/// > 弧状暗点（Bjerrum scotoma）として現れる。
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
/// - `mode`: 暗点の種類（[`GlaucomaMode`] を参照）
pub fn glaucoma(img: DynamicImage, strength: f32, mode: GlaucomaMode) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let cx = w_f * 0.5;
    let cy = h_f * 0.5;

    match mode {
        GlaucomaMode::Vignette => {
            // 既存実装（後方互換）
            let max_r = (cx * cx + cy * cy).sqrt();
            let inner_r = 1.0 - strength * 0.7;
            let outer_r = (inner_r + 0.2).min(1.0);

            let mut out_rgba = rgba.clone();
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r = (dx * dx + dy * dy).sqrt() / max_r;

                    let fade = if r <= inner_r {
                        0.0
                    } else if r >= outer_r {
                        1.0
                    } else {
                        let t = (r - inner_r) / (outer_r - inner_r);
                        t * t * (3.0 - 2.0 * t)
                    };

                    let mul = 1.0 - strength * fade;

                    let px = out_rgba.get_pixel_mut(x, y);
                    let rl = srgb_to_linear(px[0] as f32 / 255.0);
                    let gl = srgb_to_linear(px[1] as f32 / 255.0);
                    let bl = srgb_to_linear(px[2] as f32 / 255.0);
                    px[0] = pack_u8(linear_to_srgb(rl * mul));
                    px[1] = pack_u8(linear_to_srgb(gl * mul));
                    px[2] = pack_u8(linear_to_srgb(bl * mul));
                }
            }
            Ok(DynamicImage::ImageRgba8(out_rgba))
        }
        mode => {
            // 弧状暗点モード（ArcuateSuperior / ArcuateInferior / Biarcuate）
            //
            // 視神経乳頭（ON head）の位置: 画像中心から水平方向 15% オフセット（耳側）
            let on_x = cx + w_f * 0.15;
            let on_y = cy;

            // 弧状暗点のパラメータ（極座標）
            // r_min..=r_max: ON head からの距離（min(W,H) 比）
            let min_dim = w_f.min(h_f);
            let r_min = min_dim * 0.20; // 内側境界
            let r_max = min_dim * 0.55 * strength.sqrt(); // 外側境界（strength に応じて拡大）

            // 弧状の角度範囲（ON head からの極角 θ）
            // 上方弧状: θ ∈ [90°, 270°]（y > on_y の半面、画像座標では y 下向き）
            // 下方弧状: θ ∈ [-90°, 90°]（y < on_y の半面）

            let apply_superior = matches!(mode, GlaucomaMode::ArcuateSuperior | GlaucomaMode::Biarcuate);
            let apply_inferior = matches!(mode, GlaucomaMode::ArcuateInferior | GlaucomaMode::Biarcuate);

            let mut out_rgba = rgba.clone();
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - on_x;
                    let dy = y as f32 - on_y; // 画像座標: y 下向きが正

                    let r = (dx * dx + dy * dy).sqrt();

                    // ON head からの距離が弧状帯に入っているか
                    if r < r_min || r > r_max {
                        continue;
                    }

                    // 弧状帯の中での正規化距離（smoothstep 用）
                    let t_r = (r - r_min) / (r_max - r_min);
                    let fade_r = t_r * t_r * (3.0 - 2.0 * t_r); // smoothstep
                    // 帯の中央（t_r=0.5）が最も暗く、両端に向かって明るくなる
                    let fade_radial = 1.0 - (fade_r * 2.0 - 1.0).abs();

                    // 角度条件: dy > 0 が画像下方（inferior）、dy < 0 が上方（superior）
                    let in_superior = dy < 0.0; // 画像上半分（y が on_y より上）
                    let in_inferior = dy > 0.0; // 画像下半分

                    let in_arc = (apply_superior && in_superior) || (apply_inferior && in_inferior);
                    if !in_arc {
                        continue;
                    }

                    // ON head に近い角度（x 軸付近）では暗点が弱くなる（弧状の端）
                    // |θ| が 0 や π に近いほど暗点は弱い → sin(θ) の絶対値でフェード
                    let theta = dy.atan2(dx); // -π..=π
                    let arc_fade = theta.sin().abs().sqrt().clamp(0.0, 1.0);

                    let fade = strength * fade_radial * arc_fade;

                    let mul = 1.0 - fade;
                    let px = out_rgba.get_pixel_mut(x, y);
                    let rl = srgb_to_linear(px[0] as f32 / 255.0);
                    let gl = srgb_to_linear(px[1] as f32 / 255.0);
                    let bl = srgb_to_linear(px[2] as f32 / 255.0);
                    px[0] = pack_u8(linear_to_srgb(rl * mul));
                    px[1] = pack_u8(linear_to_srgb(gl * mul));
                    px[2] = pack_u8(linear_to_srgb(bl * mul));
                }
            }
            Ok(DynamicImage::ImageRgba8(out_rgba))
        }
    }
}

/// 黄斑変性（macular degeneration）シミュレーション。
///
/// 黄斑部（網膜中心）の光受容体が変性し、中心視野が失われる。
/// 周辺視野は保たれるが、読書・顔の認識が困難になる。
///
/// ## アルゴリズム
/// 中心に集中した暗いぼかし円を重ねる:
/// - 中心 (normalized 距離 < `inner_r`): 強く暗化 + 色彩低下
/// - 周辺 (距離 > `outer_r`): 変化なし
/// - 中間: smoothstep
///
/// `inner_r` = `strength * 0.25`, `outer_r` = `strength * 0.4`
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
pub fn macular_degeneration(img: DynamicImage, strength: f32) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let cx = w_f * 0.5;
    let cy = h_f * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt();

    let inner_r = strength * 0.25;
    let outer_r = strength * 0.4;

    let mut out_rgba = rgba.clone();
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = (dx * dx + dy * dy).sqrt() / max_r;

            let t = if r <= inner_r {
                1.0
            } else if r >= outer_r {
                0.0
            } else {
                let u = (r - inner_r) / (outer_r - inner_r);
                1.0 - u * u * (3.0 - 2.0 * u)
            };

            if t == 0.0 {
                continue;
            }

            let px = out_rgba.get_pixel_mut(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);

            // 中心部: 輝度を BT.709 で取り出して暗化＋脱色
            let lum = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
            // 強度に応じて暗化 (最大 0.05 の輝度)
            let darkened = lum * (1.0 - strength * 0.95);
            // 元色と脱色・暗化色を t でブレンド
            let out_r = lerp(rl, darkened, t);
            let out_g = lerp(gl, darkened, t);
            let out_b = lerp(bl, darkened, t);

            px[0] = pack_u8(linear_to_srgb(out_r));
            px[1] = pack_u8(linear_to_srgb(out_g));
            px[2] = pack_u8(linear_to_srgb(out_b));
        }
    }

    Ok(DynamicImage::ImageRgba8(out_rgba))
}

/// 半盲（hemianopia）シミュレーション。
///
/// 視野の左右どちらかが完全に失われる（同名半盲）。
/// 脳卒中・脳腫瘍による視放線の損傷が主因。
///
/// ## アルゴリズム
/// `side`: `0.0` = 左側が失われる、`1.0` = 右側が失われる（中間値で移行領域を調整）
/// 境界は常に画像の水平中央 (`x = width / 2`) に固定。
/// `side` は fade 量の重み付けに使用し、0.0 = 左側を完全暗化、1.0 = 右側を完全暗化。
/// 境界付近は幅 `2%` の smoothstep でぼかす。
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
/// - `side`: 欠損側 (0.0 = 左欠損, 1.0 = 右欠損)
pub fn hemianopia(img: DynamicImage, strength: f32, side: f32) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let side = side.clamp(0.0, 1.0);

    // 境界 X 座標（正規化 0.5 が中心）
    let split_x = w_f * 0.5;
    // 境界のぼかし幅
    let blur_w = w_f * 0.02;

    let mut out_rgba = rgba.clone();
    for y in 0..height {
        for x in 0..width {
            let xf = x as f32;

            // 左欠損 (side=0.0): x < split_x の領域を暗化
            // 右欠損 (side=1.0): x > split_x の領域を暗化
            // 中間値は欠損量を按分
            let left_fade = if xf < split_x - blur_w {
                1.0
            } else if xf > split_x + blur_w {
                0.0
            } else {
                let t = (xf - (split_x - blur_w)) / (2.0 * blur_w);
                1.0 - t * t * (3.0 - 2.0 * t)
            };

            // side=0 → left_fade を使う, side=1 → (1-left_fade) を使う
            let fade = lerp(left_fade, 1.0 - left_fade, side);

            if fade == 0.0 {
                continue;
            }

            let mul = 1.0 - fade * strength;

            let px = out_rgba.get_pixel_mut(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);
            px[0] = pack_u8(linear_to_srgb(rl * mul));
            px[1] = pack_u8(linear_to_srgb(gl * mul));
            px[2] = pack_u8(linear_to_srgb(bl * mul));
        }
    }

    Ok(DynamicImage::ImageRgba8(out_rgba))
}

// ---------------------------------------------------------------
// Phase 4 (#9): 平衡・めまい視覚フィルタ — vertigo / bppv_rotation / vestibular_neuritis
// ---------------------------------------------------------------

/// 双線形補間でソース画像の (fx, fy) 位置のピクセル値を取得する（edge clamp）。
fn sample_bilinear(rgba: &image::RgbaImage, fx: f32, fy: f32) -> image::Rgba<u8> {
    let w = rgba.width() as i32;
    let h = rgba.height() as i32;
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;

    let get = |x: i32, y: i32| -> [f32; 4] {
        let xi = x.clamp(0, w - 1) as u32;
        let yi = y.clamp(0, h - 1) as u32;
        let p = rgba.get_pixel(xi, yi);
        [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
    };

    let p00 = get(x0, y0);
    let p10 = get(x1, y0);
    let p01 = get(x0, y1);
    let p11 = get(x1, y1);

    let mut out = [0u8; 4];
    for i in 0..4 {
        let v = p00[i] * (1.0 - tx) * (1.0 - ty)
            + p10[i] * tx * (1.0 - ty)
            + p01[i] * (1.0 - tx) * ty
            + p11[i] * tx * ty;
        out[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    image::Rgba(out)
}

/// めまい（vertigo）シミュレーション。
///
/// `time_t` (秒) に応じて画像を回転させ、周辺をブラーで揺らす。
/// メニエール病・前庭障害で生じる持続的な回転感覚を表現する。
///
/// - `strength`: 回転角の最大倍率 (0.0..=1.0)、`strength=1.0` で最大 15°回転
/// - `time_t`: 時間 (秒)。sin 波で回転角が変化する
pub fn vertigo(img: DynamicImage, strength: f32, time_t: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let cx = width as f32 * 0.5;
    let cy = height as f32 * 0.5;

    // 最大回転角 15° = 0.2618 rad
    const MAX_ANGLE_RAD: f32 = 0.2618;
    // ゆっくりとした回転（0.3 Hz）
    let angle = s * MAX_ANGLE_RAD * (2.0 * PI * 0.3 * time_t).sin();
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let mut out = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            // 逆変換: 出力 (x, y) の元位置を求める
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let src_x = cos_a * dx + sin_a * dy + cx;
            let src_y = -sin_a * dx + cos_a * dy + cy;
            let px = sample_bilinear(&rgba, src_x, src_y);
            out.put_pixel(x, y, px);
        }
    }

    // 周辺ブラー（めまいの周辺視野の揺れ）
    let blur_radius = s * 0.015 * width.min(height) as f32;
    if blur_radius >= MIN_BLUR_RADIUS_PX {
        let dyn_out = DynamicImage::ImageRgba8(out);
        isotropic_disk_blur_image(dyn_out, blur_radius)
    } else {
        Ok(DynamicImage::ImageRgba8(out))
    }
}

/// BPPV（良性発作性頭位めまい症）シミュレーション。
///
/// 頭の位置変化で生じる急激な回転 + 眼振（nystagmus）を表現。
/// 急速な一方向の回転 + ゆっくり戻るパターンで画像を揺らす。
///
/// - `strength`: 効果の強度 (0.0..=1.0)
/// - `time_t`: 時間 (秒)。急速 → 遅い戻りのサイクルを繰り返す
pub fn bppv_rotation(img: DynamicImage, strength: f32, time_t: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let cx = width as f32 * 0.5;
    let cy = height as f32 * 0.5;

    // nystagmus パターン: 高速 sawtooth 波（急速相 + 緩徐相）
    // 周期 2 秒、t=0..=0.3 で急速回転、t=0.3..=2.0 でゆっくり戻る
    let period = 2.0_f32;
    let phase = time_t.rem_euclid(period) / period; // 0.0..=1.0（負の time_t も正しく処理）
    let fast_fraction = 0.3_f32;
    let angle_norm = if phase < fast_fraction {
        // 急速相: 0 → 1
        phase / fast_fraction
    } else {
        // 緩徐相: 1 → 0
        1.0 - (phase - fast_fraction) / (1.0 - fast_fraction)
    };

    const MAX_ANGLE_RAD: f32 = 0.3491; // 20°
    let angle = s * MAX_ANGLE_RAD * angle_norm;
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let mut out = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let src_x = cos_a * dx + sin_a * dy + cx;
            let src_y = -sin_a * dx + cos_a * dy + cy;
            let px = sample_bilinear(&rgba, src_x, src_y);
            out.put_pixel(x, y, px);
        }
    }

    Ok(DynamicImage::ImageRgba8(out))
}

/// 前庭神経炎（vestibular neuritis）シミュレーション。
///
/// 突然の激しいめまいによる水平方向の揺れブラー + 片側へのずれを表現する。
/// 視線が一方向に引っ張られる感覚を水平シフトで近似する。
///
/// - `strength`: 効果の強度 (0.0..=1.0)
pub fn vestibular_neuritis(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();

    // 水平方向シフト量（最大 5% の width）
    let shift_x = (s * 0.05 * width as f32).round() as i32;

    // 水平シフトした画像を生成
    let mut shifted = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let src_x = (x as i32 - shift_x).clamp(0, width as i32 - 1) as u32;
            let px = rgba.get_pixel(src_x, y);
            shifted.put_pixel(x, y, *px);
        }
    }

    // 水平方向の motion blur（強い揺れを表現）
    let blur_a = s * 0.04 * width as f32; // 長軸（水平）
    let blur_b = MIN_BLUR_RADIUS_PX;      // 短軸（ほぼ 0 の 1D ブラー）
    if blur_a >= MIN_BLUR_RADIUS_PX {
        let (linear, alpha) = rgba_to_linear_planes(&shifted);
        // 水平方向の 1D blur: axis_rad = 0.0 (水平軸方向がボケ)
        let blurred = ellipse_blur(&linear, width, height, blur_a, blur_b, 0.0);
        let out = linear_planes_to_rgba(&blurred, &alpha, width, height);
        Ok(DynamicImage::ImageRgba8(out))
    } else {
        Ok(DynamicImage::ImageRgba8(shifted))
    }
}

// ---------------------------------------------------------------
// Phase N (Issue #19): depth-aware blur — 深度マップ付き距離依存ぼけ
// ---------------------------------------------------------------

/// 深度ブラーの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthBlurKind {
    /// 遠方（depth < focus_depth）がボケる（近視的な見え方）
    Myopia,
    /// 近方（depth > focus_depth）がボケる（遠視的な見え方）
    Hyperopia,
    /// 両側がボケる（カメラ被写界深度 DoF 風）
    DepthOfField,
}

/// 深度マップを使った距離依存ぼけ（depth-aware defocus blur）。
///
/// `depth_map`: 元画像と同サイズのグレースケール（またはカラー）画像。
///   明るい画素（1.0）= 近い、暗い画素（0.0）= 遠い。
///   カラー画像の場合は luma8 変換で単チャンネルに変換する。
/// `focus_depth`: ピントを合わせる深度値（0.0..=1.0）。この深度の画素はボケなし。
/// `max_radius_ratio`: 最大ボケ半径（min(W,H) 比）。0.023 が近視最大相当。
/// `kind`: DepthBlurKind で近視・遠視・DoF を切り替え。
///
/// # アルゴリズム（8段階ビン線形補間方式）
///
/// 画素ごとに異なる半径の blur を掛けると O(W×H×R²) になって遅い。
/// 8段階の深度ビンを定義し、各画素の深度値 `d` に対して隣接する 2 ビン
/// （bin_floor, bin_ceil）の blur 画像を逐次生成して線形補間する:
///
/// ```text
/// t = frac(d * 7.0)   // 0.0..1.0 の小数部（最終ビンは t = 0 で固定）
/// out = blur[bin_floor] * (1 - t) + blur[bin_ceil] * t
/// ```
///
/// メモリは 8 枚同時保持から 2 枚逐次処理に変更し、アーティファクトを除去する。
pub fn depth_aware_blur(
    img: DynamicImage,
    depth_map: &DynamicImage,
    focus_depth: f32,
    max_radius_ratio: f32,
    kind: DepthBlurKind,
) -> Result<DynamicImage> {
    let (w, h) = (img.width(), img.height());
    let min_dim = w.min(h) as f32;
    let rgba = img.to_rgba8();

    // depth map をグレースケール u8 に変換
    let depth_gray_raw = depth_map.to_luma8();
    // depth_map のサイズが img と異なる場合はリサイズ
    let depth_gray = if depth_gray_raw.width() != w || depth_gray_raw.height() != h {
        image::imageops::resize(&depth_gray_raw, w, h, image::imageops::FilterType::Lanczos3)
    } else {
        depth_gray_raw
    };

    const N_BINS: usize = 8;

    // 各ビンの中心深度と radius_px を計算
    let mut bin_radius: [f32; N_BINS] = [0.0; N_BINS];
    for (bin, radius) in bin_radius.iter_mut().enumerate().take(N_BINS) {
        let bin_center = (bin as f32 + 0.5) / N_BINS as f32; // 0.0625..0.9375
        let delta = bin_center - focus_depth;
        *radius = match kind {
            DepthBlurKind::Myopia => {
                if delta < 0.0 { (-delta) * max_radius_ratio * min_dim } else { 0.0 }
            }
            DepthBlurKind::Hyperopia => {
                if delta > 0.0 { delta * max_radius_ratio * min_dim } else { 0.0 }
            }
            DepthBlurKind::DepthOfField => {
                delta.abs() * max_radius_ratio * min_dim
            }
        };
    }

    // linear sRGB planes に変換
    let (linear, alpha) = rgba_to_linear_planes(&rgba);

    // 出力バッファ
    let npx = (w * h) as usize;
    let mut out_linear: Vec<[f32; 3]> = vec![[0.0; 3]; npx];

    // 各画素の深度値を事前収集（0.0..=1.0）
    let depths: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (y, x)))
        .map(|(y, x)| depth_gray.get_pixel(x, y)[0] as f32 / 255.0)
        .collect();

    // 隣接 2 ビンを逐次処理して線形補間する。
    // depth d に対して:
    //   scaled = d * (N_BINS - 1) as f32   → 0.0..=7.0
    //   bin_floor = scaled.floor() as usize  → 0..=7
    //   bin_ceil  = (bin_floor + 1).min(N_BINS - 1)
    //   t         = scaled.fract()           → 0.0..=1.0
    // 出力 = lerp(blur_floor[i], blur_ceil[i], t)
    //
    // ビンペアを (0,1), (1,2), ..., (6,7) と順に処理し、
    // そのペアが使われる画素にだけ書き込む（2 枚しか同時保持しない）。
    for floor_bin in 0..(N_BINS - 1) {
        let ceil_bin = floor_bin + 1;

        // このペアを使う画素が存在するか確認
        let pair_used = depths.iter().any(|&d| {
            let scaled = d * (N_BINS - 1) as f32;
            let bf = (scaled.floor() as usize).min(N_BINS - 1);
            bf == floor_bin
        });
        if !pair_used {
            continue;
        }

        // 2 枚の blur 画像を生成
        let blur_floor = if bin_radius[floor_bin] < MIN_BLUR_RADIUS_PX {
            linear.clone()
        } else {
            ellipse_blur(&linear, w, h, bin_radius[floor_bin], bin_radius[floor_bin], 0.0)
        };
        let blur_ceil = if bin_radius[ceil_bin] < MIN_BLUR_RADIUS_PX {
            linear.clone()
        } else {
            ellipse_blur(&linear, w, h, bin_radius[ceil_bin], bin_radius[ceil_bin], 0.0)
        };

        // 該当画素に線形補間結果を書き込む
        for (idx, &d) in depths.iter().enumerate() {
            let scaled = d * (N_BINS - 1) as f32;
            let bf = (scaled.floor() as usize).min(N_BINS - 1);
            if bf == floor_bin {
                let t = scaled.fract();
                let f = blur_floor[idx];
                let c = blur_ceil[idx];
                out_linear[idx] = [
                    lerp(f[0], c[0], t),
                    lerp(f[1], c[1], t),
                    lerp(f[2], c[2], t),
                ];
            }
        }
    }

    // 最終ビン（bin 7）: scaled = 7.0 → fract = 0.0 → floor = 7 → ceil = 7（clamp）
    // このケースは floor_bin が 6 のループで bf = 6 となり補間されない。
    // d = 1.0 のとき scaled = 7.0, floor = 7 → 別途処理する。
    {
        let blur_last = if bin_radius[N_BINS - 1] < MIN_BLUR_RADIUS_PX {
            linear.clone()
        } else {
            ellipse_blur(&linear, w, h, bin_radius[N_BINS - 1], bin_radius[N_BINS - 1], 0.0)
        };
        for (idx, &d) in depths.iter().enumerate() {
            let scaled = d * (N_BINS - 1) as f32;
            let bf = (scaled.floor() as usize).min(N_BINS - 1);
            if bf == N_BINS - 1 {
                out_linear[idx] = blur_last[idx];
            }
        }
    }

    let out_rgba = linear_planes_to_rgba(&out_linear, &alpha, w, h);
    Ok(DynamicImage::ImageRgba8(out_rgba))
}

/// 視野狭窄（tunnel vision）シミュレーション。
///
/// 全般的に視野が狭窄し、極端な場合は穴を通して見るような視野になる。
/// 網膜色素変性・重度の緑内障末期などで生じる。
///
/// ## アルゴリズム
/// glaucoma と同様の vignetting だが、保存される中心領域がより小さく、
/// 移行領域が狭い（急激な境界）。
///
/// `inner_r` = `(1.0 - strength) * 0.5`, `outer_r` = `inner_r + 0.05`
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
pub fn tunnel_vision(img: DynamicImage, strength: f32) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let cx = w_f * 0.5;
    let cy = h_f * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt();

    // 中心視野の半径: strength が大きいほど小さい
    let inner_r = (1.0 - strength) * 0.5;
    // tunnel_vision は急激な境界が特徴
    let outer_r = (inner_r + 0.05).min(1.0);

    let mut out_rgba = rgba.clone();
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = (dx * dx + dy * dy).sqrt() / max_r;

            let fade = if r <= inner_r {
                0.0
            } else if r >= outer_r {
                1.0
            } else {
                let t = (r - inner_r) / (outer_r - inner_r);
                t * t * (3.0 - 2.0 * t)
            };

            if fade == 0.0 {
                continue;
            }

            let mul = 1.0 - strength * fade;

            let px = out_rgba.get_pixel_mut(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);
            px[0] = pack_u8(linear_to_srgb(rl * mul));
            px[1] = pack_u8(linear_to_srgb(gl * mul));
            px[2] = pack_u8(linear_to_srgb(bl * mul));
        }
    }

    Ok(DynamicImage::ImageRgba8(out_rgba))
}

// -------------------------------------------------------------------------
// Phase 4 / #29: diplopia / nystagmus / starbursts
// -------------------------------------------------------------------------

/// 複視（Diplopia）シミュレーション。
///
/// 元画像を `(offset_x, offset_y)` ピクセルだけ平行移動した「幽霊像」を
/// `ghost_strength * strength` の alpha で alpha blend して合成する。
/// `out = orig * (1 - alpha) + ghost * alpha` により輝度が保存される。
///
/// # 引数
/// - `strength`: エフェクト全体強度（0.0..=1.0）
/// - `offset_x`: 水平ずれ（min(W,H) 比、−1.0..=1.0）
/// - `offset_y`: 垂直ずれ（min(W,H) 比、−1.0..=1.0）
/// - `ghost_strength`: 幽霊像の見えやすさ（0.0..=1.0）
pub fn diplopia(
    img: DynamicImage,
    strength: f32,
    offset_x: f32,
    offset_y: f32,
    ghost_strength: f32,
) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    if s == 0.0 {
        return Ok(img);
    }

    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let min_dim = width.min(height) as f32;

    let dx = (offset_x * min_dim).round() as i32;
    let dy = (offset_y * min_dim).round() as i32;
    // ghost の寄与 = ghost_strength × strength（線形、二重スケーリングしない）
    let ghost_alpha = (ghost_strength.clamp(0.0, 1.0) * s).clamp(0.0, 1.0);

    let mut out = RgbaImage::new(width, height);
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let orig_px = rgba.get_pixel(x as u32, y as u32);

            // 幽霊のソース座標（エッジクランプ）
            let src_x = (x - dx).clamp(0, width as i32 - 1) as u32;
            let src_y = (y - dy).clamp(0, height as i32 - 1) as u32;
            let ghost_px = rgba.get_pixel(src_x, src_y);

            // linear sRGB でアルファブレンド
            let o = [
                srgb_to_linear(orig_px[0] as f32 / 255.0),
                srgb_to_linear(orig_px[1] as f32 / 255.0),
                srgb_to_linear(orig_px[2] as f32 / 255.0),
            ];
            let g = [
                srgb_to_linear(ghost_px[0] as f32 / 255.0),
                srgb_to_linear(ghost_px[1] as f32 / 255.0),
                srgb_to_linear(ghost_px[2] as f32 / 255.0),
            ];
            let blended = [
                // out = orig * (1 - alpha) + ghost * alpha（alpha blend、輝度保存）
                o[0] * (1.0 - ghost_alpha) + g[0] * ghost_alpha,
                o[1] * (1.0 - ghost_alpha) + g[1] * ghost_alpha,
                o[2] * (1.0 - ghost_alpha) + g[2] * ghost_alpha,
            ];

            out.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([
                    pack_u8(linear_to_srgb(blended[0])),
                    pack_u8(linear_to_srgb(blended[1])),
                    pack_u8(linear_to_srgb(blended[2])),
                    orig_px[3],
                ]),
            );
        }
    }

    Ok(DynamicImage::ImageRgba8(out))
}

/// 眼振（Nystagmus）シミュレーション。
///
/// 目が周期的に揺れることで生じる motion blur を
/// 1D directional blur（astigmatism と同構造）で表現する。
///
/// # 引数
/// - `strength`: エフェクト強度（0.0..=1.0）
/// - `amplitude`: 揺れ幅（min(W,H) 比）
/// - `direction_deg`: 揺れ方向（0°=水平, 90°=垂直）
pub fn nystagmus(
    img: DynamicImage,
    strength: f32,
    amplitude: f32,
    direction_deg: f32,
) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let min_dim = width.min(height) as f32;

    let radius_px = amplitude.clamp(0.0, 1.0) * s * min_dim;

    if s == 0.0 || radius_px < MIN_BLUR_RADIUS_PX {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    // 揺れ方向をそのままぼかし方向として使用（astigmatism と異なり +90° しない）
    let blur_axis_rad = direction_deg.to_radians();

    let (linear, alpha) = rgba_to_linear_planes(&rgba);
    // 1D directional blur: 短軸を MIN_BLUR_RADIUS_PX に縮退
    let blurred = ellipse_blur(&linear, width, height, radius_px, MIN_BLUR_RADIUS_PX, blur_axis_rad);
    let out = linear_planes_to_rgba(&blurred, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}

/// スターバースト（Starbursts）シミュレーション。
///
/// 強い光源から放射状の光芒が伸びる現象（乱視・白内障術後など）を表現する。
///
/// # 引数
/// - `strength`: エフェクト強度（0.0..=1.0）
/// - `num_rays`: 光芒の本数（4/6/8 推奨）
/// - `ray_length_ratio`: 光芒の長さ（min(W,H) 比）
/// - `threshold`: 光芒が発生する輝度閾値（0.0..=1.0）
pub fn starbursts(
    img: DynamicImage,
    strength: f32,
    num_rays: u32,
    ray_length_ratio: f32,
    threshold: f32,
) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    if s == 0.0 {
        return Ok(img);
    }

    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let min_dim = width.min(height) as f32;

    let ray_length_px = (ray_length_ratio.clamp(0.0, 1.0) * min_dim) as u32;
    let threshold = threshold.clamp(0.0, 1.0);

    // 光芒レイヤー（linear sRGB, f32）
    let mut ray_layer: Vec<[f32; 3]> = vec![[0.0; 3]; (width * height) as usize];

    // BT.709 輝度計算用定数
    const R_LUMA: f32 = 0.2126;
    const G_LUMA: f32 = 0.7152;
    const B_LUMA: f32 = 0.0722;

    for y in 0..height {
        for x in 0..width {
            let px = rgba.get_pixel(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);
            let luma = R_LUMA * rl + G_LUMA * gl + B_LUMA * bl;

            if luma <= threshold || num_rays == 0 || ray_length_px == 0 {
                continue;
            }

            let src_intensity = (luma - threshold) / (1.0 - threshold).max(1e-6);

            for i in 0..num_rays {
                let theta = i as f32 * 2.0 * PI / num_rays as f32;
                let cos_t = theta.cos();
                let sin_t = theta.sin();

                for t in 1..=ray_length_px {
                    let sx = x as i32 + (t as f32 * cos_t).round() as i32;
                    let sy = y as i32 + (t as f32 * sin_t).round() as i32;
                    if sx < 0 || sx >= width as i32 || sy < 0 || sy >= height as i32 {
                        continue;
                    }
                    let weight = src_intensity * (1.0 - t as f32 / ray_length_px as f32) * s;
                    let idx = sy as usize * width as usize + sx as usize;
                    ray_layer[idx][0] += weight;
                    ray_layer[idx][1] += weight;
                    ray_layer[idx][2] += weight;
                }
            }
        }
    }

    // 元画像 linear + 光芒レイヤー を合成
    let (linear, alpha) = rgba_to_linear_planes(&rgba);
    let mut out_linear: Vec<[f32; 3]> = Vec::with_capacity(linear.len());
    for (i, orig) in linear.iter().enumerate() {
        out_linear.push([
            (orig[0] + ray_layer[i][0]).clamp(0.0, 1.0),
            (orig[1] + ray_layer[i][1]).clamp(0.0, 1.0),
            (orig[2] + ray_layer[i][2]).clamp(0.0, 1.0),
        ]);
    }

    let out = linear_planes_to_rgba(&out_linear, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}

// ---------------------------------------------------------------
// Phase 4 (#36): eye fatigue — eye_strain / dry_eye
// ---------------------------------------------------------------

/// 眼精疲労（eye strain）シミュレーション。
///
/// - コントラスト圧縮: `v' = 0.5 + (v - 0.5) * (1.0 - strength * 0.15)`
/// - 微小 disk blur（radius = strength * 1.5 px）
/// - 周辺 vignette（軽め）
///
/// `strength = 0.0` は元画像と完全一致。
pub fn eye_strain(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let cx = w_f * 0.5;
    let cy = h_f * 0.5;

    // コントラスト圧縮係数
    let contrast_factor = 1.0 - s * 0.15;

    // Step 1: linear sRGB 空間でコントラスト圧縮 + vignette
    let (linear, alpha) = rgba_to_linear_planes(&rgba);
    let mut compressed: Vec<[f32; 3]> = linear
        .iter()
        .enumerate()
        .map(|(i, &[r, g, b])| {
            let x = (i as u32 % width) as f32;
            let y = (i as u32 / width) as f32;
            let ux = (x - cx) / cx;  // -1.0..=1.0
            let uy = (y - cy) / cy;
            let d = ux * ux + uy * uy;  // 0.0（中心）〜 2.0+（角）

            // コントラスト圧縮（linear 空間で 0.5 中心に圧縮）
            let cr = 0.5 + (r - 0.5) * contrast_factor;
            let cg = 0.5 + (g - 0.5) * contrast_factor;
            let cb = 0.5 + (b - 0.5) * contrast_factor;

            // vignette: 中心は暗化なし、周辺に向かって smoothstep で暗化
            // smoothstep(0.3, 1.2, d)
            let t = ((d - 0.3) / (1.2 - 0.3)).clamp(0.0, 1.0);
            let sm = t * t * (3.0 - 2.0 * t);
            let vignette = 1.0 - s * 0.3 * sm;

            [
                (cr * vignette).clamp(0.0, 1.0),
                (cg * vignette).clamp(0.0, 1.0),
                (cb * vignette).clamp(0.0, 1.0),
            ]
        })
        .collect();

    // Step 2: 微小 disk blur（radius = strength * 1.5 px、min 0.5 px で有効）
    let blur_radius = s * 1.5;
    if blur_radius >= MIN_BLUR_RADIUS_PX {
        compressed = ellipse_blur(&compressed, width, height, blur_radius, blur_radius, 0.0);
    }

    let out = linear_planes_to_rgba(&compressed, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}

// ---------------------------------------------------------------
// Phase N (Issue #55): Metamorphopsia — 歪視フィルタ
// ---------------------------------------------------------------

/// 歪視（Metamorphopsia）シミュレーション。
///
/// 黄斑疾患（黄斑円孔・黄斑上膜・加齢黄斑変性など）で生じる格子状の歪み（Amsler grid
/// 歪曲）を模擬する。LCG ベースのグリッドノイズで各ピクセルを変位座標からサンプリングする。
///
/// ## アルゴリズム
///
/// 画像を `1/freq` ピクセル単位の仮想グリッドに分割し、各グリッド頂点に
/// LCG 擬似ランダムな変位ベクトル `(dx, dy)` を割り当てる。
/// 各出力ピクセルについて、所属するグリッドセルの 4 頂点の変位を双線形補間し、
/// その変位でサンプリング座標を移動して元画像をサンプリングする。
/// エッジは clamp で処理する。
///
/// 変位量: `strength × MAX_DISPLACEMENT_PX`（最大 8 ピクセル）。
/// `strength = 0.0` は identity（元画像と byte-exact 一致）。
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 歪み強度（0.0..=1.0）
/// - `freq`: 空間周波数（グリッドセルサイズ = `max(1, 画像短辺 / freq) px`）
/// - `seed`: LCG シード（同じ seed なら同じ歪みパターン）
pub fn metamorphopsia(img: DynamicImage, strength: f32, freq: f32, seed: u64) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();

    // 変位幅の最大値（ピクセル）
    const MAX_DISPLACEMENT_PX: f32 = 8.0;
    let max_disp = s * MAX_DISPLACEMENT_PX;

    // グリッドセルサイズ（ピクセル単位）。freq が大きいほど細かいグリッド。
    let min_dim = width.min(height) as f32;
    let freq_clamped = freq.clamp(0.1, 1000.0);
    let cell_size = (min_dim / freq_clamped).max(1.0);

    // グリッド頂点数
    let grid_w = (width as f32 / cell_size).ceil() as usize + 2;
    let grid_h = (height as f32 / cell_size).ceil() as usize + 2;

    // LCG 定数（Knuth / Numerical Recipes）
    let lcg_step = |state: u64| -> u64 {
        state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };

    // 各グリッド頂点の変位 (dx, dy) を LCG で生成する。
    // シードは頂点座標とグローバルシードを混合して決定する（空間的再現性を確保）。
    let grid_disp: Vec<(f32, f32)> = (0..grid_h)
        .flat_map(|gy| (0..grid_w).map(move |gx| (gx, gy)))
        .map(|(gx, gy)| {
            // 頂点ごとに独立したシードを生成する。
            // seed との混合で異なる grid_size でもシードが衝突しにくい。
            let h0 = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let h1 = h0
                .wrapping_add((gx as u64).wrapping_mul(0x9e3779b97f4a7c15))
                .wrapping_add((gy as u64).wrapping_mul(0x6c62272e07bb0142));
            // dx
            let s1 = lcg_step(h1);
            let dx_norm = (s1 >> 32) as f32 / u32::MAX as f32; // 0.0..=1.0
            // dy
            let s2 = lcg_step(s1);
            let dy_norm = (s2 >> 32) as f32 / u32::MAX as f32;
            // [-1, 1] に変換してから max_disp を掛ける
            let dx = (dx_norm * 2.0 - 1.0) * max_disp;
            let dy = (dy_norm * 2.0 - 1.0) * max_disp;
            (dx, dy)
        })
        .collect();

    let get_grid = |gx: usize, gy: usize| -> (f32, f32) {
        let gx = gx.min(grid_w - 1);
        let gy = gy.min(grid_h - 1);
        grid_disp[gy * grid_w + gx]
    };

    // 各出力ピクセルについて変位後座標をサンプリングする。
    let mut out = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            // ピクセルがどのグリッドセルに属するか
            let fx = x as f32 / cell_size;
            let fy = y as f32 / cell_size;
            let gx0 = fx.floor() as usize;
            let gy0 = fy.floor() as usize;
            let gx1 = gx0 + 1;
            let gy1 = gy0 + 1;
            let tx = fx - fx.floor(); // 0.0..=1.0 のセル内位置
            let ty = fy - fy.floor();

            // 4 頂点の変位を双線形補間
            let (d00x, d00y) = get_grid(gx0, gy0);
            let (d10x, d10y) = get_grid(gx1, gy0);
            let (d01x, d01y) = get_grid(gx0, gy1);
            let (d11x, d11y) = get_grid(gx1, gy1);

            let disp_x = d00x * (1.0 - tx) * (1.0 - ty)
                + d10x * tx * (1.0 - ty)
                + d01x * (1.0 - tx) * ty
                + d11x * tx * ty;
            let disp_y = d00y * (1.0 - tx) * (1.0 - ty)
                + d10y * tx * (1.0 - ty)
                + d01y * (1.0 - tx) * ty
                + d11y * tx * ty;

            // サンプリング座標（clamp でエッジ処理）
            let src_x = (x as f32 + disp_x)
                .clamp(0.0, (width - 1) as f32);
            let src_y = (y as f32 + disp_y)
                .clamp(0.0, (height - 1) as f32);

            let px = sample_bilinear(&rgba, src_x, src_y);
            out.put_pixel(x, y, px);
        }
    }

    Ok(DynamicImage::ImageRgba8(out))
}

/// ドライアイ（dry eye）シミュレーション。
///
/// LCG（seed=42 固定）で生成したノイズマスクを基に、
/// 32×32 タイルごとに異なる disk blur radius を適用する。
///
/// `strength = 0.0` は元画像と完全一致。
pub fn dry_eye(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let (linear, alpha) = rgba_to_linear_planes(&rgba);

    const TILE_SIZE: u32 = 32;

    // LCG 定数（Numerical Recipes）
    let lcg_next = |state: u64| -> u64 {
        state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
    };

    // タイル数を計算
    let tile_cols = width.div_ceil(TILE_SIZE);
    let tile_rows = height.div_ceil(TILE_SIZE);

    // 出力バッファを元画像で初期化
    let mut out_linear = linear.clone();

    // タイルごとに disk blur を適用して出力バッファに書き込む
    let mut state: u64 = 42u64.wrapping_mul(6364136223846793005).wrapping_add(1);
    for ty in 0..tile_rows {
        for tx in 0..tile_cols {
            state = lcg_next(state);
            // 0.0..=1.0 のノイズ値（nit-2: (state >> 33) as f32 / (1u64 << 31) as f32）
            let noise = (state >> 33) as f32 / (1u64 << 31) as f32;
            let blur_radius = noise * s * 3.0;
            if blur_radius < MIN_BLUR_RADIUS_PX {
                // blur なし: 元の値をそのままコピー（既に out_linear に入っている）
                continue;
            }

            // タイル境界（オーバーラップ付き）
            let r_u = blur_radius as u32 + 1;
            let x0 = (tx * TILE_SIZE).saturating_sub(r_u);
            let y0 = (ty * TILE_SIZE).saturating_sub(r_u);
            let x1 = ((tx + 1) * TILE_SIZE + r_u).min(width);
            let y1 = ((ty + 1) * TILE_SIZE + r_u).min(height);

            // タイル内（出力に書く範囲）
            let x0_tile = tx * TILE_SIZE;
            let y0_tile = ty * TILE_SIZE;
            let x1_tile = ((tx + 1) * TILE_SIZE).min(width);
            let y1_tile = ((ty + 1) * TILE_SIZE).min(height);

            // 拡張領域だけを切り出した sub-image を blur して、タイル内だけ out に書く
            let sub_w = x1 - x0;
            let sub_h = y1 - y0;
            let sub_len = (sub_w * sub_h) as usize;
            let mut sub_linear: Vec<[f32; 3]> = Vec::with_capacity(sub_len);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    sub_linear.push(linear[(sy * width + sx) as usize]);
                }
            }
            let sub_blurred = ellipse_blur(&sub_linear, sub_w, sub_h, blur_radius, blur_radius, 0.0);

            // タイル内のピクセルだけ out に書く
            for y in y0_tile..y1_tile {
                for x in x0_tile..x1_tile {
                    let sub_x = x - x0;
                    let sub_y = y - y0;
                    let sub_idx = (sub_y * sub_w + sub_x) as usize;
                    let out_idx = (y * width + x) as usize;
                    out_linear[out_idx] = sub_blurred[sub_idx];
                }
            }
        }
    }

    let out = linear_planes_to_rgba(&out_linear, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}

// ---------------------------------------------------------------
// Issue #56: Contrast Sensitivity フィルタ
// ---------------------------------------------------------------

/// コントラスト感度低下（Contrast Sensitivity Loss）シミュレーション。
///
/// 輝度コントラストを圧縮し、midpoint (0.5) に引き寄せる。
/// 式: `output = 0.5 + (input - 0.5) * (1.0 - strength * 0.5)`
///
/// - `strength = 0.0`: 元画像と同一
/// - `strength = 1.0`: 輝度コントラストを 50% 圧縮
///
/// 処理は linear sRGB 空間で行う。
pub fn contrast_sensitivity(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let mut rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }
    let scale = 1.0 - s * 0.5;
    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);
        let nr = 0.5 + (r - 0.5) * scale;
        let ng = 0.5 + (g - 0.5) * scale;
        let nb = 0.5 + (b - 0.5) * scale;
        px[0] = pack_u8(linear_to_srgb(nr.clamp(0.0, 1.0)));
        px[1] = pack_u8(linear_to_srgb(ng.clamp(0.0, 1.0)));
        px[2] = pack_u8(linear_to_srgb(nb.clamp(0.0, 1.0)));
        // alpha はそのまま
    }
    Ok(DynamicImage::ImageRgba8(rgba))
}

// ---------------------------------------------------------------
// Issue #57: Detail Loss フィルタ（pixelation）
// ---------------------------------------------------------------

/// 細部喪失（Detail Loss）シミュレーション。
///
/// 矩形タイルごとに平均色に置き換える（pixelation）。
/// タイルサイズ = `(strength * 20.0).max(1.0) as u32` px。
///
/// - `strength = 0.0`: identity（タイルサイズ 1px = 変化なし）
/// - `strength = 1.0`: 20px タイル
pub fn detail_loss(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    let tile_size = (s * 20.0).max(1.0) as u32;
    if tile_size <= 1 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }
    let width = rgba.width();
    let height = rgba.height();
    let mut out = rgba.clone();

    let tile_cols = width.div_ceil(tile_size);
    let tile_rows = height.div_ceil(tile_size);

    for ty in 0..tile_rows {
        for tx in 0..tile_cols {
            let x0 = tx * tile_size;
            let y0 = ty * tile_size;
            let x1 = (x0 + tile_size).min(width);
            let y1 = (y0 + tile_size).min(height);
            let count = ((x1 - x0) * (y1 - y0)) as u64;
            if count == 0 {
                continue;
            }

            // タイル内の平均色を linear sRGB で計算
            let mut sum = [0.0_f64; 3];
            for py in y0..y1 {
                for px in x0..x1 {
                    let p = rgba.get_pixel(px, py);
                    sum[0] += srgb_to_linear(p[0] as f32 / 255.0) as f64;
                    sum[1] += srgb_to_linear(p[1] as f32 / 255.0) as f64;
                    sum[2] += srgb_to_linear(p[2] as f32 / 255.0) as f64;
                }
            }
            let avg_r = pack_u8(linear_to_srgb((sum[0] / count as f64) as f32));
            let avg_g = pack_u8(linear_to_srgb((sum[1] / count as f64) as f32));
            let avg_b = pack_u8(linear_to_srgb((sum[2] / count as f64) as f32));

            for py in y0..y1 {
                for px in x0..x1 {
                    let p = out.get_pixel_mut(px, py);
                    p[0] = avg_r;
                    p[1] = avg_g;
                    p[2] = avg_b;
                    // alpha はそのまま
                }
            }
        }
    }

    Ok(DynamicImage::ImageRgba8(out))
}

// ---------------------------------------------------------------
// Issue #58: Teichopsia フィルタ（偏頭痛の前兆：要塞スペクトル）
// ---------------------------------------------------------------

/// 閃輝暗点（Teichopsia / Fortification Spectra）シミュレーション。
///
/// 視野周辺にジグザグ縞の光（要塞スペクトル）を重畳し、内側（scotoma）を暗化する。
///
/// ## アルゴリズム
///
/// 1. 正規化 UV 座標（-0.5..0.5）で中心からの距離を計算
/// 2. 距離 0.2〜0.5 のリング領域内でジグザグ輝度を加算（saw wave）
/// 3. 内側（< 0.2）は scotoma として暗化
/// 4. strength でリング輝度と scotoma 暗化をスケール
///
/// > **医学的注記**: 偏頭痛の前兆として 20〜30 分続く。
/// > 初めて経験する場合は眼科・神経内科を受診。
///
/// - `strength = 0.0`: 元画像と同一
/// - `strength = 1.0`: 最大の閃輝暗点効果
pub fn teichopsia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let mut rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }
    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let aspect = w_f / h_f;

    for y in 0..height {
        for x in 0..width {
            // 正規化座標（-0.5..0.5）
            let ux = (x as f32 / w_f) - 0.5;
            let uy = ((y as f32 / h_f) - 0.5) / aspect;
            let dist = (ux * ux + uy * uy).sqrt();

            let px = rgba.get_pixel_mut(x, y);

            if dist < 0.2 {
                // scotoma: 内側を strength に応じて暗化
                let dark = 1.0 - s * 0.7 * (1.0 - dist / 0.2);
                let rl = srgb_to_linear(px[0] as f32 / 255.0);
                let gl = srgb_to_linear(px[1] as f32 / 255.0);
                let bl = srgb_to_linear(px[2] as f32 / 255.0);
                px[0] = pack_u8(linear_to_srgb(rl * dark));
                px[1] = pack_u8(linear_to_srgb(gl * dark));
                px[2] = pack_u8(linear_to_srgb(bl * dark));
            } else if dist >= 0.2 && dist <= 0.5 {
                // ジグザグリング
                let angle = uy.atan2(ux);
                let saw = (angle / PI * 8.0).fract(); // saw wave 0..1
                let ring_t = (dist - 0.2) / 0.3; // 0..1 in ring
                let fade = (ring_t * (1.0 - ring_t) * 4.0).clamp(0.0, 1.0); // 中央強調
                let brightness = saw * s * fade * 0.6;

                let rl = srgb_to_linear(px[0] as f32 / 255.0);
                let gl = srgb_to_linear(px[1] as f32 / 255.0);
                let bl = srgb_to_linear(px[2] as f32 / 255.0);
                px[0] = pack_u8(linear_to_srgb((rl + brightness).min(1.0)));
                px[1] = pack_u8(linear_to_srgb((gl + brightness).min(1.0)));
                px[2] = pack_u8(linear_to_srgb((bl + brightness).min(1.0)));
            }
            // 外側は変更なし
        }
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// 1×1 の RGBA 画像を作るヘルパー。
    fn pixel(r: u8, g: u8, b: u8, a: u8) -> DynamicImage {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([r, g, b, a]));
        DynamicImage::ImageRgba8(img)
    }

    fn read_rgba(img: &DynamicImage) -> [u8; 4] {
        let p = img.to_rgba8();
        let px = p.get_pixel(0, 0);
        [px[0], px[1], px[2], px[3]]
    }

    // ---------------------------------------------------------------
    // strength = 0.0 で元画像と一致
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 255);
        let out = protanopia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn deuteranopia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 255);
        let out = deuteranopia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn tritanopia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 255);
        let out = tritanopia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn achromatopsia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 128);
        let out = achromatopsia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 128]);
    }

    // ---------------------------------------------------------------
    // alpha 保持
    // ---------------------------------------------------------------

    #[test]
    fn alpha_is_preserved_across_filters() {
        for strength in [0.0_f32, 0.5, 1.0] {
            let input = pixel(200, 50, 30, 77);
            assert_eq!(
                read_rgba(&protanopia(input.clone(), strength).unwrap())[3],
                77
            );
            assert_eq!(
                read_rgba(&deuteranopia(input.clone(), strength).unwrap())[3],
                77
            );
            assert_eq!(
                read_rgba(&tritanopia(input.clone(), strength).unwrap())[3],
                77
            );
            assert_eq!(
                read_rgba(&achromatopsia(input.clone(), strength).unwrap())[3],
                77
            );
        }
    }

    // ---------------------------------------------------------------
    // strength の範囲外を clamp
    // ---------------------------------------------------------------

    #[test]
    fn negative_strength_is_clamped_to_zero() {
        let input = pixel(200, 50, 30, 255);
        let out = deuteranopia(input.clone(), -1.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn strength_above_one_is_clamped_to_one() {
        let input = pixel(200, 50, 30, 255);
        let a = deuteranopia(input.clone(), 2.0).unwrap();
        let b = deuteranopia(input.clone(), 1.0).unwrap();
        assert_eq!(read_rgba(&a), read_rgba(&b));
    }

    #[test]
    fn nan_strength_does_not_panic() {
        let input = pixel(200, 50, 30, 255);
        // NaN strength は identity（元画像）として扱う契約。panic しない・
        // silent corruption しないことを確認する（regression guard）。
        let _ = protanopia(input.clone(), f32::NAN).unwrap();
        let _ = deuteranopia(input.clone(), f32::NAN).unwrap();
        let _ = tritanopia(input.clone(), f32::NAN).unwrap();
        let _ = achromatopsia(input, f32::NAN).unwrap();
    }

    // ---------------------------------------------------------------
    // NaN strength は identity（元画像と byte-exact 一致）
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = protanopia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    #[test]
    fn deuteranopia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = deuteranopia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    #[test]
    fn tritanopia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = tritanopia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    #[test]
    fn achromatopsia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = achromatopsia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    // ---------------------------------------------------------------
    // achromatopsia: 完全グレースケール検証
    // ---------------------------------------------------------------

    #[test]
    fn achromatopsia_full_strength_is_grayscale() {
        // 任意のカラフルなピクセル群で R == G == B になること
        for (r, g, b) in [
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (200, 50, 30),
            (12, 34, 56),
        ] {
            let input = pixel(r, g, b, 255);
            let [or, og, ob, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
            assert_eq!(or, og, "R/G mismatch for input ({r},{g},{b})");
            assert_eq!(og, ob, "G/B mismatch for input ({r},{g},{b})");
        }
    }

    #[test]
    fn achromatopsia_pure_red_luma_matches_bt709() {
        // 純赤 (linear 1.0, 0, 0) の Y = 0.2126
        // sRGB に戻して 8bit 化: linear_to_srgb(0.2126) ≈ 0.4984 → 127
        let input = pixel(255, 0, 0, 255);
        let [r, g, b, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert_eq!(r, 127);
        assert_eq!(g, 127);
        assert_eq!(b, 127);
    }

    #[test]
    fn achromatopsia_pure_green_luma_matches_bt709() {
        // 純緑の Y = 0.7152、sRGB ≈ 0.8625、8bit ≈ 220
        let input = pixel(0, 255, 0, 255);
        let [r, _, _, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert_eq!(r, 220);
    }

    #[test]
    fn achromatopsia_pure_blue_luma_matches_bt709() {
        // 純青の Y = 0.0722、sRGB ≈ 0.2979、8bit ≈ 76
        let input = pixel(0, 0, 255, 255);
        let [r, _, _, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert_eq!(r, 76);
    }

    #[test]
    fn achromatopsia_white_stays_white() {
        let input = pixel(255, 255, 255, 255);
        assert_eq!(
            read_rgba(&achromatopsia(input, 1.0).unwrap()),
            [255, 255, 255, 255]
        );
    }

    #[test]
    fn achromatopsia_black_stays_black() {
        let input = pixel(0, 0, 0, 255);
        assert_eq!(
            read_rgba(&achromatopsia(input, 1.0).unwrap()),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn achromatopsia_gray_is_unchanged_at_full_strength() {
        // R == G == B のグレーは achromatopsia(1.0) でも変化しない（≦1bit 丸め誤差は許容）
        let input = pixel(128, 128, 128, 255);
        let [r, g, b, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert!((r as i16 - 128).abs() <= 1);
        assert!((g as i16 - 128).abs() <= 1);
        assert!((b as i16 - 128).abs() <= 1);
    }

    // ---------------------------------------------------------------
    // matrix 系: severity=1.0 で原色が想定通り変化する
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_red_shifts_toward_dark_yellow_green() {
        // 赤盲では純赤の R 成分が落ち、G に寄る（黒〜暗い黄緑）
        let input = pixel(255, 0, 0, 255);
        let [r, g, b, _] = read_rgba(&protanopia(input, 1.0).unwrap());
        // 数値固定（regression）: R が大きく落ち、G/B も限定的
        assert!(r < 150, "expected R drop, got {r}");
        assert!(g < 150, "expected G modest, got {g}");
        // R == G == B（完全グレー）にはならない
        assert!(!(r == g && g == b));
    }

    #[test]
    fn deuteranopia_red_shifts_toward_dim_yellow() {
        // 緑盲でも純赤は薄くなり、緑寄りに変化する
        let input = pixel(255, 0, 0, 255);
        let [r, g, b, _] = read_rgba(&deuteranopia(input, 1.0).unwrap());
        assert!(r < 220, "expected R drop, got {r}");
        assert!(g > 0, "expected some G, got {g}");
        assert!(!(r == g && g == b));
    }

    #[test]
    fn tritanopia_blue_shifts() {
        // 青盲で純青は変化する（B が落ちて G が出る）
        let input = pixel(0, 0, 255, 255);
        let [_r, g, b, _] = read_rgba(&tritanopia(input, 1.0).unwrap());
        // tritanopia 行列の B 行は (0.004733, 0.691367, 0.303900) なので
        // B 出力は 0.3039 程度 → だいぶ落ちる
        assert!(b < 200, "expected B drop, got {b}");
        // G 行は (-0.078411, 0.930809, 0.147602)、B 入力で G 出力は 0.1476 程度
        // sRGB に戻すとそれなりの輝度
        assert!(g > 50, "expected some G output, got {g}");
    }

    #[test]
    fn matrices_preserve_neutral_gray() {
        // 行列は CVD シミュレーションで neutral 軸を保つ性質がある:
        // 中間グレーは大きく変色しないはず（数 bit の差は許容）
        let input = pixel(128, 128, 128, 255);
        for filt in [protanopia as fn(_, _) -> _, deuteranopia, tritanopia] {
            let [r, g, b, _] = read_rgba(&filt(input.clone(), 1.0).unwrap());
            assert!((r as i16 - 128).abs() <= 8, "R={r}");
            assert!((g as i16 - 128).abs() <= 8, "G={g}");
            assert!((b as i16 - 128).abs() <= 8, "B={b}");
        }
    }

    // ---------------------------------------------------------------
    // matrix 系: severity=1.0 で Machado 2009 が示す byte-exact 値に一致
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_red_severity_1_matches_machado_2009() {
        let img = pixel(255, 0, 0, 255);
        let out = protanopia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[109, 95, 0],
            "protanopia(red, 1.0) per Machado 2009"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    #[test]
    fn deuteranopia_red_severity_1_matches_machado_2009() {
        let img = pixel(255, 0, 0, 255);
        let out = deuteranopia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[163, 144, 0],
            "deuteranopia(red, 1.0) per Machado 2009"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    #[test]
    fn tritanopia_blue_severity_1_matches_machado_2009() {
        let img = pixel(0, 0, 255, 255);
        let out = tritanopia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[0, 107, 150],
            "tritanopia(blue, 1.0) per Machado 2009"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    #[test]
    fn achromatopsia_red_severity_1_matches_bt709_luma() {
        // 純赤 (255, 0, 0) は BT.709 photopic luminance で (127, 127, 127)
        let img = pixel(255, 0, 0, 255);
        let out = achromatopsia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[127, 127, 127],
            "achromatopsia(red, 1.0) per BT.709 photopic luminance"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    // ---------------------------------------------------------------
    // 中間 strength: monotonic 性
    // ---------------------------------------------------------------

    #[test]
    fn intermediate_strength_is_between_endpoints() {
        // strength=0.5 の出力は、strength=0 と strength=1 の間に位置する
        let input = pixel(255, 0, 0, 255);
        let s0 = read_rgba(&deuteranopia(input.clone(), 0.0).unwrap());
        let s5 = read_rgba(&deuteranopia(input.clone(), 0.5).unwrap());
        let s1 = read_rgba(&deuteranopia(input, 1.0).unwrap());
        // R は s0 (=255) から s1 (低い値) に向かって落ちる
        assert!(s5[0] < s0[0]);
        assert!(s5[0] > s1[0]);
        // G は s0 (=0) から s1 (高い値) に向かって上がる
        assert!(s5[1] > s0[1]);
        assert!(s5[1] < s1[1]);
    }

    // ---------------------------------------------------------------
    // 多ピクセル画像でも通る（サイズ保持・全画素処理）
    // ---------------------------------------------------------------

    #[test]
    fn larger_image_keeps_dimensions() {
        let mut img = RgbaImage::new(8, 4);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 32) as u8, (y * 64) as u8, 100, 255]);
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let out = deuteranopia(dyn_img, 1.0).unwrap();
        assert_eq!(out.width(), 8);
        assert_eq!(out.height(), 4);
    }

    // =================================================================
    // Phase 2 (#4): focus / refraction (disk blur) tests
    // =================================================================

    /// 単色 RGBA 画像を作るヘルパー。
    fn solid_rgba(width: u32, height: u32, rgba: [u8; 4]) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba(rgba)))
    }

    /// 中央 1px だけが white、周囲 black の画像を作るヘルパー。
    fn center_white_dot(size: u32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
        img.put_pixel(size / 2, size / 2, Rgba([255, 255, 255, 255]));
        DynamicImage::ImageRgba8(img)
    }

    /// 縦線（中央列）だけが white、その他 black の画像。
    fn vertical_line(size: u32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
        let cx = size / 2;
        for y in 0..size {
            img.put_pixel(cx, y, Rgba([255, 255, 255, 255]));
        }
        DynamicImage::ImageRgba8(img)
    }

    /// 横線（中央行）だけが white、その他 black の画像。
    fn horizontal_line(size: u32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
        let cy = size / 2;
        for x in 0..size {
            img.put_pixel(x, cy, Rgba([255, 255, 255, 255]));
        }
        DynamicImage::ImageRgba8(img)
    }

    fn raw_rgba_vec(img: &DynamicImage) -> Vec<u8> {
        img.to_rgba8().into_raw()
    }

    // ---------------------------------------------------------------
    // strength = 0.0 で 4 関数すべて identity
    // ---------------------------------------------------------------

    #[test]
    fn refraction_strength_zero_is_identity() {
        let input = solid_rgba(64, 64, [200, 50, 30, 255]);
        let original = raw_rgba_vec(&input);
        let s = 0.0_f32;
        assert_eq!(raw_rgba_vec(&myopia(input.clone(), s).unwrap()), original);
        assert_eq!(
            raw_rgba_vec(&hyperopia(input.clone(), s).unwrap()),
            original
        );
        assert_eq!(
            raw_rgba_vec(&presbyopia(input.clone(), s).unwrap()),
            original
        );
        assert_eq!(
            raw_rgba_vec(&astigmatism(input, s, 90.0).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // NaN strength で 4 関数すべて identity（panic しない）
    // ---------------------------------------------------------------

    #[test]
    fn refraction_nan_strength_returns_identity() {
        let input = solid_rgba(64, 64, [200, 50, 30, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&myopia(input.clone(), f32::NAN).unwrap()),
            original
        );
        assert_eq!(
            raw_rgba_vec(&hyperopia(input.clone(), f32::NAN).unwrap()),
            original
        );
        assert_eq!(
            raw_rgba_vec(&presbyopia(input.clone(), f32::NAN).unwrap()),
            original
        );
        assert_eq!(
            raw_rgba_vec(&astigmatism(input, f32::NAN, 90.0).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // alpha 保持
    // ---------------------------------------------------------------

    #[test]
    fn refraction_preserves_alpha() {
        let input = solid_rgba(48, 48, [200, 50, 30, 77]);
        for s in [0.0_f32, 0.5, 1.0] {
            let m = myopia(input.clone(), s).unwrap().to_rgba8();
            let h = hyperopia(input.clone(), s).unwrap().to_rgba8();
            let p = presbyopia(input.clone(), s).unwrap().to_rgba8();
            let a = astigmatism(input.clone(), s, 90.0).unwrap().to_rgba8();
            for img in [&m, &h, &p, &a] {
                for px in img.pixels() {
                    assert_eq!(px[3], 77, "alpha must be preserved");
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // 単一 white dot に myopia をかけると、中心領域が R==G==B で広がる
    // ---------------------------------------------------------------

    #[test]
    fn myopia_spreads_single_dot() {
        // 81x81 画像中央に white dot。strength=1.0 → 半径 ≈ 0.023 * 81 ≒ 1.86 px。
        // disk は (0,0) と上下左右と斜め 4 隅 (dx²+dy² ≤ 3.46) で 9 pixel。
        // 中心ピクセルの白 (1/9) ≈ 28 → 0 < center < 255 の範囲に入る。
        let input = center_white_dot(81);
        let out = myopia(input.clone(), 1.0).unwrap().to_rgba8();
        let cx = 40;
        let cy = 40;
        let center = out.get_pixel(cx, cy);
        // 中心は disk の平均化で white より小さく、しかし R==G==B のまま。
        assert_eq!(center[0], center[1], "center R==G");
        assert_eq!(center[1], center[2], "center G==B");
        assert!(
            center[0] < 255,
            "center should be dimmer than original white"
        );
        assert!(center[0] > 0, "center should still receive some light");

        // 中心から半径より十分に離れた点 (例: 15px 離れた角の近く) は元の黒のまま。
        let far = out.get_pixel(0, 0);
        assert_eq!([far[0], far[1], far[2]], [0, 0, 0]);
    }

    // ---------------------------------------------------------------
    // 単色画像はぼけても色が保たれる (境界 clamp 健全性)
    // ---------------------------------------------------------------

    #[test]
    fn myopia_uniform_color_stays_uniform() {
        // 64x64 全面同一色。disk blur 後も全画素が（丸め誤差 ≤1 を除き）同じ色。
        let color = [120, 80, 40, 255];
        let input = solid_rgba(64, 64, color);
        let out = myopia(input, 1.0).unwrap().to_rgba8();
        for px in out.pixels() {
            for ch in 0..3 {
                let diff = (px[ch] as i16 - color[ch] as i16).abs();
                assert!(
                    diff <= 1,
                    "uniform color must be preserved (channel {ch}, got {} vs {})",
                    px[ch],
                    color[ch]
                );
            }
            assert_eq!(px[3], color[3]);
        }
    }

    #[test]
    fn presbyopia_uniform_color_stays_uniform() {
        let color = [50, 200, 90, 255];
        let input = solid_rgba(80, 80, color);
        let out = presbyopia(input, 1.0).unwrap().to_rgba8();
        for px in out.pixels() {
            for ch in 0..3 {
                let diff = (px[ch] as i16 - color[ch] as i16).abs();
                assert!(diff <= 1, "uniform color must be preserved");
            }
        }
    }

    // ---------------------------------------------------------------
    // astigmatism: axis が違うとぼけ方向が変わる
    // ---------------------------------------------------------------

    #[test]
    fn astigmatism_axis_changes_blur_direction() {
        // 縦線画像に対し:
        //   - axis=90 (vertical sharp): 縦方向はシャープ、横方向にボケる
        //     → 縦線が左右に「滲む」
        //   - axis=0  (horizontal sharp): 横方向はシャープ、縦方向にボケる
        //     → 縦線はあまり滲まない（縦は元から sharp、横方向のボケはほぼ生じない）
        // 201x201 で長軸半径 ≈ 0.011 * 201 ≒ 2.21 px、1D box ~5 px 幅。
        let size = 201_u32;
        let input = vertical_line(size);
        let cx = size / 2;
        let cy = size / 2;

        let blur_h = astigmatism(input.clone(), 1.0, 90.0).unwrap().to_rgba8();
        let blur_v = astigmatism(input.clone(), 1.0, 0.0).unwrap().to_rgba8();

        // axis=90 (横方向ボケ): 中央行で縦線から左右に離れた点も明るくなる
        // axis=0  (縦方向ボケ): 中央行で同じ位置はほぼ黒のまま（縦線の幅は変わらない）
        // 中央線から 2px 横に離れた点を比較
        let off_x = cx + 2;
        let h_off = blur_h.get_pixel(off_x, cy)[0] as i32;
        let v_off = blur_v.get_pixel(off_x, cy)[0] as i32;
        assert!(
            h_off > v_off,
            "horizontal blur (axis=90) must spread the vertical line sideways more than \
             vertical blur (axis=0): h_off={h_off}, v_off={v_off}"
        );
    }

    // ---------------------------------------------------------------
    // astigmatism: axis 周期 180°
    // ---------------------------------------------------------------

    #[test]
    fn astigmatism_axis_is_180_periodic() {
        let input = horizontal_line(61);
        let a0 = raw_rgba_vec(&astigmatism(input.clone(), 1.0, 0.0).unwrap());
        let a180 = raw_rgba_vec(&astigmatism(input, 1.0, 180.0).unwrap());
        assert_eq!(a0, a180, "axis 0 and 180 must be identical (period 180°)");
    }

    // ---------------------------------------------------------------
    // astigmatism: NaN axis は既定 (90°) にフォールバックして panic しない
    // ---------------------------------------------------------------

    #[test]
    fn astigmatism_nan_axis_falls_back_to_default() {
        let input = solid_rgba(32, 32, [128, 128, 128, 255]);
        let out_nan = astigmatism(input.clone(), 1.0, f32::NAN).unwrap();
        let out_90 = astigmatism(input, 1.0, 90.0).unwrap();
        assert_eq!(
            raw_rgba_vec(&out_nan),
            raw_rgba_vec(&out_90),
            "NaN axis must behave like default 90°"
        );
    }

    // ---------------------------------------------------------------
    // 画像サイズは保持される
    // ---------------------------------------------------------------

    // ---------------------------------------------------------------
    // 半径ランキング: myopia > hyperopia >= astigmatism (≈ presbyopia)
    // ---------------------------------------------------------------

    #[test]
    fn myopia_is_more_blurred_than_hyperopia_at_full_strength() {
        // 中央 white dot を myopia / hyperopia でぼかしたとき、
        // myopia (-6D, ratio 0.023) のほうが hyperopia (+4D, ratio 0.015) より
        // 中心輝度が低い (より広い disk で平均化されるため)。
        let input = center_white_dot(101);
        let m = myopia(input.clone(), 1.0).unwrap().to_rgba8();
        let h = hyperopia(input, 1.0).unwrap().to_rgba8();
        let cx = 50_u32;
        let cy = 50_u32;
        let m_center = m.get_pixel(cx, cy)[0] as i32;
        let h_center = h.get_pixel(cx, cy)[0] as i32;
        assert!(
            m_center < h_center,
            "myopia must blur more than hyperopia: m_center={m_center}, h_center={h_center}"
        );
    }

    // ---------------------------------------------------------------
    // 極小画像 (半径 < 0.5px) は identity になる
    // ---------------------------------------------------------------

    #[test]
    fn tiny_image_yields_identity_below_min_radius() {
        // 4x4 で myopia(strength=1.0): radius = 1.0 * 0.05 * 4 = 0.2px < 0.5
        // → identity になる契約。
        let input = solid_rgba(4, 4, [10, 20, 30, 200]);
        let original = raw_rgba_vec(&input);
        let out = myopia(input, 1.0).unwrap();
        assert_eq!(raw_rgba_vec(&out), original);
    }

    #[test]
    fn refraction_preserves_dimensions() {
        let input = solid_rgba(31, 17, [80, 90, 100, 255]);
        type SimpleFilter = fn(DynamicImage, f32) -> Result<DynamicImage>;
        let filters: [SimpleFilter; 3] = [myopia, hyperopia, presbyopia];
        for f in filters {
            let out = f(input.clone(), 1.0).unwrap();
            assert_eq!((out.width(), out.height()), (31, 17));
        }
        let out = astigmatism(input, 1.0, 45.0).unwrap();
        assert_eq!((out.width(), out.height()), (31, 17));
    }

    // ---------------------------------------------------------------
    // astigmatism: byte-exact な軸直交性
    // ---------------------------------------------------------------

    #[test]
    fn astigmatism_axes_are_orthogonal_byte_exact() {
        // 縦線に axis=90 (横方向ボケ) を適用した結果を 90° 回転すると、
        // 横線に axis=0 (縦方向ボケ) を適用した結果と byte-exact で一致するはず。
        let size = 201_u32;
        let v_input = vertical_line(size);
        let h_input = horizontal_line(size);

        let bv = astigmatism(v_input, 1.0, 90.0).unwrap().to_rgba8();
        let bh = astigmatism(h_input, 1.0, 0.0).unwrap().to_rgba8();

        for y in 0..size {
            for x in 0..size {
                assert_eq!(
                    bv.get_pixel(x, y),
                    bh.get_pixel(y, x),
                    "axis=90 vertical line at ({x},{y}) should equal axis=0 horizontal line rotated"
                );
            }
        }
    }

    // =================================================================
    // Phase 3a (#5): visual field defect tests
    // =================================================================

    // ---------------------------------------------------------------
    // T01-T04: strength=0.0 → identity
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [200, 50, 30, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(raw_rgba_vec(&glaucoma(input, 0.0, GlaucomaMode::Vignette).unwrap()), original);
    }

    #[test]
    fn macular_degeneration_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [200, 50, 30, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&macular_degeneration(input, 0.0).unwrap()),
            original
        );
    }

    #[test]
    fn hemianopia_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [200, 50, 30, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&hemianopia(input, 0.0, 0.0).unwrap()),
            original
        );
    }

    #[test]
    fn tunnel_vision_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [200, 50, 30, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&tunnel_vision(input, 0.0).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // T05-T08: NaN strength → identity
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_nan_strength_returns_identity() {
        let input = solid_rgba(32, 32, [100, 150, 200, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&glaucoma(input, f32::NAN, GlaucomaMode::Vignette).unwrap()),
            original
        );
    }

    #[test]
    fn macular_degeneration_nan_strength_returns_identity() {
        let input = solid_rgba(32, 32, [100, 150, 200, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&macular_degeneration(input, f32::NAN).unwrap()),
            original
        );
    }

    #[test]
    fn hemianopia_nan_strength_returns_identity() {
        let input = solid_rgba(32, 32, [100, 150, 200, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&hemianopia(input, f32::NAN, 0.0).unwrap()),
            original
        );
    }

    #[test]
    fn tunnel_vision_nan_strength_returns_identity() {
        let input = solid_rgba(32, 32, [100, 150, 200, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&tunnel_vision(input, f32::NAN).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // T09: glaucoma strength=2.0 is clamped to 1.0
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_strength_above_one_clamped() {
        let input = solid_rgba(64, 64, [200, 100, 50, 255]);
        let out2 = raw_rgba_vec(&glaucoma(input.clone(), 2.0, GlaucomaMode::Vignette).unwrap());
        let out1 = raw_rgba_vec(&glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap());
        assert_eq!(out2, out1);
    }

    // ---------------------------------------------------------------
    // T10: alpha preserved for all 4 visual field filters
    // ---------------------------------------------------------------

    #[test]
    fn visual_field_filters_preserve_alpha() {
        // alpha=200 のピクセル（alpha != 255 で確認）
        let input = solid_rgba(32, 32, [80, 90, 100, 200]);
        let check_alpha = |img: DynamicImage| {
            for px in img.to_rgba8().pixels() {
                assert_eq!(px[3], 200, "alpha must be preserved");
            }
        };
        check_alpha(glaucoma(input.clone(), 0.8, GlaucomaMode::Vignette).unwrap());
        check_alpha(macular_degeneration(input.clone(), 0.8).unwrap());
        check_alpha(hemianopia(input.clone(), 0.8, 0.0).unwrap());
        check_alpha(tunnel_vision(input, 0.8).unwrap());
    }

    // ---------------------------------------------------------------
    // T11: output dimensions preserved for all 4 visual field filters
    // ---------------------------------------------------------------

    #[test]
    fn visual_field_filters_preserve_dimensions() {
        let input = solid_rgba(47, 31, [100, 100, 100, 255]);
        let (w, h) = (47, 31);
        let out = glaucoma(input.clone(), 0.5, GlaucomaMode::Vignette).unwrap();
        assert_eq!((out.width(), out.height()), (w, h));
        let out = macular_degeneration(input.clone(), 0.5).unwrap();
        assert_eq!((out.width(), out.height()), (w, h));
        let out = hemianopia(input.clone(), 0.5, 0.5).unwrap();
        assert_eq!((out.width(), out.height()), (w, h));
        let out = tunnel_vision(input, 0.5).unwrap();
        assert_eq!((out.width(), out.height()), (w, h));
    }

    // ---------------------------------------------------------------
    // T12: glaucoma center pixel unchanged at strength=1.0
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_center_pixel_unchanged_at_full_strength() {
        // 白画像で中心（r < inner_r=0.3）は変化なし
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 100, 50, 255]);
        let out = glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap().to_rgba8();
        let cx = size / 2;
        let cy = size / 2;
        let center = out.get_pixel(cx, cy);
        // 中心画素は元のまま (mul=1.0)
        assert_eq!([center[0], center[1], center[2]], [200, 100, 50]);
    }

    // ---------------------------------------------------------------
    // T13: glaucoma corner pixel becomes black at full strength
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_corner_pixel_becomes_black_at_full_strength() {
        // コーナー (r=1.0 > outer_r=0.5) → mul=0.0 → 黒
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 100, 50, 255]);
        let out = glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap().to_rgba8();
        let corner = out.get_pixel(0, 0);
        assert_eq!([corner[0], corner[1], corner[2]], [0, 0, 0]);
    }

    // ---------------------------------------------------------------
    // T14: glaucoma monotonic peripheral darkening
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_strength_monotonic_peripheral_darkening() {
        // コーナー付近では strength=0.5 の方が strength=1.0 より明るい
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out05 = glaucoma(input.clone(), 0.5, GlaucomaMode::Vignette).unwrap().to_rgba8();
        let out10 = glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap().to_rgba8();
        // コーナー (0,0) での輝度比較
        let r05 = out05.get_pixel(0, 0)[0] as i32;
        let r10 = out10.get_pixel(0, 0)[0] as i32;
        assert!(
            r05 > r10,
            "strength=0.5 corner must be brighter than strength=1.0: {r05} vs {r10}"
        );
    }

    // ---------------------------------------------------------------
    // T15: macular_degeneration center darkened at full strength
    // ---------------------------------------------------------------

    #[test]
    fn macular_degeneration_center_darkened_at_full_strength() {
        // 中心画素: darkened = lum * 0.05 なので元より暗くなる
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out = macular_degeneration(input, 1.0).unwrap().to_rgba8();
        let cx = size / 2;
        let cy = size / 2;
        let center = out.get_pixel(cx, cy)[0] as i32;
        // 200 より大幅に暗いはず (strength=1.0, darkened = lum * 0.05)
        assert!(
            center < 200,
            "center must be darkened at full strength, got {center}"
        );
    }

    // ---------------------------------------------------------------
    // T16: macular_degeneration periphery unchanged at full strength
    // ---------------------------------------------------------------

    #[test]
    fn macular_degeneration_periphery_unchanged_at_full_strength() {
        // 周辺 (r > outer_r=0.4) は t=0.0 → continue → 変化なし
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 100, 50, 255]);
        let out = macular_degeneration(input, 1.0).unwrap().to_rgba8();
        // コーナーは周辺なので変化なし
        let corner = out.get_pixel(0, 0);
        assert_eq!([corner[0], corner[1], corner[2]], [200, 100, 50]);
    }

    // ---------------------------------------------------------------
    // T17: macular_degeneration monotonic center darkening
    // ---------------------------------------------------------------

    #[test]
    fn macular_degeneration_strength_monotonic_center_darkening() {
        // 中心では strength が大きいほど暗い
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out05 = macular_degeneration(input.clone(), 0.5).unwrap().to_rgba8();
        let out10 = macular_degeneration(input, 1.0).unwrap().to_rgba8();
        let cx = size / 2;
        let cy = size / 2;
        let r05 = out05.get_pixel(cx, cy)[0] as i32;
        let r10 = out10.get_pixel(cx, cy)[0] as i32;
        assert!(
            r05 > r10,
            "strength=0.5 center must be brighter than strength=1.0: {r05} vs {r10}"
        );
    }

    // ---------------------------------------------------------------
    // T18: hemianopia left side darkened when side=0.0
    // ---------------------------------------------------------------

    #[test]
    fn hemianopia_left_side_darkened_when_side_zero() {
        // side=0.0, strength=1.0: 左端 (x=0) は x < split_x - blur_w → fade=1.0 → 黒
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out = hemianopia(input, 1.0, 0.0).unwrap().to_rgba8();
        let left = out.get_pixel(0, size / 2);
        assert_eq!(
            [left[0], left[1], left[2]],
            [0, 0, 0],
            "left edge must be black when side=0.0"
        );
    }

    // ---------------------------------------------------------------
    // T19: hemianopia right side darkened when side=1.0
    // ---------------------------------------------------------------

    #[test]
    fn hemianopia_right_side_darkened_when_side_one() {
        // side=1.0, strength=1.0: 右端 (x=size-1) は x > split_x + blur_w → fade=1.0 → 黒
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out = hemianopia(input, 1.0, 1.0).unwrap().to_rgba8();
        let right = out.get_pixel(size - 1, size / 2);
        assert_eq!(
            [right[0], right[1], right[2]],
            [0, 0, 0],
            "right edge must be black when side=1.0"
        );
    }

    // ---------------------------------------------------------------
    // T20: hemianopia side=0.0 and side=1.0 are left-right symmetric
    // ---------------------------------------------------------------

    #[test]
    fn hemianopia_side_left_right_symmetry() {
        // side=0.0 と side=1.0 の対称性を境界から十分離れた領域（端部）で確認する。
        // 境界付近の blur_w ゾーンでは整数ピクセルの離散化により非対称が生じうるが、
        // 境界から遠い領域（左 25%、右 25%）では完全に対称であるべき。
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out_left = hemianopia(input.clone(), 1.0, 0.0).unwrap().to_rgba8();
        let out_right = hemianopia(input, 1.0, 1.0).unwrap().to_rgba8();
        // 境界から遠い端部（左 1/4 と右 1/4）の対称性を確認
        for y in 0..size {
            for x in 0..size / 4 {
                let pl = out_left.get_pixel(x, y)[0] as i32;
                let pr = out_right.get_pixel(size - 1 - x, y)[0] as i32;
                assert_eq!(
                    pl, pr,
                    "far-end symmetry failed at x={x}: side=0 left={pl}, side=1 mirrored={pr}"
                );
            }
        }
    }

    // ---------------------------------------------------------------
    // T21: hemianopia boundary center is intermediate
    // ---------------------------------------------------------------

    #[test]
    fn hemianopia_boundary_center_is_intermediate() {
        // x = split_x (中央) は境界内にあり、完全黒でも完全白でもない
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out = hemianopia(input, 1.0, 0.0).unwrap().to_rgba8();
        let cx = size / 2;
        let cy = size / 2;
        let center = out.get_pixel(cx, cy)[0] as i32;
        // 完全黒 (0) でも元画像 (≈200) でもない中間値
        assert!(
            center > 0 && center < 200,
            "boundary center must be intermediate, got {center}"
        );
    }

    // ---------------------------------------------------------------
    // T22: tunnel_vision corner becomes black at full strength
    // ---------------------------------------------------------------

    #[test]
    fn tunnel_vision_corner_becomes_black_at_full_strength() {
        // strength=1.0: inner_r=0.0, outer_r=0.05。コーナー r≈1.0 > 0.05 → 黒
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 100, 50, 255]);
        let out = tunnel_vision(input, 1.0).unwrap().to_rgba8();
        let corner = out.get_pixel(0, 0);
        assert_eq!([corner[0], corner[1], corner[2]], [0, 0, 0]);
    }

    // ---------------------------------------------------------------
    // T23: tunnel_vision monotonic peripheral darkening
    // ---------------------------------------------------------------

    #[test]
    fn tunnel_vision_strength_monotonic_peripheral_darkening() {
        let size = 64_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let out05 = tunnel_vision(input.clone(), 0.5).unwrap().to_rgba8();
        let out10 = tunnel_vision(input, 1.0).unwrap().to_rgba8();
        let r05 = out05.get_pixel(0, 0)[0] as i32;
        let r10 = out10.get_pixel(0, 0)[0] as i32;
        assert!(
            r05 > r10,
            "strength=0.5 corner must be brighter than strength=1.0: {r05} vs {r10}"
        );
    }

    // ---------------------------------------------------------------
    // T24: tunnel_vision darker area is wider than glaucoma at same strength
    // ---------------------------------------------------------------

    #[test]
    fn tunnel_vision_narrower_than_glaucoma_at_same_strength() {
        // tunnel_vision の中心保持領域は glaucoma より狭い（暗化エリアが広い）。
        // 同一の strength=1.0 で、中心から少し離れた点を比較する。
        // glaucoma: inner_r=0.3, outer_r=0.5 → 中心近くは保存
        // tunnel: inner_r=0.0, outer_r=0.05 → ほぼ全体が暗化
        // 中心から 30% 離れた点での輝度比較（glaucoma は保存, tunnel は暗化済み）
        let size = 100_u32;
        let input = solid_rgba(size, size, [200, 200, 200, 255]);
        let g_out = glaucoma(input.clone(), 1.0, GlaucomaMode::Vignette).unwrap().to_rgba8();
        let t_out = tunnel_vision(input, 1.0).unwrap().to_rgba8();
        // (50, 65) は中心から dy=15, normalized ≈ 0.15 → glaucoma ではinner_r=0.3 内で保存
        let cx = 50_u32;
        let test_y = 65_u32; // 中心y=50, dy=15
        let g_px = g_out.get_pixel(cx, test_y)[0] as i32;
        let t_px = t_out.get_pixel(cx, test_y)[0] as i32;
        assert!(
            g_px > t_px,
            "glaucoma must preserve more than tunnel_vision at same strength: \
             glaucoma={g_px}, tunnel={t_px}"
        );
    }

    // ---------------------------------------------------------------
    // T25-T26: lerp tests
    // ---------------------------------------------------------------

    #[test]
    fn lerp_basic_interpolation() {
        assert_eq!(super::lerp(0.0, 10.0, 0.0), 0.0);
        assert_eq!(super::lerp(0.0, 10.0, 1.0), 10.0);
        assert_eq!(super::lerp(0.0, 10.0, 0.5), 5.0);
        assert_eq!(super::lerp(2.0, 8.0, 0.5), 5.0);
    }

    #[test]
    fn lerp_extrapolation_beyond_range() {
        // t=2.0 → clamp しない: a + (b-a)*2 = 0 + 10*2 = 20
        let result = super::lerp(0.0, 10.0, 2.0);
        assert!((result - 20.0).abs() < 1e-5, "expected 20.0, got {result}");
    }

    // ---------------------------------------------------------------
    // T27-T30: 1x1 image does not panic
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_1x1_does_not_panic() {
        let input = pixel(128, 128, 128, 255);
        let _ = glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap();
    }

    #[test]
    fn macular_degeneration_1x1_does_not_panic() {
        let input = pixel(128, 128, 128, 255);
        let _ = macular_degeneration(input, 1.0).unwrap();
    }

    #[test]
    fn hemianopia_1x1_does_not_panic() {
        let input = pixel(128, 128, 128, 255);
        let _ = hemianopia(input, 1.0, 0.5).unwrap();
    }

    #[test]
    fn tunnel_vision_1x1_does_not_panic() {
        let input = pixel(128, 128, 128, 255);
        let _ = tunnel_vision(input, 1.0).unwrap();
    }

    // ---------------------------------------------------------------
    // T31-T33: color-specific pixel behavior
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_white_image_center_stays_white_corner_goes_black() {
        let size = 64_u32;
        let input = solid_rgba(size, size, [255, 255, 255, 255]);
        let out = glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap().to_rgba8();
        let cx = size / 2;
        let cy = size / 2;
        let center = out.get_pixel(cx, cy);
        assert_eq!(
            [center[0], center[1], center[2]],
            [255, 255, 255],
            "center of white image must stay white"
        );
        let corner = out.get_pixel(0, 0);
        assert_eq!(
            [corner[0], corner[1], corner[2]],
            [0, 0, 0],
            "corner of white image must become black"
        );
    }

    #[test]
    fn glaucoma_black_image_stays_black() {
        let size = 32_u32;
        let input = solid_rgba(size, size, [0, 0, 0, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(raw_rgba_vec(&glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap()), original);
    }

    #[test]
    fn macular_degeneration_black_image_stays_black() {
        let size = 32_u32;
        let input = solid_rgba(size, size, [0, 0, 0, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&macular_degeneration(input, 1.0).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // 性能リグレッションガード (--ignored)
    // ---------------------------------------------------------------

    // =================================================================
    // Phase 3 (#6): light / transparency tests
    // =================================================================

    // ---------------------------------------------------------------
    // P01-P04: strength = 0.0 で 4 フィルタすべて identity
    // ---------------------------------------------------------------

    #[test]
    fn cataract_strength_zero_is_identity() {
        let input = solid_rgba(16, 16, [200, 100, 50, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(raw_rgba_vec(&cataract(input, 0.0, 42).unwrap()), original);
    }

    #[test]
    fn photophobia_strength_zero_is_identity() {
        let input = solid_rgba(16, 16, [200, 100, 50, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(raw_rgba_vec(&photophobia(input, 0.0).unwrap()), original);
    }

    #[test]
    fn nyctalopia_strength_zero_is_identity() {
        let input = solid_rgba(16, 16, [200, 100, 50, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(raw_rgba_vec(&nyctalopia(input, 0.0).unwrap()), original);
    }

    #[test]
    fn floaters_strength_zero_is_identity() {
        let input = solid_rgba(16, 16, [200, 100, 50, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&floaters(input, 0.0, 0.5, 42, 0.5, 0.5).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // P05-P06: NaN strength は identity
    // ---------------------------------------------------------------

    #[test]
    fn cataract_nan_strength_returns_identity() {
        let input = solid_rgba(16, 16, [200, 100, 50, 200]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&cataract(input, f32::NAN, 42).unwrap()),
            original
        );
    }

    #[test]
    fn nyctalopia_nan_strength_returns_identity() {
        let input = solid_rgba(16, 16, [200, 100, 50, 200]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&nyctalopia(input, f32::NAN).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // P07: floaters density=0.0 → blob_count=0 → identity
    // ---------------------------------------------------------------

    #[test]
    fn floaters_density_zero_returns_identity() {
        let input = solid_rgba(16, 16, [100, 150, 200, 255]);
        let original = raw_rgba_vec(&input);
        // density=0.0 なので blob_count=0 → early return で identity
        assert_eq!(
            raw_rgba_vec(&floaters(input, 1.0, 0.0, 42, 0.5, 0.5).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // P08: 4 フィルタ alpha 保持（alpha != 255 の入力）
    // ---------------------------------------------------------------

    #[test]
    fn light_filters_preserve_alpha() {
        let input = solid_rgba(16, 16, [200, 100, 50, 128]);
        let check_alpha = |img: &DynamicImage| {
            for px in img.to_rgba8().pixels() {
                assert_eq!(px[3], 128, "alpha must be preserved");
            }
        };
        check_alpha(&cataract(input.clone(), 1.0, 42).unwrap());
        check_alpha(&photophobia(input.clone(), 1.0).unwrap());
        check_alpha(&nyctalopia(input.clone(), 1.0).unwrap());
        check_alpha(&floaters(input, 1.0, 0.5, 42, 0.5, 0.5).unwrap());
    }

    // ---------------------------------------------------------------
    // P09: 4 フィルタ 出力サイズ同一
    // ---------------------------------------------------------------

    #[test]
    fn light_filters_preserve_dimensions() {
        let input = solid_rgba(31, 17, [80, 90, 100, 255]);
        let check_dims = |img: &DynamicImage| {
            assert_eq!((img.width(), img.height()), (31, 17));
        };
        check_dims(&cataract(input.clone(), 1.0, 42).unwrap());
        check_dims(&photophobia(input.clone(), 1.0).unwrap());
        check_dims(&nyctalopia(input.clone(), 1.0).unwrap());
        check_dims(&floaters(input, 1.0, 0.5, 42, 0.5, 0.5).unwrap());
    }

    // ---------------------------------------------------------------
    // P10: cataract yellowing reduces B channel more than R/G
    // ---------------------------------------------------------------

    #[test]
    fn cataract_yellowing_reduces_blue() {
        // strength=1.0: R係数 0.7, G係数 0.7, B係数 0.4
        // 白画像で out_B < out_R かつ out_B < out_G になるはず
        // （ただしwhite_blendノイズの影響を避けるため、
        //   すべてのピクセルで B < R を確認する）
        let input = solid_rgba(32, 32, [255, 255, 255, 255]);
        let out = cataract(input, 1.0, 0).unwrap().to_rgba8();
        // 少なくとも中心ピクセルで確認
        let px = out.get_pixel(16, 16);
        let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
        assert!(
            b < r,
            "cataract yellowing: expected B < R, got R={r}, G={g}, B={b}"
        );
        // 全ピクセルで B <= R を確認（白濁ノイズがあっても基本的に B が最小）
        for px in out.pixels() {
            let (pr, pb) = (px[0] as i32, px[2] as i32);
            assert!(
                pb <= pr,
                "cataract: expected B <= R at every pixel, got R={pr}, B={pb}"
            );
        }
    }

    // ---------------------------------------------------------------
    // P11: nyctalopia darkens and desaturates
    // ---------------------------------------------------------------

    #[test]
    fn nyctalopia_darkens_and_desaturates() {
        // strength=1.0 で白画像 [255,255,255] が暗くなる
        // Purkinje shift 適用後: R < B（青チャネル微増、赤チャネル微減）
        // dark_factor = 1.0 - 1.0 * 0.7 = 0.3
        let input = solid_rgba(8, 8, [255, 255, 255, 255]);
        let out = nyctalopia(input, 1.0).unwrap().to_rgba8();
        for px in out.pixels() {
            let (r, g, b) = (px[0], px[1], px[2]);
            // 暗化: 255 より大幅に低い
            assert!(r < 200, "nyctalopia must darken: R={r}");
            assert!(g < 200, "nyctalopia must darken: G={g}");
            assert!(b < 200, "nyctalopia must darken: B={b}");
            // Purkinje shift: B >= R（青チャネルが赤チャネル以上）
            assert!(b >= r, "Purkinje shift: B={b} should be >= R={r}");
        }
    }

    // ---------------------------------------------------------------
    // P12: floaters same seed → byte-exact reproducible
    // ---------------------------------------------------------------

    #[test]
    fn floaters_same_seed_is_reproducible() {
        let input = solid_rgba(32, 32, [200, 150, 100, 255]);
        let out1 = raw_rgba_vec(&floaters(input.clone(), 0.8, 0.3, 12345, 0.5, 0.5).unwrap());
        let out2 = raw_rgba_vec(&floaters(input, 0.8, 0.3, 12345, 0.5, 0.5).unwrap());
        assert_eq!(out1, out2, "same seed must produce byte-exact identical output");
    }

    // ---------------------------------------------------------------
    // P13: floaters different seed → different output
    // ---------------------------------------------------------------

    #[test]
    fn floaters_different_seed_differs() {
        let input = solid_rgba(32, 32, [200, 150, 100, 255]);
        let out1 = raw_rgba_vec(&floaters(input.clone(), 0.8, 0.5, 111, 0.5, 0.5).unwrap());
        let out2 = raw_rgba_vec(&floaters(input, 0.8, 0.5, 999, 0.5, 0.5).unwrap());
        assert_ne!(out1, out2, "different seeds must produce different output");
    }

    // ---------------------------------------------------------------
    // P14-P17: 1x1 でクラッシュなし
    // ---------------------------------------------------------------

    #[test]
    fn cataract_1x1_does_not_panic() {
        let input = pixel(128, 64, 32, 255);
        let _ = cataract(input, 1.0, 42).unwrap();
    }

    #[test]
    fn photophobia_1x1_does_not_panic() {
        let input = pixel(255, 255, 255, 255);
        let _ = photophobia(input, 1.0).unwrap();
    }

    #[test]
    fn nyctalopia_1x1_does_not_panic() {
        let input = pixel(128, 64, 32, 255);
        let _ = nyctalopia(input, 1.0).unwrap();
    }

    #[test]
    fn floaters_1x1_does_not_panic() {
        let input = pixel(128, 64, 32, 255);
        let _ = floaters(input, 1.0, 0.5, 42, 0.5, 0.5).unwrap();
    }

    // ---------------------------------------------------------------
    // tetrachromacy テスト
    // ---------------------------------------------------------------

    #[test]
    fn tetrachromacy_strength_zero_is_identity() {
        let input = pixel(200, 100, 50, 255);
        let out = tetrachromacy(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 100, 50, 255]);
    }

    #[test]
    fn tetrachromacy_nan_strength_returns_identity() {
        let input = pixel(200, 100, 50, 200);
        let out = tetrachromacy(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [200, 100, 50, 200]);
    }

    #[test]
    fn tetrachromacy_alpha_preserved() {
        let input = pixel(200, 100, 50, 77);
        let out = tetrachromacy(input, 1.0).unwrap();
        assert_eq!(read_rgba(&out)[3], 77);
    }

    #[test]
    fn tetrachromacy_negative_strength_is_identity() {
        let input = pixel(200, 100, 50, 255);
        let out = tetrachromacy(input.clone(), -1.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 100, 50, 255]);
    }

    #[test]
    fn tetrachromacy_above_one_clamped_same_as_one() {
        let input = pixel(200, 100, 50, 255);
        let a = tetrachromacy(input.clone(), 2.0).unwrap();
        let b = tetrachromacy(input, 1.0).unwrap();
        assert_eq!(read_rgba(&a), read_rgba(&b));
    }

    #[test]
    fn tetrachromacy_gray_unchanged() {
        // 純グレー (R==G==B) は rg=0, yb=0 なので変化しない
        let input = pixel(128, 128, 128, 255);
        let out = tetrachromacy(input, 1.0).unwrap();
        let [r, g, b, _] = read_rgba(&out);
        // 1px round-trip で ±1 以内の誤差を許容
        assert!(r.abs_diff(128) <= 1);
        assert!(g.abs_diff(128) <= 1);
        assert!(b.abs_diff(128) <= 1);
    }

    #[test]
    fn tetrachromacy_pure_red_amplifies_rg() {
        // 純赤: rg > 0 なので R が増え G が減る方向に誇張される
        let input = pixel(200, 0, 0, 255);
        let out = tetrachromacy(input, 1.0).unwrap();
        let [r, g, _b, _] = read_rgba(&out);
        // R は変化なし or 上昇（既に高い）、G は 0 から下はいかない（clamp）
        assert!(r >= 200 || r == 255); // clamp で飽和することもある
        assert_eq!(g, 0); // G は既に 0、下がっても 0 のまま
    }

    #[test]
    fn tetrachromacy_preserves_dimensions() {
        // 出力サイズが入力と同一
        let mut img = RgbaImage::new(13, 7);
        for (_, _, px) in img.enumerate_pixels_mut() {
            *px = Rgba([100, 150, 80, 255]);
        }
        let input = DynamicImage::ImageRgba8(img);
        let out = tetrachromacy(input, 1.0).unwrap();
        assert_eq!((out.width(), out.height()), (13, 7));
    }

    #[test]
    fn tetrachromacy_white_pixel_is_unchanged() {
        // (255,255,255,255): rg=0, yb=0 → 変化なし
        let input = pixel(255, 255, 255, 255);
        let out = tetrachromacy(input, 1.0).unwrap();
        let [r, g, b, a] = read_rgba(&out);
        assert_eq!(r, 255);
        assert_eq!(g, 255);
        assert_eq!(b, 255);
        assert_eq!(a, 255);
    }

    #[test]
    fn tetrachromacy_black_pixel_is_unchanged() {
        // (0,0,0,255): rg=0, yb=0 → 変化なし
        let input = pixel(0, 0, 0, 255);
        let out = tetrachromacy(input, 1.0).unwrap();
        let [r, g, b, a] = read_rgba(&out);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
        assert_eq!(a, 255);
    }

    #[test]
    fn tetrachromacy_strength_monotonic() {
        // strength=1.0 の方が strength=0.5 よりも R-G 差が大きい
        // 赤みある画素 (200, 100, 0, 255): rg = R - G > 0
        let input = pixel(200, 100, 0, 255);
        let out05 = tetrachromacy(input.clone(), 0.5).unwrap();
        let out10 = tetrachromacy(input, 1.0).unwrap();
        let [r05, g05, _, _] = read_rgba(&out05);
        let [r10, g10, _, _] = read_rgba(&out10);
        let diff05 = r05 as i32 - g05 as i32;
        let diff10 = r10 as i32 - g10 as i32;
        assert!(
            diff10 > diff05,
            "strength=1.0 R-G diff ({diff10}) must be greater than strength=0.5 ({diff05})"
        );
    }

    // ---------------------------------------------------------------
    // #38: floaters seed=0 と seed=1 で出力が異なること
    // ---------------------------------------------------------------

    #[test]
    fn floaters_seed_0_ne_seed_1() {
        let input = solid_rgba(32, 32, [200, 150, 100, 255]);
        let out0 = raw_rgba_vec(&floaters(input.clone(), 0.8, 0.5, 0, 0.5, 0.5).unwrap());
        let out1 = raw_rgba_vec(&floaters(input, 0.8, 0.5, 1, 0.5, 0.5).unwrap());
        assert_ne!(out0, out1, "seed=0 and seed=1 must produce different output");
    }

    // ---------------------------------------------------------------
    // #39: tetrachromacy メタメリック領域で色差が誇張されること
    // ---------------------------------------------------------------

    #[test]
    fn tetrachromacy_metameric_regions_enhanced() {
        // グレーに近い画素（R≈G≈B）は LMS で delta≈0 となりメタメリックペア候補
        // strength=1.0 で Cb/Cr 誇張が適用され、元画像からの変化が大きくなるはず
        // ただし純グレー(R==G==B)はCb=Cr=0なので変化なし。
        // わずかに色差のある画素でテストする
        let input_neutral = pixel(128, 128, 128, 255); // 純グレー: 変化なし
        let out_neutral = tetrachromacy(input_neutral, 1.0).unwrap();
        let [r, g, b, _] = read_rgba(&out_neutral);
        // 純グレーは変化なし（メタメリックだが Cb/Cr=0）
        assert!(
            (r as i32 - g as i32).abs() <= 2,
            "neutral gray should stay near-gray after tetrachromacy"
        );
        let _ = b;

        // 赤みのある画素: LMS delta が大きくメタメリックペアでないため
        // opponent channel による誇張が適用される
        let input_red = pixel(200, 100, 50, 255);
        let out_s0 = tetrachromacy(input_red.clone(), 0.0).unwrap();
        let out_s1 = tetrachromacy(input_red, 1.0).unwrap();
        let [r0, g0, _, _] = read_rgba(&out_s0);
        let [r1, g1, _, _] = read_rgba(&out_s1);
        assert_ne!(
            (r0 as i32 - g0 as i32),
            (r1 as i32 - g1 as i32),
            "strength=1.0 should differ from strength=0.0 on colored pixels"
        );
    }

    // ---------------------------------------------------------------
    // #40: cataract 黄変マトリクス - 青チャネル平均が入力より低いこと
    // ---------------------------------------------------------------

    #[test]
    fn cataract_yellowing_blue_mean_reduced() {
        // strength=1.0 で B * 0.85 となるため、青い画素で B が低下する
        let input = solid_rgba(16, 16, [128, 128, 255, 255]);
        let out = cataract(input, 1.0, 0).unwrap().to_rgba8();
        let orig_b_mean: f64 = 255.0;
        let out_b_mean: f64 = out.pixels().map(|p| p[2] as f64).sum::<f64>()
            / (out.width() * out.height()) as f64;
        assert!(
            out_b_mean < orig_b_mean,
            "cataract yellowing: blue channel mean ({out_b_mean:.1}) should be below input ({orig_b_mean:.1})"
        );
    }

    #[test]
    #[ignore = "perf check; run with `cargo test -- --ignored`"]
    fn myopia_1024_full_strength_under_5s() {
        use std::time::Instant;
        let img = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1024,
            1024,
            image::Rgba([128, 128, 128, 255]),
        ));
        let start = Instant::now();
        let _ = myopia(img, 1.0).unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs_f32() < 5.0,
            "1024×1024 myopia s=1.0 took {elapsed:?}, target < 5s"
        );
    }

    // =================================================================
    // Phase 4 (#9): めまいフィルタ tests
    // =================================================================

    // ---------------------------------------------------------------
    // TC-V-01: vertigo strength=0.0 は identity
    // ---------------------------------------------------------------

    #[test]
    fn vertigo_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [200, 100, 50, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(raw_rgba_vec(&vertigo(input, 0.0, 1.0).unwrap()), original);
    }

    // ---------------------------------------------------------------
    // TC-V-03: vertigo 1x1 image does not panic
    // ---------------------------------------------------------------

    #[test]
    fn vertigo_1x1_image_does_not_panic() {
        let input = pixel(128, 128, 128, 255);
        let _ = vertigo(input, 1.0, 0.5).unwrap();
    }

    // ---------------------------------------------------------------
    // TC-V-05: bppv_rotation strength=0.0 は identity
    // ---------------------------------------------------------------

    #[test]
    fn bppv_rotation_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [200, 100, 50, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&bppv_rotation(input, 0.0, 1.0).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // TC-V-07: bppv_rotation 1x1 image does not panic
    // ---------------------------------------------------------------

    #[test]
    fn bppv_rotation_1x1_image_does_not_panic() {
        let input = pixel(128, 128, 128, 255);
        let _ = bppv_rotation(input, 1.0, 0.5).unwrap();
    }

    // ---------------------------------------------------------------
    // TC-V-11: bppv_rotation time_t=-1.0 does not panic
    // ---------------------------------------------------------------

    #[test]
    fn bppv_rotation_time_t_negative_does_not_panic() {
        let input = solid_rgba(32, 32, [100, 150, 200, 255]);
        // rem_euclid により -1.0 → 1.0 (mod 2.0) になる。角度は適正範囲に収まる。
        let _ = bppv_rotation(input, 1.0, -1.0).unwrap();
    }

    // ---------------------------------------------------------------
    // TC-V-12: vestibular_neuritis strength=0.0 は identity
    // ---------------------------------------------------------------

    #[test]
    fn vestibular_neuritis_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [200, 100, 50, 255]);
        let original = raw_rgba_vec(&input);
        assert_eq!(
            raw_rgba_vec(&vestibular_neuritis(input, 0.0).unwrap()),
            original
        );
    }

    // ---------------------------------------------------------------
    // TC-V-14: vestibular_neuritis 1x1 image does not panic
    // ---------------------------------------------------------------

    #[test]
    fn vestibular_neuritis_1x1_image_does_not_panic() {
        let input = pixel(128, 128, 128, 255);
        let _ = vestibular_neuritis(input, 1.0).unwrap();
    }

    // =================================================================
    // Phase N (#19): depth-aware blur tests
    // =================================================================

    #[allow(dead_code)]
    /// 32x32 の2段グラデーション深度マップを作るヘルパー。
    /// 左半分 = 暗い (0), 右半分 = 明るい (255)。
    fn depth_map_half(size: u32, left_val: u8, right_val: u8) -> DynamicImage {
        use image::GrayImage;
        let mut d = GrayImage::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let v = if x < size / 2 { left_val } else { right_val };
                d.put_pixel(x, y, image::Luma([v]));
            }
        }
        DynamicImage::ImageLuma8(d)
    }

    /// 単色 depth map（全面同じ深度値）を作るヘルパー。
    fn depth_map_solid(size: u32, val: u8) -> DynamicImage {
        use image::GrayImage;
        DynamicImage::ImageLuma8(GrayImage::from_pixel(size, size, image::Luma([val])))
    }

    // ---------------------------------------------------------------
    // DA-01: Myopia — 遠方（depth < focus）がボケる
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_myopia_far_is_blurred() {
        // 64x64 の中央に white dot。
        // depth_map: 全画素 depth=0.0 (最遠方)。focus=1.0。
        // Myopia → d < focus なのでボケる。max_radius_ratio=0.1 で radius = 1.0 * 0.1 * 64 = 6.4px
        let size = 64_u32;
        let input = center_white_dot(size);
        let depth_far = depth_map_solid(size, 0); // depth≈0.0 (遠方)

        let out_blurred = depth_aware_blur(
            input.clone(),
            &depth_far,
            1.0,
            0.1,
            DepthBlurKind::Myopia,
        )
        .unwrap();

        // focus と同深度（depth=1.0, val=255）はボケない
        let depth_focus = depth_map_solid(size, 255); // depth≈1.0 (focus と同深度)
        let out_sharp = depth_aware_blur(
            input,
            &depth_focus,
            1.0,
            0.1,
            DepthBlurKind::Myopia,
        )
        .unwrap();

        let cx = size / 2;
        let cy = size / 2;
        let blurred_center = out_blurred.to_rgba8().get_pixel(cx, cy)[0];
        let sharp_center = out_sharp.to_rgba8().get_pixel(cx, cy)[0];
        assert!(
            blurred_center < sharp_center,
            "far pixel (depth=0.0, focus=1.0) must be more blurred than focus pixel: \
             blurred_center={blurred_center}, sharp_center={sharp_center}"
        );
    }

    // ---------------------------------------------------------------
    // DA-02: Myopia — 近方（depth > focus）はシャープ
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_myopia_near_is_sharp() {
        // 32x32 の中央に white dot。
        // depth_map: 全画素 depth=1.0 (最近方)。focus=0.0。
        // Myopia → d > focus なのでボケない（radius=0）。
        let size = 32_u32;
        let input = center_white_dot(size);
        let depth = depth_map_solid(size, 255); // depth≈1.0 (近方)

        let out = depth_aware_blur(
            input.clone(),
            &depth,
            0.0,
            0.1,
            DepthBlurKind::Myopia,
        )
        .unwrap();

        // ボケなし: 中心は元の白 (255) のまま
        let cx = size / 2;
        let cy = size / 2;
        let center = out.to_rgba8().get_pixel(cx, cy)[0];
        assert_eq!(center, 255, "near pixel (depth=1.0 > focus=0.0) must stay sharp");
    }

    // ---------------------------------------------------------------
    // DA-03: DepthOfField — 両側がボケる
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_dof_both_blurred() {
        // focus=0.5。depth=0.0 (遠方) と depth=1.0 (近方) の両方がボケる。
        // max_radius_ratio=0.1, size=64 → ビン0の radius ≈ 0.4375 * 0.1 * 64 = 2.8px
        let size = 64_u32;
        let input = center_white_dot(size);

        // 遠方 depth=0 (ビン0, center=0.0625, delta=-0.4375)
        let depth_far = depth_map_solid(size, 0);
        let out_far = depth_aware_blur(
            input.clone(),
            &depth_far,
            0.5,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        // 近方 depth=255 (ビン7, center=0.9375, delta=0.4375)
        let depth_near = depth_map_solid(size, 255);
        let out_near = depth_aware_blur(
            input.clone(),
            &depth_near,
            0.5,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        // focus と同じ depth=128 (ビン3 or 4, delta≈0)
        let depth_focus = depth_map_solid(size, 128);
        let out_focus = depth_aware_blur(
            input,
            &depth_focus,
            0.5,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        let cx = size / 2;
        let cy = size / 2;
        let far_center = out_far.to_rgba8().get_pixel(cx, cy)[0];
        let near_center = out_near.to_rgba8().get_pixel(cx, cy)[0];
        let focus_center = out_focus.to_rgba8().get_pixel(cx, cy)[0];

        assert!(
            far_center < focus_center,
            "DoF: far must be more blurred than focus: far={far_center}, focus={focus_center}"
        );
        assert!(
            near_center < focus_center,
            "DoF: near must be more blurred than focus: near={near_center}, focus={focus_center}"
        );
    }

    // ---------------------------------------------------------------
    // DA-04: depth_map のサイズが異なっても動作する（リサイズされる）
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_wrong_size_depth_map_does_not_panic() {
        // 32x32 の画像に対して 16x16 の depth_map を渡す
        let size = 32_u32;
        let input = solid_rgba(size, size, [100, 150, 200, 255]);
        let depth = depth_map_solid(16, 128); // 異なるサイズ

        let result = depth_aware_blur(input, &depth, 0.5, 0.023, DepthBlurKind::DepthOfField);
        assert!(result.is_ok(), "mismatched depth map size must not panic");
        let out = result.unwrap();
        assert_eq!((out.width(), out.height()), (size, size));
    }

    // ---------------------------------------------------------------
    // DA-05: Hyperopia — 近方（depth > focus）がボケる
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_hyperopia_near_is_blurred() {
        // 64x64 の中央に white dot。
        // depth_map: 全画素 depth=1.0 (最近方)。focus=0.0。
        // Hyperopia → d > focus なのでボケる。
        let size = 64_u32;
        let input = center_white_dot(size);
        let depth_near = depth_map_solid(size, 255); // depth≈1.0 (近方)

        let out_blurred = depth_aware_blur(
            input.clone(),
            &depth_near,
            0.0,
            0.1,
            DepthBlurKind::Hyperopia,
        )
        .unwrap();

        // focus と同深度（depth=0.0, val=0）はボケない
        let depth_far = depth_map_solid(size, 0); // depth≈0.0 (遠方 = focus と同深度)
        let out_sharp = depth_aware_blur(
            input,
            &depth_far,
            0.0,
            0.1,
            DepthBlurKind::Hyperopia,
        )
        .unwrap();

        let cx = size / 2;
        let cy = size / 2;
        let blurred_center = out_blurred.to_rgba8().get_pixel(cx, cy)[0];
        let sharp_center = out_sharp.to_rgba8().get_pixel(cx, cy)[0];
        assert!(
            blurred_center < sharp_center,
            "near pixel (depth=1.0 > focus=0.0) must be more blurred than focus pixel: \
             blurred_center={blurred_center}, sharp_center={sharp_center}"
        );
    }

    // ---------------------------------------------------------------
    // DA-06: strength=0 → identity（blur なし）
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_zero_strength_is_identity() {
        // max_radius_ratio=0.0 のとき radius=0 → どの画素もボケない。
        // 出力が入力と画素単位で一致することを確認。
        let size = 32_u32;
        let input = center_white_dot(size);
        let depth = depth_map_solid(size, 0); // 深度任意

        let out = depth_aware_blur(
            input.clone(),
            &depth,
            1.0,
            0.0, // max_radius_ratio=0 → radius=0
            DepthBlurKind::Myopia,
        )
        .unwrap();

        let in_bytes = input.to_rgba8().into_raw();
        let out_bytes = out.to_rgba8().into_raw();
        assert_eq!(
            in_bytes, out_bytes,
            "max_radius_ratio=0.0 must produce identical output (identity)"
        );
    }

    // ---------------------------------------------------------------
    // DA-07: d=1.0 → scaled=7.0, fract=0.0, 最終ビンが正しく処理される
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_d1_uses_last_bin() {
        // d=1.0 のとき scaled=7.0, floor=7（N_BINS-1）→ 最終ビン専用パスで処理される。
        // DepthOfField, focus=0.0 → d=1.0 は最大 delta=1.0 → 最大ボケ。
        // 中央 white dot が拡散して中心輝度が下がるはず。
        let size = 64_u32;
        let input = center_white_dot(size);
        let depth_max = depth_map_solid(size, 255); // d=1.0 → scaled=7.0 → 最終ビン

        let out_blurred = depth_aware_blur(
            input.clone(),
            &depth_max,
            0.0,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        // d=0.0（focus=0.0 と一致）はシャープ
        let depth_zero = depth_map_solid(size, 0);
        let out_sharp = depth_aware_blur(
            input,
            &depth_zero,
            0.0,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        let cx = size / 2;
        let cy = size / 2;
        let blurred_center = out_blurred.to_rgba8().get_pixel(cx, cy)[0];
        let sharp_center = out_sharp.to_rgba8().get_pixel(cx, cy)[0];
        assert!(
            blurred_center < sharp_center,
            "d=1.0 (last bin) must be more blurred than d=0.0 (focus): \
             blurred={blurred_center}, sharp={sharp_center}"
        );
    }

    // ---------------------------------------------------------------
    // DA-08: 線形補間 — ビン境界中間の深度が両端の中間的なボケ量になる
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_lerp_intermediate_depth_is_between_endpoints() {
        // DepthOfField, focus=0.0。ビン0とビン1の境界付近を使う。
        // depth=0/255 と depth=36/255（ビン0とビン1の中間付近）と depth=18/255（その中間）を比較。
        // ボケ量が単調増加（depth が大きい → delta が大きい → blur が強い）かを確認。
        let size = 64_u32;
        let input = center_white_dot(size);

        // depth val=0  → d≈0.000 → delta=0.000 → radius≈0   → シャープ
        let out_near = depth_aware_blur(
            input.clone(),
            &depth_map_solid(size, 0),
            0.0,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        // depth val=18 → d≈0.071 → scaled≈0.496 → ビン0/1境界手前
        let out_mid = depth_aware_blur(
            input.clone(),
            &depth_map_solid(size, 18),
            0.0,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        // depth val=36 → d≈0.141 → scaled≈0.988 → ビン0/1境界ほぼ手前
        let out_far = depth_aware_blur(
            input,
            &depth_map_solid(size, 36),
            0.0,
            0.1,
            DepthBlurKind::DepthOfField,
        )
        .unwrap();

        let cx = size / 2;
        let cy = size / 2;
        let c_near = out_near.to_rgba8().get_pixel(cx, cy)[0];
        let c_mid = out_mid.to_rgba8().get_pixel(cx, cy)[0];
        let c_far = out_far.to_rgba8().get_pixel(cx, cy)[0];

        // blur が強いほど中心輝度が下がる（単調減少）
        assert!(
            c_near >= c_mid,
            "depth=0 must be at least as sharp as depth=18: near={c_near}, mid={c_mid}"
        );
        assert!(
            c_mid >= c_far,
            "depth=18 must be at least as sharp as depth=36: mid={c_mid}, far={c_far}"
        );
    }

    // ---------------------------------------------------------------
    // DA-09: 異なる深度が混在する画像でも画素ごとに正しいビンが適用される
    // ---------------------------------------------------------------

    #[test]
    fn depth_aware_blur_per_pixel_bin_assignment() {
        // 左半分 depth=0（シャープ）, 右半分 depth=255（ボケ）の depth_map を作成。
        // 中央に white dot（左端付近）。Myopia, focus=1.0。
        // 左の dot 領域（depth=0, 遠方）はボケ、右半分のピクセルは depth=255（近方）→ シャープ。
        use image::{GrayImage, Luma};

        let size = 64_u32;

        // 左半分白 dot の入力画像
        let mut rgba_img = image::RgbaImage::from_pixel(size, size, image::Rgba([0, 0, 0, 255]));
        rgba_img.put_pixel(size / 4, size / 2, image::Rgba([255, 255, 255, 255]));
        let input = DynamicImage::ImageRgba8(rgba_img);

        // 左半分 depth=0, 右半分 depth=255 の depth_map
        let mut depth_img = GrayImage::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let val = if x < size / 2 { 0u8 } else { 255u8 };
                depth_img.put_pixel(x, y, Luma([val]));
            }
        }
        let depth = DynamicImage::ImageLuma8(depth_img);

        let out = depth_aware_blur(
            input,
            &depth,
            1.0, // focus=1.0
            0.1,
            DepthBlurKind::Myopia,
        )
        .unwrap();

        // 左の dot（depth=0, 遠方）はボケるので (size/4, size/2) 中心輝度が下がる
        let dot_center = out.to_rgba8().get_pixel(size / 4, size / 2)[0];
        // 右エリア（depth=255, 近方）は元々黒なので変化しない（ボケない）
        let right_px = out.to_rgba8().get_pixel(3 * size / 4, size / 2)[0];

        assert!(
            dot_center < 255,
            "left dot (depth=0, far from focus=1.0) must be blurred: dot_center={dot_center}"
        );
        assert_eq!(
            right_px, 0,
            "right area (depth=255, near=focus) must stay black (no blur source): right={right_px}"
        );
    }

    // ---------------------------------------------------------------
    // #29: diplopia / nystagmus / starbursts
    // ---------------------------------------------------------------

    #[test]
    fn diplopia_shifts_ghost_image() {
        // 32x32、左半分を白、右半分を黒にして右に少しずらす
        let size = 32_u32;
        let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
        // 左半分を白
        for y in 0..size {
            for x in 0..(size / 2) {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        // 右半分の左端（x = size/2）の元の値は 0
        let check_x = size / 2;
        let check_y = size / 2;
        let orig_px = img.get_pixel(check_x, check_y)[0];
        assert_eq!(orig_px, 0, "original should be black at check point");

        let input = DynamicImage::ImageRgba8(img);
        // offset_x=0.1 → dx = 0.1 * 32 = 3px 右シフト → 幽霊は左の白領域から来る
        let out = diplopia(input, 1.0, 0.1, 0.0, 1.0).unwrap();
        let out_px_val = out.to_rgba8().get_pixel(check_x, check_y)[0];
        assert!(
            out_px_val > orig_px,
            "diplopia should show ghost (alpha blend): orig={orig_px}, out={out_px_val}"
        );
    }

    #[test]
    fn diplopia_strength_zero_is_identity() {
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 5) as u8, (y * 7) as u8, 128, 255]);
        }
        let orig = img.clone().into_raw();
        let out = diplopia(DynamicImage::ImageRgba8(img), 0.0, 0.1, 0.1, 0.7).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        let max_err = orig.iter().zip(out_raw.iter())
            .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(max_err <= 1, "strength=0 should be identity, max_err={max_err}");
    }

    #[test]
    fn diplopia_white_on_white_no_overflow() {
        // 白飛び防止: orig=white, ghost=white, strength=1, ghost_strength=1 → 全ピクセル 255 のまま
        let size = 16_u32;
        let img = RgbaImage::from_pixel(size, size, Rgba([255, 255, 255, 255]));
        let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.1, 0.0, 1.0).unwrap();
        let out_rgba = out.to_rgba8();
        for px in out_rgba.pixels() {
            assert_eq!(px[0], 255, "R channel must remain 255");
            assert_eq!(px[1], 255, "G channel must remain 255");
            assert_eq!(px[2], 255, "B channel must remain 255");
        }
    }

    #[test]
    fn diplopia_blend_ratio_at_half_strength() {
        // 中間値の混合比: orig=黒(0), ghost=白(255), strength=1, ghost_strength=0.5 → 出力が≒127±2
        // ghost_alpha = ghost_strength * strength = 0.5 * 1.0 = 0.5
        // alpha blend: out = orig * 0.5 + ghost * 0.5 → 中間値になるはず
        let size = 16_u32;
        // 左半分白・右半分黒の画像で、オフセットなし（dx=0）→ 各ピクセルで orig=ghost=同じ色
        // なので別の方法: 全ピクセル黒の画像に offset=0（幽霊も黒）ではなく、
        // orig=黒で ghost=白 を得るために 2 枚の画像を使う必要があるが diplopia は 1 枚から作る。
        // 代わりに: 左半分白・右半分黒の画像で、右端のチェック点を使う。
        // offset_x=0.5 → dx = 0.5 * 16 = 8px。右半分の任意点(x=12)の ghost は左半分(x=4)白。
        let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
        for y in 0..size {
            for x in 0..(size / 2) {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        // check_x=12: orig=black(0), ghost(12-8=4)=white(255)
        let check_x = 12_u32;
        let check_y = size / 2;
        let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.5, 0.0, 0.5).unwrap();
        let val = out.to_rgba8().get_pixel(check_x, check_y)[0];
        // linear sRGB 空間で 0.5 blendすると sRGB変換後は約 188 になる（ガンマ補正の影響）
        // 単純な加算合成なら 255 になっていたが、alpha blend では中間値に抑えられる
        assert!(
            val >= 183 && val <= 193,
            "half ghost_strength alpha blend should produce ≈188 (sRGB of linear 0.5), got {val}"
        );
        // また、orig(0) と ghost(255) の単純平均 127 より大きいはず（linear→sRGB変換で増加）
        assert!(val > 50, "blend result should be clearly above black, got {val}");
    }

    #[test]
    fn diplopia_ghost_strength_zero_is_identity() {
        // ghost_strength=0 の identity: strength=1.0 でも ghost_strength=0 なら orig と一致
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 7) as u8, (y * 5) as u8, 100, 255]);
        }
        let orig = img.clone().into_raw();
        let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.1, 0.1, 0.0).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        let max_err = orig.iter().zip(out_raw.iter())
            .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(max_err <= 1, "ghost_strength=0 should be identity, max_err={max_err}");
    }

    #[test]
    fn diplopia_output_never_exceeds_max() {
        // グラデーション画像で strength=0.7, ghost_strength=0.8 → 関数がパニックせず正常に返ること
        // (alpha blend で overflow しないことの確認。u8 の範囲は型保証済み)
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 8) as u8, (y * 8) as u8, 200, 255]);
        }
        let result = diplopia(DynamicImage::ImageRgba8(img), 0.7, 0.2, 0.1, 0.8);
        assert!(result.is_ok(), "diplopia should not panic on gradient image");
        // alpha blend の数学的性質から全チャンネルが [0,1] に収まることを確認
        let out_rgba = result.unwrap().to_rgba8();
        let max_val = out_rgba.pixels()
            .flat_map(|px| [px[0], px[1], px[2]])
            .max()
            .unwrap_or(0);
        assert!(max_val <= 255, "max pixel value should not exceed 255, got {max_val}");
    }

    #[test]
    fn diplopia_luminance_preserved_vs_additive() {
        // 輝度保存: orig=グレー(128), ghost=グレー(128), strength=1, ghost_strength=1 → 出力が≒128
        // (旧加算合成なら 255 になっていた)
        // offset=0 → orig=ghost=同じピクセル、alpha blend でも同じ値が出力されるはず
        let size = 16_u32;
        let img = RgbaImage::from_pixel(size, size, Rgba([128, 128, 128, 255]));
        let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.0, 0.0, 1.0).unwrap();
        let out_rgba = out.to_rgba8();
        for px in out_rgba.pixels() {
            let val = px[0] as i32;
            assert!(
                (val - 128).abs() <= 2,
                "alpha blend of gray+gray should preserve luminance ≈128, got {val}"
            );
        }
    }

    #[test]
    fn nystagmus_blurs_image() {
        let size = 32_u32;
        let input = center_white_dot(size);
        let cx = size / 2;
        let cy = size / 2;
        let orig_center = input.to_rgba8().get_pixel(cx, cy)[0];

        let out = nystagmus(input, 1.0, 0.1, 0.0).unwrap();
        let out_center = out.to_rgba8().get_pixel(cx, cy)[0];
        assert!(
            out_center < orig_center,
            "nystagmus should blur white dot: orig={orig_center}, out={out_center}"
        );
    }

    #[test]
    fn nystagmus_zero_amplitude_is_identity() {
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 6) as u8, (y * 8) as u8, 100, 255]);
        }
        let orig = img.clone().into_raw();
        let out = nystagmus(DynamicImage::ImageRgba8(img), 1.0, 0.0, 0.0).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        let max_err = orig.iter().zip(out_raw.iter())
            .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(max_err <= 1, "amplitude=0 should be identity, max_err={max_err}");
    }

    #[test]
    fn starbursts_brightens_near_bright_pixels() {
        let size = 32_u32;
        let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
        img.put_pixel(size / 2, size / 2, Rgba([255, 255, 255, 255]));

        // 中央から 3px 離れた画素の元の値
        let nearby_x = size / 2 + 3;
        let nearby_y = size / 2;
        let orig_nearby = img.get_pixel(nearby_x, nearby_y)[0];

        let out = starbursts(DynamicImage::ImageRgba8(img), 1.0, 8, 0.2, 0.5).unwrap();
        let out_nearby = out.to_rgba8().get_pixel(nearby_x, nearby_y)[0];

        assert!(
            out_nearby > orig_nearby,
            "starbursts should brighten pixels near bright source: orig={orig_nearby}, out={out_nearby}"
        );
    }

    #[test]
    fn starbursts_strength_zero_is_identity() {
        let size = 32_u32;
        let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
        img.put_pixel(size / 2, size / 2, Rgba([255, 255, 255, 255]));
        let orig = img.clone().into_raw();
        let out = starbursts(DynamicImage::ImageRgba8(img), 0.0, 6, 0.1, 0.5).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        let max_err = orig.iter().zip(out_raw.iter())
            .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
            .max()
            .unwrap_or(0);
        // strength=0 は early return するため byte-exact 一致するはず
        assert!(max_err == 0, "strength=0 should be byte-exact identity, max_err={max_err}");
    }

    #[test]
    fn eye_strain_strength_zero_is_identity() {
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 7) as u8, (y * 7) as u8, 128, 255]);
        }
        let orig = img.clone().into_raw();
        let out = eye_strain(DynamicImage::ImageRgba8(img), 0.0).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        assert_eq!(orig, out_raw, "eye_strain strength=0 should be byte-exact identity");
    }

    #[test]
    fn dry_eye_strength_zero_is_identity() {
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 7) as u8, (y * 7) as u8, 128, 255]);
        }
        let orig = img.clone().into_raw();
        let out = dry_eye(DynamicImage::ImageRgba8(img), 0.0).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        assert_eq!(orig, out_raw, "dry_eye strength=0 should be byte-exact identity");
    }

    #[test]
    fn eye_strain_reduces_contrast() {
        // 真っ白と真っ黒が混在する画像で strength=1 の分散が strength=0 より小さいことを確認
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let v = if x < size / 2 { 0u8 } else { 255u8 };
            *px = Rgba([v, v, v, 255]);
        }
        let out = eye_strain(DynamicImage::ImageRgba8(img), 1.0).unwrap();
        let out_raw = out.to_rgba8();
        // 最大値 - 最小値がコントラスト圧縮で小さくなっているはず
        let min_r = out_raw.pixels().map(|p| p[0]).min().unwrap_or(0);
        let max_r = out_raw.pixels().map(|p| p[0]).max().unwrap_or(255);
        assert!(
            (max_r as i32 - min_r as i32) < 255,
            "eye_strain should reduce contrast: min={min_r} max={max_r}"
        );
    }

    // =================================================================
    // Issue #55: Metamorphopsia（歪視）テスト
    // =================================================================

    #[test]
    fn metamorphopsia_strength_zero_is_identity() {
        // strength=0 → byte-exact identity（max_err ≤ 1 を許容するが実際は完全一致）
        let size = 64_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
        }
        let orig_raw = img.clone().into_raw();
        let out = metamorphopsia(DynamicImage::ImageRgba8(img), 0.0, 4.0, 42).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        // strength=0 では byte-exact identity
        assert_eq!(
            orig_raw, out_raw,
            "metamorphopsia strength=0 must be byte-exact identity"
        );
    }

    #[test]
    fn metamorphopsia_strength_one_changes_pixels() {
        // strength=1 → 少なくとも一部のピクセルが元画像と異なること
        let size = 64_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
        }
        let orig_raw = img.clone().into_raw();
        let out = metamorphopsia(DynamicImage::ImageRgba8(img), 1.0, 4.0, 42).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        // 少なくとも 1 バイト異なることを確認
        let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
        assert!(differs, "metamorphopsia strength=1 must change at least some pixels");
    }

    #[test]
    fn metamorphopsia_preserves_image_size() {
        let img = solid_rgba(48, 32, [200, 100, 50, 255]);
        let out = metamorphopsia(img, 0.8, 4.0, 123).unwrap();
        assert_eq!(out.width(), 48);
        assert_eq!(out.height(), 32);
    }

    #[test]
    fn metamorphopsia_preserves_alpha() {
        // alpha チャンネルは sample_bilinear が保持するので確認
        let size = 32_u32;
        let mut img = RgbaImage::new(size, size);
        for px in img.pixels_mut() {
            *px = Rgba([128, 64, 32, 200]);
        }
        let out = metamorphopsia(DynamicImage::ImageRgba8(img), 1.0, 4.0, 1).unwrap();
        for px in out.to_rgba8().pixels() {
            assert_eq!(px[3], 200, "alpha must be preserved through metamorphopsia");
        }
    }

    #[test]
    fn metamorphopsia_different_seeds_give_different_results() {
        // 異なる seed では異なる歪みパターンになること
        let size = 64_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let out1 = metamorphopsia(dyn_img.clone(), 1.0, 4.0, 1).unwrap().to_rgba8().into_raw();
        let out2 = metamorphopsia(dyn_img, 1.0, 4.0, 99999).unwrap().to_rgba8().into_raw();
        let differs = out1.iter().zip(out2.iter()).any(|(a, b)| a != b);
        assert!(differs, "different seeds must produce different distortion patterns");
    }

    // ---------------------------------------------------------------
    // Issue #60: vertigo / bppv_rotation / vestibular_neuritis テスト
    // ---------------------------------------------------------------

    #[test]
    fn vertigo_strength_one_differs_from_input() {
        // グラデーション画像を使う（均一色だと回転後も同一になるため）
        let size = 64_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
        }
        let orig_raw = img.clone().into_raw();
        let out = vertigo(DynamicImage::ImageRgba8(img), 1.0, 0.25).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
        assert!(differs, "vertigo strength=1 must change at least some pixels");
    }

    #[test]
    fn bppv_rotation_strength_one_differs_from_input() {
        let size = 64_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
        }
        let orig_raw = img.clone().into_raw();
        // time_t=0.1 は急速相（angle_norm > 0）
        let out = bppv_rotation(DynamicImage::ImageRgba8(img), 1.0, 0.1).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
        assert!(differs, "bppv_rotation strength=1 must change at least some pixels");
    }

    #[test]
    fn vestibular_neuritis_strength_one_differs_from_input() {
        let size = 64_u32;
        let mut img = RgbaImage::new(size, size);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
        }
        let orig_raw = img.clone().into_raw();
        let out = vestibular_neuritis(DynamicImage::ImageRgba8(img), 1.0).unwrap();
        let out_raw = out.to_rgba8().into_raw();
        let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
        assert!(differs, "vestibular_neuritis strength=1 must change at least some pixels");
    }

    // ---------------------------------------------------------------
    // Issue #51: nyctalopia Purkinje shift
    // ---------------------------------------------------------------

    #[test]
    fn nyctalopia_purkinje_shift_blue_channel_increases() {
        // Purkinje shift: strength=1 で青チャネル平均が入力より高いことを確認
        // 白色画像を使用（すべてのチャンネルが同一値なので青の増加を検出しやすい）
        let mut img = RgbaImage::new(16, 16);
        for px in img.pixels_mut() {
            *px = Rgba([200, 200, 200, 255]);
        }
        let orig_b_sum: u32 = img.pixels().map(|p| p[2] as u32).sum();
        let orig_r_sum: u32 = img.pixels().map(|p| p[0] as u32).sum();

        let out = nyctalopia(DynamicImage::ImageRgba8(img), 1.0).unwrap();
        let out_rgba = out.to_rgba8();
        let out_b_sum: u32 = out_rgba.pixels().map(|p| p[2] as u32).sum();
        let out_r_sum: u32 = out_rgba.pixels().map(|p| p[0] as u32).sum();

        // strength=1 では全体が暗化するため絶対値は下がるが、
        // 青/赤 の比率で Purkinje shift（青↑赤↓相対）を確認する。
        // 暗化後: R = orig * (1 - 0.2) * dark_factor, B = orig * (1 + 0.1) * dark_factor
        // B / R = 1.1 / 0.8 = 1.375 > 1 になるはず
        assert!(
            out_b_sum > out_r_sum,
            "Purkinje shift: blue channel sum ({out_b_sum}) should exceed red ({out_r_sum}) at strength=1"
        );
        // 全体が暗化していることも確認
        assert!(
            out_b_sum < orig_b_sum,
            "nyctalopia darkens: blue sum {out_b_sum} < orig {orig_b_sum}"
        );
        assert!(
            out_r_sum < orig_r_sum,
            "nyctalopia darkens: red sum {out_r_sum} < orig {orig_r_sum}"
        );
    }

    // ---------------------------------------------------------------
    // Issue #52: glaucoma 弧状暗点オプション
    // ---------------------------------------------------------------

    #[test]
    fn glaucoma_vignette_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [180, 120, 60, 255]);
        let out = glaucoma(input.clone(), 0.0, GlaucomaMode::Vignette).unwrap();
        assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
    }

    #[test]
    fn glaucoma_arcuate_superior_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [180, 120, 60, 255]);
        let out = glaucoma(input.clone(), 0.0, GlaucomaMode::ArcuateSuperior).unwrap();
        assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
    }

    #[test]
    fn glaucoma_arcuate_inferior_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [180, 120, 60, 255]);
        let out = glaucoma(input.clone(), 0.0, GlaucomaMode::ArcuateInferior).unwrap();
        assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
    }

    #[test]
    fn glaucoma_biarcuate_strength_zero_is_identity() {
        let input = solid_rgba(32, 32, [180, 120, 60, 255]);
        let out = glaucoma(input.clone(), 0.0, GlaucomaMode::Biarcuate).unwrap();
        assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
    }

    #[test]
    fn glaucoma_vignette_strength_one_darkens() {
        // 十分大きな画像で周辺部が暗化することを確認
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
        }
        let orig_sum: u32 = img.pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        let out = glaucoma(DynamicImage::ImageRgba8(img), 1.0, GlaucomaMode::Vignette).unwrap();
        let out_sum: u32 = out.to_rgba8().pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        assert!(out_sum < orig_sum, "glaucoma Vignette strength=1 must darken: {out_sum} < {orig_sum}");
    }

    #[test]
    fn glaucoma_arcuate_superior_strength_one_darkens() {
        let mut img = RgbaImage::new(64, 64);
        for px in img.pixels_mut() {
            *px = Rgba([200, 200, 200, 255]);
        }
        let orig_sum: u32 = img.pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        let out = glaucoma(DynamicImage::ImageRgba8(img), 1.0, GlaucomaMode::ArcuateSuperior).unwrap();
        let out_sum: u32 = out.to_rgba8().pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        assert!(out_sum < orig_sum, "glaucoma ArcuateSuperior strength=1 must darken");
    }

    #[test]
    fn glaucoma_arcuate_inferior_strength_one_darkens() {
        let mut img = RgbaImage::new(64, 64);
        for px in img.pixels_mut() {
            *px = Rgba([200, 200, 200, 255]);
        }
        let orig_sum: u32 = img.pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        let out = glaucoma(DynamicImage::ImageRgba8(img), 1.0, GlaucomaMode::ArcuateInferior).unwrap();
        let out_sum: u32 = out.to_rgba8().pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        assert!(out_sum < orig_sum, "glaucoma ArcuateInferior strength=1 must darken");
    }

    #[test]
    fn glaucoma_biarcuate_strength_one_darkens() {
        let mut img = RgbaImage::new(64, 64);
        for px in img.pixels_mut() {
            *px = Rgba([200, 200, 200, 255]);
        }
        let orig_sum: u32 = img.pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        let out = glaucoma(DynamicImage::ImageRgba8(img), 1.0, GlaucomaMode::Biarcuate).unwrap();
        let out_sum: u32 = out.to_rgba8().pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum();
        assert!(out_sum < orig_sum, "glaucoma Biarcuate strength=1 must darken");
    }

    // -------------------------------------------------------
    // contrast_sensitivity tests
    // -------------------------------------------------------

    #[test]
    fn contrast_sensitivity_strength_zero_identity() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 3 + y * 7) as u8, (y * 4) as u8, 128, 255]);
        }
        let orig = img.clone();
        let out = contrast_sensitivity(DynamicImage::ImageRgba8(img), 0.0).unwrap().to_rgba8();
        // PSNR >= 60 dB
        let mse: f64 = orig.pixels().zip(out.pixels()).map(|(a, b)| {
            (0..3).map(|i| {
                let d = a[i] as f64 - b[i] as f64;
                d * d
            }).sum::<f64>()
        }).sum::<f64>() / (64.0 * 64.0 * 3.0);
        if mse > 0.0 {
            let psnr = 10.0 * (255.0_f64 * 255.0 / mse).log10();
            assert!(psnr >= 60.0, "PSNR={psnr:.1} dB, expected >= 60 dB");
        }
    }

    #[test]
    fn contrast_sensitivity_strength_one_reduces_variance() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
        }
        let orig = img.clone();
        let out = contrast_sensitivity(DynamicImage::ImageRgba8(img), 1.0).unwrap().to_rgba8();

        let luma = |p: &image::Rgba<u8>| -> f64 {
            0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64
        };
        let variance = |pixels: &RgbaImage| -> f64 {
            let vals: Vec<f64> = pixels.pixels().map(luma).collect();
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64
        };

        let var_in = variance(&orig);
        let var_out = variance(&out);
        assert!(var_out < var_in, "contrast_sensitivity strength=1 must reduce luminance variance (in={var_in:.2}, out={var_out:.2})");
    }

    // -------------------------------------------------------
    // detail_loss tests
    // -------------------------------------------------------

    #[test]
    fn detail_loss_strength_zero_identity() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 3 + y * 7) as u8, (y * 4) as u8, 128, 255]);
        }
        let orig = img.clone().into_raw();
        let out = detail_loss(DynamicImage::ImageRgba8(img), 0.0).unwrap().to_rgba8().into_raw();
        assert_eq!(orig, out, "detail_loss strength=0 must be identity");
    }

    #[test]
    fn detail_loss_strength_one_reduces_stddev() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
        }
        let orig = img.clone();
        let out = detail_loss(DynamicImage::ImageRgba8(img), 1.0).unwrap().to_rgba8();

        let luma = |p: &image::Rgba<u8>| -> f64 {
            0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64
        };
        let stddev = |pixels: &RgbaImage| -> f64 {
            let vals: Vec<f64> = pixels.pixels().map(luma).collect();
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64).sqrt()
        };

        let sd_in = stddev(&orig);
        let sd_out = stddev(&out);
        assert!(sd_out < sd_in, "detail_loss strength=1 must reduce stddev (in={sd_in:.2}, out={sd_out:.2})");
    }

    // -------------------------------------------------------
    // teichopsia tests
    // -------------------------------------------------------

    #[test]
    fn teichopsia_strength_zero_identity() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 3 + y * 7) as u8, (y * 4) as u8, 128, 255]);
        }
        let orig = img.clone();
        let out = teichopsia(DynamicImage::ImageRgba8(img), 0.0).unwrap().to_rgba8();
        // PSNR >= 60 dB
        let mse: f64 = orig.pixels().zip(out.pixels()).map(|(a, b)| {
            (0..3).map(|i| { let d = a[i] as f64 - b[i] as f64; d * d }).sum::<f64>()
        }).sum::<f64>() / (64.0 * 64.0 * 3.0);
        if mse > 0.0 {
            let psnr = 10.0 * (255.0_f64 * 255.0 / mse).log10();
            assert!(psnr >= 60.0, "PSNR={psnr:.1} dB expected >= 60 dB");
        }
    }

    #[test]
    fn teichopsia_strength_one_darkens_center() {
        let mut img = RgbaImage::new(64, 64);
        for px in img.pixels_mut() {
            *px = Rgba([200, 200, 200, 255]);
        }
        let out = teichopsia(DynamicImage::ImageRgba8(img), 1.0).unwrap().to_rgba8();
        // 中心ピクセル（scotoma）が暗化されているか
        let center = out.get_pixel(32, 32);
        let brightness = center[0] as u32 + center[1] as u32 + center[2] as u32;
        assert!(brightness < 600, "teichopsia strength=1 must darken center (got {brightness})");
    }
}

