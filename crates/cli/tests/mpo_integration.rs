//! MPO ステレオ画像 CLI インテグレーションテスト。
//!
//! `--mpo` フラグの正常系・異常系・境界値を `sensus` バイナリを直接呼び出して検証する。

use std::process::Command;

use tempfile::TempDir;

fn cargo_run() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sensus"))
}

/// 合成 MPO バイト列を生成する（左右 RgbImage を JPEG 連結）。
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

fn write_solid_png(path: &std::path::Path, w: u32, h: u32, rgba: [u8; 4]) {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba(rgba));
    img.save(path).expect("write test PNG");
}

// ---------------------------------------------------------------
// C-01: 有効MPO + --filter myopia-depth → 出力ファイルが生成される
// ---------------------------------------------------------------

#[test]
fn cli_mpo_with_myopia_depth_filter_succeeds() {
    let dir = TempDir::new().unwrap();
    let mpo_path = dir.path().join("test.mpo");
    let input_path = dir.path().join("in.png");
    let output_path = dir.path().join("out.png");

    // 合成MPOを書き出す
    let mpo_bytes = make_synthetic_mpo(32, 32);
    std::fs::write(&mpo_path, &mpo_bytes).unwrap();

    // --input にはダミー画像（MPOフローでは使用されるが、MPO左画像が代わりに使われる）
    write_solid_png(&input_path, 32, 32, [100, 150, 200, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input_path.to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--mpo",
            mpo_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(
        status.success(),
        "valid MPO + myopia-depth filter should succeed"
    );
    assert!(output_path.exists(), "output file should be created");
}

// ---------------------------------------------------------------
// C-01b: --mpo 単独（--input なし）で成功する
// ---------------------------------------------------------------

#[test]
fn cli_mpo_without_input_succeeds() {
    let dir = TempDir::new().unwrap();
    let mpo_path = dir.path().join("test.mpo");
    let output_path = dir.path().join("out.png");

    let mpo_bytes = make_synthetic_mpo(32, 32);
    std::fs::write(&mpo_path, &mpo_bytes).unwrap();

    let status = cargo_run()
        .args([
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--mpo",
            mpo_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(status.success(), "--mpo without --input should succeed");
    assert!(output_path.exists(), "output file should be created");
}

// ---------------------------------------------------------------
// C-02: --mpo + --depth 同時指定 → 失敗
// ---------------------------------------------------------------

#[test]
fn cli_mpo_and_depth_together_returns_error() {
    let dir = TempDir::new().unwrap();
    let mpo_path = dir.path().join("test.mpo");
    let depth_path = dir.path().join("depth.png");
    let input_path = dir.path().join("in.png");
    let output_path = dir.path().join("out.png");

    let mpo_bytes = make_synthetic_mpo(16, 16);
    std::fs::write(&mpo_path, &mpo_bytes).unwrap();
    write_solid_png(&input_path, 16, 16, [100, 150, 200, 255]);
    write_solid_png(&depth_path, 16, 16, [128, 128, 128, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input_path.to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--mpo",
            mpo_path.to_str().unwrap(),
            "--depth",
            depth_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(!status.success(), "--mpo and --depth together should fail");
}

// ---------------------------------------------------------------
// C-03: --mpo + depth以外のフィルタ → 失敗
// ---------------------------------------------------------------

#[test]
fn cli_mpo_without_depth_filter_returns_error() {
    let dir = TempDir::new().unwrap();
    let mpo_path = dir.path().join("test.mpo");
    let input_path = dir.path().join("in.png");
    let output_path = dir.path().join("out.png");

    let mpo_bytes = make_synthetic_mpo(16, 16);
    std::fs::write(&mpo_path, &mpo_bytes).unwrap();
    write_solid_png(&input_path, 16, 16, [100, 150, 200, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input_path.to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "protanopia",
            "--mpo",
            mpo_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(!status.success(), "--mpo with non-depth filter should fail");
}

// ---------------------------------------------------------------
// C-04: ランダムバイト列のMPOファイル → 失敗
// ---------------------------------------------------------------

#[test]
fn cli_mpo_invalid_mpo_content_returns_error() {
    let dir = TempDir::new().unwrap();
    let mpo_path = dir.path().join("invalid.mpo");
    let input_path = dir.path().join("in.png");
    let output_path = dir.path().join("out.png");

    // ランダムなバイト列（有効なJPEGではない）
    let garbage: Vec<u8> = (0..256).map(|i: u32| (i * 37 % 256) as u8).collect();
    std::fs::write(&mpo_path, &garbage).unwrap();
    write_solid_png(&input_path, 16, 16, [100, 150, 200, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input_path.to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
            "--filter",
            "myopia-depth",
            "--mpo",
            mpo_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(!status.success(), "invalid MPO content should fail");
}
