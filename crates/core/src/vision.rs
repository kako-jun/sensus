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
// Phase 3: light / transparency (cataract, photophobia, nyctalopia, floaters)
// ---------------------------------------------------------------------

/// 白内障（Cataract）シミュレーション。
///
/// linear sRGB 空間で輝度低下・黄色み追加・局所白濁ノイズを適用する。
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

    // チャンネルごとの乗数（黄色み: B を強く抑制）
    let r_factor = 1.0 - strength * 0.3_f32;
    let g_factor = 1.0 - strength * 0.3_f32;
    let b_factor = 1.0 - strength * 0.6_f32;

    // 白濁ノイズの最大ブレンド量
    const WHITE_BLEND_MAX: f32 = 0.4;
    // 8x8 ブロック単位でノイズを決定（白内障の白濁は粗い濁り）
    const BLOCK_SIZE: u32 = 8;

    // ブロックノイズ値を事前計算
    let block_cols = width.div_ceil(BLOCK_SIZE);
    let block_rows = height.div_ceil(BLOCK_SIZE);
    let mut block_noise: Vec<f32> =
        Vec::with_capacity((block_cols * block_rows) as usize);
    for by in 0..block_rows {
        for bx in 0..block_cols {
            // ハッシュで擬似ランダム値を生成
            let h = seed
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add((bx as u64).wrapping_mul(0x517cc1b727220a95))
                .wrapping_add((by as u64).wrapping_mul(0x6c62272e07bb0142));
            // 上位ビットを使って 0.0..=1.0 に正規化
            let n = (h >> 32) as f32 / u32::MAX as f32;
            block_noise.push(n);
        }
    }

    for y in 0..height {
        for x in 0..width {
            let px = rgba.get_pixel_mut(x, y);

            // linear sRGB に変換
            let r = srgb_to_linear(px[0] as f32 / 255.0);
            let g = srgb_to_linear(px[1] as f32 / 255.0);
            let b = srgb_to_linear(px[2] as f32 / 255.0);

            // チャンネル別輝度低下・黄色み
            let nr = r * r_factor;
            let ng = g * g_factor;
            let nb = b * b_factor;

            // ブロックノイズによる白濁
            let bx = (x / BLOCK_SIZE) as usize;
            let by = (y / BLOCK_SIZE) as usize;
            let noise = block_noise[by * block_cols as usize + bx];
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
    if bloom_radius >= MIN_BLUR_RADIUS_PX {
        highlight = ellipse_blur(&highlight, width, height, bloom_radius, bloom_radius, 0.0);
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

        let y = LUMA_R * r + LUMA_G * g + LUMA_B * b;

        // 脱色（グレーに寄せる）してから暗化
        let dr = r + (y - r) * desat;
        let dg = g + (y - g) * desat;
        let db = b + (y - b) * desat;

        let fr = dr * dark_factor;
        let fg = dg * dark_factor;
        let fb = db * dark_factor;

        px[0] = pack_u8(linear_to_srgb(fr));
        px[1] = pack_u8(linear_to_srgb(fg));
        px[2] = pack_u8(linear_to_srgb(fb));
        // alpha はそのまま
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

/// 飛蚊症（Floaters）シミュレーション。
///
/// 視野内に暗い blob が浮かぶオーバーレイを乗算ブレンドで適用する。
///
/// - `strength`: 0.0 = 元画像, 1.0 = 強い飛蚊症
/// - `density`: blob 密度 (0.0..=1.0)
/// - `seed`: blob 配置のランダムシード
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

    // blob 数と半径
    let blob_count = ((density * 200.0) as usize).max(1);
    let blob_radius = (w_f.min(h_f) * 0.04).max(2.0);
    let blob_radius_sq = blob_radius * blob_radius;

    // blob 中心位置を seed から生成
    let mut centers: Vec<(f32, f32)> = Vec::with_capacity(blob_count);
    for i in 0..blob_count {
        let hx = seed
            .wrapping_mul(0x9e3779b97f4a7c15)
            .wrapping_add((i as u64).wrapping_mul(0x517cc1b727220a95))
            .wrapping_add(0xdeadbeefcafe1234);
        let hy = seed
            .wrapping_mul(0x6c62272e07bb0142)
            .wrapping_add((i as u64).wrapping_mul(0x9e3779b97f4a7c15))
            .wrapping_add(0xc0ffee0102030405);
        let cx = (hx >> 32) as f32 / u32::MAX as f32 * w_f + offset_x;
        let cy = (hy >> 32) as f32 / u32::MAX as f32 * h_f + offset_y;
        centers.push((cx, cy));
    }

    // フローターマスクを生成して元画像に乗算ブレンド
    let mut out_rgba = rgba.clone();
    for y in 0..height {
        for x in 0..width {
            let xf = x as f32;
            let yf = y as f32;

            // 最も近い blob との距離でマスク値を決定
            let mut min_dist_sq = f32::MAX;
            for &(cx, cy) in &centers {
                let dx = xf - cx;
                let dy = yf - cy;
                let d2 = dx * dx + dy * dy;
                if d2 < min_dist_sq {
                    min_dist_sq = d2;
                }
            }

            // blob 内部: smoothstep 減衰でマスク値を計算
            // mask = 0.0 → フローター（暗い）、1.0 → 元画像
            let mask = if min_dist_sq < blob_radius_sq {
                let t = min_dist_sq / blob_radius_sq;
                // smoothstep: 外側ほど 1.0 に近い
                t * t * (3.0 - 2.0 * t)
            } else {
                1.0
            };

            // 元画像 × (1.0 - strength * (1.0 - mask)) で乗算ブレンド
            let blend = 1.0 - strength * (1.0 - mask);

            let px = out_rgba.get_pixel_mut(x, y);
            px[0] = pack_u8(px[0] as f32 / 255.0 * blend);
            px[1] = pack_u8(px[1] as f32 / 255.0 * blend);
            px[2] = pack_u8(px[2] as f32 / 255.0 * blend);
            // alpha はそのまま
        }
    }

    Ok(DynamicImage::ImageRgba8(out_rgba))
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

    // ---------------------------------------------------------------
    // 性能リグレッションガード (--ignored)
    // ---------------------------------------------------------------

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
}
