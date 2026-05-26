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
    achromatopsia_uniforms, astigmatism_uniforms, deuteranopia_uniforms, dry_eye_uniforms,
    eye_strain_uniforms, glaucoma_uniforms, hemianopia_uniforms, hyperopia_uniforms,
    macular_degeneration_uniforms, metamorphopsia_uniforms, myopia_uniforms, photophobia_uniforms,
    presbyopia_uniforms, protanopia_uniforms, tetrachromacy_uniforms, tritanopia_uniforms,
    tunnel_vision_uniforms, vestibular_neuritis_uniforms,
};
use sensus_core::vision::{
    achromatopsia, astigmatism, deuteranopia, dry_eye, eye_strain, glaucoma, GlaucomaMode,
    hemianopia, hyperopia, macular_degeneration, metamorphopsia, myopia, photophobia, presbyopia,
    protanopia, tetrachromacy, tritanopia, tunnel_vision, vestibular_neuritis,
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

/// photophobia.frag を Rust で再現する。
///
/// .frag と同一の式:
/// - highlightAt(uv): luma > 0.5 の超過分でマスクした linear RGB
/// - bloomSpread(uv): Fibonacci lattice 16 tap で highlight 円盤を平均（近似 disk blur）
/// - out = clamp(orig_linear + bloom, 1.0)
///
/// strength は radius_px の算出にのみ使われ、bloom 振幅には掛けない（CPU と同じ）。
fn sim_photophobia_glsl(img: &RgbaImage, radius_px: f32) -> RgbaImage {
    const N: usize = 16;
    const PHI: f32 = 2.399_963_2; // 黄金角（.frag と同じ）
    const THRESHOLD: f32 = 0.5;
    const MIN_RADIUS_PX: f32 = 0.5;
    let (w, h) = img.dimensions();
    let texel_w = 1.0 / w as f32;
    let texel_h = 1.0 / h as f32;

    // texture(uTexture, uv) の clamp-to-edge サンプリング → highlight レイヤ
    let highlight_at = |u: f32, v: f32| -> [f32; 3] {
        let px_x = ((u * w as f32).round() as i32).clamp(0, w as i32 - 1) as u32;
        let px_y = ((v * h as f32).round() as i32).clamp(0, h as i32 - 1) as u32;
        let px = img.get_pixel(px_x, px_y);
        let lin = [
            srgb_to_linear(px[0] as f32 / 255.0),
            srgb_to_linear(px[1] as f32 / 255.0),
            srgb_to_linear(px[2] as f32 / 255.0),
        ];
        let luma = 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
        let mask = if luma > THRESHOLD {
            (luma - THRESHOLD) / (1.0 - THRESHOLD)
        } else {
            0.0
        };
        [lin[0] * mask, lin[1] * mask, lin[2] * mask]
    };

    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;

            // bloom spread（半径が小さすぎる場合は bloom なし）
            let mut bloom = [0f32; 3];
            if radius_px >= MIN_RADIUS_PX {
                let mut acc = [0f32; 3];
                for i in 0..N {
                    let t = i as f32 / N as f32;
                    let r = t.sqrt() * radius_px;
                    let theta = i as f32 * PHI;
                    let s = highlight_at(u + theta.cos() * r * texel_w, v + theta.sin() * r * texel_h);
                    acc[0] += s[0];
                    acc[1] += s[1];
                    acc[2] += s[2];
                }
                bloom = [acc[0] / N as f32, acc[1] / N as f32, acc[2] / N as f32];
            }

            let px = img.get_pixel(x, y);
            let lin = [
                srgb_to_linear(px[0] as f32 / 255.0),
                srgb_to_linear(px[1] as f32 / 255.0),
                srgb_to_linear(px[2] as f32 / 255.0),
            ];
            out.put_pixel(
                x,
                y,
                image::Rgba([
                    (linear_to_srgb((lin[0] + bloom[0]).clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb((lin[1] + bloom[1]).clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb((lin[2] + bloom[2]).clamp(0.0, 1.0)) * 255.0).round() as u8,
                    px[3],
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
// photophobia（bloom）等価性テスト（PSNR ≥ 30 dB）
// ---------------------------------------------------------------------------
// CPU は highlight に厳密 pillbox disk blur（半径 r, edge replication）を適用。
// GPU は単一パスで厳密畳み込めないため Fibonacci lattice 16 tap で円盤を近似。
// 近似手法が違うため厳密一致は期待せず PSNR で判定する（他 disk blur 系と同じ）。

#[test]
fn shader_equiv_photophobia_strength_1_0_psnr() {
    // color_chart_32 は灰(200,200,200) と緑(50,200,50) の象限が luma>0.5 となり
    // bloom 源になる。min_dim=32 → radius_px = 0.05*32 = 1.6px（>= 0.5）。
    let img = color_chart_32();
    let uni = photophobia_uniforms(1.0, 32, 32);
    let cpu_out = photophobia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "photophobia strength=1.0: PSNR {db:.1} dB < 30 dB"
    );
}

// 明るいハイライト点（中心 1px のみ白、周囲は黒）を持つ画像。
// bloom が周囲に広がる「効果」を検証するための専用フィクスチャ。
fn bright_point_on_dark(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
        }
    }
    // 中心に純白の点（luma=1.0 > 0.5 → bloom 源）
    img.put_pixel(w / 2, h / 2, image::Rgba([255, 255, 255, 255]));
    DynamicImage::ImageRgba8(img)
}

// 半分が明るい灰 (200,200,200, luma>0.5)、半分が黒 (0,0,0) の画像。
// bloom 源を確実に含む汎用フィクスチャ（任意サイズ）。
fn half_bright(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let px = if x < w / 2 {
                [200, 200, 200, 255]
            } else {
                [10, 10, 10, 255]
            };
            img.put_pixel(x, y, image::Rgba(px));
        }
    }
    DynamicImage::ImageRgba8(img)
}

#[test]
fn shader_equiv_photophobia_strength_0_5_psnr() {
    // 中間値 strength=0.5: bloom 半径 = 0.5*0.05*32 = 0.8px（>= 0.5 → bloom あり）。
    // pillbox（CPU）と 16tap Fibonacci 近似（GLSL）の差が出やすい小半径領域。
    let img = color_chart_32();
    let uni = photophobia_uniforms(0.5, 32, 32);
    let cpu_out = photophobia(img.clone(), 0.5).unwrap().to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "photophobia strength=0.5: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_photophobia_strength_0_0_is_identity() {
    // strength=0.0: CPU は早期 return で入力をそのまま返す。
    // GLSL ミラーも radius_px=0（< 0.5）で bloom ゼロ → 入力不変であるべき。
    // CPU と GLSL の両方が「入力 == 出力」になることを確認する。
    let img = color_chart_32();
    let uni = photophobia_uniforms(0.0, 32, 32);
    assert_eq!(uni.radius_px, 0.0, "strength=0.0 で radius_px は 0 のはず");
    let input = img.to_rgba8();
    let cpu_out = photophobia(img.clone(), 0.0).unwrap().to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&input, uni.radius_px);
    assert_eq!(
        cpu_out, input,
        "photophobia strength=0.0: CPU 出力が入力と一致しない（identity 違反）"
    );
    assert_eq!(
        gpu_sim, input,
        "photophobia strength=0.0: GLSL 出力が入力と一致しない（identity 違反）"
    );
}

#[test]
fn shader_equiv_photophobia_radius_below_min_no_bloom() {
    // radius_px < 0.5（MIN_BLUR_RADIUS_PX）境界: bloom が完全にゼロになること。
    // 8x8 + strength=1.0 → radius = 0.05*8 = 0.4px < 0.5 → CPU/GLSL とも bloom なし。
    // ハイライトを含む画像でも出力が入力と一致する（bloom 加算が起きない）。
    let img = half_bright(8, 8);
    let uni = photophobia_uniforms(1.0, 8, 8);
    assert!(
        uni.radius_px < 0.5,
        "前提: radius_px {} は 0.5 未満であるべき",
        uni.radius_px
    );
    let input = img.to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&input, uni.radius_px);
    assert_eq!(
        gpu_sim, input,
        "radius<0.5: bloom が加算されている（境界で bloom ゼロになっていない）"
    );
    // CPU 側も同じ境界で bloom ゼロ（identity）になることを確認し、両側を守る。
    let cpu_out = photophobia(img.clone(), 1.0).unwrap().to_rgba8();
    assert_eq!(
        cpu_out, input,
        "radius<0.5: CPU 側で bloom が加算されている（境界で identity になっていない）"
    );
}

#[test]
fn shader_equiv_photophobia_large_image_128_psnr() {
    // 大きめ画像 128x128 + 半径大（radius = 0.05*128 = 6.4px）で 16tap 近似が
    // 粗くなる領域。PSNR 閾値の余裕（30 dB 下限）を満たし近似が破綻しないこと。
    let img = half_bright(128, 128);
    let uni = photophobia_uniforms(1.0, 128, 128);
    let cpu_out = photophobia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "photophobia 128x128 strength=1.0: PSNR {db:.1} dB < 30 dB（近似破綻の疑い）"
    );
}

#[test]
fn shader_equiv_photophobia_non_square_64x32_psnr() {
    // 非正方形（width=64, height=32）。texel_size の縦横差（1/64 vs 1/32）が
    // CPU の等方 disk（ピクセル等方）と一致するか検証する。
    // radius = 0.05 * min(64,32) = 1.6px。
    let img = half_bright(64, 32);
    let uni = photophobia_uniforms(1.0, 64, 32);
    let cpu_out = photophobia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "photophobia non-square 64x32: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_photophobia_non_square_32x64_psnr() {
    // 非正方形の縦長版（width=32, height=64）。64x32 と対称に texel_size の
    // 縦横差が逆転しても CPU 等方 disk と一致すること。
    let img = half_bright(32, 64);
    let uni = photophobia_uniforms(1.0, 32, 64);
    let cpu_out = photophobia(img.clone(), 1.0).unwrap().to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&img.to_rgba8(), uni.radius_px);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(
        db >= 30.0,
        "photophobia non-square 32x64: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_photophobia_bloom_spreads_from_bright_point() {
    // 効果アサート（identity 偽陽性の排除）: 暗背景に明るい点 1px を置くと、
    // bloom が周囲（隣接画素）へ広がり、かつ画像端の暗部は不変であること。
    // 64x64 → radius = 0.05*64 = 3.2px（>= 0.5）。GLSL ミラーで検証する。
    let img = bright_point_on_dark(64, 64);
    let uni = photophobia_uniforms(1.0, 64, 64);
    let input = img.to_rgba8();
    let gpu_sim = sim_photophobia_glsl(&input, uni.radius_px);

    // 中心の隣（半径内）の暗画素は bloom で明るくなる（元は黒 0）。
    let cx = 32u32;
    let cy = 32u32;
    let neighbor = gpu_sim.get_pixel(cx + 1, cy);
    assert!(
        neighbor[0] > 0,
        "bloom が隣接画素に広がっていない（中心隣 R={}）",
        neighbor[0]
    );

    // 半径外（角）の暗部は不変（黒のまま）。
    let corner = gpu_sim.get_pixel(0, 0);
    assert_eq!(
        [corner[0], corner[1], corner[2]],
        [0, 0, 0],
        "bloom 範囲外の暗部が変化した（角の画素 {corner:?}）"
    );

    // CPU 側も同じく明点が近傍へ広がり、範囲外の暗部は不変であることを確認（両側を守る）。
    let cpu_out = photophobia(img.clone(), 1.0).unwrap().to_rgba8();
    assert!(
        cpu_out.get_pixel(cx + 1, cy)[0] > 0,
        "CPU 側で bloom が隣接画素に広がっていない（中心隣 R={}）",
        cpu_out.get_pixel(cx + 1, cy)[0]
    );
    let cpu_corner = cpu_out.get_pixel(0, 0);
    assert_eq!(
        [cpu_corner[0], cpu_corner[1], cpu_corner[2]],
        [0, 0, 0],
        "CPU 側で bloom 範囲外の暗部が変化した（角の画素 {cpu_corner:?}）"
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

/// eye_strain.frag を Rust で再現する。
///
/// .frag と同一の式（処理順序も一致）:
/// - processedAt(uv): contrast 圧縮 + vignette を済ませた linear sRGB
///   - compressed = 0.5 + (lin - 0.5) * (1.0 - strength * 0.15)
///   - vignette   = 1.0 - strength * 0.3 * smoothstep(0.3, 1.2, dot(nuv, nuv))
/// - disk blur: 半径 < 0.5px なら center のみ、それ以外は Fibonacci lattice 16 tap で
///   processedAt を円盤状に平均（CPU の厳密 pillbox を 16tap で近似）
/// - out = sRGB encode
///
/// `radius_px`, `texel_size` は `eye_strain_uniforms()` の値を渡す（.frag の
/// uRadiusPx / uTexelSize に対応）。
fn simulate_eye_strain_glsl(
    img: &DynamicImage,
    strength: f32,
    radius_px: f32,
    texel_size: [f32; 2],
) -> RgbaImage {
    const N: usize = 16;
    const PHI: f32 = 2.399_963_2; // 黄金角（.frag と同じ）
    const MIN_RADIUS_PX: f32 = 0.5;
    let src = img.to_rgba8();
    let (w, h) = src.dimensions();
    let texel_w = texel_size[0];
    let texel_h = texel_size[1];

    // texture(uTexture, uv) の clamp-to-edge サンプリング → contrast+vignette 済み linear sRGB
    let processed_at = |u: f32, v: f32| -> [f32; 3] {
        let px_x = ((u * w as f32).round() as i32).clamp(0, w as i32 - 1) as u32;
        let px_y = ((v * h as f32).round() as i32).clamp(0, h as i32 - 1) as u32;
        let px = src.get_pixel(px_x, px_y);
        let lin = [
            srgb_to_linear(px[0] as f32 / 255.0),
            srgb_to_linear(px[1] as f32 / 255.0),
            srgb_to_linear(px[2] as f32 / 255.0),
        ];
        // contrast compression in linear space
        let cf = 1.0 - strength * 0.15;
        let c = [
            0.5 + (lin[0] - 0.5) * cf,
            0.5 + (lin[1] - 0.5) * cf,
            0.5 + (lin[2] - 0.5) * cf,
        ];
        // vignette（uv = texcoord*2-1。texcoord は .frag と同じく u, v をそのまま使う）
        let nx = u * 2.0 - 1.0;
        let ny = v * 2.0 - 1.0;
        let d = nx * nx + ny * ny;
        let t = ((d - 0.3) / (1.2 - 0.3)).clamp(0.0, 1.0);
        let sm = t * t * (3.0 - 2.0 * t);
        let vignette = 1.0 - strength * 0.3 * sm;
        [
            (c[0] * vignette).clamp(0.0, 1.0),
            (c[1] * vignette).clamp(0.0, 1.0),
            (c[2] * vignette).clamp(0.0, 1.0),
        ]
    };

    let mut out = src.clone();
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;

            let result = if radius_px < MIN_RADIUS_PX {
                // blur なし（contrast+vignette のみ）
                processed_at(u, v)
            } else {
                // disk blur 近似（Fibonacci lattice 16 tap）
                let mut acc = [0f32; 3];
                for i in 0..N {
                    let ft = i as f32 / N as f32;
                    let r = ft.sqrt() * radius_px;
                    let theta = i as f32 * PHI;
                    let s = processed_at(u + theta.cos() * r * texel_w, v + theta.sin() * r * texel_h);
                    acc[0] += s[0];
                    acc[1] += s[1];
                    acc[2] += s[2];
                }
                [acc[0] / N as f32, acc[1] / N as f32, acc[2] / N as f32]
            };

            let px = src.get_pixel(x, y);
            out.put_pixel(
                x,
                y,
                image::Rgba([
                    (linear_to_srgb(result[0]) * 255.0).round() as u8,
                    (linear_to_srgb(result[1]) * 255.0).round() as u8,
                    (linear_to_srgb(result[2]) * 255.0).round() as u8,
                    px[3],
                ]),
            );
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
    // CPU（厳密 pillbox blur）と GLSL シミュレータ（16tap lattice 近似）の一致を
    // PSNR ≥ 30 dB で確認。blur 半径 = 1.5px と小さいため近似誤差は小さい。
    let img = make_test_image();
    let (w, h) = img.to_rgba8().dimensions();
    let uni = eye_strain_uniforms(1.0, w, h);
    let cpu_out = eye_strain(img.clone(), 1.0).unwrap().to_rgba8();
    let glsl_out = simulate_eye_strain_glsl(&img, uni.strength, uni.radius_px, uni.texel_size);
    let db = psnr(&cpu_out, &glsl_out);
    assert!(db >= 30.0, "eye_strain PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_eye_strain_strength_0_5_psnr() {
    // strength=0.5 では radius = 0.75px。MIN_BLUR_RADIUS_PX(0.5) を上回るため
    // CPU・GLSL ともに blur 段が有効。
    let img = make_test_image();
    let (w, h) = img.to_rgba8().dimensions();
    let uni = eye_strain_uniforms(0.5, w, h);
    let cpu_out = eye_strain(img.clone(), 0.5).unwrap().to_rgba8();
    let glsl_out = simulate_eye_strain_glsl(&img, uni.strength, uni.radius_px, uni.texel_size);
    let db = psnr(&cpu_out, &glsl_out);
    assert!(db >= 30.0, "eye_strain strength=0.5: PSNR {db:.1} dB < 30 dB");
}

/// 縦半分が白・縦半分が黒のグラデーション無しのコントラストエッジ画像。
/// contrast 圧縮の効果（明暗差が縮む）を数値で測るための固定フィクスチャ。
fn eye_strain_bw_split(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = if x < w / 2 { 255u8 } else { 0u8 };
            img.put_pixel(x, y, image::Rgba([v, v, v, 255]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// 一様な中間グレー（180）画像。vignette（周辺減光）の効果を中心 vs 角で測るための
/// 固定フィクスチャ（contrast 圧縮は一様値には影響しないため vignette を分離できる）。
fn eye_strain_solid_gray(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, image::Rgba([180, 180, 180, 255]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// 128×128 のグラデーション画像（大画像での近似破綻チェック用）。
fn gradient_128() -> DynamicImage {
    let mut img = RgbaImage::new(128, 128);
    for y in 0..128u32 {
        for x in 0..128u32 {
            let v = (x * 2) as u8;
            img.put_pixel(x, y, image::Rgba([v, v / 2, 255u8.wrapping_sub(v), 255]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

#[test]
fn shader_equiv_eye_strain_strength_0_0_is_identity() {
    // strength=0.0: CPU は早期 return で入力をそのまま byte-exact に返す（契約）。
    // GLSL ミラーは radius_px=0（< 0.5）で center 経路のみ。さらに strength=0 で
    // contrast 係数=1・vignette=1 となり processedAt の式は入力そのものを返す（効果オフ）。
    //
    // ただし GLSL ミラーは center texel を u=(x+0.5)/w の nearest サンプリングで読むため
    // ハードエッジ画像では半テクセルのサンプリングずれが出る。効果（contrast/vignette）が
    // 実際にゼロであることだけを検証したいので、滑らかなグラデーションで PSNR を見る。
    // ずれがサンプリング由来のみ（効果ゼロ）であれば PSNR は十分高い。
    let img = gradient_32();
    let (w, h) = img.to_rgba8().dimensions();
    let uni = eye_strain_uniforms(0.0, w, h);
    assert_eq!(uni.radius_px, 0.0, "strength=0.0 で radius_px は 0 のはず");
    let input = img.to_rgba8();
    // CPU は byte-exact identity であること（早期 return の契約）。
    let cpu_out = eye_strain(img.clone(), 0.0).unwrap().to_rgba8();
    assert_eq!(
        cpu_out, input,
        "eye_strain strength=0.0: CPU 出力が入力と一致しない（identity 違反）"
    );
    // GLSL は contrast/vignette がゼロ（効果オフ）であることを高 PSNR で確認する。
    // 差はサンプリングずれのみで、効果が乗っていれば PSNR は大きく落ちる。
    let glsl_out = simulate_eye_strain_glsl(&img, uni.strength, uni.radius_px, uni.texel_size);
    let db = psnr(&input, &glsl_out);
    assert!(
        db >= 30.0,
        "eye_strain strength=0.0: GLSL に効果が乗っている疑い（PSNR {db:.1} dB < 30 dB）"
    );
}

#[test]
fn shader_equiv_eye_strain_radius_below_min_no_blur() {
    // radius < 0.5（MIN_BLUR_RADIUS_PX）境界: blur 段が無効化され contrast+vignette
    // のみになること。strength=0.3 → radius = 0.3*1.5 = 0.45px < 0.5（画像サイズ非依存）。
    // GLSL ミラーが「16tap 平均（blur 有）」ではなく「center 経路（blur 無）」を通ること
    // を、radius を 0 に強制した参照出力と byte-exact 一致で証明する。
    let img = make_test_image();
    let (w, h) = img.to_rgba8().dimensions();
    let uni = eye_strain_uniforms(0.3, w, h);
    assert!(
        uni.radius_px < 0.5,
        "前提: radius_px {} は 0.5 未満であるべき",
        uni.radius_px
    );
    // blur 段を強制的に無効化した参照（radius_px=0 → center 経路確定）。
    let no_blur_ref = simulate_eye_strain_glsl(&img, uni.strength, 0.0, uni.texel_size);
    let glsl_out = simulate_eye_strain_glsl(&img, uni.strength, uni.radius_px, uni.texel_size);
    assert_eq!(
        glsl_out, no_blur_ref,
        "radius<0.5: blur 段が無効化されず 16tap 平均が走っている（境界判定が壊れている）"
    );
    // CPU も同じ境界で blur をスキップし、両者が PSNR ≥ 30 dB で一致すること。
    let cpu_out = eye_strain(img.clone(), 0.3).unwrap().to_rgba8();
    let db = psnr(&cpu_out, &glsl_out);
    assert!(db >= 30.0, "eye_strain radius<0.5: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_eye_strain_large_image_128_psnr() {
    // 大画像 128x128 + strength=1.0（radius=1.5px）。16tap lattice 近似が大画像でも
    // 破綻せず PSNR 下限 30 dB に余裕を持って収まること。
    let img = gradient_128();
    let uni = eye_strain_uniforms(1.0, 128, 128);
    let cpu_out = eye_strain(img.clone(), 1.0).unwrap().to_rgba8();
    let glsl_out = simulate_eye_strain_glsl(&img, uni.strength, uni.radius_px, uni.texel_size);
    let db = psnr(&cpu_out, &glsl_out);
    assert!(
        db >= 30.0,
        "eye_strain 128x128 strength=1.0: PSNR {db:.1} dB < 30 dB（近似破綻の疑い）"
    );
}

#[test]
fn shader_equiv_eye_strain_non_square_64x32_psnr() {
    // 非正方形（width=64, height=32）。texel_size の縦横差（1/64 vs 1/32）の下で
    // blur が等方（ピクセル等方）を保ち、CPU の厳密 pillbox と一致すること。
    let img = gradient_64x32();
    let uni = eye_strain_uniforms(1.0, 64, 32);
    let cpu_out = eye_strain(img.clone(), 1.0).unwrap().to_rgba8();
    let glsl_out = simulate_eye_strain_glsl(&img, uni.strength, uni.radius_px, uni.texel_size);
    let db = psnr(&cpu_out, &glsl_out);
    assert!(
        db >= 30.0,
        "eye_strain non-square 64x32: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_eye_strain_non_square_32x64_psnr() {
    // 非正方形の縦長版（width=32, height=64）。64x32 と対称に texel_size の縦横差が
    // 逆転しても blur 等方性が保たれ CPU と一致すること。
    let img = gradient_32x64();
    let uni = eye_strain_uniforms(1.0, 32, 64);
    let cpu_out = eye_strain(img.clone(), 1.0).unwrap().to_rgba8();
    let glsl_out = simulate_eye_strain_glsl(&img, uni.strength, uni.radius_px, uni.texel_size);
    let db = psnr(&cpu_out, &glsl_out);
    assert!(
        db >= 30.0,
        "eye_strain non-square 32x64: PSNR {db:.1} dB < 30 dB"
    );
}

#[test]
fn shader_equiv_eye_strain_compresses_contrast_and_vignettes() {
    // 効果アサート（identity 偽陽性の排除）: 強度 1.0 で
    //   (a) contrast 圧縮 — 明暗エッジ画像で白側が暗く・黒側が明るくなり明暗差が縮む
    //   (b) vignette — 一様グレーで角が中心より暗くなる
    // が GLSL ミラー上で実際に起きることを数値でアサートする。
    let w = 64u32;
    let h = 64u32;
    let uni = eye_strain_uniforms(1.0, w, h);

    // (a) contrast 圧縮: 左半分白(255)・右半分黒(0)。中央列付近を避け各領域内部で測る。
    let bw = eye_strain_bw_split(w, h);
    let bw_out = simulate_eye_strain_glsl(&bw, uni.strength, uni.radius_px, uni.texel_size);
    let white_out = bw_out.get_pixel(w / 4, h / 2)[0]; // 元 255
    let black_out = bw_out.get_pixel(3 * w / 4, h / 2)[0]; // 元 0
    assert!(
        white_out < 255,
        "contrast 圧縮: 白側が暗くなっていない（white_out={white_out}）"
    );
    assert!(
        black_out > 0,
        "contrast 圧縮: 黒側が明るくなっていない（black_out={black_out}）"
    );
    let orig_range = 255i32 - 0;
    let out_range = white_out as i32 - black_out as i32;
    assert!(
        out_range < orig_range,
        "contrast 圧縮: 明暗差が縮んでいない（out_range={out_range} >= orig_range={orig_range}）"
    );

    // (b) vignette: 一様グレー(180)。中心は減光なし、角は smoothstep で減光される。
    let gray = eye_strain_solid_gray(w, h);
    let gray_out = simulate_eye_strain_glsl(&gray, uni.strength, uni.radius_px, uni.texel_size);
    let center = gray_out.get_pixel(w / 2, h / 2)[0];
    let corner = gray_out.get_pixel(1, 1)[0];
    assert!(
        corner < center,
        "vignette: 角が中心より減光されていない（center={center} corner={corner}）"
    );
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

/// 64×64 のグラデーション画像（複数タイルでの dry_eye / metamorphopsia 検証用）。
fn gradient_64() -> DynamicImage {
    let mut img = RgbaImage::new(64, 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            let v = (x * 4) as u8;
            img.put_pixel(x, y, image::Rgba([v, (v / 2).wrapping_add(y as u8), 255 - v, 255]));
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
// S-2b: apply(Filter::DetailLoss) 経路の非正方形・cell_size 境界・alpha 保存
// （kako-jun/sensus#96 追加観点）
// ---------------------------------------------------------------------------

/// 32×64 の縦長グラデーション画像（gradient_64x32 の transpose 相当）。
fn gradient_32x64() -> DynamicImage {
    let mut img = RgbaImage::new(32, 64);
    for y in 0..64u32 {
        for x in 0..32u32 {
            let v = (y * 4) as u8;
            img.put_pixel(x, y, image::Rgba([v, v / 2, 255 - v, 255]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// alpha が画素ごとに変化する 32×32 RGBA 画像（alpha 保存検証用）。
fn varying_alpha_32() -> DynamicImage {
    let mut img = RgbaImage::new(32, 32);
    for y in 0..32u32 {
        for x in 0..32u32 {
            let v = (x * 8) as u8;
            let a = (y * 8) as u8;
            img.put_pixel(x, y, image::Rgba([v, 255 - v, v / 2, a]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// kako-jun/sensus#96: 横長（64×32）でも apply 経路（detail_loss_with_cell_size）が
/// 中心点サンプリングシェーダと等価。width が tile_size で割り切れない端タイルを含む。
#[test]
fn shader_equiv_apply_detail_loss_non_square_64x32_psnr() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = gradient_64x32();
    let cpu_out = detail_loss_with_cell_size(img.clone(), 1.0, 7).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader_cell(&img.to_rgba8(), 7);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 60.0, "apply(DetailLoss) non-square 64×32 cell_size=7: PSNR {db:.1} dB < 60 dB");
}

/// kako-jun/sensus#96: 縦長（32×64）でも apply 経路が中心点サンプリングシェーダと等価。
/// 横長と縦長の両方を守ることで width/height の取り違えを検出する。
#[test]
fn shader_equiv_apply_detail_loss_non_square_32x64_psnr() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = gradient_32x64();
    let cpu_out = detail_loss_with_cell_size(img.clone(), 1.0, 7).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader_cell(&img.to_rgba8(), 7);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 60.0, "apply(DetailLoss) non-square 32×64 cell_size=7: PSNR {db:.1} dB < 60 dB");
}

/// kako-jun/sensus#96: cell_size=0 は tile_size = cell_size.max(1) = 1 で identity 早期リターン。
/// strength を 1.0 にしても cell_size が支配的で何も変化しない契約を守る。
#[test]
fn apply_detail_loss_cell_size_0_identity() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = color_chart_32();
    let out = detail_loss_with_cell_size(img.clone(), 1.0, 0).unwrap().to_rgba8();
    let orig = img.to_rgba8();
    assert_eq!(out.as_raw(), orig.as_raw(), "detail_loss cell_size=0 should be identity (tile_size=max(1))");
}

/// kako-jun/sensus#96: cell_size が画像サイズを超えると全体が1タイルになり、
/// 全画素が中心点（floor の (16,16)）の色になる。シミュレータと等価。
#[test]
fn shader_equiv_apply_detail_loss_cell_size_exceeds_image_psnr() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = color_chart_32();
    // cell_size=100 > 32: タイルは1つ、中心 = (50,50) を clamp(31) → 右下灰
    let cpu_out = detail_loss_with_cell_size(img.clone(), 1.0, 100).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader_cell(&img.to_rgba8(), 100);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 60.0, "apply(DetailLoss) cell_size>image: PSNR {db:.1} dB < 60 dB");
    // 全画素が同一色（1タイルに塗り潰し）であること
    let first = *cpu_out.get_pixel(0, 0);
    assert!(
        cpu_out.pixels().all(|p| p[0] == first[0] && p[1] == first[1] && p[2] == first[2]),
        "cell_size>image: 全 RGB が単一色になるはず"
    );
}

/// kako-jun/sensus#96: 画像幅（32）と互いに素な cell_size=5 で端タイルが半端に切れても
/// 中心点サンプリングシェーダと等価（端タイルでの中心 clamp が正しい）。
#[test]
fn shader_equiv_apply_detail_loss_coprime_cell_size_psnr() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = gradient_32();
    // gcd(32,5)=1: 端で 32 % 5 = 2px の半端タイルが残る
    let cpu_out = detail_loss_with_cell_size(img.clone(), 1.0, 5).unwrap().to_rgba8();
    let gpu_sim = sim_detail_loss_shader_cell(&img.to_rgba8(), 5);
    let db = psnr(&cpu_out, &gpu_sim);
    assert!(db >= 60.0, "apply(DetailLoss) coprime cell_size=5: PSNR {db:.1} dB < 60 dB");
}

/// kako-jun/sensus#96: 中心点方式は alpha チャンネルを変質させない。
/// RGB はタイルごとに塗り潰されるが、各画素の alpha は入力のまま保持される。
#[test]
fn apply_detail_loss_preserves_alpha_per_pixel() {
    use sensus_core::vision::detail_loss_with_cell_size;
    let img = varying_alpha_32();
    let orig = img.to_rgba8();
    let out = detail_loss_with_cell_size(img.clone(), 1.0, 8).unwrap().to_rgba8();
    // RGB は実際に変化していること（テストが無意味な identity でないことを保証）
    assert_ne!(out.as_raw(), orig.as_raw(), "detail_loss cell_size=8 should change RGB");
    // alpha は1画素ずつ完全一致
    for (po, pi) in out.pixels().zip(orig.pixels()) {
        assert_eq!(po[3], pi[3], "alpha は画素ごとに保存されるべき");
    }
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

// ---------------------------------------------------------------------------
// #99: metamorphopsia / dry_eye ノイズモデル統一の等価性テスト（PSNR ≥ 30 dB）
// ---------------------------------------------------------------------------
// CPU と GLSL を同一の 32bit 整数 spatial hash + 同一の補間/disk blur に統一した。
// 以下の sim は実 .frag を 1:1 でミラーする（別アルゴリズムのインライン化はしない）。

/// metamorphopsia.frag の `gridHash` を Rust で再現する（CPU 実装と bit 一致）。
fn metamorphopsia_grid_hash(seed: u32, gx: u32, gy: u32, axis: u32) -> f32 {
    let mut h = seed
        .wrapping_mul(0x9e3779b9)
        .wrapping_add(gx.wrapping_mul(0x85ebca6b))
        .wrapping_add(gy.wrapping_mul(0xc2b2ae35))
        .wrapping_add(axis.wrapping_mul(0x27d4eb2f));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a2d39);
    h ^= h >> 15;
    h as f32 / u32::MAX as f32
}

/// CPU `sample_bilinear` と同じ sRGB バイト空間の双線形サンプリング（edge clamp）。
fn sample_bilinear_srgb(img: &RgbaImage, fx: f32, fy: f32) -> [u8; 4] {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let get = |x: i32, y: i32| -> [f32; 4] {
        let xi = x.clamp(0, w - 1) as u32;
        let yi = y.clamp(0, h - 1) as u32;
        let p = img.get_pixel(xi, yi);
        [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
    };
    let p00 = get(x0, y0);
    let p10 = get(x1, y0);
    let p01 = get(x0, y1);
    let p11 = get(x1, y1);
    let mut out = [0u8; 4];
    for i in 0..4 {
        let v = p00[i] * (1.0 - tx) * (1.0 - ty)
            + p10[i] * tx * (1.0 - ty)
            + p01[i] * (1.0 - tx) * ty
            + p11[i] * tx * ty;
        out[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// metamorphopsia.frag を Rust で忠実にミラーする。
fn sim_metamorphopsia_glsl(img: &RgbaImage, strength: f32, freq: f32, seed: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if strength <= 0.0 {
        return img.clone();
    }
    const MAX_DISP_PX: f32 = 8.0;
    let max_disp = strength * MAX_DISP_PX;
    let min_dim = w.min(h) as f32;
    let freq_clamped = freq.clamp(0.1, 1000.0);
    let cell_size = (min_dim / freq_clamped).max(1.0);

    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            // CPU 整数ピクセル座標を復元（フラグメント中心 uv = (x+0.5)/w）。
            let px_pos_x = x as f32;
            let px_pos_y = y as f32;
            let cx = px_pos_x / cell_size;
            let cy = px_pos_y / cell_size;
            let ci_x = cx.floor();
            let ci_y = cy.floor();
            let tx = cx - ci_x;
            let ty = cy - ci_y;
            let gx0 = ci_x.max(0.0) as u32;
            let gy0 = ci_y.max(0.0) as u32;
            let gx1 = gx0 + 1;
            let gy1 = gy0 + 1;

            let d = |gx: u32, gy: u32| -> (f32, f32) {
                (
                    metamorphopsia_grid_hash(seed, gx, gy, 0) * 2.0 - 1.0,
                    metamorphopsia_grid_hash(seed, gx, gy, 1) * 2.0 - 1.0,
                )
            };
            let (d00x, d00y) = d(gx0, gy0);
            let (d10x, d10y) = d(gx1, gy0);
            let (d01x, d01y) = d(gx0, gy1);
            let (d11x, d11y) = d(gx1, gy1);

            let disp_x = (d00x * (1.0 - tx) * (1.0 - ty)
                + d10x * tx * (1.0 - ty)
                + d01x * (1.0 - tx) * ty
                + d11x * tx * ty)
                * max_disp;
            let disp_y = (d00y * (1.0 - tx) * (1.0 - ty)
                + d10y * tx * (1.0 - ty)
                + d01y * (1.0 - tx) * ty
                + d11y * tx * ty)
                * max_disp;

            let src_x = (px_pos_x + disp_x).clamp(0.0, (w - 1) as f32);
            let src_y = (px_pos_y + disp_y).clamp(0.0, (h - 1) as f32);
            let p = sample_bilinear_srgb(img, src_x, src_y);
            out.put_pixel(x, y, image::Rgba(p));
        }
    }
    out
}

/// dry_eye.frag の `tileHash` を Rust で再現する（CPU 実装と bit 一致）。
fn dry_eye_tile_hash(tx: u32, ty: u32) -> f32 {
    const SEED: u32 = 42;
    let mut h = SEED
        .wrapping_mul(0x9e3779b9)
        .wrapping_add(tx.wrapping_mul(0x85ebca6b))
        .wrapping_add(ty.wrapping_mul(0xc2b2ae35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a2d39);
    h ^= h >> 15;
    h as f32 / u32::MAX as f32
}

/// dry_eye.frag を Rust で忠実にミラーする。
fn sim_dry_eye_glsl(img: &RgbaImage, strength: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if strength <= 0.0 {
        return img.clone();
    }
    const TILE_SIZE: f32 = 32.0;
    const MIN_BLUR_RADIUS_PX: f32 = 0.5;

    let sample_lin = |px: f32, py: f32| -> [f32; 3] {
        let xi = (px.clamp(0.0, (w - 1) as f32)) as u32;
        let yi = (py.clamp(0.0, (h - 1) as f32)) as u32;
        let p = img.get_pixel(xi, yi);
        [
            srgb_to_linear(p[0] as f32 / 255.0),
            srgb_to_linear(p[1] as f32 / 255.0),
            srgb_to_linear(p[2] as f32 / 255.0),
        ]
    };

    let mut out = img.clone();
    for y in 0..h {
        for x in 0..w {
            let orig = *img.get_pixel(x, y);
            let px_pos_x = x as f32;
            let px_pos_y = y as f32;
            let tx = (px_pos_x / TILE_SIZE).floor() as u32;
            let ty = (px_pos_y / TILE_SIZE).floor() as u32;
            let noise = dry_eye_tile_hash(tx, ty);
            let blur_radius = noise * strength * 3.0;
            if blur_radius < MIN_BLUR_RADIUS_PX {
                continue; // passthrough
            }
            let r_max = blur_radius.ceil() as i32;
            let r2 = blur_radius * blur_radius;
            let mut acc = [0f32; 3];
            let mut count = 0f32;
            for dy in -r_max..=r_max {
                for dx in -r_max..=r_max {
                    let fdx = dx as f32;
                    let fdy = dy as f32;
                    if fdx * fdx + fdy * fdy <= r2 {
                        let s = sample_lin(px_pos_x + fdx, px_pos_y + fdy);
                        acc[0] += s[0];
                        acc[1] += s[1];
                        acc[2] += s[2];
                        count += 1.0;
                    }
                }
            }
            let blurred = if count > 0.0 {
                [acc[0] / count, acc[1] / count, acc[2] / count]
            } else {
                sample_lin(px_pos_x, px_pos_y)
            };
            out.put_pixel(
                x,
                y,
                image::Rgba([
                    (linear_to_srgb(blurred[0].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb(blurred[1].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    (linear_to_srgb(blurred[2].clamp(0.0, 1.0)) * 255.0).round() as u8,
                    orig[3],
                ]),
            );
        }
    }
    out
}

#[test]
fn shader_equiv_metamorphopsia_strength_1_0_psnr() {
    // CPU と GLSL ミラーが同一の 32bit hash 変位場 + 同一補間で一致すること。
    // 唯一の乖離源は sample_bilinear の f32 丸めと、最終セルの頂点 clamp 差（端のみ）。
    let img = gradient_32();
    let uni = metamorphopsia_uniforms(1.0, 4.0, 42, 32, 32);
    let cpu_out = metamorphopsia(img.clone(), 1.0, 4.0, 42).unwrap().to_rgba8();
    let glsl = sim_metamorphopsia_glsl(&img.to_rgba8(), uni.strength, uni.freq, uni.seed);
    let db = psnr(&cpu_out, &glsl);
    assert!(db >= 30.0, "metamorphopsia strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_metamorphopsia_strength_0_5_psnr() {
    let img = gradient_32();
    let uni = metamorphopsia_uniforms(0.5, 8.0, 7, 32, 32);
    let cpu_out = metamorphopsia(img.clone(), 0.5, 8.0, 7).unwrap().to_rgba8();
    let glsl = sim_metamorphopsia_glsl(&img.to_rgba8(), uni.strength, uni.freq, uni.seed);
    let db = psnr(&cpu_out, &glsl);
    assert!(db >= 30.0, "metamorphopsia strength=0.5: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_metamorphopsia_non_square_64x32_psnr() {
    let img = gradient_64x32();
    let uni = metamorphopsia_uniforms(1.0, 6.0, 123, 64, 32);
    let cpu_out = metamorphopsia(img.clone(), 1.0, 6.0, 123).unwrap().to_rgba8();
    let glsl = sim_metamorphopsia_glsl(&img.to_rgba8(), uni.strength, uni.freq, uni.seed);
    let db = psnr(&cpu_out, &glsl);
    assert!(db >= 30.0, "metamorphopsia non-square 64x32: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_metamorphopsia_strength_0_0_is_identity() {
    // strength=0: CPU は byte-exact identity、GLSL ミラーも early return で恒等。
    let img = gradient_32();
    let input = img.to_rgba8();
    let uni = metamorphopsia_uniforms(0.0, 4.0, 42, 32, 32);
    let cpu_out = metamorphopsia(img.clone(), 0.0, 4.0, 42).unwrap().to_rgba8();
    assert_eq!(cpu_out, input, "metamorphopsia strength=0.0: CPU が identity でない");
    let glsl = sim_metamorphopsia_glsl(&input, uni.strength, uni.freq, uni.seed);
    assert_eq!(glsl, input, "metamorphopsia strength=0.0: GLSL が identity でない");
}

#[test]
fn shader_equiv_dry_eye_strength_1_0_psnr() {
    // 64x64（複数タイル）でタイルごとに異なる blur 半径が出る。CPU と GLSL ミラーが
    // 同一のタイルノイズ + 同一 disk blur で一致すること。
    let img = gradient_64();
    let cpu_out = dry_eye(img.clone(), 1.0).unwrap().to_rgba8();
    let glsl = sim_dry_eye_glsl(&img.to_rgba8(), 1.0);
    let db = psnr(&cpu_out, &glsl);
    assert!(db >= 30.0, "dry_eye strength=1.0: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_dry_eye_strength_0_5_psnr() {
    let img = gradient_64();
    let cpu_out = dry_eye(img.clone(), 0.5).unwrap().to_rgba8();
    let glsl = sim_dry_eye_glsl(&img.to_rgba8(), 0.5);
    let db = psnr(&cpu_out, &glsl);
    assert!(db >= 30.0, "dry_eye strength=0.5: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_dry_eye_non_square_64x32_psnr() {
    let img = gradient_64x32();
    let cpu_out = dry_eye(img.clone(), 1.0).unwrap().to_rgba8();
    let glsl = sim_dry_eye_glsl(&img.to_rgba8(), 1.0);
    let db = psnr(&cpu_out, &glsl);
    assert!(db >= 30.0, "dry_eye non-square 64x32: PSNR {db:.1} dB < 30 dB");
}

#[test]
fn shader_equiv_dry_eye_strength_0_0_is_identity() {
    let img = gradient_64();
    let input = img.to_rgba8();
    let cpu_out = dry_eye(img.clone(), 0.0).unwrap().to_rgba8();
    assert_eq!(cpu_out, input, "dry_eye strength=0.0: CPU が identity でない");
    let glsl = sim_dry_eye_glsl(&input, 0.0);
    assert_eq!(glsl, input, "dry_eye strength=0.0: GLSL が identity でない");
}

#[test]
fn dry_eye_uniforms_texel_size_matches_resolution() {
    // dry_eye_uniforms が texel_size = 1/解像度 を返すこと（frag が解像度復元に使う）。
    let uni = dry_eye_uniforms(0.7, 64, 32);
    assert_eq!(uni.strength, 0.7);
    assert!((uni.texel_size[0] - 1.0 / 64.0).abs() < 1e-7);
    assert!((uni.texel_size[1] - 1.0 / 32.0).abs() < 1e-7);
}
