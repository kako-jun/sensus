//! Integration tests for #108: composing non-depth filters with a depth blur
//! filter at the CLI (`--filter <color> --filter <depth> --depth <map>`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn sensus_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sensus"))
}

/// 色のある入力画像（protanopia で変化が出る）を書く。
fn write_input(path: &Path) {
    use image::{DynamicImage, RgbImage};
    let img = RgbImage::from_fn(32, 32, |x, _y| image::Rgb([(x * 8) as u8, 200, 60]));
    DynamicImage::ImageRgb8(img).save(path).unwrap();
}

/// 水平グラデーションの深度マップ（depth blur が効く）。
fn write_depth(path: &Path) {
    use image::{DynamicImage, GrayImage};
    let img = GrayImage::from_fn(32, 32, |x, _y| image::Luma([(x * 8) as u8]));
    DynamicImage::ImageLuma8(img).save(path).unwrap();
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(sensus_bin()).args(args).output().unwrap()
}

#[test]
fn depth_filter_composes_with_color_filter() {
    let dir = tempfile::tempdir().unwrap();
    let inp = dir.path().join("in.png");
    let depth = dir.path().join("d.png");
    let out_compose = dir.path().join("compose.png");
    let out_depth_only = dir.path().join("depth_only.png");
    write_input(&inp);
    write_depth(&depth);

    // 合成: protanopia → myopia-depth（#108 で hard error が解消され成功する）
    let s1 = run(&[
        "-i",
        inp.to_str().unwrap(),
        "--depth",
        depth.to_str().unwrap(),
        "-o",
        out_compose.to_str().unwrap(),
        "--filter",
        "protanopia",
        "--filter",
        "myopia-depth",
        "-s",
        "1.0",
    ]);
    assert!(
        s1.status.success(),
        "compose should succeed: {}",
        String::from_utf8_lossy(&s1.stderr)
    );
    assert!(out_compose.exists());

    // depth のみ
    let s2 = run(&[
        "-i",
        inp.to_str().unwrap(),
        "--depth",
        depth.to_str().unwrap(),
        "-o",
        out_depth_only.to_str().unwrap(),
        "--filter",
        "myopia-depth",
        "-s",
        "1.0",
    ]);
    assert!(s2.status.success());

    // protanopia が前段で効くので、合成結果は depth-only と異なるはず
    let a = image::open(&out_compose).unwrap().to_rgba8();
    let b = image::open(&out_depth_only).unwrap().to_rgba8();
    assert_ne!(
        a.as_raw(),
        b.as_raw(),
        "composed output must differ from depth-only (color filter applied first)"
    );
}

#[test]
fn two_depth_filters_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let inp = dir.path().join("in.png");
    let depth = dir.path().join("d.png");
    let out = dir.path().join("out.png");
    write_input(&inp);
    write_depth(&depth);

    // depth フィルタ 2 つは不可（depth_aware_blur は単一 kind）
    let o = run(&[
        "-i",
        inp.to_str().unwrap(),
        "--depth",
        depth.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--filter",
        "myopia-depth",
        "--filter",
        "hyperopia-depth",
    ]);
    assert!(!o.status.success(), "two depth filters must be rejected");
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(
        stderr.contains("depth blur filter"),
        "error should mention depth blur filter: {stderr}"
    );
}
