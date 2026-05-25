//! MPO ステレオ写真の分割と深度マップ生成。
//!
//! # 関数
//! - [`split_mpo`]: MPO バイト列を左目・右目 JPEG に分割する
//! - [`stereo_to_depth`]: 左右画像から SAD ブロックマッチングで深度マップを生成する

use image::{DynamicImage, GrayImage};

use crate::{Error, Result};

/// SAD ブロックマッチングのブロックサイズ（片側 = HALF = 3px）。
const BLOCK_SIZE: u32 = 7;
/// 最大視差（右方向へのオフセット探索ピクセル数）。
const MAX_DISPARITY: u32 = 64;

/// MPO バイト列を左目・右目 [`DynamicImage`] のペアに分割する。
///
/// MPO は JPEG のスーパーセット。先頭 JPEG が左目、`FFD9 FFD8` パターン
/// 直後のバイト列が右目 JPEG となる。右目が見つからない場合は
/// [`Error::InvalidMpo`] を返す。
pub fn split_mpo(data: &[u8]) -> Result<(DynamicImage, DynamicImage)> {
    // FFD9 FFD8 パターンを探す（FFD9 = EOI, FFD8 = SOI）
    let split_pos = data
        .windows(4)
        .position(|w| w[0] == 0xFF && w[1] == 0xD9 && w[2] == 0xFF && w[3] == 0xD8)
        .ok_or(Error::InvalidMpo)?;

    // 左目: 先頭〜EOI（FFD9 含む）
    let left_bytes = &data[..split_pos + 2];
    // 右目: FFD8 から末尾まで
    let right_bytes = &data[split_pos + 2..];

    let left = image::load_from_memory(left_bytes)?;
    let right = image::load_from_memory(right_bytes)?;

    Ok((left, right))
}

/// 左右ステレオ画像から SAD ブロックマッチングで深度マップを生成する。
///
/// - 左右画像サイズが異なる場合は [`Error::SizeMismatch`] を返す。
/// - アルゴリズム: 各画素について左画像のブロックと右画像の水平スキャン窓を
///   比較し、最小 SAD のオフセット（視差値）を深度として正規化する。
/// - 視差が大きい（近い被写体）ほど明るい (255)。
/// - 出力は [`DynamicImage::ImageLuma8`] グレースケール画像。
pub fn stereo_to_depth(left: &DynamicImage, right: &DynamicImage) -> Result<DynamicImage> {
    let (w, h) = (left.width(), left.height());
    if right.width() != w || right.height() != h {
        return Err(Error::SizeMismatch {
            left_w: w,
            left_h: h,
            right_w: right.width(),
            right_h: right.height(),
        });
    }

    let left_gray = left.to_luma8();
    let right_gray = right.to_luma8();

    let half = BLOCK_SIZE / 2;
    let mut depth = GrayImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let mut best_sad = u32::MAX;
            let mut best_disp: u32 = 0;

            for d in 0..=MAX_DISPARITY {
                // 右画像の対応 x 座標（左方向に視差分オフセット）
                if x < d {
                    break;
                }
                let rx = x - d;

                let mut sad: u32 = 0;
                for dy in 0..BLOCK_SIZE {
                    // ブロック内の絶対行座標（クランプ）
                    let row = (y + dy).saturating_sub(half).min(h - 1);

                    for dx in 0..BLOCK_SIZE {
                        // 左画像のブロック内列座標（クランプ）
                        let lcol = (x + dx).saturating_sub(half).min(w - 1);
                        // 右画像の対応列座標（同じオフセット、クランプ）
                        let rcol = (rx + dx).saturating_sub(half).min(w - 1);

                        let lv = left_gray.get_pixel(lcol, row)[0] as i32;
                        let rv = right_gray.get_pixel(rcol, row)[0] as i32;
                        sad += (lv - rv).unsigned_abs();
                    }
                }

                if sad < best_sad {
                    best_sad = sad;
                    best_disp = d;
                }
            }

            // 視差を 0..=255 に正規化（大きい視差 = 近い = 明るい）
            let pixel_val = ((best_disp as f32 / MAX_DISPARITY as f32) * 255.0) as u8;
            depth.put_pixel(x, y, image::Luma([pixel_val]));
        }
    }

    Ok(DynamicImage::ImageLuma8(depth))
}
