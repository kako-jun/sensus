//! #165 レビュー M1: 「strength=1.0 で新旧 byte 一致」という主張を全数検証する。
//!
//! ADR-0008 は当初「strength=1.0 では新方式（テーブル解決 + 直接適用）と旧方式
//! （severity=1.0 行列 + strength blend, ADR-0002・#165 で撤去）が byte-identical
//! である」と記載していたが、これは **実数演算としては正しい**（新方式は
//! `table[10]` を直接適用するだけで、`table[10]` は旧方式が使っていた
//! severity=1.0 行列と同じ値）ものの、**f32 の非結合性**
//! （`r + (sr - r)` は一般に `sr` と bit 単位で一致しない）により、
//! ごく稀に ±1 LSB の差が出うる。
//!
//! 本ファイルは全 256^3 = 16,777,216 通りの入力 RGB で実測し、この drift を
//! 定量化する。重い（3 フィルタ × 16.7M ピクセル）ため `#[ignore]` を付け、
//! 通常の `cargo test` では実行しない。手動実行:
//!
//! ```sh
//! cargo test --release --test color_severity1_full_sweep -- --ignored --nocapture
//! ```
//!
//! （デバッグビルドでも動くが `powf` 呼び出しが多く著しく遅いため `--release` 推奨）
//!
//! 新方式は実際のクレート関数（`vision::protanopia` 等, strength=1.0）を呼ぶ。
//! 旧方式はこのファイル内で **f32 精度**で独立に再実装する — f64 で再実装すると
//! 非結合性の丸め挙動が再現されずテストの意味がなくなるため、あえて f32 を使う
//! （gamma 変換自体は production の pub util `vision::srgb_to_linear` /
//! `linear_to_srgb` を使い、gamma 往復の丸めは production と完全に一致させる）。

use image::{DynamicImage, Rgba, RgbaImage};
use sensus_core::vision::{deuteranopia, linear_to_srgb, protanopia, srgb_to_linear, tritanopia};

/// 256^3 = 16,777,216 pixel を 1 枚の正方形画像に敷き詰める（4096 = 2^12）。
const WIDTH: u32 = 4096;
const HEIGHT: u32 = 4096;

type StrengthFilter = fn(DynamicImage, f32) -> sensus_core::Result<DynamicImage>;

/// [0,1] clamp して 8bit に丸める（f32 版, NaN は 0）。production の
/// private `pack_u8` とは独立に再実装（KAT の非トートロジー方針を踏襲）。
fn pack_u8_f32(c: f32) -> u8 {
    if c.is_nan() {
        0
    } else {
        (c.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

/// 旧方式（ADR-0002, #165 で撤去済み）: severity=1.0 行列を適用した結果と
/// 原色を `strength` で linear blend する。f32 精度で独立に再実装。
fn old_formula_pixel(m: &[[f32; 3]; 3], rgb: [u8; 3], strength: f32) -> [u8; 3] {
    let r = srgb_to_linear(rgb[0] as f32 / 255.0);
    let g = srgb_to_linear(rgb[1] as f32 / 255.0);
    let b = srgb_to_linear(rgb[2] as f32 / 255.0);

    let sr = m[0][0] * r + m[0][1] * g + m[0][2] * b;
    let sg = m[1][0] * r + m[1][1] * g + m[1][2] * b;
    let sb = m[2][0] * r + m[2][1] * g + m[2][2] * b;

    // 旧実装そのままの式: n = v + (s - v) * strength。strength=1.0 でも
    // 代数的には `sr` と同じだが、f32 演算としては非結合性で ±1 ULP ずれうる。
    let nr = r + (sr - r) * strength;
    let ng = g + (sg - g) * strength;
    let nb = b + (sb - b) * strength;

    [
        pack_u8_f32(linear_to_srgb(nr)),
        pack_u8_f32(linear_to_srgb(ng)),
        pack_u8_f32(linear_to_srgb(nb)),
    ]
}

/// severity=1.0 行列（f32 精度、独立リテラル）。
/// `crates/core/tests/color_kat.rs` の f64 版 `SRC_PROTANOPIA` 等とは
/// 物理的に別コピー — 非結合性の再現には f32 が必須なので f64 版を流用しない。
const SEV1_PROTANOPIA_F32: [[f32; 3]; 3] = [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
];

const SEV1_DEUTERANOPIA_F32: [[f32; 3]; 3] = [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
];

const SEV1_TRITANOPIA_F32: [[f32; 3]; 3] = [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
];

/// index (0..256^3) から (r, g, b) を復元する。`full_rgb_cube_image` の
/// pixel 敷き詰め順序と対応させて使う。
fn idx_to_rgb(y: u32, x: u32) -> [u8; 3] {
    let idx = (y as u64) * (WIDTH as u64) + (x as u64);
    [
        ((idx >> 16) & 0xFF) as u8,
        ((idx >> 8) & 0xFF) as u8,
        (idx & 0xFF) as u8,
    ]
}

/// 全 256^3 RGB 組み合わせを 1 枚の画像に敷き詰める（1 pixel = 1 色, alpha=255）。
fn full_rgb_cube_image() -> RgbaImage {
    let mut img = RgbaImage::new(WIDTH, HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b] = idx_to_rgb(y, x);
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    img
}

/// 1 フィルタぶんの全数比較を行い、(max_diff_lsb, mismatched_pixel_count) を返す。
/// 新方式は実クレート関数を 1 回の画像処理で呼ぶ（16.7M 回の個別呼び出しは
/// image オーバーヘッドで非現実的に遅いため）。旧方式は idx から直接計算する。
fn run_full_sweep(name: &str, sev1: &[[f32; 3]; 3], apply_new: StrengthFilter) -> (i32, u64) {
    let img = full_rgb_cube_image();
    let new_out = apply_new(DynamicImage::ImageRgba8(img), 1.0)
        .unwrap()
        .to_rgba8();

    let mut max_diff: i32 = 0;
    let mut mismatched_pixels: u64 = 0;

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let rgb = idx_to_rgb(y, x);
            let old = old_formula_pixel(sev1, rgb, 1.0);
            let new = new_out.get_pixel(x, y);
            let mut pixel_differs = false;
            for ch in 0..3 {
                let d = (old[ch] as i32 - new[ch] as i32).abs();
                if d > max_diff {
                    max_diff = d;
                }
                if d > 0 {
                    pixel_differs = true;
                }
            }
            if pixel_differs {
                mismatched_pixels += 1;
            }
        }
    }

    let total = (WIDTH as u64) * (HEIGHT as u64);
    println!(
        "{name}: max_diff={max_diff} LSB, mismatched_pixels={mismatched_pixels} / {total} (strength=1.0, full 256^3 sweep)"
    );
    (max_diff, mismatched_pixels)
}

#[test]
#[ignore = "16.7M-pixel exhaustive sweep (~5s in --release, very slow in debug); run manually with `cargo test --release --test color_severity1_full_sweep -- --ignored --nocapture`"]
fn protanopia_severity1_full_sweep_matches_old_formula_within_1_lsb() {
    let (max_diff, mismatched) = run_full_sweep("protanopia", &SEV1_PROTANOPIA_F32, protanopia);
    assert!(
        max_diff <= 1,
        "protanopia strength=1.0 full sweep: max diff {max_diff} LSB > 1 (mismatched_pixels={mismatched})"
    );
}

#[test]
#[ignore = "16.7M-pixel exhaustive sweep (~5s in --release, very slow in debug); run manually with `cargo test --release --test color_severity1_full_sweep -- --ignored --nocapture`"]
fn deuteranopia_severity1_full_sweep_matches_old_formula_within_1_lsb() {
    let (max_diff, mismatched) =
        run_full_sweep("deuteranopia", &SEV1_DEUTERANOPIA_F32, deuteranopia);
    assert!(
        max_diff <= 1,
        "deuteranopia strength=1.0 full sweep: max diff {max_diff} LSB > 1 (mismatched_pixels={mismatched})"
    );
}

#[test]
#[ignore = "16.7M-pixel exhaustive sweep (~5s in --release, very slow in debug); run manually with `cargo test --release --test color_severity1_full_sweep -- --ignored --nocapture`"]
fn tritanopia_severity1_full_sweep_matches_old_formula_within_1_lsb() {
    let (max_diff, mismatched) = run_full_sweep("tritanopia", &SEV1_TRITANOPIA_F32, tritanopia);
    assert!(
        max_diff <= 1,
        "tritanopia strength=1.0 full sweep: max diff {max_diff} LSB > 1 (mismatched_pixels={mismatched})"
    );
}
