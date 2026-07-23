//! 知覚低下・歪み・閃輝・光視症フィルタ。
//!
//! コントラスト感度低下・detail loss・変視症・閃輝暗点・光視症。
//! `star_hash32` は `flickering_stars` 専用ハッシュのため本モジュールに置く。

use super::*;
use crate::Result;
use image::DynamicImage;
use std::f32::consts::PI;

// ---------------------------------------------------------------
// Phase N (Issue #55): Metamorphopsia — 歪視フィルタ
// ---------------------------------------------------------------

/// 歪視（Metamorphopsia）シミュレーション。
///
/// 黄斑疾患（黄斑円孔・黄斑上膜・加齢黄斑変性など）で生じる格子状の歪み（Amsler grid
/// 歪曲）を模擬する。グリッド頂点ごとの決定論的ノイズで各ピクセルを変位座標から
/// サンプリングする。
///
/// ## アルゴリズム
///
/// 画像を `1/freq` ピクセル単位の仮想グリッドに分割し、各グリッド頂点に
/// 32bit 整数 spatial hash で擬似ランダムな変位ベクトル `(dx, dy)` を割り当てる。
/// ノイズは seed と頂点座標だけの決定論的関数で、`metamorphopsia.frag` の `gridHash`
/// と bit 単位に一致する（#99 で CPU/GLSL のノイズモデルを統一）。
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
pub fn metamorphopsia(
    img: DynamicImage,
    strength: f32,
    freq: f32,
    seed: u64,
) -> Result<DynamicImage> {
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

    // グリッド頂点ごとの変位を 32bit 整数ハッシュで生成する。
    //
    // #99: GLSL ES 3.00 の `uint`（32bit）で bit 単位に再現できるよう、
    // 旧 64bit Knuth LCG（`(state>>32)` の高位ビット抽出を含む）から、
    // 純粋な 32bit 整数 spatial hash に変更した。各頂点の値は seed と頂点座標
    // (gx, gy) だけの決定論的関数で、逐次状態を持たない（並列・GPU 再現可能）。
    // `metamorphopsia.frag` の `gridHash` と完全に同一の系列を出す（u32 演算は
    // GLSL の `uint` 同様 mod 2^32 で wrap する）。
    //
    // 唯一の乖離源はサンプリング段の f32 丸めのみで、変位場は CPU↔GLSL で一致する。
    let seed32 = seed as u32;
    // 1 頂点・1 軸ぶんの 32bit ハッシュ（axis: 0=dx, 1=dy）。0.0..=1.0 を返す。
    let grid_hash = |gx: u32, gy: u32, axis: u32| -> f32 {
        // 各入力を 32bit 黄金比定数で混合してから XOR-shift で雪崩効果を付ける。
        let mut h = seed32
            .wrapping_mul(0x9e3779b9)
            .wrapping_add(gx.wrapping_mul(0x85ebca6b))
            .wrapping_add(gy.wrapping_mul(0xc2b2ae35))
            .wrapping_add(axis.wrapping_mul(0x27d4eb2f));
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a2d39);
        h ^= h >> 15;
        h as f32 / u32::MAX as f32 // 0.0..=1.0
    };

    let grid_disp: Vec<(f32, f32)> = (0..grid_h)
        .flat_map(|gy| (0..grid_w).map(move |gx| (gx, gy)))
        .map(|(gx, gy)| {
            let dx_norm = grid_hash(gx as u32, gy as u32, 0);
            let dy_norm = grid_hash(gx as u32, gy as u32, 1);
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
            let src_x = (x as f32 + disp_x).clamp(0.0, (width - 1) as f32);
            let src_y = (y as f32 + disp_y).clamp(0.0, (height - 1) as f32);

            let px = sample_bilinear(&rgba, src_x, src_y);
            out.put_pixel(x, y, px);
        }
    }

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
///
/// 注意: midpoint は linear sRGB 空間で 0.5 を使用しています。
/// 知覚的な中間輝度（sRGB 0.5 = linear ≈ 0.214）とは異なります。
/// 視覚的な中間点ではなく数学的な中間点を基準とした簡易近似です。
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
/// 矩形タイルごとにタイル中心点の色に置き換える（pixelation）。
/// タイルサイズ = `(strength * 20.0).max(1.0) as u32` px。
///
/// - `strength = 0.0`: identity（タイルサイズ 1px = 変化なし）
/// - `strength = 1.0`: 20px タイル
///
/// ## アルゴリズムの注意
/// このバリアントは**タイル中心点参照**（GLSL シェーダと同一アルゴリズム）。
/// `apply(Filter::DetailLoss)` 経由時は `detail_loss_with_cell_size` を呼ぶが、
/// kako-jun/sensus#96 で同関数も中心点サンプリングに統一済みなので
/// アルゴリズムは同一（異なるのはタイルサイズの決め方だけ → [`detail_loss_with_cell_size`] 参照）。
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
            if x1 <= x0 || y1 <= y0 {
                continue;
            }

            // タイル中心1点をサンプリング（GLSL と同一アルゴリズム: pixelation）
            let cx = (x0 + tile_size / 2).min(width - 1);
            let cy = (y0 + tile_size / 2).min(height - 1);
            let cp = rgba.get_pixel(cx, cy);
            let avg_r = cp[0];
            let avg_g = cp[1];
            let avg_b = cp[2];

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

/// ディテールロス（ピクセル化）シミュレーション（cell_size 直接指定版）。
///
/// `cell_size`: タイルサイズ (px)。1 以下の場合は identity を返す。
/// `strength` (0.0..=1.0) は「タイル中心点の色（pixelation 結果）」と「元の画素値」を
/// **linear sRGB 空間**で線形補間する比率として使う（他フィルタと同じ blend 流儀。
/// 色覚系 `apply_machado_matrix` を参照）。`strength = 0.0` は identity（早期 return で
/// byte 恒等）、`strength = 1.0` は完全にタイル中心色（従来どおりの pixelation 効果）。
/// NaN は `normalize_strength` により 0.0（identity）として扱う。
///
/// `cell_size` が 1 の場合は各ピクセルが単独のセルになるため identity と等価です（早期リターン）。
///
/// ## アルゴリズムの注意
/// タイル中心点サンプリング（pixelation）を使用する点は [`detail_loss`] および GLSL
/// シェーダ（`detail_loss.frag`）と同一。異なるのはタイルサイズの決め方（こちらは
/// `cell_size` 直接指定、[`detail_loss`] は `strength` から導出）。`apply(Filter::DetailLoss)`
/// 経由時はこのバリアントが呼ばれる。
///
/// kako-jun/sensus#96: 以前はタイル内全ピクセルの linear sRGB 平均を使用しており、
/// 公開 API（apply 経由）が GLSL シェーダとも等価テスト済み関数とも異なる出力を出していた。
/// シェーダ（universal-experience の表示経路 = 正本）の中心点サンプリングに統一した。
///
/// kako-jun/sensus#167: `strength` を無視してタイル化していたため `strength=0` でも
/// pixelation がかかる、クレート内で唯一 `strength=0=元画像` に違反する関数だった。
/// linear sRGB 空間での blend を配線し、他フィルタと同じ契約に揃えた。
pub fn detail_loss_with_cell_size(
    img: DynamicImage,
    strength: f32,
    cell_size: u32,
) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if s == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }
    let tile_size = cell_size.max(1);
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
            if x1 <= x0 || y1 <= y0 {
                continue;
            }

            // タイル中心1点をサンプリング（GLSL シェーダと同一アルゴリズム: pixelation）
            let cx = (x0 + tile_size / 2).min(width - 1);
            let cy = (y0 + tile_size / 2).min(height - 1);
            let cp = rgba.get_pixel(cx, cy);
            let sim_r = srgb_to_linear(cp[0] as f32 / 255.0);
            let sim_g = srgb_to_linear(cp[1] as f32 / 255.0);
            let sim_b = srgb_to_linear(cp[2] as f32 / 255.0);

            for py in y0..y1 {
                for px in x0..x1 {
                    let p = out.get_pixel_mut(px, py);
                    let r = srgb_to_linear(p[0] as f32 / 255.0);
                    let g = srgb_to_linear(p[1] as f32 / 255.0);
                    let b = srgb_to_linear(p[2] as f32 / 255.0);
                    // strength で linear blend（0.0 = 原色, 1.0 = 完全 pixelation）
                    let nr = lerp(r, sim_r, s);
                    let ng = lerp(g, sim_g, s);
                    let nb = lerp(b, sim_b, s);
                    p[0] = pack_u8(linear_to_srgb(nr));
                    p[1] = pack_u8(linear_to_srgb(ng));
                    p[2] = pack_u8(linear_to_srgb(nb));
                    // alpha はそのまま
                }
            }
        }
    }

    Ok(DynamicImage::ImageRgba8(out))
}

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
            } else if (0.2..=0.5).contains(&dist) {
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

// ---------------------------------------------------------------
// Issue #59: Flickering Stars フィルタ
// ---------------------------------------------------------------

/// 点群（flickering_stars / floaters）用の 32bit spatial hash（#134）。
///
/// draw index `k` と `seed` から決定論的に 0..=u32::MAX を返す。#99/#125 の
/// cataract grid_hash と同じ黄金比定数混合 + XOR-shift finalizer 系列で、GLSL の
/// `uint` 演算（mod 2^32 wrap）と bit 一致する。64bit LCG と違い 32bit GPU で再現可能。
pub(crate) fn star_hash32(seed: u32, k: u32) -> u32 {
    let mut h = seed
        .wrapping_mul(0x9e3779b9)
        .wrapping_add(k.wrapping_mul(0x85ebca6b));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a2d39);
    h ^= h >> 15;
    h
}

/// 閃光光点（Flickering Stars）シミュレーション。
///
/// LCG でランダムな光点を生成して additive blend する。
/// 各光点は半径 2 px の矩形ブロブ（簡易実装）。
///
/// > **医学的注記**: 急激な光点の増加・カーテン状の視野欠損を伴う場合は
/// > 網膜剥離の前兆。即受診。
///
/// - `strength`: 0.0 = 光点ゼロ（identity）, 1.0 = 200 点
/// - `seed`: LCG の初期シード（フレーム間の一貫性に使用）
pub fn flickering_stars(img: DynamicImage, strength: f32, seed: u64) -> Result<DynamicImage> {
    let s = normalize_strength(strength);
    let rgba = img.to_rgba8();
    let count = (s * 200.0) as usize;
    if count == 0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;

    // #134: 32bit spatial hash で点ごとの位置・輝度を生成する。
    // 旧 64bit LCG（`(state>>32)` 抽出）は GLSL の 32bit uint で再現できず CPU↔GLSL が
    // 乖離していた。点 i は draw index 3i, 3i+1, 3i+2 を引く（順序固定）。
    // `flickering_stars.frag` / sim_flickering_stars_glsl と bit 一致する。
    let seed32 = seed as u32;
    let hash01 = |k: u32| -> f32 { star_hash32(seed32, k) as f32 / u32::MAX as f32 };

    // linear sRGB に変換して作業
    let (mut linear, alpha) = rgba_to_linear_planes(&rgba);

    const BLOB_RADIUS: i32 = 2;

    for i in 0..count {
        let i = i as u32;
        let fx = hash01(3 * i);
        let fy = hash01(3 * i + 1);
        // 輝度 0.5..1.0（白っぽい光点）
        let fb = hash01(3 * i + 2);

        let cx = (fx * w_f) as i32;
        let cy = (fy * h_f) as i32;
        let brightness = 0.5 + fb * 0.5;

        for dy in -BLOB_RADIUS..=BLOB_RADIUS {
            for dx in -BLOB_RADIUS..=BLOB_RADIUS {
                let px = cx + dx;
                let py = cy + dy;
                if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                    continue;
                }
                let idx = py as usize * width as usize + px as usize;
                let p = &mut linear[idx];
                p[0] = (p[0] + brightness).min(1.0);
                p[1] = (p[1] + brightness).min(1.0);
                p[2] = (p[2] + brightness).min(1.0);
            }
        }
    }

    let out = linear_planes_to_rgba(&linear, &alpha, width, height);
    Ok(DynamicImage::ImageRgba8(out))
}
