//! Vision filters: color vision deficiency, blur / refraction, visual field
//! defects, light sensitivity, etc.
//!
//! Phase 1 (Issue #2) では色覚特性 4 種を実装する:
//!
//! - [`protanopia`]    — 1 型 2 色覚（L 錐体欠損, 赤盲）
//! - [`deuteranopia`]  — 2 型 2 色覚（M 錐体欠損, 緑盲）
//! - [`tritanopia`]    — 3 型 2 色覚（S 錐体欠損, 青盲）
//! - [`achromatopsia`] — 全色盲（錐体機能不全）
//!
//! # アルゴリズム
//!
//! ## protanopia / deuteranopia / tritanopia
//!
//! Machado, Oliveira, Fernandes (2009)
//! "A Physiologically-based Model for Simulation of Color Vision Deficiency"
//! IEEE TVCG, DOI: [10.1109/TVCG.2009.113][doi]
//! の severity = 1.0 行列を **linear sRGB → simulated linear sRGB** に
//! 直接適用する。著者ページの supplementary に同じ値が掲載されている:
//! <https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html>
//!
//! 中間 strength は Machado 自身が示唆する通り、linear sRGB 空間で
//! `lerp(original, simulated, strength)` する。これは
//! anomalous trichromacy（軽度色覚異常）の臨床的近似として
//! DaltonLens 等で広く採用されている方式。
//!
//! ## achromatopsia
//!
//! LMS 経路は使わない（錐体機能不全のため三刺激値の前提が成立しない）。
//! CIE photopic luminance を BT.709 係数 (0.2126, 0.7152, 0.0722) で
//! linear sRGB から計算し、`(Y, Y, Y)` と原色を strength で linear blend する。
//!
//! BT.601 (0.299, 0.587, 0.114) は **使わない** — NTSC CRT 規格であり
//! sRGB / linear 空間には不適切。
//!
//! # 色空間
//!
//! 全処理は **linear sRGB 空間** で行う。入力 sRGB を gamma 解除 → 行列適用 /
//! luma 計算 → strength で linear blend → sRGB に gamma 戻し。アルファは
//! そのまま保持する。
//!
//! [doi]: https://doi.org/10.1109/TVCG.2009.113

use crate::Result;
use image::{DynamicImage, RgbaImage};

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典: Machado, Oliveira, Fernandes 2009, Table 3 / 5 相当の severity=1.0
/// プリ計算行列。著者ページ:
/// <https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html>
/// および DaltonLens 公開データ <https://daltonlens.org/> と一致。
const PROTANOPIA: [[f32; 3]; 3] = [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
];

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典: 上記 [`PROTANOPIA`] と同じ。
const DEUTERANOPIA: [[f32; 3]; 3] = [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
];

/// Machado 2009 severity = 1.0 行列（linear sRGB → simulated linear sRGB）。
///
/// 出典: 上記 [`PROTANOPIA`] と同じ。
const TRITANOPIA: [[f32; 3]; 3] = [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
];

/// BT.709 / sRGB photopic luminance 係数（CIE Y）。
const LUMA_R: f32 = 0.2126;
const LUMA_G: f32 = 0.7152;
const LUMA_B: f32 = 0.0722;

/// sRGB (0.0..=1.0) → linear sRGB の標準ガンマ解除。
#[inline]
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// linear sRGB → sRGB (0.0..=1.0) の標準ガンマ適用。
#[inline]
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `[0.0, 1.0]` に clamp してから 8 bit に丸めて保存する。
///
/// NaN は明示的に 0 として扱う（saturating cast の暗黙挙動に依存しない）。
#[inline]
fn pack_u8(c: f32) -> u8 {
    if c.is_nan() {
        0
    } else {
        (c.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

/// Protanopia (1 型 2 色覚, L 錐体欠損 / 赤盲) シミュレーション。
///
/// `strength` を Machado 2009 severity (0.0..=1.0) として扱い、範囲外は clamp する。
/// `0.0` は元画像と同一、`1.0` で完全 dichromacy。
pub fn protanopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &PROTANOPIA, strength)
}

/// Deuteranopia (2 型 2 色覚, M 錐体欠損 / 緑盲) シミュレーション。
///
/// `strength` の意味は [`protanopia`] と同じ。
pub fn deuteranopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &DEUTERANOPIA, strength)
}

/// Tritanopia (3 型 2 色覚, S 錐体欠損 / 青盲) シミュレーション。
///
/// `strength` の意味は [`protanopia`] と同じ。
pub fn tritanopia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    apply_machado_matrix(img, &TRITANOPIA, strength)
}

/// Achromatopsia (全色盲) シミュレーション。
///
/// LMS 経路ではなく、BT.709 photopic luminance によるグレースケール化を行う。
/// `strength = 1.0` で完全グレースケール (R == G == B)。`strength = 0.0` で原画像。
/// 中間値は linear sRGB 空間で線形補間。
pub fn achromatopsia(img: DynamicImage, strength: f32) -> Result<DynamicImage> {
    // NaN strength は identity（元画像）として扱う。
    // f32::NAN.clamp(0.0, 1.0) は NaN のままだが、上流で 0.0 に置換しているので
    // silent な全画素 0 出力にはならない。
    let strength = if strength.is_nan() {
        0.0
    } else {
        strength.clamp(0.0, 1.0)
    };
    let mut rgba = img.to_rgba8();

    // strength == 0.0 のショートカット（元画像と完全一致を保証）。
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);

        let y = LUMA_R * r + LUMA_G * g + LUMA_B * b;

        // linear 空間で原色 → 完全グレースケールへブレンド
        let nr = r + (y - r) * strength;
        let ng = g + (y - g) * strength;
        let nb = b + (y - b) * strength;

        px[0] = pack_u8(linear_to_srgb(nr));
        px[1] = pack_u8(linear_to_srgb(ng));
        px[2] = pack_u8(linear_to_srgb(nb));
        // alpha (px[3]) はそのまま保持
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

/// linear sRGB 上で 3x3 行列を掛けたシミュレーション結果と原色を、
/// strength で linear blend する内部実装。
///
/// 行列は LMS 空間のものではなく、Machado 2009 がプリ計算した
/// linear sRGB → simulated linear sRGB の severity = 1.0 行列。
fn apply_machado_matrix(
    img: DynamicImage,
    matrix: &[[f32; 3]; 3],
    strength: f32,
) -> Result<DynamicImage> {
    // NaN strength は identity（元画像）として扱う。
    // f32::NAN.clamp(0.0, 1.0) は NaN のままだが、上流で 0.0 に置換しているので
    // silent な全画素 0 出力にはならない。
    let strength = if strength.is_nan() {
        0.0
    } else {
        strength.clamp(0.0, 1.0)
    };
    let mut rgba: RgbaImage = img.to_rgba8();

    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    for px in rgba.pixels_mut() {
        let r = srgb_to_linear(px[0] as f32 / 255.0);
        let g = srgb_to_linear(px[1] as f32 / 255.0);
        let b = srgb_to_linear(px[2] as f32 / 255.0);

        let sr = matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b;
        let sg = matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b;
        let sb = matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b;

        // strength で linear blend（0.0 = 原色, 1.0 = 完全 dichromacy）
        let nr = r + (sr - r) * strength;
        let ng = g + (sg - g) * strength;
        let nb = b + (sb - b) * strength;

        px[0] = pack_u8(linear_to_srgb(nr));
        px[1] = pack_u8(linear_to_srgb(ng));
        px[2] = pack_u8(linear_to_srgb(nb));
        // alpha はそのまま
    }

    Ok(DynamicImage::ImageRgba8(rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// 1×1 の RGBA 画像を作るヘルパー。
    fn pixel(r: u8, g: u8, b: u8, a: u8) -> DynamicImage {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([r, g, b, a]));
        DynamicImage::ImageRgba8(img)
    }

    fn read_rgba(img: &DynamicImage) -> [u8; 4] {
        let p = img.to_rgba8();
        let px = p.get_pixel(0, 0);
        [px[0], px[1], px[2], px[3]]
    }

    // ---------------------------------------------------------------
    // strength = 0.0 で元画像と一致
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 255);
        let out = protanopia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn deuteranopia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 255);
        let out = deuteranopia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn tritanopia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 255);
        let out = tritanopia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn achromatopsia_strength_zero_is_identity() {
        let input = pixel(200, 50, 30, 128);
        let out = achromatopsia(input.clone(), 0.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 128]);
    }

    // ---------------------------------------------------------------
    // alpha 保持
    // ---------------------------------------------------------------

    #[test]
    fn alpha_is_preserved_across_filters() {
        for strength in [0.0_f32, 0.5, 1.0] {
            let input = pixel(200, 50, 30, 77);
            assert_eq!(
                read_rgba(&protanopia(input.clone(), strength).unwrap())[3],
                77
            );
            assert_eq!(
                read_rgba(&deuteranopia(input.clone(), strength).unwrap())[3],
                77
            );
            assert_eq!(
                read_rgba(&tritanopia(input.clone(), strength).unwrap())[3],
                77
            );
            assert_eq!(
                read_rgba(&achromatopsia(input.clone(), strength).unwrap())[3],
                77
            );
        }
    }

    // ---------------------------------------------------------------
    // strength の範囲外を clamp
    // ---------------------------------------------------------------

    #[test]
    fn negative_strength_is_clamped_to_zero() {
        let input = pixel(200, 50, 30, 255);
        let out = deuteranopia(input.clone(), -1.0).unwrap();
        assert_eq!(read_rgba(&out), [200, 50, 30, 255]);
    }

    #[test]
    fn strength_above_one_is_clamped_to_one() {
        let input = pixel(200, 50, 30, 255);
        let a = deuteranopia(input.clone(), 2.0).unwrap();
        let b = deuteranopia(input.clone(), 1.0).unwrap();
        assert_eq!(read_rgba(&a), read_rgba(&b));
    }

    #[test]
    fn nan_strength_does_not_panic() {
        let input = pixel(200, 50, 30, 255);
        // NaN strength は identity（元画像）として扱う契約。panic しない・
        // silent corruption しないことを確認する（regression guard）。
        let _ = protanopia(input.clone(), f32::NAN).unwrap();
        let _ = deuteranopia(input.clone(), f32::NAN).unwrap();
        let _ = tritanopia(input.clone(), f32::NAN).unwrap();
        let _ = achromatopsia(input, f32::NAN).unwrap();
    }

    // ---------------------------------------------------------------
    // NaN strength は identity（元画像と byte-exact 一致）
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = protanopia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    #[test]
    fn deuteranopia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = deuteranopia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    #[test]
    fn tritanopia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = tritanopia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    #[test]
    fn achromatopsia_nan_strength_returns_identity() {
        let input = pixel(255, 0, 0, 200);
        let out = achromatopsia(input.clone(), f32::NAN).unwrap();
        assert_eq!(read_rgba(&out), [255, 0, 0, 200]);
    }

    // ---------------------------------------------------------------
    // achromatopsia: 完全グレースケール検証
    // ---------------------------------------------------------------

    #[test]
    fn achromatopsia_full_strength_is_grayscale() {
        // 任意のカラフルなピクセル群で R == G == B になること
        for (r, g, b) in [
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (200, 50, 30),
            (12, 34, 56),
        ] {
            let input = pixel(r, g, b, 255);
            let [or, og, ob, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
            assert_eq!(or, og, "R/G mismatch for input ({r},{g},{b})");
            assert_eq!(og, ob, "G/B mismatch for input ({r},{g},{b})");
        }
    }

    #[test]
    fn achromatopsia_pure_red_luma_matches_bt709() {
        // 純赤 (linear 1.0, 0, 0) の Y = 0.2126
        // sRGB に戻して 8bit 化: linear_to_srgb(0.2126) ≈ 0.4984 → 127
        let input = pixel(255, 0, 0, 255);
        let [r, g, b, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert_eq!(r, 127);
        assert_eq!(g, 127);
        assert_eq!(b, 127);
    }

    #[test]
    fn achromatopsia_pure_green_luma_matches_bt709() {
        // 純緑の Y = 0.7152、sRGB ≈ 0.8625、8bit ≈ 220
        let input = pixel(0, 255, 0, 255);
        let [r, _, _, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert_eq!(r, 220);
    }

    #[test]
    fn achromatopsia_pure_blue_luma_matches_bt709() {
        // 純青の Y = 0.0722、sRGB ≈ 0.2979、8bit ≈ 76
        let input = pixel(0, 0, 255, 255);
        let [r, _, _, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert_eq!(r, 76);
    }

    #[test]
    fn achromatopsia_white_stays_white() {
        let input = pixel(255, 255, 255, 255);
        assert_eq!(
            read_rgba(&achromatopsia(input, 1.0).unwrap()),
            [255, 255, 255, 255]
        );
    }

    #[test]
    fn achromatopsia_black_stays_black() {
        let input = pixel(0, 0, 0, 255);
        assert_eq!(
            read_rgba(&achromatopsia(input, 1.0).unwrap()),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn achromatopsia_gray_is_unchanged_at_full_strength() {
        // R == G == B のグレーは achromatopsia(1.0) でも変化しない（≦1bit 丸め誤差は許容）
        let input = pixel(128, 128, 128, 255);
        let [r, g, b, _] = read_rgba(&achromatopsia(input, 1.0).unwrap());
        assert!((r as i16 - 128).abs() <= 1);
        assert!((g as i16 - 128).abs() <= 1);
        assert!((b as i16 - 128).abs() <= 1);
    }

    // ---------------------------------------------------------------
    // matrix 系: severity=1.0 で原色が想定通り変化する
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_red_shifts_toward_dark_yellow_green() {
        // 赤盲では純赤の R 成分が落ち、G に寄る（黒〜暗い黄緑）
        let input = pixel(255, 0, 0, 255);
        let [r, g, b, _] = read_rgba(&protanopia(input, 1.0).unwrap());
        // 数値固定（regression）: R が大きく落ち、G/B も限定的
        assert!(r < 150, "expected R drop, got {r}");
        assert!(g < 150, "expected G modest, got {g}");
        // R == G == B（完全グレー）にはならない
        assert!(!(r == g && g == b));
    }

    #[test]
    fn deuteranopia_red_shifts_toward_dim_yellow() {
        // 緑盲でも純赤は薄くなり、緑寄りに変化する
        let input = pixel(255, 0, 0, 255);
        let [r, g, b, _] = read_rgba(&deuteranopia(input, 1.0).unwrap());
        assert!(r < 220, "expected R drop, got {r}");
        assert!(g > 0, "expected some G, got {g}");
        assert!(!(r == g && g == b));
    }

    #[test]
    fn tritanopia_blue_shifts() {
        // 青盲で純青は変化する（B が落ちて G が出る）
        let input = pixel(0, 0, 255, 255);
        let [_r, g, b, _] = read_rgba(&tritanopia(input, 1.0).unwrap());
        // tritanopia 行列の B 行は (0.004733, 0.691367, 0.303900) なので
        // B 出力は 0.3039 程度 → だいぶ落ちる
        assert!(b < 200, "expected B drop, got {b}");
        // G 行は (-0.078411, 0.930809, 0.147602)、B 入力で G 出力は 0.1476 程度
        // sRGB に戻すとそれなりの輝度
        assert!(g > 50, "expected some G output, got {g}");
    }

    #[test]
    fn matrices_preserve_neutral_gray() {
        // 行列は CVD シミュレーションで neutral 軸を保つ性質がある:
        // 中間グレーは大きく変色しないはず（数 bit の差は許容）
        let input = pixel(128, 128, 128, 255);
        for filt in [protanopia as fn(_, _) -> _, deuteranopia, tritanopia] {
            let [r, g, b, _] = read_rgba(&filt(input.clone(), 1.0).unwrap());
            assert!((r as i16 - 128).abs() <= 8, "R={r}");
            assert!((g as i16 - 128).abs() <= 8, "G={g}");
            assert!((b as i16 - 128).abs() <= 8, "B={b}");
        }
    }

    // ---------------------------------------------------------------
    // matrix 系: severity=1.0 で Machado 2009 が示す byte-exact 値に一致
    // ---------------------------------------------------------------

    #[test]
    fn protanopia_red_severity_1_matches_machado_2009() {
        let img = pixel(255, 0, 0, 255);
        let out = protanopia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[109, 95, 0],
            "protanopia(red, 1.0) per Machado 2009"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    #[test]
    fn deuteranopia_red_severity_1_matches_machado_2009() {
        let img = pixel(255, 0, 0, 255);
        let out = deuteranopia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[163, 144, 0],
            "deuteranopia(red, 1.0) per Machado 2009"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    #[test]
    fn tritanopia_blue_severity_1_matches_machado_2009() {
        let img = pixel(0, 0, 255, 255);
        let out = tritanopia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[0, 107, 150],
            "tritanopia(blue, 1.0) per Machado 2009"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    #[test]
    fn achromatopsia_red_severity_1_matches_bt709_luma() {
        // 純赤 (255, 0, 0) は BT.709 photopic luminance で (127, 127, 127)
        let img = pixel(255, 0, 0, 255);
        let out = achromatopsia(img, 1.0).unwrap();
        let raw = out.to_rgba8().into_raw();
        assert_eq!(
            &raw[..3],
            &[127, 127, 127],
            "achromatopsia(red, 1.0) per BT.709 photopic luminance"
        );
        assert_eq!(raw[3], 255, "alpha preserved");
    }

    // ---------------------------------------------------------------
    // 中間 strength: monotonic 性
    // ---------------------------------------------------------------

    #[test]
    fn intermediate_strength_is_between_endpoints() {
        // strength=0.5 の出力は、strength=0 と strength=1 の間に位置する
        let input = pixel(255, 0, 0, 255);
        let s0 = read_rgba(&deuteranopia(input.clone(), 0.0).unwrap());
        let s5 = read_rgba(&deuteranopia(input.clone(), 0.5).unwrap());
        let s1 = read_rgba(&deuteranopia(input, 1.0).unwrap());
        // R は s0 (=255) から s1 (低い値) に向かって落ちる
        assert!(s5[0] < s0[0]);
        assert!(s5[0] > s1[0]);
        // G は s0 (=0) から s1 (高い値) に向かって上がる
        assert!(s5[1] > s0[1]);
        assert!(s5[1] < s1[1]);
    }

    // ---------------------------------------------------------------
    // 多ピクセル画像でも通る（サイズ保持・全画素処理）
    // ---------------------------------------------------------------

    #[test]
    fn larger_image_keeps_dimensions() {
        let mut img = RgbaImage::new(8, 4);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 32) as u8, (y * 64) as u8, 100, 255]);
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let out = deuteranopia(dyn_img, 1.0).unwrap();
        assert_eq!(out.width(), 8);
        assert_eq!(out.height(), 4);
    }
}
