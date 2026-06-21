//! 前庭・動きフィルタ。
//!
//! めまい・BPPV・前庭神経炎・複視・眼振・星芒。`VERTIGO_STILL_TIME_S` /
//! `BPPV_STILL_TIME_S` 定数と `hsl_rainbow_to_linear`（starbursts 専用）も
//! この領域に置く。

use super::*;
use crate::Result;
use image::{DynamicImage, RgbaImage};
use std::f32::consts::PI;

/// 静止画で [`vertigo`] を 1 フレーム描くときの代表時刻（秒）。
///
/// `vertigo` の回転角は 0.3 Hz の sin 波で、`time_t = 1/(4·0.3) ≈ 0.833 s` で sin = 1
/// となり回転が最大になる。静止画は時間軸を持てないため、効果が最も伝わるこのピーク位相を
/// 既定とする（`apply(Filter::Vertigo)` 経由）。アニメーションは GLSL 側の time uniform が担当する。
pub const VERTIGO_STILL_TIME_S: f32 = 0.8333333;

/// 静止画で [`bppv_rotation`] を 1 フレーム描くときの代表時刻（秒）。
///
/// `bppv_rotation` は周期 2 s の sawtooth で、急速相の終わり（`phase = 0.3`、すなわち
/// `time_t = 0.6 s`）で回転角が最大になる。`time_t = 0` では回転角が 0 になり恒等変換に
/// なってしまうため、静止画ではこのピーク位相を既定とする（`apply(Filter::BppvRotation)` 経由）。
pub const BPPV_STILL_TIME_S: f32 = 0.6;

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
///
/// ## CPU/GLSL シフト定義の対応関係
/// - CPU: `shift_x = strength * 0.05 * width` ピクセル（本関数内の実装）
/// - GLSL: `shift_texel = strength * 0.05`（テクセル単位）= `shift_x / width` と等価
/// - blur 方式: CPU は `ellipse_blur`（1D box フィルタ）、GLSL は固定 16-tap 水平ブラー。
///   両者は手法が異なるが PSNR ≥ 30 dB で等価と確認済み。
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
    let blur_b = MIN_BLUR_RADIUS_PX; // 短軸（ほぼ 0 の 1D ブラー）
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
    let blurred = ellipse_blur(
        &linear,
        width,
        height,
        radius_px,
        MIN_BLUR_RADIUS_PX,
        blur_axis_rad,
    );
    let out = linear_planes_to_rgba(&blurred, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}

/// HSL (hue 0..360, s=1, l=0.5) → linear sRGB の変換（分散レイ色に使用）。
/// 純粋な彩度 1 の虹色を返す内部ヘルパー。
#[inline]
fn hsl_rainbow_to_linear(hue_deg: f32) -> [f32; 3] {
    // H ∈ [0, 360), S = 1, L = 0.5 の特殊ケースを展開する。
    // C = (1 - |2L - 1|) × S = 1, X = C × (1 - |H/60 mod 2 - 1|), m = L - C/2 = 0
    let h = hue_deg.rem_euclid(360.0);
    let sector = (h / 60.0) as u32;
    let f = h / 60.0 - sector as f32;
    let (r, g, b) = match sector {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    };
    // sRGB → linear（HSL で L=0.5 なら既に sRGB と見なして gamma 解除する）
    [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)]
}

/// 光芒（Starbursts）シミュレーション。
///
/// LASIK / 白内障手術後や高度乱視でレンズ面の回折から生じる放射状の光芒を再現する。
///
/// - `strength`: 0.0 = 元画像, 1.0 = 強い光芒
/// - `num_rays`: 光芒の本数（0 で無効化）
/// - `ray_length_ratio`: 光芒長（min(W,H) 比, 0.0..=1.0）
/// - `threshold`: 光芒を発生させる輝度閾値（0.0..=1.0, BT.709 luma）
/// - `dispersion`: 波長分散による虹色光芒（0.0 = 白, 1.0 = 完全虹色）
pub fn starbursts(
    img: DynamicImage,
    strength: f32,
    num_rays: u32,
    ray_length_ratio: f32,
    threshold: f32,
    dispersion: f32,
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
    let dispersion = dispersion.clamp(0.0, 1.0);

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

                // 分散色: 各 ray の角度を色相に対応させる（虹色）
                // dispersion=0 → 白 (1,1,1), dispersion=1 → HSL 虹色
                let angle_deg = theta.to_degrees().rem_euclid(360.0);
                let rainbow = hsl_rainbow_to_linear(angle_deg);
                let ray_r = lerp(1.0, rainbow[0], dispersion);
                let ray_g = lerp(1.0, rainbow[1], dispersion);
                let ray_b = lerp(1.0, rainbow[2], dispersion);

                for t in 1..=ray_length_px {
                    let sx = x as i32 + (t as f32 * cos_t).round() as i32;
                    let sy = y as i32 + (t as f32 * sin_t).round() as i32;
                    if sx < 0 || sx >= width as i32 || sy < 0 || sy >= height as i32 {
                        continue;
                    }
                    let weight = src_intensity * (1.0 - t as f32 / ray_length_px as f32) * s;
                    let idx = sy as usize * width as usize + sx as usize;
                    ray_layer[idx][0] += weight * ray_r;
                    ray_layer[idx][1] += weight * ray_g;
                    ray_layer[idx][2] += weight * ray_b;
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
