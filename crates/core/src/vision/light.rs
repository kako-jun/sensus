//! 光・透明度フィルタ。
//!
//! 白内障・羞明・夜盲・飛蚊症。`floaters_mask` は `floaters` 専用ヘルパーだが
//! 公開 API のため本モジュールに残す。

use super::*;
use image::DynamicImage;

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
    const CELL_SIZE: f32 = 32.0;
    // VIP-Sim 二段モデルの**再解釈**によるコントラスト/輝度低下（#106）。
    // 原典 VIP-Sim（一次出典 `myBrightnessContrastGamma.shader`:
    // `color *= _BCG.x; color = (color - coeff) * _BCG.y + coeff`）は
    // brightness を `×(1-severity)` の乗算（severity=1 で全消灯）とし、
    // コントラスト pivot は ContrastCoeff そのもの (0.7, 0.7, 0.4) を使う
    // （B 係数 0.4 が最小＝白内障で青の散乱が最大であることに対応する）。
    // sensus はこれと異なり pivot を 0.5 に固定し、輝度低下も
    // `-0.1*severity` の減算オフセットとする——原典よりずっとマイルドな
    // 実装であり、原典の忠実移植ではない（挙動は本修正で変更しない。
    // 挙動変更の提案は #174 で別管理）。
    const CATARACT_PIVOT: f32 = 0.5;
    const CATARACT_BRIGHTNESS_DROP: f32 = 0.1;

    // 格子頂点ごとの白濁ノイズ値を 32bit 整数 spatial hash で生成する。
    //
    // #125: GPU 単一パスで bit 単位に再現できるよう、旧 64bit Knuth LCG
    // （`(lcg >> 32)` の高位ビット抽出を含む）を #99 と同じ純粋 32bit 整数
    // spatial hash（黄金比定数混合 + XOR-shift finalizer）に置き換えた。各頂点の
    // 値は seed と頂点座標 (gx, gy) だけの決定論的関数で、逐次状態を持たない。
    // `cataract.frag` の `gridHash`（= metamorphopsia/dry_eye と同一の系列）と
    // 32bit 整数ハッシュ系列が bit 一致する（u32 演算は GLSL の uint 同様
    // mod 2^32 で wrap する）。ただし最終画素値は sRGB↔linear の `pow()` が
    // CPU/GPU で last-ULP 一致を保証されないため、完全な bit 一致は主張しない
    // （CPU↔GLSL 等価テストは PSNR 閾値で安全側に判定する。#170）。
    let seed32 = seed as u32;
    // 1 頂点ぶんの 32bit ハッシュ（0.0..=1.0 を返す）。
    let grid_hash = |gx: u32, gy: u32| -> f32 {
        let mut h = seed32
            .wrapping_mul(0x9e3779b9)
            .wrapping_add(gx.wrapping_mul(0x85ebca6b))
            .wrapping_add(gy.wrapping_mul(0xc2b2ae35));
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a2d39);
        h ^= h >> 15;
        h as f32 / u32::MAX as f32 // 0.0..=1.0
    };

    // 格子頂点の値を smoothstep bilinear 補間する内部関数。
    //
    // 格子座標規約は CPU/GLSL で統一: 整数ピクセル index `px / CELL_SIZE` の
    // floor を頂点インデックスとする（top-left 規約、metamorphopsia と同一）。
    // `cataract.frag` は `vTexCoord * uResolution - 0.5` で整数ピクセル座標を
    // 復元してこの規約に合わせている（旧 .frag の `(x+0.5)/CELL` 0.5px
    // オフセットを廃止）。頂点ハッシュは座標だけの関数なので境界配列は不要。
    let grid_sample = |px: u32, py: u32| -> f32 {
        let fx = px as f32 / CELL_SIZE;
        let fy = py as f32 / CELL_SIZE;
        let gx0 = fx.floor();
        let gy0 = fy.floor();
        let tx = fx - gx0; // セル内 x 位置（0.0..=1.0）
        let ty = fy - gy0; // セル内 y 位置（0.0..=1.0）
        let gx0 = gx0 as u32;
        let gy0 = gy0 as u32;
        let gx1 = gx0 + 1;
        let gy1 = gy0 + 1;

        // 4 頂点の値を取得
        let v00 = grid_hash(gx0, gy0);
        let v10 = grid_hash(gx1, gy0);
        let v01 = grid_hash(gx0, gy1);
        let v11 = grid_hash(gx1, gy1);

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

            // VIP-Sim 二段モデルの**再解釈**（#106）: severity 比例の輝度・コントラスト低下。
            // 白内障の霞み感の核。c_ch = 1 - s*(1 - coeff_ch) で strength=0 のとき恒等。
            // CPU/GLSL/sim で同一演算。原典との差分・出典・pivot/brightness の詳細は
            // 上記 CATARACT_PIVOT / CATARACT_BRIGHTNESS_DROP 定義のコメント参照
            // （挙動変更の提案は #174 で別管理）。
            let nr = ((nr - CATARACT_PIVOT) * (1.0 - strength * (1.0 - 0.7)) + CATARACT_PIVOT
                - strength * CATARACT_BRIGHTNESS_DROP)
                .clamp(0.0, 1.0);
            let ng = ((ng - CATARACT_PIVOT) * (1.0 - strength * (1.0 - 0.7)) + CATARACT_PIVOT
                - strength * CATARACT_BRIGHTNESS_DROP)
                .clamp(0.0, 1.0);
            let nb = ((nb - CATARACT_PIVOT) * (1.0 - strength * (1.0 - 0.4)) + CATARACT_PIVOT
                - strength * CATARACT_BRIGHTNESS_DROP)
                .clamp(0.0, 1.0);

            // Simplex-like ノイズによる白濁（空間相関あり）
            let noise = grid_sample(x, y);
            let white_blend = strength * noise * WHITE_BLEND_MAX;

            // nr∈[0,1]・white_blend∈[0,WHITE_BLEND_MAX(0.4)] より fr∈[0,1] が常に成り立つので
            // ここでは clamp しない（pack_u8 が sRGB 空間で最終 clamp する）。GLSL/sim は linear 空間で
            // clamp(0,1) するが入力域上は等価で bit 一致する。WHITE_BLEND_MAX を上げる/黄変係数を負に
            // 振る等で fr が [0,1] を外れる変更を入れる場合は、CPU/GLSL の clamp 段を揃え直すこと。
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

/// 飛蚊症（Floaters）のマスクを生成する（#134, 方針 B）。
///
/// 円形 blob 30% + 糸くず形状（ランダムウォーク折れ線） 70% を描画し、box blur
/// (radius 1px) でエッジをソフト化した単チャンネルマスクを返す。各画素は
/// `0`（完全フローター = 不透明）..`255`（透明）で、最終的な乗算ブレンドは
/// strength に応じて呼び出し側（[`floaters`] / GLSL `floaters.frag`）が行う。
///
/// **strength 非依存**: マスクは density / seed / gaze だけで決まるので、consumer
/// （universal-experience GLSL）は本マスクを `uMask` テクスチャとして 1 回アップロードし、
/// strength は uniform で可変にできる。CPU [`floaters`] も同じ u8 マスクからブレンドするため
/// CPU↔GLSL が bit 一致する。
///
/// - `density`: blob 密度 (0.0..=1.0)
/// - `seed`: blob 配置のランダムシード
/// - `gaze_x`: 視線 X 位置 (0.0 = 左, 1.0 = 右)
/// - `gaze_y`: 視線 Y 位置 (0.0 = 上, 1.0 = 下)
pub fn floaters_mask(
    width: u32,
    height: u32,
    density: f32,
    seed: u64,
    gaze_x: f32,
    gaze_y: f32,
    size: f32,
) -> image::GrayImage {
    let w_f = width as f32;
    let h_f = height as f32;

    let density = density.clamp(0.0, 1.0);
    let gaze_x = gaze_x.clamp(0.0, 1.0);
    let gaze_y = gaze_y.clamp(0.0, 1.0);
    // size は blob 半径・糸くず幅の相対倍率（#110）。1.0 = 既定。0 や NaN は 1.0 にフォールバック。
    let size = if size.is_finite() && size > 0.0 {
        size.clamp(0.1, 5.0)
    } else {
        1.0
    };

    // 視線オフセット（フローターは視線に追随）
    let offset_x = (gaze_x - 0.5) * 0.3 * w_f;
    let offset_y = (gaze_y - 0.5) * 0.3 * h_f;

    // blob/糸くず 総数
    let total_count = (density * 200.0) as usize;
    if total_count == 0 {
        // フローターなし = 全面透明（255）
        return image::GrayImage::from_pixel(width, height, image::Luma([255]));
    }

    let blob_count = (total_count as f32 * 0.3).ceil() as usize; // 30% 円形
    let strand_count = total_count - blob_count; // 70% 糸くず

    let blob_radius = (w_f.min(h_f) * 0.04 * size).max(2.0);
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
        let half_w = (f_width * 3.0 + 1.0) * 0.5 * size; // 0.5..=2.0（size 倍率、#110）

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

    // ── f32 マスクを u8 に量子化して返す ─────────────────────────
    // GLSL は本マスクを uMask テクスチャ（.r）として受け取り、CPU と同じ u8 値から
    // ブレンドするため bit 一致する（#134, 方針 B）。
    let mut mask_img = image::GrayImage::new(width, height);
    for py in 0..h {
        for px in 0..w {
            let m = blurred_mask[py * w + px].clamp(0.0, 1.0);
            mask_img.put_pixel(
                px as u32,
                py as u32,
                image::Luma([(m * 255.0).round() as u8]),
            );
        }
    }
    mask_img
}

/// 飛蚊症（Floaters）シミュレーション。
///
/// [`floaters_mask`] で生成した u8 マスクを linear sRGB 空間で乗算ブレンドする。
/// マスクは strength 非依存で、GLSL（`floaters.frag`）も同じ u8 マスクを `uMask`
/// テクスチャで受け取り同一ブレンドを行うため CPU↔GLSL が bit 一致する（#134, 方針 B）。
///
/// - `strength`: 0.0 = 元画像, 1.0 = 強い飛蚊症
/// - `density`: blob 密度 (0.0..=1.0)
/// - `seed`: blob 配置のランダムシード（実際に使用される）
/// - `gaze_x`: 視線 X 位置 (0.0 = 左, 1.0 = 右)
/// - `gaze_y`: 視線 Y 位置 (0.0 = 上, 1.0 = 下)
/// - `size`: blob 半径・糸くず幅の相対倍率（#110、1.0 = 既定、0.1..=5.0 に clamp）
pub fn floaters(
    img: DynamicImage,
    strength: f32,
    density: f32,
    seed: u64,
    gaze_x: f32,
    gaze_y: f32,
    size: f32,
) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let mut rgba = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let mask = floaters_mask(width, height, density, seed, gaze_x, gaze_y, size);

    for y in 0..height {
        for x in 0..width {
            let m = mask.get_pixel(x, y)[0] as f32 / 255.0;
            let blend = 1.0 - strength * (1.0 - m);

            let px = rgba.get_pixel_mut(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);
            px[0] = pack_u8(linear_to_srgb(rl * blend));
            px[1] = pack_u8(linear_to_srgb(gl * blend));
            px[2] = pack_u8(linear_to_srgb(bl * blend));
            // alpha はそのまま
        }
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}
