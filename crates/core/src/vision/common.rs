//! クロスドメイン共有ヘルパー（複数フィルタが使う純粋関数）。
//!
//! srgb 変換・blur カーネル・bilinear サンプリングなど、特定の症状に属さず
//! 複数のサブモジュールから呼ばれる純粋関数群を集約する。`srgb_to_linear` /
//! `linear_to_srgb` は integration test (`tests/shader_equivalence.rs`) からも
//! 参照するため `pub`、それ以外は `pub(crate)` に留める。

use crate::Result;
use image::{DynamicImage, RgbaImage};

// mask_mapped_blur_desaturate (#171) が彩度低下の luma 計算に使う BT.709 係数。
use super::{LUMA_B, LUMA_G, LUMA_R};

/// sRGB (0.0..=1.0) → linear sRGB の標準ガンマ解除。
#[inline]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// linear sRGB → sRGB (0.0..=1.0) の標準ガンマ適用。
#[inline]
pub fn linear_to_srgb(c: f32) -> f32 {
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
pub(crate) fn pack_u8(c: f32) -> u8 {
    if c.is_nan() {
        0
    } else {
        (c.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

/// 識別不能とみなす最小半径 (px)。1px 未満のぼけは視認できないため identity。
pub(crate) const MIN_BLUR_RADIUS_PX: f32 = 0.5;

/// strength を 0.0..=1.0 に正規化する。NaN は 0 (identity) として扱う。
///
/// CPU フィルタ全段で適用される正規化。`shaders` の `*_uniforms` も同じ正規化を
/// 適用して `uStrength` を CPU と一致させる（#120）。
#[inline]
pub(crate) fn normalize_strength(strength: f32) -> f32 {
    if strength.is_nan() {
        0.0
    } else {
        strength.clamp(0.0, 1.0)
    }
}

/// 線形補間: `a` と `b` を `t` (0.0..=1.0) で補間する。
#[inline]
pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// RGBA8 画像を linear sRGB の `[r, g, b]` 配列 + alpha 配列に分離する。
pub(crate) fn rgba_to_linear_planes(rgba: &RgbaImage) -> (Vec<[f32; 3]>, Vec<u8>) {
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
pub(crate) fn linear_planes_to_rgba(
    linear: &[[f32; 3]],
    alpha: &[u8],
    width: u32,
    height: u32,
) -> RgbaImage {
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
pub(crate) struct EllipseSpans {
    /// dy が `dy_min..=dy_max` のとき、有効な行は dy = dy_min + i (i は 0 始まり)。
    dy_min: i32,
    /// 各行の (x_min, x_max) 包含範囲。空行は持たない (確実に origin を含む)。
    rows: Vec<(i32, i32)>,
    /// 楕円内の全ピクセル数 (= 平均化の分母)。
    count: usize,
}

pub(crate) fn build_ellipse_spans(a: f32, b: f32, axis_rad: f32) -> EllipseSpans {
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
pub(crate) fn ellipse_blur(
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
pub(crate) fn isotropic_disk_blur_image(img: DynamicImage, radius_px: f32) -> Result<DynamicImage> {
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
pub(crate) fn radius_from_strength(img: &DynamicImage, strength: f32, max_ratio: f32) -> f32 {
    let s = normalize_strength(strength);
    if s == 0.0 {
        return 0.0;
    }
    let min_dim = img.width().min(img.height()) as f32;
    s * max_ratio * min_dim
}

// ---------------------------------------------------------------
// #171: FieldLossMode::Blur 共用ヘルパー — mask 値map→半径map blur
// ---------------------------------------------------------------

/// mask 値（0.0..=1.0, 0=無傷 1=完全欠損）を disk blur 半径のスケールとして
/// 解釈し、8-bin 逐次 blur + 線形補間で近似的に半径マップ付き blur を適用する
/// （`refraction::depth_aware_blur` と同じ 8-bin 方式。O(W×H×kernel) の
/// per-pixel 畳み込みを避ける）。
///
/// `depth_aware_blur` とはビン→半径の対応が異なる: そちらは深度の対称性から
/// ビン中心 (bin+0.5)/N を使うが、本関数は mask=0.0 のとき半径 0（完全に
/// 無変化）を保証したいため、ビン 0 = 半径 0・ビン (N-1) = `max_radius_px`
/// となる等間隔割り付けを使う。
///
/// mask 値に応じて彩度低下も適用する（linear 空間で luma 方向へブレンド、
/// `desaturate_max` は mask=1.0 における最大低下率）。alpha は元画像から
/// 変更せず保持する。
pub(crate) fn mask_mapped_blur_desaturate(
    rgba: &RgbaImage,
    mask: &[f32],
    max_radius_px: f32,
    desaturate_max: f32,
) -> RgbaImage {
    let width = rgba.width();
    let height = rgba.height();
    let (linear, alpha) = rgba_to_linear_planes(rgba);
    debug_assert_eq!(
        mask.len(),
        linear.len(),
        "mask length must match pixel count"
    );

    const N_BINS: usize = 8;
    // bin 0 = 半径 0（mask=0 は無変化を保証）, bin (N_BINS-1) = max_radius_px。
    let mut bin_radius = [0.0_f32; N_BINS];
    for (bin, radius) in bin_radius.iter_mut().enumerate() {
        *radius = (bin as f32 / (N_BINS - 1) as f32) * max_radius_px;
    }

    let npx = linear.len();
    let mut out_linear: Vec<[f32; 3]> = linear.clone();

    // 隣接 2 ビンを逐次処理して線形補間する（depth_aware_blur と同じ構成）。
    for floor_bin in 0..(N_BINS - 1) {
        let ceil_bin = floor_bin + 1;

        let pair_used = mask.iter().any(|&m| {
            let mc = m.clamp(0.0, 1.0);
            let scaled = mc * (N_BINS - 1) as f32;
            (scaled.floor() as usize).min(N_BINS - 1) == floor_bin
        });
        if !pair_used {
            continue;
        }

        let blur_floor = if bin_radius[floor_bin] < MIN_BLUR_RADIUS_PX {
            linear.clone()
        } else {
            ellipse_blur(
                &linear,
                width,
                height,
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
                width,
                height,
                bin_radius[ceil_bin],
                bin_radius[ceil_bin],
                0.0,
            )
        };

        for idx in 0..npx {
            let mc = mask[idx].clamp(0.0, 1.0);
            let scaled = mc * (N_BINS - 1) as f32;
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

    // 最終ビン（mask=1.0 ちょうど）: scaled = N_BINS-1 → fract = 0 → 上のループの
    // どの floor_bin にも一致しない（depth_aware_blur と同じ理由）ので別処理する。
    {
        let blur_last = if bin_radius[N_BINS - 1] < MIN_BLUR_RADIUS_PX {
            linear.clone()
        } else {
            ellipse_blur(
                &linear,
                width,
                height,
                bin_radius[N_BINS - 1],
                bin_radius[N_BINS - 1],
                0.0,
            )
        };
        for idx in 0..npx {
            let mc = mask[idx].clamp(0.0, 1.0);
            let scaled = mc * (N_BINS - 1) as f32;
            let bf = (scaled.floor() as usize).min(N_BINS - 1);
            if bf == N_BINS - 1 {
                out_linear[idx] = blur_last[idx];
            }
        }
    }

    // 彩度低下: mask に応じて luma 方向へブレンド（黒には落とさない）。
    for idx in 0..npx {
        let mc = mask[idx].clamp(0.0, 1.0);
        let desat_t = mc * desaturate_max;
        if desat_t > 0.0 {
            let px = out_linear[idx];
            let luma = LUMA_R * px[0] + LUMA_G * px[1] + LUMA_B * px[2];
            out_linear[idx] = [
                lerp(px[0], luma, desat_t),
                lerp(px[1], luma, desat_t),
                lerp(px[2], luma, desat_t),
            ];
        }
    }

    linear_planes_to_rgba(&out_linear, &alpha, width, height)
}

// ---------------------------------------------------------------
// Phase 4 (#9): 平衡・めまい視覚フィルタ — vertigo / bppv_rotation / vestibular_neuritis
// ---------------------------------------------------------------

/// 双線形補間でソース画像の (fx, fy) 位置のピクセル値を取得する（edge clamp）。
pub(crate) fn sample_bilinear(rgba: &image::RgbaImage, fx: f32, fy: f32) -> image::Rgba<u8> {
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
