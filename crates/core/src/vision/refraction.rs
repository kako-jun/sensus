//! 焦点・屈折フィルタ（disk / cylinder / depth blur）。
//!
//! 近視・遠視・老眼・乱視・深度依存 blur。各 `MAX_RADIUS_RATIO` 定数と
//! `DepthBlurKind` はこの領域専用のため本モジュールに置く。

use super::*;
use crate::Result;
use image::DynamicImage;

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

/// Myopia (近視) シミュレーション。
///
/// strength=1.0 で約 -6D 相当の defocus blur (disk 半径 ≈ 2.3% × min(W,H)、
/// `MYOPIA_MAX_RADIUS_RATIO` の導出は同定数の doc コメント参照)。
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
///
/// # consumer からの到達経路（#107）
///
/// 深度ブラーは深度マップという第 2 入力を要するため、単一入力契約の
/// [`crate::Filter`] / [`crate::apply`] には載せていない（`Filter` は `Copy` で、
/// 画像を抱える深度マップを variant に入れられない）。consumer の到達経路は 2 つ:
/// Rust ライブラリは本関数 `depth_aware_blur` を直接呼ぶ（既に `pub`）。
/// GLSL（universal-experience の Flutter 経路）は [`crate::shaders::depth_aware_blur_glsl`]
/// + [`crate::shaders::depth_aware_blur_uniforms`] を使い、深度マップを `uDepth` テクスチャで渡す。
///
/// CPU は 8 ビン box blur の多パス、GLSL は per-fragment Fibonacci 16 tap disk と算法が
/// 異なるため bit/PSNR 等価ではなく、効果（ピント面鮮明・離れるほどぼける・kind 選択）で担保する。
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
                if delta < 0.0 {
                    (-delta) * max_radius_ratio * min_dim
                } else {
                    0.0
                }
            }
            DepthBlurKind::Hyperopia => {
                if delta > 0.0 {
                    delta * max_radius_ratio * min_dim
                } else {
                    0.0
                }
            }
            DepthBlurKind::DepthOfField => delta.abs() * max_radius_ratio * min_dim,
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
            ellipse_blur(
                &linear,
                w,
                h,
                bin_radius[floor_bin],
                bin_radius[floor_bin],
                0.0,
            )
        };
        let blur_ceil = if bin_radius[ceil_bin] < MIN_BLUR_RADIUS_PX {
            linear.clone()
        } else {
            ellipse_blur(
                &linear,
                w,
                h,
                bin_radius[ceil_bin],
                bin_radius[ceil_bin],
                0.0,
            )
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
            ellipse_blur(
                &linear,
                w,
                h,
                bin_radius[N_BINS - 1],
                bin_radius[N_BINS - 1],
                0.0,
            )
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
