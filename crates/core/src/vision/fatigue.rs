//! 眼精疲労フィルタ。
//!
//! eye_strain（眼精疲労）と dry_eye（ドライアイ）。

use super::*;
use crate::Result;
use image::DynamicImage;

// ---------------------------------------------------------------
// Phase 4 (#36): eye fatigue — eye_strain / dry_eye
// ---------------------------------------------------------------

/// 眼精疲労（eye strain）シミュレーション。
///
/// 処理順序（linear sRGB 空間）:
/// 1. コントラスト圧縮: `v' = 0.5 + (v - 0.5) * (1.0 - strength * 0.15)`
/// 2. 周辺 vignette（軽め）: `1.0 - strength * 0.3 * smoothstep(0.3, 1.2, d)`
/// 3. 微小 disk（pillbox）blur（radius = strength * 1.5 px、厳密 pillbox）
///
/// `strength = 0.0` は元画像と完全一致。
///
/// GLSL 版 `eye_strain.frag` は同一順序・同一式だが、単一パス制約のため手順 3 の
/// 厳密 pillbox を Fibonacci lattice 16 tap で近似する（CPU が正本）。乖離は PSNR で担保。
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
            let ux = (x - cx) / cx; // -1.0..=1.0
            let uy = (y - cy) / cy;
            let d = ux * ux + uy * uy; // 0.0（中心）〜 2.0+（角）

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

/// ドライアイ（dry eye）シミュレーション。
///
/// 32bit 整数 spatial hash（seed=42 固定）で生成したタイルごとのノイズを基に、
/// 32×32 ピクセルタイルごとに異なる等方 disk blur radius（`noise * strength * 3px`）を
/// linear sRGB 空間で適用する。
///
/// `strength = 0.0` は元画像と完全一致。
///
/// シード値は内部で固定（42）のため、同一入力に対して毎回同一のノイズパターンになります。
/// この固定 seed は `dry_eye.frag` の `tileHash` と一致させる CPU↔GLSL 等価の前提でもある。
/// フレームごとに異なるパターン（動画用）を出すには CPU と `.frag` の双方に seed uniform を
/// 通す必要があり、現状はスコープ外（必要になれば別途 seed 対応版を設計する）。
///
/// #99: タイルノイズを行優先の逐次 64bit LCG から、タイル座標だけの決定論的 32bit
/// spatial hash に変更した。これにより `dry_eye.frag` がフラグメント単位で同じノイズ場・
/// 同じ disk blur を単一パスで再現でき、CPU↔GLSL が等価になった（PSNR で担保）。
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
    // ドライアイの seed（固定 42）。タイルごとのノイズ生成に使う。
    const DRY_EYE_SEED: u32 = 42;

    // タイルごとの 32bit 整数 spatial hash（0.0..=1.0）。
    //
    // #99: 旧実装はタイルを行優先で走査しながら 64bit LCG 状態を逐次更新していた
    // （各タイルのノイズが先行タイル数 = タイルグリッド寸法に依存していた）。これは
    // フラグメントシェーダの並列実行では再現できない。各タイルのノイズを seed と
    // タイル座標 (tx, ty) だけの決定論的関数に変更し、`dry_eye.frag` の `tileHash` と
    // 完全に同一の系列を出す（u32 演算は GLSL の uint 同様 mod 2^32 で wrap）。
    let tile_hash = |tx: u32, ty: u32| -> f32 {
        let mut h = DRY_EYE_SEED
            .wrapping_mul(0x9e3779b9)
            .wrapping_add(tx.wrapping_mul(0x85ebca6b))
            .wrapping_add(ty.wrapping_mul(0xc2b2ae35));
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a2d39);
        h ^= h >> 15;
        h as f32 / u32::MAX as f32 // 0.0..=1.0
    };

    // タイル数を計算
    let tile_cols = width.div_ceil(TILE_SIZE);
    let tile_rows = height.div_ceil(TILE_SIZE);

    // 出力バッファを元画像で初期化
    let mut out_linear = linear.clone();

    // タイルごとに disk blur を適用して出力バッファに書き込む
    for ty in 0..tile_rows {
        for tx in 0..tile_cols {
            // 0.0..=1.0 のノイズ値（タイル座標の spatial hash）
            let noise = tile_hash(tx, ty);
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
            let sub_blurred =
                ellipse_blur(&sub_linear, sub_w, sub_h, blur_radius, blur_radius, 0.0);

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
