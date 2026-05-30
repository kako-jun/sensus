//! Android ポートレートモード JPEG CLI インテグレーションテスト。
//!
//! `--portrait` フラグの正常系・異常系・境界値を `sensus` バイナリを直接呼び出して検証する。

use std::process::Command;

use tempfile::TempDir;

fn cargo_run() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sensus"))
}

// ---------------------------------------------------------------
// ヘルパー
// ---------------------------------------------------------------

/// 1x1 グレー PNG バイト列を生成する。
fn make_tiny_gray_png() -> Vec<u8> {
    let img = image::GrayImage::from_pixel(1, 1, image::Luma([128u8]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageLuma8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    buf
}

/// base64 エンコード（標準ライブラリのみ）。
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[(n >> 18) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// GDepth:Data 属性を含む合成ポートレート JPEG を生成する。
fn make_portrait_jpeg_with_depth() -> Vec<u8> {
    let png = make_tiny_gray_png();
    let b64 = base64_encode(&png);

    let xmp = format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GDepth="http://ns.google.com/photos/1.0/depthmap/" GDepth:Data="{b64}"/></rdf:RDF></x:xmpmeta>"#
    );
    let xmp_bytes = xmp.as_bytes();
    let seg_len = (xmp_bytes.len() + 2) as u16;

    // 1x1 RGB JPEG に APP1 セグメントを先頭に挿入する
    let rgb = image::RgbImage::from_pixel(1, 1, image::Rgb([100u8, 150u8, 200u8]));
    let mut jpeg_buf = Vec::new();
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_buf, 90);
        enc.encode_image(&rgb).unwrap();
    }

    let mut result = Vec::new();
    result.extend_from_slice(&jpeg_buf[..2]); // FF D8
    result.push(0xFF);
    result.push(0xE1);
    result.extend_from_slice(&seg_len.to_be_bytes());
    result.extend_from_slice(xmp_bytes);
    result.extend_from_slice(&jpeg_buf[2..]); // 残りの JPEG データ
    result
}

/// GDepth:Data を含まない合成 JPEG を生成する。
fn make_portrait_jpeg_without_depth() -> Vec<u8> {
    let rgb = image::RgbImage::from_pixel(1, 1, image::Rgb([100u8, 150u8, 200u8]));
    let mut jpeg_buf = Vec::new();
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_buf, 90);
        enc.encode_image(&rgb).unwrap();
    }
    jpeg_buf
}

fn make_synthetic_mpo(w: u32, h: u32) -> Vec<u8> {
    let left = image::RgbImage::from_pixel(w, h, image::Rgb([200, 100, 50]));
    let right = image::RgbImage::from_pixel(w, h, image::Rgb([50, 100, 200]));
    let mut mpo = jpeg_bytes(&left);
    mpo.extend_from_slice(&jpeg_bytes(&right));
    mpo
}

fn jpeg_bytes(img: &image::RgbImage) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 95);
    enc.encode_image(img).unwrap();
    buf
}

// ---------------------------------------------------------------
// C-P01: 合成ポートレート JPEG + --filter myopia-depth → 出力生成
// ---------------------------------------------------------------

#[test]
fn cli_portrait_with_myopia_depth_filter_succeeds() {
    let dir = TempDir::new().unwrap();
    let portrait_path = dir.path().join("portrait.jpg");
    let output_path = dir.path().join("out.png");

    let portrait_bytes = make_portrait_jpeg_with_depth();
    std::fs::write(&portrait_path, &portrait_bytes).unwrap();

    let status = cargo_run()
        .args([
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--portrait",
            portrait_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(
        status.success(),
        "valid portrait JPEG + myopia-depth should succeed"
    );
    assert!(output_path.exists(), "output file should be created");
}

// ---------------------------------------------------------------
// C-P02: --input なしで --portrait のみ → 出力生成（portrait自体が入力）
// ---------------------------------------------------------------

#[test]
fn cli_portrait_without_input_succeeds() {
    let dir = TempDir::new().unwrap();
    let portrait_path = dir.path().join("portrait.jpg");
    let output_path = dir.path().join("out.png");

    let portrait_bytes = make_portrait_jpeg_with_depth();
    std::fs::write(&portrait_path, &portrait_bytes).unwrap();

    let status = cargo_run()
        .args([
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--portrait",
            portrait_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(
        status.success(),
        "--portrait without --input should succeed"
    );
    assert!(output_path.exists(), "output file should be created");
}

// ---------------------------------------------------------------
// C-P03: --portrait + --mpo 同時指定 → clap が conflicts_with でエラーにする
// ---------------------------------------------------------------

#[test]
fn cli_portrait_and_mpo_together_returns_error() {
    let dir = TempDir::new().unwrap();
    let portrait_path = dir.path().join("portrait.jpg");
    let mpo_path = dir.path().join("test.mpo");
    let output_path = dir.path().join("out.png");

    let portrait_bytes = make_portrait_jpeg_with_depth();
    std::fs::write(&portrait_path, &portrait_bytes).unwrap();
    let mpo_bytes = make_synthetic_mpo(8, 8);
    std::fs::write(&mpo_path, &mpo_bytes).unwrap();

    // --portrait と --mpo の同時指定は clap が conflicts_with でエラーにする
    let status = cargo_run()
        .args([
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--mpo",
            mpo_path.to_str().unwrap(),
            "--portrait",
            portrait_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(
        !status.success(),
        "--portrait and --mpo together should fail with clap conflicts_with error"
    );
}

// ---------------------------------------------------------------
// C-P03b: --portrait + --depth 同時指定 → 失敗
// ---------------------------------------------------------------

#[test]
fn cli_portrait_and_depth_together_returns_error() {
    let dir = TempDir::new().unwrap();
    let portrait_path = dir.path().join("portrait.jpg");
    let depth_path = dir.path().join("depth.png");
    let output_path = dir.path().join("out.png");

    let portrait_bytes = make_portrait_jpeg_with_depth();
    std::fs::write(&portrait_path, &portrait_bytes).unwrap();
    let depth_img = image::GrayImage::from_pixel(1, 1, image::Luma([128u8]));
    depth_img.save(&depth_path).unwrap();

    let status = cargo_run()
        .args([
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--portrait",
            portrait_path.to_str().unwrap(),
            "--depth",
            depth_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(
        !status.success(),
        "--portrait and --depth together should fail"
    );
}

// ---------------------------------------------------------------
// C-P04: --portrait + depth 以外のフィルタ → 失敗
// ---------------------------------------------------------------

#[test]
fn cli_portrait_without_depth_filter_returns_error() {
    let dir = TempDir::new().unwrap();
    let portrait_path = dir.path().join("portrait.jpg");
    let output_path = dir.path().join("out.png");

    let portrait_bytes = make_portrait_jpeg_with_depth();
    std::fs::write(&portrait_path, &portrait_bytes).unwrap();

    let status = cargo_run()
        .args([
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "protanopia",
            "--portrait",
            portrait_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(
        !status.success(),
        "--portrait with non-depth filter should fail"
    );
}

// ---------------------------------------------------------------
// C-P05: GDepth:Data がない JPEG → 失敗
// ---------------------------------------------------------------

#[test]
fn cli_portrait_invalid_jpeg_returns_error() {
    let dir = TempDir::new().unwrap();
    let portrait_path = dir.path().join("portrait_no_depth.jpg");
    let output_path = dir.path().join("out.png");

    let portrait_bytes = make_portrait_jpeg_without_depth();
    std::fs::write(&portrait_path, &portrait_bytes).unwrap();

    let status = cargo_run()
        .args([
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--portrait",
            portrait_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(
        !status.success(),
        "portrait JPEG without GDepth:Data should fail"
    );
}
