//! #17 CPU 実装⇄GLSL シェーダ等価性回帰テスト
//!
//! GPU を使わず、GLSL シェーダと同じ数学を Rust でシミュレートした
//! ソフトウェアレンダラと CPU 実装の出力を比較する。
//!
//! ## 許容誤差
//! GLSL は mediump float（最低 16 bit 仮数部）を使う。Rust は f32（24 bit 仮数部）。
//! また disk blur の Fibonacci lattice は 16 tap の近似なので厳密一致は期待しない。
//! 以下の閾値で判定する:
//! - 色覚フィルタ（行列演算）: max per-channel 絶対誤差 ≤ 2/255
//! - ぼかしフィルタ（disk blur）: PSNR ≥ 30 dB（見た目が同等なら OK）
//! - 乱視フィルタ（directional blur）: PSNR ≥ 30 dB

use image::{DynamicImage, RgbaImage};
use sensus_core::shaders::{
    achromatopsia_uniforms, astigmatism_uniforms, deuteranopia_uniforms,
    glaucoma_uniforms, hemianopia_uniforms, hyperopia_uniforms,
    macular_degeneration_uniforms, myopia_uniforms, presbyopia_uniforms,
    protanopia_uniforms, tetrachromacy_uniforms, tritanopia_uniforms, tunnel_vision_uniforms,
    vestibular_neuritis_uniforms,
};
use sensus_core::vision::{
    achromatopsia, astigmatism, deuteranopia, eye_strain, glaucoma, GlaucomaMode, hemianopia,
    hyperopia, macular_degeneration, myopia, presbyopia, protanopia, tetrachromacy,
    tritanopia, tunnel_vision, vestibular_neuritis,
};

// ---------------------------------------------------------------------------
// ソフトウェアシミュレータ（GLSL 数学を Rust で再現）
// ---------------------------------------------------------------------------

/// sRGB → linear sRGB（GLSL `srgbToLinear` と同じ式）
#[inline]
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// linear sRGB → sRGB（GLSL `linearToSrgb` と同じ式）
#[inline]
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// 色覚フィルタシェーダ（protanopia.frag / deuteranopia.frag / tritanopia.frag）を
/// ソフトウェアシミュレートする。
fn sim_color_matrix(img: &RgbaImage, matrix: &[f32; 9], strength: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            let r = srgb_to_linear(px[0] as f32 / 255.0);
            let g = srgb_to_linear(px[1] as f32 / 255.0);
            let b = srgb_to_linear(px[2] as f32 / 255.0);

            let sr = matrix[0] * r + matrix[1] * g + matrix[2] * b;
            let sg = matrix[3] * r + matrix[4] * g + matrix[5] * b;
            let sb = matrix[6] * r + matrix[7] * g + matrix[8] * b;

            let nr = (r + (sr - r) * strength).clamp(0.0, 1.0);
            let ng = (g + (sg - g) * strength).clamp(0.0, 1.0);
            let nb = (b + (sb - b) * strength).clamp(0.0, 1.0);

            out.put_pixel(
                x,
                y,
                image::Rgba([
                    (linear_to_srgb(nr) * 255.0).round() as u8,
                    (linear_to_srgb(ng) * 255.0).round() as u8,
                    (linear_to_srgb(nb) * 255.0).round() as u8,
                    px[3],
                ]),
            );
        }
    }
    out
}

/// 全色盲フィルタシェーダ（achromatopsia.frag）をソフトウェアシミュレートする。
fn sim_achromatopsia(
    img: &RgbaImage,
    r_w: f32,
    g_w: f32,
    b_w: f32,
    strength: f32,
) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            let r = srgb_to_linear(px[0] as f32 / 255.0);
            let g = srgb_to_linear(px[1] as f32 / 255.0);
            let b = srgb_to_linear(px[2] as f32 / 255.0);

            let y_luma = r_w * r + g_w * g + b_w * b;

            let nr = (r + (y_luma - r) * strength).clamp(0.0, 1.0);
            let ng = (g + (y_luma - g) * strength).clamp(0.0, 1.0);
            let nb = (b + (y_luma - b) * strength).clamp(0.0, 1.0);

            out.put_pixel(
                x,
                y,
                image::Rgba([
                    (linear_to_srgb(nr) * 255.0).round() as u8,
                    (linear_to_srgb(ng) * 255.0).round() as u8,
                    (linear_to_srgb(nb) * 255.0).round() as u8,
                    px[3],
                ]),
            );
        }
    }
    out
}

/// disk blur シェーダ（myopia.frag / hyperopia.frag / presbyopia.frag）を
/// ソフトウェアシミュレートする。Fibonacci lattice 16 tap。
fn sim_disk_blur(img: &RgbaImage, radius_px: f32) -> RgbaImage {
    const N: usize = 16;
    const PHI: f32 = 2.399_963_2; // 黄金角
    let (w, h) = img.dimensions();
    let texel_w = 1.0 / w as f32;
    let texel_h = 1.0 / h as f32;

    // texture(uTexture, uv) の clamp-to-edge サンプリング
    let sample = |img: &RgbaImage, u: f32, v: f32| -> [f32; 3] {
        let px_x = ((u * w as f32).round() as i32).clamp(0, w as i32 - 1) as u32;
        let px_y = ((v * h as f32).round() as i32).clamp(0, h as i32 - 1) as u32;
        let px = img.get_pixel(px_x, px_y);
        [
            srgb_to_linear(px[0] as f32 / 255.0),
            srgb_to_linear(px[1] as f32 / 255.0),
            srgb_to_linear(px[2] as f32 / 255.0),
        ]
    };

    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;

            if radius_px < 0.5 {
                // pass-through
                continue;
            }

            let mut acc = [0f32; 3];
            for i in 0..N {
                let t = i as f32 / N as f32;
                let r = t.sqrt() * radius_px;
                let theta = i as f32 * PHI;
                let offset_u = theta.cos() * r * texel_w;
                let offset_v = theta.sin() * r * texel_h;
                let s = sample(img, u + offset_u, v + offset_v);
                acc[0] += s[0];
                acc[1] += s[1];
                acc[2] += s[2];
            }
            let blurred = [acc[0] / N as f32, acc[1] / N as f32, acc[2] / N as f32];

            let src = img.get_pixel(x, y);
            out.put_pixel(
                x,
                y,
                image::Rgba([
                    (linear_to_srgb(blurred[0].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb(blurred[1].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb(blurred[2].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    src[3],
                ]),
            );
        }
    }
    out
}

/// 乱視シェーダ（astigmatism.frag）をソフトウェアシミュレートする。
/// 16 tap の directional blur。`axis_deg` はぼかし方向（シェーダに渡す値）。
fn sim_astigmatism(img: &RgbaImage, radius_px: f32, axis_deg: f32) -> RgbaImage {
    const N: usize = 16;
    let (w, h) = img.dimensions();
    let texel_w = 1.0 / w as f32;
    let texel_h = 1.0 / h as f32;

    let sample = |img: &RgbaImage, u: f32, v: f32| -> [f32; 3] {
        let px_x = ((u * w as f32).round() as i32).clamp(0, w as i32 - 1) as u32;
        let px_y = ((v * h as f32).round() as i32).clamp(0, h as i32 - 1) as u32;
        let px = img.get_pixel(px_x, px_y);
        [
            srgb_to_linear(px[0] as f32 / 255.0),
            srgb_to_linear(px[1] as f32 / 255.0),
            srgb_to_linear(px[2] as f32 / 255.0),
        ]
    };

    let mut out = img.clone();

    if radius_px < 0.5 {
        return out;
    }

    let rad = axis_deg * std::f32::consts::PI / 180.0;
    let dir_x = rad.cos();
    let dir_y = rad.sin();

    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;

            let mut acc = [0f32; 3];
            for i in 0..N {
                // t in -1..+1
                let t = (i as f32 / (N - 1) as f32) * 2.0 - 1.0;
                let offset_u = dir_x * (t * radius_px) * texel_w;
                let offset_v = dir_y * (t * radius_px) * texel_h;
                let s = sample(img, u + offset_u, v + offset_v);
                acc[0] += s[0];
                acc[1] += s[1];
                acc[2] += s[2];
            }
            let blurred = [acc[0] / N as f32, acc[1] / N as f32, acc[2] / N as f32];

            let src = img.get_pixel(x, y);
            out.put_pixel(
                x,
                y,
                image::Rgba([
                    (linear_to_srgb(blurred[0].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb(blurred[1].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb(blurred[2].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    src[3],
                ]),
            );
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 比較ユーティリティ
// ---------------------------------------------------------------------------

/// 2画像の max per-channel absolute error（0..=255 スケール）を返す。
fn max_channel_error(a: &RgbaImage, b: &RgbaImage) -> u8 {
    assert_eq!(a.dimensions(), b.dimensions());
    let mut max_err = 0u8;
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        for c in 0..3 {
            let diff = (pa[c] as i32 - pb[c] as i32).unsigned_abs() as u8;
            max_err = max_err.max(diff);
        }
    }
    max_err
}

/// 2画像の PSNR を計算する（dB）。同一画像は f32::INFINITY を返す。
fn psnr(a: &RgbaImage, b: &RgbaImage) -> f32 {
    assert_eq!(a.dimensions(), b.dimensions());
    let (w, h) = a.dimensions();
    let n = (w * h * 3) as f64; // RGB のみ
    let mut mse = 0f64;
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        for c in 0..3 {
            let diff = pa[c] as f64 - pb[c] as f64;
            mse += diff * diff;
        }
    }
    mse /= n;
    if mse == 0.0 {
        return f32::INFINITY;
    }
    (10.0 * (255.0f64 * 255.0 / mse).log10()) as f32
}

// ---------------------------------------------------------------------------
// フィクスチャ生成
// ---------------------------------------------------------------------------

/// 4色コーナーカラーチャート（32x32）。
/// 左上=赤, 右上=緑, 左下=青, 右下=白。
fn color_chart_32() -> DynamicImage {
    let mut img = RgbaImage::new(32, 32);
    for y in 0..32u32 {
        for x in 0..32u32 {
            let px = match (x < 16, y < 16) {
                (true, true) => [220, 50, 50, 255],   // 赤
                (false, true) => [50, 200, 50, 255],  // 緑
                (true, false) => [50, 50, 220, 255],  // 青
                (false, false) => [200, 200, 200, 255], // 灰
            };
            img.put_pixel(x, y, image::Rgba(px));
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// linear グラデーション（32x32）。
fn gradient_32() -> DynamicImage {
    let mut img = RgbaImage::new(32, 32);
    for y in 0..32u32 {
        for x in 0..32u32 {
            let v = (x * 8) as u8;
            img.put_pixel(x, y, image::Rgba([v, v / 2, 255 - v, 255]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

// ---------------------------------------------------------------------------
// 色覚フィルタ等価性テスト
// ---------------------------------------------------------------------------

#[test]
fn shader_equiv_protanopia_strength_1_0() {
    let img = color_chart_32();
    let uni = protanopia_uniforms(1.0);
    let cpu_out = protanopia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_color_matrix(&img.to_rgba8(), &uni.matrix, uni.strength);
    let err = max_channel_error(&cpu_out, &gpu_sim);
    assert!(
        err <= 2,
        "protanopia strength=1.0: max channel error {err}/255 > 2"
    );
}

#[test]
fn shader_equiv_protanopia_strength_0_5() {
    let img = gradient_32();
    let uni = protanopia_uniforms(0.5);
    let cpu_out = protanopia(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_color_matrix(&img.to_rgba8(), &uni.matrix, uni.strength);
    let err = max_channel_error(&cpu_out, &gpu_sim);
    assert!(
        err <= 2,
        "protanopia strength=0.5: max channel error {err}/255 > 2"
    );
}

#[test]
fn shader_equiv_deuteranopia_strength_1_0() {
    let img = color_chart_32();
    let uni = deuteranopia_uniforms(1.0);
    let cpu_out = deuteranopia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_color_matrix(&img.to_rgba8(), &uni.matrix, uni.strength);
    let err = max_channel_error(&cpu_out, &gpu_sim);
    assert!(
        err <= 2,
        "deuteranopia strength=1.0: max channel error {err}/255 > 2"
    );
}

#[test]
fn shader_equiv_tritanopia_strength_1_0() {
    let img = color_chart_32();
    let uni = tritanopia_uniforms(1.0);
    let cpu_out = tritanopia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_color_matrix(&img.to_rgba8(), &uni.matrix, uni.strength);
    let err = max_channel_error(&cpu_out, &gpu_sim);
    assert!(
        err <= 2,
        "tritanopia strength=1.0: max channel error {err}/255 > 2"
    );
}

#[test]
fn shader_equiv_achromatopsia_strength_1_0() {
    let img = color_chart_32();
    let uni = achromatopsia_uniforms(1.0);
    let cpu_out = achromatopsia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_achromatopsia(&img.to_rgba8(), uni.r_weight, uni.g_weight, uni.b_weight, uni.strength);
    let err = max_channel_error(&cpu_out, &gpu_sim);
    assert!(
        err <= 2,
        "achromatopsia strength=1.0: max channel error {err}/255 > 2"
    );
}

#[test]
fn shader_equiv_achromatopsia_strength_0_0() {
    // strength=0 → 元画像と同じ（誤差 0）
    let img = color_chart_32();
    let uni = achromatopsia_uniforms(0.0);
    let cpu_out = achromatopsia(img.clone(), 0.0).unwrap().to_rgba8();
    let gpu_sim = sim_achromatopsia(&img.to_rgba8(), uni.r_weight, uni.g_weight, uni.b_weight, uni.strength);
    let err = max_channel_error(&cpu_out, &gpu_sim);
    assert!(
        err <= 2,
        "achromatopsia strength=0.0: max channel error {err}/255 > 2"
    );
}

// ---------------------------------------------------------------------------
// ぼかしフィルタ等価性テスト（PSNR ≥ 30 dB）
// CPU disk blur（brute-force）と GPU disk blur（Fibonacci 16 tap）は
// 近似手法が異なるため厳密一致は期待しない。PSNR で等価性を判定する。
// ---------------------------------------------------------------------------

#[test]
fn shader_equiv_myopia_strength_1_0_psnr() {
    let img = gradient_32();
    let uni = myopia_uniforms(1.0, 32);
    let cpu_out = myopia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_disk_blur(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "myopia strength=1.0: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_hyperopia_strength_1_0_psnr() {
    let img = gradient_32();
    let uni = hyperopia_uniforms(1.0, 32);
    let cpu_out = hyperopia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_disk_blur(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "hyperopia strength=1.0: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_presbyopia_strength_1_0_psnr() {
    let img = gradient_32();
    let uni = presbyopia_uniforms(1.0, 32);
    let cpu_out = presbyopia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_disk_blur(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "presbyopia strength=1.0: PSNR {db:.1} dB < 30 dB"
    );
}

// ---------------------------------------------------------------------------
// 乱視フィルタ等価性テスト（PSNR ≥ 30 dB）
// ---------------------------------------------------------------------------

#[test]
fn shader_equiv_astigmatism_axis_0_psnr() {
    let img = gradient_32();
    let uni = astigmatism_uniforms(1.0, 32, 0.0);
    let cpu_out = astigmatism(img.clone(), 1.0, 0.0).unwrap().to_rgba8();
    // シェーダへ渡す axis_deg は uni.axis_deg（ぼかし方向）
    let gpu_sim = sim_astigmatism(&img.to_rgba8(), uni.radius_px, uni.axis_deg);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "astigmatism axis=0°: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_astigmatism_axis_45_psnr() {
    let img = gradient_32();
    let uni = astigmatism_uniforms(1.0, 32, 45.0);
    let cpu_out = astigmatism(img.clone(), 1.0, 45.0).unwrap().to_rgba8();
    let gpu_sim = sim_astigmatism(&img.to_rgba8(), uni.radius_px, uni.axis_deg);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "astigmatism axis=45°: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_astigmatism_axis_90_psnr() {
    let img = gradient_32();
    let uni = astigmatism_uniforms(1.0, 32, 90.0);
    let cpu_out = astigmatism(img.clone(), 1.0, 90.0).unwrap().to_rgba8();
    let gpu_sim = sim_astigmatism(&img.to_rgba8(), uni.radius_px, uni.axis_deg);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "astigmatism axis=90°: PSNR {db:.1} dB < 30 dB"
    );
}

/// テスト用の 32x32 グラデーション画像を作るヘルパー。
fn make_test_image() -> DynamicImage {
    gradient_32()
}

/// eye_strain の GLSL シェーダ処理を Rust で再現する。
/// sRGB decode → contrast compression → vignette → sRGB encode
fn simulate_eye_strain_glsl(img: &DynamicImage, strength: f32) -> RgbaImage {
    let src = img.to_rgba8();
    let (w, h) = src.dimensions();
    let mut out = src.clone();
    for y in 0..h {
        for x in 0..w {
            let px = src.get_pixel(x, y);
            // decode to linear
            let r = srgb_to_linear(px[0] as f32 / 255.0);
            let g = srgb_to_linear(px[1] as f32 / 255.0);
            let b = srgb_to_linear(px[2] as f32 / 255.0);
            // contrast compression in linear space
            let cr = 0.5 + (r - 0.5) * (1.0 - strength * 0.15);
            let cg = 0.5 + (g - 0.5) * (1.0 - strength * 0.15);
            let cb = 0.5 + (b - 0.5) * (1.0 - strength * 0.15);
            // vignette: v_texcoord = (x+0.5)/w, (y+0.5)/h → uv = texcoord*2-1
            let uv_x = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let uv_y = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
            let d = uv_x * uv_x + uv_y * uv_y;
            let t = ((d - 0.3) / (1.2 - 0.3)).clamp(0.0, 1.0);
            let sm = t * t * (3.0 - 2.0 * t);
            let vignette = 1.0 - strength * 0.3 * sm;
            let fr = (cr * vignette).clamp(0.0, 1.0);
            let fg = (cg * vignette).clamp(0.0, 1.0);
            let fb = (cb * vignette).clamp(0.0, 1.0);
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(fr) * 255.0).round() as u8,
                (linear_to_srgb(fg) * 255.0).round() as u8,
                (linear_to_srgb(fb) * 255.0).round() as u8,
                px[3],
            ]));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// eye_strain シェーダ等価性テスト（PSNR ≥ 30 dB）
// ---------------------------------------------------------------------------
// 注意: dry_eye は空間ランダム処理（タイルごとにランダム blur radius）のため、
// GLSL シェーダと CPU の1:1比較は意味を持たない（シェーダ側も同一乱数で同じ
// タイルパターンを再現する必要があり、実用上不可能）。そのため dry_eye の
// シェーダ等価性テストは省略する。

#[test]
fn shader_equiv_eye_strain_strength_1_0_psnr() {
    // CPU と GLSL シミュレータの一致を PSNR ≥ 30 dB で確認
    let img = make_test_image();
    let cpu_out = eye_strain(img.clone(), 1.0).unwrap().to_rgba8();
    let glsl_out = simulate_eye_strain_glsl(&img, 1.0);
    let db = psnr(&cpu_out, &glsl_out);
    assert!(db >= 30.0, "eye_strain PSNR {db:.1} dB < 30 dB");
}

// ---------------------------------------------------------------------------
// strength=0.0 → 変化なし（全フィルタ共通の境界値テスト）
// ---------------------------------------------------------------------------

#[test]
fn shader_equiv_strength_zero_no_change() {
    // strength=0 のとき CPU 実装は元画像を返す。
    // シミュレータも strength=0 → 恒等変換なので最大誤差は 0 であるべき。
    let img = color_chart_32();

    for (name, cpu_result) in [
        ("protanopia", protanopia(img.clone(), 0.0)),
        ("deuteranopia", deuteranopia(img.clone(), 0.0)),
        ("tritanopia", tritanopia(img.clone(), 0.0)),
        ("achromatopsia", achromatopsia(img.clone(), 0.0)),
        ("glaucoma", glaucoma(img.clone(), 0.0, GlaucomaMode::Vignette)),
        ("tunnel_vision", tunnel_vision(img.clone(), 0.0)),
        ("macular_degeneration", macular_degeneration(img.clone(), 0.0)),
        ("hemianopia", hemianopia(img.clone(), 0.0, 0.5)),
    ] {
        let cpu_out = cpu_result.unwrap().to_rgba8();
        let orig = img.to_rgba8();
        let err = max_channel_error(&cpu_out, &orig);
        assert!(
            err <= 1,
            "{name} strength=0.0: output differs from input by {err}/255 > 1"
        );
    }
}

// ---------------------------------------------------------------------------
// 視野欠損フィルタシェーダシミュレータ
// ---------------------------------------------------------------------------

/// glaucoma.frag / tunnel_vision.frag の計算を Rust で再現する。
/// `inner_r`, `outer_r` を外から渡すことで両方に使用する。
/// `aspect` = width / height（シェーダの uAspect と同じ）。
fn sim_vignette_fov(img: &RgbaImage, strength: f32, inner_r: f32, outer_r: f32, aspect: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    // シェーダと同じ aspect 補正済みコーナー距離: sqrt((0.5*aspect)^2 + 0.5^2)
    let corner_dist = (0.5 * aspect * 0.5 * aspect + 0.5 * 0.5_f32).sqrt();
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            // UV 座標 (pixel center)
            let uv_x = (x as f32 + 0.5) / w as f32 - 0.5;
            let uv_y = (y as f32 + 0.5) / h as f32 - 0.5;
            // aspect 補正してから距離計算（シェーダと同じ）
            let dx = uv_x * aspect;
            let dy = uv_y;
            let d = dx.hypot(dy) / corner_dist;

            let t = ((d - inner_r) / (outer_r - inner_r)).clamp(0.0, 1.0);
            let fade = t * t * (3.0 - 2.0 * t);
            let mul = 1.0 - strength * fade;

            let px = img.get_pixel(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb((rl * mul).clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb((gl * mul).clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb((bl * mul).clamp(0.0, 1.0)) * 255.0).round() as u8,
                px[3],
            ]));
        }
    }
    out
}

/// macular_degeneration.frag の計算を Rust で再現する。
fn sim_macular_degeneration(img: &RgbaImage, strength: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let corner_dist = std::f32::consts::FRAC_1_SQRT_2;
    let inner_r = strength * 0.25;
    let outer_r = strength * 0.4;
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let uv_x = (x as f32 + 0.5) / w as f32 - 0.5;
            let uv_y = (y as f32 + 0.5) / h as f32 - 0.5;
            let d = (uv_x * uv_x + uv_y * uv_y).sqrt() / corner_dist;

            let range = (outer_r - inner_r).max(1e-5);
            let u_t = ((d - inner_r) / range).clamp(0.0, 1.0);
            let t = 1.0 - u_t * u_t * (3.0 - 2.0 * u_t);

            let px = img.get_pixel(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);

            let lum = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
            let darkened = lum * (1.0 - strength * 0.95);
            let out_r = (rl + (darkened - rl) * t).clamp(0.0, 1.0);
            let out_g = (gl + (darkened - gl) * t).clamp(0.0, 1.0);
            let out_b = (bl + (darkened - bl) * t).clamp(0.0, 1.0);

            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(out_r) * 255.0).round() as u8,
                (linear_to_srgb(out_g) * 255.0).round() as u8,
                (linear_to_srgb(out_b) * 255.0).round() as u8,
                px[3],
            ]));
        }
    }
    out
}

/// macular_degeneration.frag の計算を Rust で再現する（aspect 補正付き）。
/// GLSL シェーダと同じ `uvA = vec2(uv.x * aspect, uv.y)` を使用。
fn sim_macular_degeneration_aspect(img: &RgbaImage, strength: f32, aspect: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let uv_x = (x as f32 + 0.5) / w as f32 - 0.5;
            let uv_y = (y as f32 + 0.5) / h as f32 - 0.5;
            // GLSL と同じ aspect 補正: uvA = vec2(uv.x * aspect, uv.y)
            let ua_x = uv_x * aspect;
            let ua_y = uv_y;
            let corner_dist = (0.5 * aspect * 0.5 * aspect + 0.5 * 0.5_f32).sqrt();
            let d = (ua_x * ua_x + ua_y * ua_y).sqrt() / corner_dist;

            let inner_r = strength * 0.25;
            let outer_r = strength * 0.4;
            let range = (outer_r - inner_r).max(1e-5);
            let u_t = ((d - inner_r) / range).clamp(0.0, 1.0);
            let t = 1.0 - u_t * u_t * (3.0 - 2.0 * u_t);

            let px = img.get_pixel(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);

            let lum = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
            let darkened = lum * (1.0 - strength * 0.95);
            let out_r = (rl + (darkened - rl) * t).clamp(0.0, 1.0);
            let out_g = (gl + (darkened - gl) * t).clamp(0.0, 1.0);
            let out_b = (bl + (darkened - bl) * t).clamp(0.0, 1.0);

            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(out_r) * 255.0).round() as u8,
                (linear_to_srgb(out_g) * 255.0).round() as u8,
                (linear_to_srgb(out_b) * 255.0).round() as u8,
                px[3],
            ]));
        }
    }
    out
}

/// hemianopia.frag の計算を Rust で再現する。
/// `side_glsl`: 1.0=右側欠損, -1.0=左側欠損（GLSL uSide 値）
fn sim_hemianopia(img: &RgbaImage, strength: f32, side_glsl: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let w_f = w as f32;
    let blur_w = w_f * 0.02; // vision.rs と同じ pixel 単位
    let split_x = w_f * 0.5;
    // GLSL uSide: 1.0=右欠損, -1.0=左欠損 → vision.rs side: 1.0=右欠損, 0.0=左欠損
    let side = (side_glsl + 1.0) * 0.5; // [-1,1] → [0,1]
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let xf = x as f32;

            let left_fade = if xf < split_x - blur_w {
                1.0_f32
            } else if xf > split_x + blur_w {
                0.0_f32
            } else {
                let t = (xf - (split_x - blur_w)) / (2.0 * blur_w);
                1.0 - t * t * (3.0 - 2.0 * t)
            };

            // vision.rs: fade = lerp(left_fade, 1-left_fade, side)
            let fade = left_fade + (1.0 - left_fade - left_fade) * side;
            let mul = 1.0 - fade * strength;

            let px = img.get_pixel(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb((rl * mul).clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb((gl * mul).clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb((bl * mul).clamp(0.0, 1.0)) * 255.0).round() as u8,
                px[3],
            ]));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 視野欠損フィルタ等価性テスト（PSNR ≥ 30 dB）
// ---------------------------------------------------------------------------

#[test]
fn shader_equiv_glaucoma_strength_1_0_psnr() {
    let img = gradient_32();
    let u = glaucoma_uniforms(1.0, 32, 32);
    let inner_r = 1.0 - u.strength * 0.7;
    let outer_r = (inner_r + 0.2_f32).min(1.0);
    let cpu_out = glaucoma(img.clone(), 1.0, GlaucomaMode::Vignette).unwrap().to_rgba8();
    let gpu_sim = sim_vignette_fov(&img.to_rgba8(), u.strength, inner_r, outer_r, 1.0);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "glaucoma strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_glaucoma_strength_0_5_psnr() {
    let img = color_chart_32();
    let u = glaucoma_uniforms(0.5, 32, 32);
    let inner_r = 1.0 - u.strength * 0.7;
    let outer_r = (inner_r + 0.2_f32).min(1.0);
    let cpu_out = glaucoma(img.clone(), 0.5, GlaucomaMode::Vignette).unwrap().to_rgba8();
    let gpu_sim = sim_vignette_fov(&img.to_rgba8(), u.strength, inner_r, outer_r, 1.0);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "glaucoma strength=0.5: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_macular_degeneration_strength_1_0_psnr() {
    let img = gradient_32();
    let u = macular_degeneration_uniforms(1.0, 32, 32);
    let cpu_out = macular_degeneration(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_macular_degeneration(&img.to_rgba8(), u.strength);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "macular_degeneration strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_macular_degeneration_strength_0_5_psnr() {
    let img = color_chart_32();
    let u = macular_degeneration_uniforms(0.5, 32, 32);
    let cpu_out = macular_degeneration(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_macular_degeneration(&img.to_rgba8(), u.strength);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "macular_degeneration strength=0.5: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_hemianopia_right_strength_1_0_psnr() {
    let img = gradient_32();
    let u = hemianopia_uniforms(1.0, 1.0); // 右側欠損
    let cpu_out = hemianopia(img.clone(), 1.0, 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_hemianopia(&img.to_rgba8(), u.strength, u.side);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "hemianopia right strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_hemianopia_left_strength_1_0_psnr() {
    let img = gradient_32();
    let u = hemianopia_uniforms(1.0, -1.0); // 左側欠損
    // vision::hemianopia は side=0.0 で左欠損
    let cpu_out = hemianopia(img.clone(), 1.0, 0.0).unwrap().to_rgba8();
    let gpu_sim = sim_hemianopia(&img.to_rgba8(), u.strength, u.side);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "hemianopia left strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_tunnel_vision_strength_1_0_psnr() {
    let img = gradient_32();
    let u = tunnel_vision_uniforms(1.0, 32, 32);
    let inner_r = (1.0 - u.strength) * 0.5;
    let outer_r = (inner_r + 0.05_f32).min(1.0);
    let cpu_out = tunnel_vision(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_vignette_fov(&img.to_rgba8(), u.strength, inner_r, outer_r, 1.0);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "tunnel_vision strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_tunnel_vision_strength_0_5_psnr() {
    let img = color_chart_32();
    let u = tunnel_vision_uniforms(0.5, 32, 32);
    let inner_r = (1.0 - u.strength) * 0.5;
    let outer_r = (inner_r + 0.05_f32).min(1.0);
    let cpu_out = tunnel_vision(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_vignette_fov(&img.to_rgba8(), u.strength, inner_r, outer_r, 1.0);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "tunnel_vision strength=0.5: PSNR {db:.1} dB < 30 dB");
}

// ---------------------------------------------------------------------------
// tetrachromacy シェーダ等価性テスト（PSNR ≥ 30 dB）
// GPU: tetrachromacy.frag — LMS 変換 + Cb/Cr 誇張（CPU と同一ロジック）
// ---------------------------------------------------------------------------

/// tetrachromacy シェーダ（tetrachromacy.frag）を Rust でシミュレートする。
fn sim_tetrachromacy(img: &RgbaImage, strength: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let orig = img.get_pixel(x, y);
            let r = srgb_to_linear(orig[0] as f32 / 255.0);
            let g = srgb_to_linear(orig[1] as f32 / 255.0);
            let b = srgb_to_linear(orig[2] as f32 / 255.0);
            let l_cone = 0.4002 * r + 0.7076 * g + (-0.0808) * b;
            let m_cone = (-0.2263) * r + 1.1653 * g + 0.0457 * b;
            let delta = m_cone - l_cone;
            let rg = r - g;
            const K_RG: f32 = 0.5;
            let (nr, ng, nb) = if delta.abs() < 0.05 {
                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let cb = b - luma;
                let cr = r - luma;
                let scale = strength * 2.0;
                (
                    (luma + cr * scale).clamp(0.0, 1.0),
                    luma.clamp(0.0, 1.0),
                    (luma + cb * scale).clamp(0.0, 1.0),
                )
            } else {
                (
                    (r + strength * rg * K_RG).clamp(0.0, 1.0),
                    (g - strength * rg * K_RG).clamp(0.0, 1.0),
                    b,
                )
            };
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(nr) * 255.0).round() as u8,
                (linear_to_srgb(ng) * 255.0).round() as u8,
                (linear_to_srgb(nb) * 255.0).round() as u8,
                orig[3],
            ]));
        }
    }
    out
}

#[test]
fn shader_equiv_tetrachromacy_strength_1_0_psnr() {
    let img = gradient_32();
    let u = tetrachromacy_uniforms(1.0);
    let cpu_out = tetrachromacy(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_tetrachromacy(&img.to_rgba8(), u.strength);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "tetrachromacy strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_tetrachromacy_strength_0_psnr() {
    let img = gradient_32();
    let _u = tetrachromacy_uniforms(0.0);
    let cpu_out = tetrachromacy(img.clone(), 0.0).unwrap().to_rgba8();
    let orig = img.to_rgba8();
    let db = psnr(&cpu_out, &orig);
    assert!(db >= 30.0, "tetrachromacy strength=0: PSNR {db:.1} dB < 30 dB");
}

// ---------------------------------------------------------------------------
// vestibular_neuritis シェーダ等価性テスト（PSNR ≥ 30 dB）
// GPU: vestibular_neuritis.frag — 水平シフト + 1D blur
// CPU: vision::vestibular_neuritis — 同構造
// ---------------------------------------------------------------------------

/// vestibular_neuritis シェーダを Rust でシミュレートする。
fn sim_vestibular_neuritis(img: &RgbaImage, radius_px: f32, shift_texel: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let texel_w = 1.0 / w as f32;
    let sample = |img: &RgbaImage, u: f32, v: f32| -> [f32; 3] {
        let px_x = ((u * w as f32).round() as i32).clamp(0, w as i32 - 1) as u32;
        let px_y = ((v * h as f32).round() as i32).clamp(0, h as i32 - 1) as u32;
        let px = img.get_pixel(px_x, px_y);
        [
            srgb_to_linear(px[0] as f32 / 255.0),
            srgb_to_linear(px[1] as f32 / 255.0),
            srgb_to_linear(px[2] as f32 / 255.0),
        ]
    };
    const N: usize = 16;
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let u_base = (x as f32 + 0.5) / w as f32 - shift_texel;
            let v = (y as f32 + 0.5) / h as f32;
            let u_base = u_base.clamp(0.0, 1.0);
            if radius_px < 0.5 {
                let px = img.get_pixel(
                    ((u_base * w as f32).round() as i32).clamp(0, w as i32 - 1) as u32,
                    y,
                );
                out.put_pixel(x, y, *px);
                continue;
            }
            let mut acc = [0f32; 3];
            for i in 0..N {
                let t = (i as f32 / (N - 1) as f32) * 2.0 - 1.0;
                let offset_u = t * radius_px * texel_w;
                let s = sample(img, (u_base + offset_u).clamp(0.0, 1.0), v);
                acc[0] += s[0];
                acc[1] += s[1];
                acc[2] += s[2];
            }
            let blurred = [acc[0] / N as f32, acc[1] / N as f32, acc[2] / N as f32];
            let src = img.get_pixel(x, y);
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(blurred[0].clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(blurred[1].clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(blurred[2].clamp(0.0, 1.0)) * 255.0).round() as u8,
                src[3],
            ]));
        }
    }
    out
}

#[test]
fn shader_equiv_vestibular_neuritis_strength_1_0_psnr() {
    let img = gradient_32();
    let u = vestibular_neuritis_uniforms(1.0, 32, 32);
    let cpu_out = vestibular_neuritis(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_vestibular_neuritis(&img.to_rgba8(), u.radius_px, u.shift_texel);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "vestibular_neuritis strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_vestibular_neuritis_strength_0_psnr() {
    let img = gradient_32();
    let _u = vestibular_neuritis_uniforms(0.0, 32, 32);
    let cpu_out = vestibular_neuritis(img.clone(), 0.0).unwrap().to_rgba8();
    let orig = img.to_rgba8();
    let db = psnr(&cpu_out, &orig);
    assert!(db >= 30.0, "vestibular_neuritis strength=0: PSNR {db:.1} dB < 30 dB");
}

// ---------------------------------------------------------------------------
// vertigo / bppv_rotation / floaters — コンパイルテストのみ（include_str!）
// ---------------------------------------------------------------------------

#[test]
fn shader_vertigo_glsl_is_not_empty() {
    use sensus_core::shaders::vertigo_glsl;
    assert!(!vertigo_glsl().is_empty());
}

#[test]
fn shader_bppv_rotation_glsl_is_not_empty() {
    use sensus_core::shaders::bppv_rotation_glsl;
    assert!(!bppv_rotation_glsl().is_empty());
}

#[test]
fn shader_floaters_glsl_is_not_empty() {
    use sensus_core::shaders::floaters_glsl;
    assert!(!floaters_glsl().is_empty());
}

#[test]
fn shader_contrast_sensitivity_glsl_is_not_empty() {
    use sensus_core::shaders::contrast_sensitivity_glsl;
    assert!(!contrast_sensitivity_glsl().is_empty());
}

#[test]
fn shader_detail_loss_glsl_is_not_empty() {
    use sensus_core::shaders::detail_loss_glsl;
    assert!(!detail_loss_glsl().is_empty());
}

#[test]
fn shader_teichopsia_glsl_is_not_empty() {
    use sensus_core::shaders::teichopsia_glsl;
    assert!(!teichopsia_glsl().is_empty());
}

#[test]
fn shader_flickering_stars_glsl_is_not_empty() {
    use sensus_core::shaders::flickering_stars_glsl;
    assert!(!flickering_stars_glsl().is_empty());
}

// ---------------------------------------------------------------------------
// N-1: 新4フィルタの shader_equivalence テスト
// ---------------------------------------------------------------------------

/// contrast_sensitivity シェーダ（contrast_sensitivity.frag）を Rust でシミュレート。
fn sim_contrast_sensitivity(img: &RgbaImage, strength: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    let scale = 1.0 - strength * 0.5;
    for y in 0..h {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            let r = srgb_to_linear(px[0] as f32 / 255.0);
            let g = srgb_to_linear(px[1] as f32 / 255.0);
            let b = srgb_to_linear(px[2] as f32 / 255.0);
            let nr = (0.5 + (r - 0.5) * scale).clamp(0.0, 1.0);
            let ng = (0.5 + (g - 0.5) * scale).clamp(0.0, 1.0);
            let nb = (0.5 + (b - 0.5) * scale).clamp(0.0, 1.0);
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(nr) * 255.0).round() as u8,
                (linear_to_srgb(ng) * 255.0).round() as u8,
                (linear_to_srgb(nb) * 255.0).round() as u8,
                px[3],
            ]));
        }
    }
    out
}

#[test]
fn shader_equiv_contrast_sensitivity_strength_0_identity() {
    use sensus_core::vision::contrast_sensitivity;
    let img = gradient_32();
    let cpu_out = contrast_sensitivity(img.clone(), 0.0).unwrap().to_rgba8();
    let gpu_sim = sim_contrast_sensitivity(&img.to_rgba8(), 0.0);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "contrast_sensitivity strength=0 identity: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_contrast_sensitivity_strength_0_5_psnr() {
    use sensus_core::vision::contrast_sensitivity;
    let img = color_chart_32();
    let cpu_out = contrast_sensitivity(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_contrast_sensitivity(&img.to_rgba8(), 0.5);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "contrast_sensitivity strength=0.5: PSNR {db:.1} dB < 30 dB");
}

/// detail_loss シェーダ（detail_loss.frag, 3×3サンプル近似）を Rust でシミュレート。
/// CPU は全タイル内平均、GPU シミュレータは3×3グリッドサンプル平均で近似する。
fn sim_detail_loss_shader(img: &RgbaImage, strength: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let tile_size = (strength * 20.0_f32).max(1.0);
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let px_x = x as f32;
            let px_y = y as f32;
            let tile_ox = (px_x / tile_size).floor() * tile_size;
            let tile_oy = (px_y / tile_size).floor() * tile_size;
            let center_px = (tile_ox + tile_size * 0.5, tile_oy + tile_size * 0.5);
            // 中心1点サンプリング（M-2: CPU/GPU 統一）
            let sx = (center_px.0.clamp(0.0, (w - 1) as f32)) as u32;
            let sy = (center_px.1.clamp(0.0, (h - 1) as f32)) as u32;
            let s = img.get_pixel(sx, sy);
            let lin_r = srgb_to_linear(s[0] as f32 / 255.0);
            let lin_g = srgb_to_linear(s[1] as f32 / 255.0);
            let lin_b = srgb_to_linear(s[2] as f32 / 255.0);
            let orig_alpha = img.get_pixel(x, y)[3];
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(lin_r.clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(lin_g.clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(lin_b.clamp(0.0, 1.0)) * 255.0).round() as u8,
                orig_alpha,
            ]));
        }
    }
    out
}

#[test]
fn shader_equiv_detail_loss_strength_0_identity() {
    use sensus_core::vision::detail_loss;
    let img = gradient_32();
    let cpu_out = detail_loss(img.clone(), 0.0).unwrap().to_rgba8();
    let orig = img.to_rgba8();
    let db = psnr(&cpu_out, &orig);
    assert!(db >= 60.0, "detail_loss strength=0 identity: PSNR {db:.1} dB < 60 dB");
}

#[test]
fn shader_equiv_detail_loss_cpu_gpu_psnr() {
    use sensus_core::vision::detail_loss;
    // strength=0.5 で CPU と GPU シミュレータが近い（PSNR ≥ 30 dB）
    let img = color_chart_32();
    let cpu_out = detail_loss(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader(&img.to_rgba8(), 0.5);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "detail_loss CPU/GPU strength=0.5: PSNR {db:.1} dB < 30 dB");
}

/// [M-2] detail_loss strength=1.0: CPU と GPU シミュレータが一致（PSNR ≥ 30 dB）
#[test]
fn shader_equiv_detail_loss_strength_1_psnr() {
    use sensus_core::vision::detail_loss;
    let img = color_chart_32();
    let cpu_out = detail_loss(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader(&img.to_rgba8(), 1.0);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "detail_loss CPU/GPU strength=1.0: PSNR {db:.1} dB < 30 dB");
}

/// teichopsia コンパイルテスト + strength=0 で元画像と近い（PSNR ≥ 25 dB）
#[test]
fn shader_equiv_teichopsia_strength_0_near_identity() {
    use sensus_core::vision::teichopsia;
    let img = gradient_32();
    let cpu_out = teichopsia(img.clone(), 0.0).unwrap().to_rgba8();
    let orig = img.to_rgba8();
    let db = psnr(&cpu_out, &orig);
    assert!(db >= 25.0, "teichopsia strength=0 near identity: PSNR {db:.1} dB < 25 dB");
}

#[test]
fn shader_teichopsia_glsl_compiles() {
    use sensus_core::shaders::teichopsia_glsl;
    // コンパイルテスト: glsl ソースが空でないこと
    assert!(!teichopsia_glsl().is_empty());
}

/// [M-1] teichopsia strength=0.5: CPU と GLSL シミュレータが近い（PSNR ≥ 25 dB）
/// GLSL は y / uAspect 方式、CPU も同じ方式に揃えたことを確認する。
fn sim_teichopsia(img: &RgbaImage, strength: f32) -> RgbaImage {
    use std::f32::consts::PI;
    let (w, h) = img.dimensions();
    let aspect = w as f32 / h as f32;
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            let rl = srgb_to_linear(px[0] as f32 / 255.0);
            let gl = srgb_to_linear(px[1] as f32 / 255.0);
            let bl = srgb_to_linear(px[2] as f32 / 255.0);

            let uv_x = (x as f32 / w as f32) - 0.5;
            let uv_y = (y as f32 / h as f32) - 0.5;
            // GLSL: vec2 uvA = vec2(uv.x, uv.y / uAspect)
            let ua_x = uv_x;
            let ua_y = uv_y / aspect;
            let dist = (ua_x * ua_x + ua_y * ua_y).sqrt();

            let (nr, ng, nb) = if dist < 0.2 {
                let dark = 1.0 - strength * 0.7 * (1.0 - dist / 0.2);
                (rl * dark, gl * dark, bl * dark)
            } else if dist <= 0.5 {
                let angle = ua_y.atan2(ua_x);
                let saw = (angle / PI * 8.0).fract();
                let ring_t = (dist - 0.2) / 0.3;
                let fade = (ring_t * (1.0 - ring_t) * 4.0).clamp(0.0, 1.0);
                let brightness = saw * strength * fade * 0.6;
                ((rl + brightness).clamp(0.0, 1.0), (gl + brightness).clamp(0.0, 1.0), (bl + brightness).clamp(0.0, 1.0))
            } else {
                (rl, gl, bl)
            };

            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(nr.clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(ng.clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(nb.clamp(0.0, 1.0)) * 255.0).round() as u8,
                px[3],
            ]));
        }
    }
    out
}

#[test]
fn shader_equiv_teichopsia_strength_05_psnr() {
    use sensus_core::vision::teichopsia;
    let img = color_chart_32();
    let cpu_out = teichopsia(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_teichopsia(&img.to_rgba8(), 0.5);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 25.0, "teichopsia CPU/GPU strength=0.5: PSNR {db:.1} dB < 25 dB");
}

/// flickering_stars: コンパイルテスト（ランダム描画なので等価テストは行わない）
#[test]
fn shader_flickering_stars_glsl_compiles_and_not_empty() {
    use sensus_core::shaders::flickering_stars_glsl;
    assert!(!flickering_stars_glsl().is_empty());
}

// ---------------------------------------------------------------------------
// [S-1] 非正方形（64×32）の vignette_fov テスト
// aspect 補正が正しく機能すれば CPU と GPU シミュレータの PSNR ≥ 30 dB
// ---------------------------------------------------------------------------

/// 64×32 の非正方形グラデーション画像を生成する。
fn gradient_64x32() -> DynamicImage {
    let mut img = RgbaImage::new(64, 32);
    for y in 0..32u32 {
        for x in 0..64u32 {
            let v = (x * 4) as u8;
            img.put_pixel(x, y, image::Rgba([v, v / 2, 255 - v, 255]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

#[test]
fn shader_equiv_glaucoma_non_square_64x32_psnr() {
    // 非正方形（width=64, height=32, aspect=2.0）で aspect 補正が機能することを確認
    let img = gradient_64x32();
    let u = glaucoma_uniforms(1.0, 64, 32);
    let inner_r = 1.0 - u.strength * 0.7;
    let outer_r = (inner_r + 0.2_f32).min(1.0);
    let cpu_out = glaucoma(img.clone(), 1.0, GlaucomaMode::Vignette).unwrap().to_rgba8();
    let aspect = 64.0_f32 / 32.0_f32; // 2.0
    let gpu_sim = sim_vignette_fov(&img.to_rgba8(), u.strength, inner_r, outer_r, aspect);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "glaucoma non-square 64×32: PSNR {db:.1} dB < 30 dB");
}

// ---------------------------------------------------------------------------
// [N-2] photophobia コンパイルテスト
// ---------------------------------------------------------------------------

#[test]
fn shader_photophobia_glsl_is_not_empty() {
    use sensus_core::shaders::photophobia_glsl;
    assert!(!photophobia_glsl().is_empty());
}

// ---------------------------------------------------------------------------
// [N-3] teichopsia / macular_degeneration / tunnel_vision の非正方形テスト
// 64×32 の非正方形画像で aspect 補正が機能することを確認
// ---------------------------------------------------------------------------

#[test]
fn shader_equiv_teichopsia_non_square_psnr() {
    use sensus_core::vision::teichopsia;
    // 64×32 の非正方形グラデーション画像
    let img = gradient_64x32();
    let cpu_out = teichopsia(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_teichopsia(&img.to_rgba8(), 0.5);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 25.0, "teichopsia non-square 64×32: PSNR {db:.1} dB < 25 dB");
}

#[test]
fn shader_equiv_macular_degeneration_non_square_psnr() {
    // 64×32 の非正方形グラデーション画像で aspect 補正付きシミュレータを使う
    let img = gradient_64x32();
    let u = macular_degeneration_uniforms(1.0, 64, 32);
    let cpu_out = macular_degeneration(img.clone(), 1.0).unwrap().to_rgba8();
    // aspect 補正付きシミュレータで GPU 側の動作を再現
    let gpu_sim = sim_macular_degeneration_aspect(&img.to_rgba8(), u.strength, u.aspect);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "macular_degeneration non-square 64×32: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_tunnel_vision_non_square_psnr() {
    // 64×32 の非正方形グラデーション画像
    let img = gradient_64x32();
    let u = tunnel_vision_uniforms(1.0, 64, 32);
    let inner_r = (1.0 - u.strength) * 0.5;
    let outer_r = (inner_r + 0.05_f32).min(1.0);
    let cpu_out = tunnel_vision(img.clone(), 1.0).unwrap().to_rgba8();
    let aspect = 64.0_f32 / 32.0_f32; // 2.0
    let gpu_sim = sim_vignette_fov(&img.to_rgba8(), u.strength, inner_r, outer_r, aspect);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 30.0, "tunnel_vision non-square 64×32: PSNR {db:.1} dB < 30 dB");
}

// ---------------------------------------------------------------------------
// [M-2] cataract の PSNR テスト（ノイズ含むため PSNR ≥ 25 dB 許容）
// ---------------------------------------------------------------------------

#[test]
fn shader_equiv_cataract_strength_zero_psnr() {
    use sensus_core::shaders::{cataract_glsl, cataract_uniforms};
    use sensus_core::vision::cataract;
    // strength=0 は identity: CPU と GPU シミュレータ（入力をそのまま返す）が一致するはず
    let img = gradient_32();
    let _u = cataract_uniforms(0.0, 42);
    let cpu_out = cataract(img.clone(), 0.0, 42).unwrap().to_rgba8();
    // strength=0 なので入力と完全一致のはず
    let db = psnr(&cpu_out, &img.to_rgba8());
    assert!(db >= 40.0, "cataract strength=0: PSNR {db:.1} dB < 40 dB (should be identity)");
    // シェーダ文字列が空でないこともチェック
    assert!(!cataract_glsl().is_empty());
}

// ---------------------------------------------------------------------------
// S-2: apply(Filter::DetailLoss) 経由のテスト
// ---------------------------------------------------------------------------

/// detail_loss シェーダ（detail_loss.frag, 中心点サンプリング）を、タイルサイズ = cell_size
/// 直接指定で Rust シミュレートする。apply(Filter::DetailLoss) 経路（detail_loss_with_cell_size）
/// の等価検証用（kako-jun/sensus#96）。
fn sim_detail_loss_shader_cell(img: &RgbaImage, cell_size: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let tile_size = cell_size.max(1) as f32;
    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let tile_ox = (x as f32 / tile_size).floor() * tile_size;
            let tile_oy = (y as f32 / tile_size).floor() * tile_size;
            let center_px = (tile_ox + tile_size * 0.5, tile_oy + tile_size * 0.5);
            let sx = (center_px.0.clamp(0.0, (w - 1) as f32)) as u32;
            let sy = (center_px.1.clamp(0.0, (h - 1) as f32)) as u32;
            let s = img.get_pixel(sx, sy);
            let lin_r = srgb_to_linear(s[0] as f32 / 255.0);
            let lin_g = srgb_to_linear(s[1] as f32 / 255.0);
            let lin_b = srgb_to_linear(s[2] as f32 / 255.0);
            let orig_alpha = img.get_pixel(x, y)[3];
            out.put_pixel(x, y, image::Rgba([
                (linear_to_srgb(lin_r.clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(lin_g.clamp(0.0, 1.0)) * 255.0).round() as u8,
                (linear_to_srgb(lin_b.clamp(0.0, 1.0)) * 255.0).round() as u8,
                orig_alpha,
            ]));
        }
    }
    out
}

/// kako-jun/sensus#96: apply(Filter::DetailLoss) が実際に呼ぶ detail_loss_with_cell_size を
/// GLSL シェーダ（中心点サンプリング）と等価検証する。以前は同関数が全平均で、公開 API 経路が
/// シェーダとも検証済み関数とも異なる出力を出していた。
#[test]
fn shader_equiv_apply_detail_loss_cpu_gpu_psnr() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = color_chart_32();
    // tile_size に半端な境界を含むよう cell_size=7 を使う（32 / 7 でタイルがはみ出す）
    let cpu_out = detail_loss_with_cell_size(img.clone(), 1.0, 7).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader_cell(&img.to_rgba8(), 7);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 60.0, "apply(DetailLoss) cell_size=7 CPU/GPU: PSNR {db:.1} dB < 60 dB");
}

#[test]
fn shader_equiv_apply_detail_loss_cell_size_20_psnr() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = color_chart_32();
    let cpu_out = detail_loss_with_cell_size(img.clone(), 1.0, 20).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader_cell(&img.to_rgba8(), 20);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 60.0, "apply(DetailLoss) cell_size=20 CPU/GPU: PSNR {db:.1} dB < 60 dB");
}

#[test]
fn apply_detail_loss_strength_0_identity() {
    use sensus_core::{apply, Filter};
    let img = gradient_32();
    // cell_size=1 のとき早期リターンで identity
    let out = apply(Filter::DetailLoss { cell_size: 1 }, img.clone(), 0.0).unwrap().to_rgba8();
    let orig = img.to_rgba8();
    let db = psnr(&out, &orig);
    assert!(db >= 60.0, "apply(DetailLoss, cell_size=1) should be identity: PSNR {db:.1} dB < 60 dB");
}

#[test]
fn apply_detail_loss_strength_1_runs_without_crash() {
    use sensus_core::{apply, Filter};
    let img = color_chart_32();
    let out = apply(Filter::DetailLoss { cell_size: 20 }, img.clone(), 1.0).unwrap().to_rgba8();
    // 出力が入力と異なること（詳細消失フィルタが適用されている）
    assert_ne!(out.as_raw(), img.to_rgba8().as_raw(), "apply(DetailLoss) strength=1 should change image");
}

// ---------------------------------------------------------------------------
// S-3: cataract strength=1.0 のクラッシュ/動作確認テスト
// ---------------------------------------------------------------------------

#[test]
fn shader_cataract_strength_1_runs_without_crash() {
    use sensus_core::vision::cataract;
    let img = gradient_32();
    let out = cataract(img.clone(), 1.0, 42).unwrap().to_rgba8();
    // 出力が入力と異なること（白内障フィルタが適用されている）
    assert_ne!(out.as_raw(), img.to_rgba8().as_raw(), "cataract strength=1 should change image");
}
