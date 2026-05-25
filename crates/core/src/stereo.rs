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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbImage};

    // ---------------------------------------------------------------
    // ヘルパー
    // ---------------------------------------------------------------

    fn make_rgb_image(w: u32, h: u32, r: u8, g: u8, b: u8) -> RgbImage {
        RgbImage::from_pixel(w, h, image::Rgb([r, g, b]))
    }

    fn make_jpeg_bytes(img: &RgbImage) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 95);
        enc.encode_image(img).unwrap();
        buf
    }

    fn make_synthetic_mpo(left: &RgbImage, right: &RgbImage) -> Vec<u8> {
        let mut mpo = make_jpeg_bytes(left);
        mpo.extend_from_slice(&make_jpeg_bytes(right));
        mpo
    }

    // ---------------------------------------------------------------
    // split_mpo 正常系
    // ---------------------------------------------------------------

    #[test]
    fn split_mpo_valid_mpo_returns_two_images() {
        let left = make_rgb_image(8, 8, 200, 100, 50);
        let right = make_rgb_image(8, 8, 50, 100, 200);
        let mpo = make_synthetic_mpo(&left, &right);

        let result = split_mpo(&mpo);
        assert!(result.is_ok(), "valid MPO should return Ok, got: {:?}", result.err());
        let (l, r) = result.unwrap();
        assert_eq!(l.width(), 8);
        assert_eq!(l.height(), 8);
        assert_eq!(r.width(), 8);
        assert_eq!(r.height(), 8);
    }

    #[test]
    fn split_mpo_left_image_is_first_jpeg() {
        let left = make_rgb_image(16, 8, 200, 100, 50);
        let right = make_rgb_image(8, 8, 50, 100, 200);
        let mpo = make_synthetic_mpo(&left, &right);

        let (l, _r) = split_mpo(&mpo).unwrap();
        assert_eq!((l.width(), l.height()), (16, 8), "left image dimensions should match first JPEG");
    }

    #[test]
    fn split_mpo_right_image_is_second_jpeg() {
        let left = make_rgb_image(8, 8, 200, 100, 50);
        let right = make_rgb_image(12, 6, 50, 100, 200);
        let mpo = make_synthetic_mpo(&left, &right);

        let (_l, r) = split_mpo(&mpo).unwrap();
        assert_eq!((r.width(), r.height()), (12, 6), "right image dimensions should match second JPEG");
    }

    // ---------------------------------------------------------------
    // split_mpo 異常系
    // ---------------------------------------------------------------

    #[test]
    fn split_mpo_empty_bytes_returns_invalid_mpo() {
        let result = split_mpo(&[]);
        assert!(
            matches!(result, Err(Error::InvalidMpo)),
            "empty bytes should return InvalidMpo, got: {:?}", result
        );
    }

    #[test]
    fn split_mpo_single_jpeg_no_second_soi_returns_invalid_mpo() {
        // 1枚の完全なJPEG（EOIで終わるが2枚目なし）
        let img = make_rgb_image(8, 8, 100, 100, 100);
        let single_jpeg = make_jpeg_bytes(&img);
        // FFD9 で終わるが後続 FFD8 なし
        let result = split_mpo(&single_jpeg);
        assert!(
            matches!(result, Err(Error::InvalidMpo)),
            "single JPEG (no second SOI) should return InvalidMpo, got: {:?}", result
        );
    }

    #[test]
    fn split_mpo_no_eoi_marker_returns_invalid_mpo() {
        // FFD9 FFD8 パターンが存在しないバイト列
        let data = vec![0xFF, 0xD8, 0x01, 0x02, 0x03, 0x04];
        let result = split_mpo(&data);
        assert!(
            matches!(result, Err(Error::InvalidMpo)),
            "no FFD9-FFD8 pattern should return InvalidMpo, got: {:?}", result
        );
    }

    #[test]
    fn split_mpo_truncated_right_jpeg_returns_error() {
        let left = make_rgb_image(8, 8, 200, 100, 50);
        let right = make_rgb_image(8, 8, 50, 100, 200);
        let mut mpo = make_synthetic_mpo(&left, &right);
        // 右JPEG の後半を切り捨てる（先頭数バイトだけ残す）
        let left_bytes_len = make_jpeg_bytes(&left).len();
        mpo.truncate(left_bytes_len + 4);

        let result = split_mpo(&mpo);
        assert!(result.is_err(), "truncated right JPEG should return error");
    }

    // ---------------------------------------------------------------
    // split_mpo 境界値
    // ---------------------------------------------------------------

    #[test]
    fn split_mpo_multiple_ffd9_ffd8_uses_first_occurrence() {
        // FFD9 FFD8 パターンが複数回出現するケース:
        // left_jpeg + right_left_jpeg(=右1枚目) + right_right_jpeg(=右2枚目)
        let left = make_rgb_image(8, 8, 200, 100, 50);
        let right1 = make_rgb_image(8, 8, 50, 100, 200);
        let right2 = make_rgb_image(8, 8, 128, 128, 128);

        let mut mpo = make_jpeg_bytes(&left);
        mpo.extend_from_slice(&make_jpeg_bytes(&right1));
        mpo.extend_from_slice(&make_jpeg_bytes(&right2));

        // 最初の FFD9 FFD8 で分割されるため、右目には right1+right2 が入るはず
        // （image::load_from_memory は先頭JPEGだけ読む → right1 の寸法になるはず）
        let result = split_mpo(&mpo);
        assert!(result.is_ok(), "multiple FFD9-FFD8 should succeed using first occurrence");
        let (l, _r) = result.unwrap();
        // 左目は left であること
        assert_eq!((l.width(), l.height()), (8, 8));
    }

    #[test]
    fn split_mpo_ffd9_without_following_ffd8_is_not_split() {
        // FFD9 単独で末尾（後続に FFD8 なし）→ InvalidMpo
        let img = make_rgb_image(8, 8, 100, 100, 100);
        let mut data = make_jpeg_bytes(&img);
        // 末尾に余分な 0xFF 0xD9 を付加するだけ（FFD8 続かない）
        data.extend_from_slice(&[0xFF, 0xD9, 0x00, 0x00]);
        let result = split_mpo(&data);
        assert!(
            matches!(result, Err(Error::InvalidMpo)),
            "FFD9 without following FFD8 should return InvalidMpo, got: {:?}", result
        );
    }

    // ---------------------------------------------------------------
    // stereo_to_depth 正常系
    // ---------------------------------------------------------------

    #[test]
    fn stereo_to_depth_identical_images_returns_zero_disparity() {
        let img = DynamicImage::ImageRgb8(make_rgb_image(16, 16, 128, 128, 128));
        let result = stereo_to_depth(&img, &img);
        assert!(result.is_ok());
        let depth = result.unwrap();
        // 同一画像なら最小SAD は d=0 → 全画素が0
        let luma = depth.to_luma8();
        for px in luma.pixels() {
            assert_eq!(px[0], 0, "identical images should produce zero disparity");
        }
    }

    #[test]
    fn stereo_to_depth_output_is_luma8() {
        let img = DynamicImage::ImageRgb8(make_rgb_image(8, 8, 100, 150, 200));
        let result = stereo_to_depth(&img, &img).unwrap();
        assert!(
            matches!(result, DynamicImage::ImageLuma8(_)),
            "output must be DynamicImage::ImageLuma8"
        );
    }

    #[test]
    fn stereo_to_depth_output_dimensions_match_input() {
        let img = DynamicImage::ImageRgb8(make_rgb_image(20, 12, 100, 150, 200));
        let result = stereo_to_depth(&img, &img).unwrap();
        assert_eq!((result.width(), result.height()), (20, 12));
    }

    #[test]
    fn stereo_to_depth_known_disparity_produces_bright_pixel() {
        // 右画像を左方向に N px シフト → 中央付近で視差 N が最小SAD → 明るくなる
        let w = 32_u32;
        let h = 16_u32;
        let shift: u32 = 8;

        // 左画像: 中央に白い縦帯
        let mut left_img = image::GrayImage::from_pixel(w, h, image::Luma([30]));
        for y in 0..h {
            for x in (w / 2 - 2)..(w / 2 + 2) {
                left_img.put_pixel(x, y, image::Luma([220]));
            }
        }
        let left = DynamicImage::ImageLuma8(left_img);

        // 右画像: 同じ縦帯を shift px 左にずらす
        let mut right_img = image::GrayImage::from_pixel(w, h, image::Luma([30]));
        for y in 0..h {
            for x in (w / 2 - 2)..(w / 2 + 2) {
                if x >= shift {
                    right_img.put_pixel(x - shift, y, image::Luma([220]));
                }
            }
        }
        let right = DynamicImage::ImageLuma8(right_img);

        let depth = stereo_to_depth(&left, &right).unwrap();
        let luma = depth.to_luma8();

        // 中央付近の画素は明るいはず（視差があるため）
        let cx = w / 2;
        let cy = h / 2;
        let center_val = luma.get_pixel(cx, cy)[0];
        assert!(
            center_val >= 20,
            "known-disparity region should produce non-zero depth value, got {center_val}"
        );
    }

    // ---------------------------------------------------------------
    // stereo_to_depth 異常系
    // ---------------------------------------------------------------

    #[test]
    fn stereo_to_depth_size_mismatch_returns_error() {
        let left = DynamicImage::ImageRgb8(make_rgb_image(16, 16, 100, 100, 100));
        let right = DynamicImage::ImageRgb8(make_rgb_image(8, 8, 100, 100, 100));
        let result = stereo_to_depth(&left, &right);
        assert!(
            matches!(result, Err(Error::SizeMismatch { .. })),
            "size mismatch should return SizeMismatch error, got: {:?}", result
        );
    }

    // ---------------------------------------------------------------
    // stereo_to_depth 境界値
    // ---------------------------------------------------------------

    #[test]
    fn stereo_to_depth_1x1_image_does_not_panic() {
        let img = DynamicImage::ImageRgb8(make_rgb_image(1, 1, 128, 128, 128));
        let result = stereo_to_depth(&img, &img);
        assert!(result.is_ok(), "1x1 image should not panic");
    }

    #[test]
    fn stereo_to_depth_edge_pixels_do_not_panic() {
        // 左端・右端・上端・下端の画素でpanicしないこと
        let w = 8_u32;
        let h = 8_u32;
        let img = DynamicImage::ImageRgb8(make_rgb_image(w, h, 128, 128, 128));
        let result = stereo_to_depth(&img, &img);
        assert!(result.is_ok(), "edge pixels should not panic");
        let depth = result.unwrap();
        // 全境界画素にアクセスしてpanicしないか確認
        let luma = depth.to_luma8();
        let _ = luma.get_pixel(0, 0);
        let _ = luma.get_pixel(w - 1, 0);
        let _ = luma.get_pixel(0, h - 1);
        let _ = luma.get_pixel(w - 1, h - 1);
    }

    // ---------------------------------------------------------------
    // stereo_to_depth 同値分割
    // ---------------------------------------------------------------

    #[test]
    fn stereo_to_depth_uniform_gray_image_produces_zero_disparity() {
        // 全画素同一輝度のフラット画像 → SAD は全d で同じ → d=0が最初に選ばれる → 暗い出力
        let img = DynamicImage::ImageRgb8(make_rgb_image(16, 16, 100, 100, 100));
        let depth = stereo_to_depth(&img, &img).unwrap();
        let luma = depth.to_luma8();
        // d=0 → pixel_val = 0
        for px in luma.pixels() {
            assert_eq!(px[0], 0, "uniform gray should produce zero disparity (dark output)");
        }
    }
}
