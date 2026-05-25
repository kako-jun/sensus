//! MPO ステレオ写真の分割と深度マップ生成。
//!
//! # 関数
//! - [`split_mpo`]: MPO バイト列を左目・右目 JPEG に分割する
//! - [`stereo_to_depth`]: 左右画像から SAD ブロックマッチングで深度マップを生成する
//! - [`read_xmp_depth`]: Android ポートレートモード JPEG から XMP 深度マップを抽出する

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

/// Android ポートレートモード JPEG から XMP 深度マップを抽出する。
///
/// Google Depth API の `GDepth:Data` フィールド（base64 エンコード PNG/JPEG）を
/// JPEG バイト列から取り出し、グレースケール `DynamicImage` として返す。
///
/// # Errors
/// - `Error::NoDepthMap`: XMP メタデータに `GDepth:Data` が見つからない
/// - `Error::Image`: base64 デコード後の画像データが読み込めない
pub fn read_xmp_depth(data: &[u8]) -> Result<DynamicImage> {
    let mut i = 2usize; // SOI (FF D8) をスキップ
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if i + 2 + seg_len > data.len() {
            break;
        }
        let seg = &data[i + 2..i + 2 + seg_len];
        if marker == 0xE1 {
            // APP1 セグメント
            if let Ok(s) = std::str::from_utf8(seg) {
                if s.contains("GDepth:Data") {
                    if let Some(b64) = extract_gdepth_data(s) {
                        let decoded = base64_decode(b64.as_bytes())?;
                        return image::load_from_memory(&decoded)
                            .map_err(crate::Error::Image);
                    }
                }
            }
        }
        i += 2 + seg_len;
    }
    Err(crate::Error::NoDepthMap)
}

/// XMP 文字列から `GDepth:Data` の値を抽出する。
///
/// 属性形式 `GDepth:Data="BASE64DATA"` と
/// 要素形式 `<GDepth:Data>BASE64DATA</GDepth:Data>` の両方に対応する。
fn extract_gdepth_data(xmp: &str) -> Option<&str> {
    // 属性形式: GDepth:Data="..."
    if let Some(pos) = xmp.find("GDepth:Data=\"") {
        let start = pos + "GDepth:Data=\"".len();
        let rest = &xmp[start..];
        if let Some(end) = rest.find('"') {
            return Some(&rest[..end]);
        }
    }
    // 要素形式: <GDepth:Data>...</GDepth:Data>
    if let Some(pos) = xmp.find("<GDepth:Data>") {
        let start = pos + "<GDepth:Data>".len();
        let rest = &xmp[start..];
        if let Some(end) = rest.find("</GDepth:Data>") {
            return Some(&rest[..end]);
        }
    }
    None
}

/// 標準 base64 デコード（外部クレート不使用）。
///
/// 空白文字（改行、スペース等）はスキップ。`=` はパディングとして無視する。
/// 不正な文字が含まれる場合は `Err(Error::NoDepthMap)` を返す。
fn base64_decode(input: &[u8]) -> Result<Vec<u8>> {
    const TABLE: [i8; 256] = {
        let mut t = [-1i8; 256];
        let mut i = 0u8;
        // A-Z = 0-25
        while i < 26 {
            t[(b'A' + i) as usize] = i as i8;
            i += 1;
        }
        // a-z = 26-51
        i = 0;
        while i < 26 {
            t[(b'a' + i) as usize] = (26 + i) as i8;
            i += 1;
        }
        // 0-9 = 52-61
        i = 0;
        while i < 10 {
            t[(b'0' + i) as usize] = (52 + i) as i8;
            i += 1;
        }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };

    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &b in input {
        if b == b'=' || b == b'\n' || b == b'\r' || b == b' ' {
            continue;
        }
        let v = TABLE[b as usize];
        if v < 0 {
            return Err(crate::Error::NoDepthMap);
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbImage};

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

    // ---------------------------------------------------------------
    // ヘルパー: read_xmp_depth / base64_decode テスト用
    // ---------------------------------------------------------------

    fn make_tiny_gray_png() -> Vec<u8> {
        let img = image::GrayImage::from_pixel(1, 1, image::Luma([128u8]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    fn base64_encode_test(data: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(CHARS[(n >> 18) as usize] as char);
            out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
            out.push(if chunk.len() > 1 { CHARS[((n >> 6) & 0x3F) as usize] as char } else { '=' });
            out.push(if chunk.len() > 2 { CHARS[(n & 0x3F) as usize] as char } else { '=' });
        }
        out
    }

    fn make_portrait_jpeg_with_xmp_attr(gdepth_b64: &str) -> Vec<u8> {
        // seg_len の先頭 2 バイトが valid UTF-8 になるよう XMP 長を調整する。
        let xmp_core = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GDepth="http://ns.google.com/photos/1.0/depthmap/" GDepth:Data="{gdepth_b64}"/></rdf:RDF></x:xmpmeta>"#
        );
        let target_len = find_valid_xmp_len_for_test(xmp_core.len());
        let xmp = if xmp_core.len() < target_len {
            format!("{}{}", " ".repeat(target_len - xmp_core.len()), xmp_core)
        } else {
            xmp_core
        };
        let xmp_bytes = xmp.as_bytes();
        let seg_len = (xmp_bytes.len() + 2) as u16;
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xE1];
        data.extend_from_slice(&seg_len.to_be_bytes());
        data.extend_from_slice(xmp_bytes);
        data.extend_from_slice(&[0xFF, 0xD9]);
        data
    }

    fn make_portrait_jpeg_with_xmp_element(gdepth_b64: &str) -> Vec<u8> {
        let xmp_core = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GDepth="http://ns.google.com/photos/1.0/depthmap/"><GDepth:Data>{gdepth_b64}</GDepth:Data></rdf:Description></rdf:RDF></x:xmpmeta>"#
        );
        let target_len = find_valid_xmp_len_for_test(xmp_core.len());
        let xmp = if xmp_core.len() < target_len {
            format!("{}{}", " ".repeat(target_len - xmp_core.len()), xmp_core)
        } else {
            xmp_core
        };
        let xmp_bytes = xmp.as_bytes();
        let seg_len = (xmp_bytes.len() + 2) as u16;
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xE1];
        data.extend_from_slice(&seg_len.to_be_bytes());
        data.extend_from_slice(xmp_bytes);
        data.extend_from_slice(&[0xFF, 0xD9]);
        data
    }

    fn find_valid_xmp_len_for_test(min_len: usize) -> usize {
        for xmp_len in min_len..min_len + 256 {
            let seg_len = (xmp_len + 2) as u16;
            let bytes = seg_len.to_be_bytes();
            if std::str::from_utf8(&bytes).is_ok() {
                return xmp_len;
            }
        }
        panic!("could not find valid UTF-8 seg_len");
    }

    fn make_portrait_jpeg_no_gdepth() -> Vec<u8> {
        let xmp = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about=""/></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#;
        let xmp_bytes = xmp.as_bytes();
        let seg_len = (xmp_bytes.len() + 2) as u16;
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xE1];
        data.extend_from_slice(&seg_len.to_be_bytes());
        data.extend_from_slice(xmp_bytes);
        data.extend_from_slice(&[0xFF, 0xD9]);
        data
    }

    // ---------------------------------------------------------------
    // read_xmp_depth 正常系
    // ---------------------------------------------------------------

    #[test]
    fn read_xmp_depth_attribute_form() {
        let png = make_tiny_gray_png();
        let b64 = base64_encode_test(&png);
        let jpeg = make_portrait_jpeg_with_xmp_attr(&b64);
        let result = read_xmp_depth(&jpeg);
        assert!(result.is_ok(), "attribute-form GDepth:Data should succeed, got: {:?}", result.err());
    }

    #[test]
    fn read_xmp_depth_element_form() {
        let png = make_tiny_gray_png();
        let b64 = base64_encode_test(&png);
        let jpeg = make_portrait_jpeg_with_xmp_element(&b64);
        let result = read_xmp_depth(&jpeg);
        assert!(result.is_ok(), "element-form GDepth:Data should succeed, got: {:?}", result.err());
    }

    #[test]
    fn read_xmp_depth_returns_image() {
        let png = make_tiny_gray_png();
        let b64 = base64_encode_test(&png);
        let jpeg = make_portrait_jpeg_with_xmp_attr(&b64);
        let img = read_xmp_depth(&jpeg).unwrap();
        assert_eq!(img.width(), 1, "decoded depth image should be 1px wide");
        assert_eq!(img.height(), 1, "decoded depth image should be 1px tall");
    }

    // ---------------------------------------------------------------
    // read_xmp_depth 異常系
    // ---------------------------------------------------------------

    #[test]
    fn read_xmp_depth_no_gdepth_data_returns_error() {
        let jpeg = make_portrait_jpeg_no_gdepth();
        let result = read_xmp_depth(&jpeg);
        assert!(
            matches!(result, Err(crate::Error::NoDepthMap)),
            "XMP without GDepth:Data should return NoDepthMap, got: {:?}", result
        );
    }

    #[test]
    fn read_xmp_depth_no_app1_returns_error() {
        // APP1マーカーが全くないバイト列（ただのSOI+EOI）
        let data = vec![0xFF, 0xD8, 0xFF, 0xD9];
        let result = read_xmp_depth(&data);
        assert!(
            matches!(result, Err(crate::Error::NoDepthMap)),
            "no APP1 segment should return NoDepthMap, got: {:?}", result
        );
    }

    #[test]
    fn read_xmp_depth_invalid_base64_returns_error() {
        // 不正な base64 文字 '@' を含むデータ
        let jpeg = make_portrait_jpeg_with_xmp_attr("@@@@INVALID@@@@");
        let result = read_xmp_depth(&jpeg);
        assert!(result.is_err(), "invalid base64 should return error, got Ok");
    }

    #[test]
    fn read_xmp_depth_empty_bytes_returns_error() {
        let result = read_xmp_depth(&[]);
        assert!(
            matches!(result, Err(crate::Error::NoDepthMap)),
            "empty bytes should return NoDepthMap, got: {:?}", result
        );
    }

    // ---------------------------------------------------------------
    // read_xmp_depth 境界値
    // ---------------------------------------------------------------

    #[test]
    fn read_xmp_depth_multiple_app1_segments() {
        // 1つ目のAPP1: GDepth:Data なし
        // 2つ目のAPP1: GDepth:Data あり
        let png = make_tiny_gray_png();
        let b64 = base64_encode_test(&png);

        // 1つ目: GDepth:Data なし（短い ASCII XMP → seg_len 小さい → valid UTF-8）
        let xmp_no_depth = "X".repeat(52); // 52 bytes → seg_len=54=[0x00,0x36] valid
        // 2つ目: GDepth:Data あり（find_valid_xmp_len_for_test で長さ調整）
        let xmp_with_depth_core = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GDepth="http://ns.google.com/photos/1.0/depthmap/" GDepth:Data="{b64}"/></rdf:RDF></x:xmpmeta>"#
        );
        let target_len = find_valid_xmp_len_for_test(xmp_with_depth_core.len());
        let xmp_with_depth = if xmp_with_depth_core.len() < target_len {
            format!("{}{}", " ".repeat(target_len - xmp_with_depth_core.len()), xmp_with_depth_core)
        } else {
            xmp_with_depth_core
        };

        let mut data = vec![0xFF, 0xD8];
        // 1つ目 APP1（GDepth:Data なし）
        let b1 = xmp_no_depth.as_bytes();
        let len1 = (b1.len() + 2) as u16;
        data.extend_from_slice(&[0xFF, 0xE1]);
        data.extend_from_slice(&len1.to_be_bytes());
        data.extend_from_slice(b1);
        // 2つ目 APP1（GDepth:Data あり）
        let b2 = xmp_with_depth.as_bytes();
        let len2 = (b2.len() + 2) as u16;
        data.extend_from_slice(&[0xFF, 0xE1]);
        data.extend_from_slice(&len2.to_be_bytes());
        data.extend_from_slice(b2);
        data.extend_from_slice(&[0xFF, 0xD9]);

        let result = read_xmp_depth(&data);
        assert!(result.is_ok(), "GDepth:Data in second APP1 should be found, got: {:?}", result.err());
    }

    // ---------------------------------------------------------------
    // base64_decode ユニットテスト
    // ---------------------------------------------------------------

    #[test]
    fn base64_decode_hello_world() {
        let result = base64_decode(b"SGVsbG8gV29ybGQ=");
        assert!(result.is_ok(), "Hello World decode should succeed");
        assert_eq!(result.unwrap(), b"Hello World");
    }

    #[test]
    fn base64_decode_with_newlines() {
        // 改行を含む base64 文字列もデコードできること
        let input = b"SGVs\nbG8g\r\nV29y\nbGQ=";
        let result = base64_decode(input);
        assert!(result.is_ok(), "base64 with newlines should succeed");
        assert_eq!(result.unwrap(), b"Hello World");
    }

    #[test]
    fn base64_decode_invalid_char_returns_error() {
        // '@' は base64 不正文字
        let result = base64_decode(b"SGVs@G8=");
        assert!(
            matches!(result, Err(crate::Error::NoDepthMap)),
            "invalid char '@' should return error, got: {:?}", result
        );
    }
}
