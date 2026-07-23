use super::*;
use crate::Result;
use image::{DynamicImage, Rgba, RgbaImage};

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

// =================================================================
// Phase 2 (#4): focus / refraction (disk blur) tests
// =================================================================

/// 単色 RGBA 画像を作るヘルパー。
fn solid_rgba(width: u32, height: u32, rgba: [u8; 4]) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba(rgba)))
}

/// 中央 1px だけが white、周囲 black の画像を作るヘルパー。
fn center_white_dot(size: u32) -> DynamicImage {
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    img.put_pixel(size / 2, size / 2, Rgba([255, 255, 255, 255]));
    DynamicImage::ImageRgba8(img)
}

/// 縦線（中央列）だけが white、その他 black の画像。
fn vertical_line(size: u32) -> DynamicImage {
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    let cx = size / 2;
    for y in 0..size {
        img.put_pixel(cx, y, Rgba([255, 255, 255, 255]));
    }
    DynamicImage::ImageRgba8(img)
}

/// 横線（中央行）だけが white、その他 black の画像。
fn horizontal_line(size: u32) -> DynamicImage {
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    let cy = size / 2;
    for x in 0..size {
        img.put_pixel(x, cy, Rgba([255, 255, 255, 255]));
    }
    DynamicImage::ImageRgba8(img)
}

fn raw_rgba_vec(img: &DynamicImage) -> Vec<u8> {
    img.to_rgba8().into_raw()
}

// ---------------------------------------------------------------
// strength = 0.0 で 4 関数すべて identity
// ---------------------------------------------------------------

#[test]
fn refraction_strength_zero_is_identity() {
    let input = solid_rgba(64, 64, [200, 50, 30, 255]);
    let original = raw_rgba_vec(&input);
    let s = 0.0_f32;
    assert_eq!(raw_rgba_vec(&myopia(input.clone(), s).unwrap()), original);
    assert_eq!(
        raw_rgba_vec(&hyperopia(input.clone(), s).unwrap()),
        original
    );
    assert_eq!(
        raw_rgba_vec(&presbyopia(input.clone(), s).unwrap()),
        original
    );
    assert_eq!(
        raw_rgba_vec(&astigmatism(input, s, 90.0).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// NaN strength で 4 関数すべて identity（panic しない）
// ---------------------------------------------------------------

#[test]
fn refraction_nan_strength_returns_identity() {
    let input = solid_rgba(64, 64, [200, 50, 30, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&myopia(input.clone(), f32::NAN).unwrap()),
        original
    );
    assert_eq!(
        raw_rgba_vec(&hyperopia(input.clone(), f32::NAN).unwrap()),
        original
    );
    assert_eq!(
        raw_rgba_vec(&presbyopia(input.clone(), f32::NAN).unwrap()),
        original
    );
    assert_eq!(
        raw_rgba_vec(&astigmatism(input, f32::NAN, 90.0).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// alpha 保持
// ---------------------------------------------------------------

#[test]
fn refraction_preserves_alpha() {
    let input = solid_rgba(48, 48, [200, 50, 30, 77]);
    for s in [0.0_f32, 0.5, 1.0] {
        let m = myopia(input.clone(), s).unwrap().to_rgba8();
        let h = hyperopia(input.clone(), s).unwrap().to_rgba8();
        let p = presbyopia(input.clone(), s).unwrap().to_rgba8();
        let a = astigmatism(input.clone(), s, 90.0).unwrap().to_rgba8();
        for img in [&m, &h, &p, &a] {
            for px in img.pixels() {
                assert_eq!(px[3], 77, "alpha must be preserved");
            }
        }
    }
}

// ---------------------------------------------------------------
// 単一 white dot に myopia をかけると、中心領域が R==G==B で広がる
// ---------------------------------------------------------------

#[test]
fn myopia_spreads_single_dot() {
    // 81x81 画像中央に white dot。strength=1.0 → 半径 ≈ 0.023 * 81 ≒ 1.86 px。
    // disk は (0,0) と上下左右と斜め 4 隅 (dx²+dy² ≤ 3.46) で 9 pixel。
    // 中心ピクセルの白 (1/9) ≈ 28 → 0 < center < 255 の範囲に入る。
    let input = center_white_dot(81);
    let out = myopia(input.clone(), 1.0).unwrap().to_rgba8();
    let cx = 40;
    let cy = 40;
    let center = out.get_pixel(cx, cy);
    // 中心は disk の平均化で white より小さく、しかし R==G==B のまま。
    assert_eq!(center[0], center[1], "center R==G");
    assert_eq!(center[1], center[2], "center G==B");
    assert!(
        center[0] < 255,
        "center should be dimmer than original white"
    );
    assert!(center[0] > 0, "center should still receive some light");

    // 中心から半径より十分に離れた点 (例: 15px 離れた角の近く) は元の黒のまま。
    let far = out.get_pixel(0, 0);
    assert_eq!([far[0], far[1], far[2]], [0, 0, 0]);
}

// ---------------------------------------------------------------
// 単色画像はぼけても色が保たれる (境界 clamp 健全性)
// ---------------------------------------------------------------

#[test]
fn myopia_uniform_color_stays_uniform() {
    // 64x64 全面同一色。disk blur 後も全画素が（丸め誤差 ≤1 を除き）同じ色。
    let color = [120, 80, 40, 255];
    let input = solid_rgba(64, 64, color);
    let out = myopia(input, 1.0).unwrap().to_rgba8();
    for px in out.pixels() {
        for ch in 0..3 {
            let diff = (px[ch] as i16 - color[ch] as i16).abs();
            assert!(
                diff <= 1,
                "uniform color must be preserved (channel {ch}, got {} vs {})",
                px[ch],
                color[ch]
            );
        }
        assert_eq!(px[3], color[3]);
    }
}

#[test]
fn presbyopia_uniform_color_stays_uniform() {
    let color = [50, 200, 90, 255];
    let input = solid_rgba(80, 80, color);
    let out = presbyopia(input, 1.0).unwrap().to_rgba8();
    for px in out.pixels() {
        for ch in 0..3 {
            let diff = (px[ch] as i16 - color[ch] as i16).abs();
            assert!(diff <= 1, "uniform color must be preserved");
        }
    }
}

// ---------------------------------------------------------------
// astigmatism: axis が違うとぼけ方向が変わる
// ---------------------------------------------------------------

#[test]
fn astigmatism_axis_changes_blur_direction() {
    // 縦線画像に対し:
    //   - axis=90 (vertical sharp): 縦方向はシャープ、横方向にボケる
    //     → 縦線が左右に「滲む」
    //   - axis=0  (horizontal sharp): 横方向はシャープ、縦方向にボケる
    //     → 縦線はあまり滲まない（縦は元から sharp、横方向のボケはほぼ生じない）
    // 201x201 で長軸半径 ≈ 0.011 * 201 ≒ 2.21 px、1D box ~5 px 幅。
    let size = 201_u32;
    let input = vertical_line(size);
    let cx = size / 2;
    let cy = size / 2;

    let blur_h = astigmatism(input.clone(), 1.0, 90.0).unwrap().to_rgba8();
    let blur_v = astigmatism(input.clone(), 1.0, 0.0).unwrap().to_rgba8();

    // axis=90 (横方向ボケ): 中央行で縦線から左右に離れた点も明るくなる
    // axis=0  (縦方向ボケ): 中央行で同じ位置はほぼ黒のまま（縦線の幅は変わらない）
    // 中央線から 2px 横に離れた点を比較
    let off_x = cx + 2;
    let h_off = blur_h.get_pixel(off_x, cy)[0] as i32;
    let v_off = blur_v.get_pixel(off_x, cy)[0] as i32;
    assert!(
        h_off > v_off,
        "horizontal blur (axis=90) must spread the vertical line sideways more than \
         vertical blur (axis=0): h_off={h_off}, v_off={v_off}"
    );
}

// ---------------------------------------------------------------
// astigmatism: axis 周期 180°
// ---------------------------------------------------------------

#[test]
fn astigmatism_axis_is_180_periodic() {
    let input = horizontal_line(61);
    let a0 = raw_rgba_vec(&astigmatism(input.clone(), 1.0, 0.0).unwrap());
    let a180 = raw_rgba_vec(&astigmatism(input, 1.0, 180.0).unwrap());
    assert_eq!(a0, a180, "axis 0 and 180 must be identical (period 180°)");
}

// ---------------------------------------------------------------
// astigmatism: NaN axis は既定 (90°) にフォールバックして panic しない
// ---------------------------------------------------------------

#[test]
fn astigmatism_nan_axis_falls_back_to_default() {
    let input = solid_rgba(32, 32, [128, 128, 128, 255]);
    let out_nan = astigmatism(input.clone(), 1.0, f32::NAN).unwrap();
    let out_90 = astigmatism(input, 1.0, 90.0).unwrap();
    assert_eq!(
        raw_rgba_vec(&out_nan),
        raw_rgba_vec(&out_90),
        "NaN axis must behave like default 90°"
    );
}

// ---------------------------------------------------------------
// 画像サイズは保持される
// ---------------------------------------------------------------

// ---------------------------------------------------------------
// 半径ランキング: myopia > hyperopia >= astigmatism (≈ presbyopia)
// ---------------------------------------------------------------

#[test]
fn myopia_is_more_blurred_than_hyperopia_at_full_strength() {
    // 中央 white dot を myopia / hyperopia でぼかしたとき、
    // myopia (-6D, ratio 0.023) のほうが hyperopia (+4D, ratio 0.015) より
    // 中心輝度が低い (より広い disk で平均化されるため)。
    let input = center_white_dot(101);
    let m = myopia(input.clone(), 1.0).unwrap().to_rgba8();
    let h = hyperopia(input, 1.0).unwrap().to_rgba8();
    let cx = 50_u32;
    let cy = 50_u32;
    let m_center = m.get_pixel(cx, cy)[0] as i32;
    let h_center = h.get_pixel(cx, cy)[0] as i32;
    assert!(
        m_center < h_center,
        "myopia must blur more than hyperopia: m_center={m_center}, h_center={h_center}"
    );
}

// ---------------------------------------------------------------
// 極小画像 (半径 < 0.5px) は identity になる
// ---------------------------------------------------------------

#[test]
fn tiny_image_yields_identity_below_min_radius() {
    // 4x4 で myopia(strength=1.0): radius = 1.0 * 0.05 * 4 = 0.2px < 0.5
    // → identity になる契約。
    let input = solid_rgba(4, 4, [10, 20, 30, 200]);
    let original = raw_rgba_vec(&input);
    let out = myopia(input, 1.0).unwrap();
    assert_eq!(raw_rgba_vec(&out), original);
}

#[test]
fn refraction_preserves_dimensions() {
    let input = solid_rgba(31, 17, [80, 90, 100, 255]);
    type SimpleFilter = fn(DynamicImage, f32) -> Result<DynamicImage>;
    let filters: [SimpleFilter; 3] = [myopia, hyperopia, presbyopia];
    for f in filters {
        let out = f(input.clone(), 1.0).unwrap();
        assert_eq!((out.width(), out.height()), (31, 17));
    }
    let out = astigmatism(input, 1.0, 45.0).unwrap();
    assert_eq!((out.width(), out.height()), (31, 17));
}

// ---------------------------------------------------------------
// astigmatism: byte-exact な軸直交性
// ---------------------------------------------------------------

#[test]
fn astigmatism_axes_are_orthogonal_byte_exact() {
    // 縦線に axis=90 (横方向ボケ) を適用した結果を 90° 回転すると、
    // 横線に axis=0 (縦方向ボケ) を適用した結果と byte-exact で一致するはず。
    let size = 201_u32;
    let v_input = vertical_line(size);
    let h_input = horizontal_line(size);

    let bv = astigmatism(v_input, 1.0, 90.0).unwrap().to_rgba8();
    let bh = astigmatism(h_input, 1.0, 0.0).unwrap().to_rgba8();

    for y in 0..size {
        for x in 0..size {
            assert_eq!(
                bv.get_pixel(x, y),
                bh.get_pixel(y, x),
                "axis=90 vertical line at ({x},{y}) should equal axis=0 horizontal line rotated"
            );
        }
    }
}

// =================================================================
// Phase 3a (#5): visual field defect tests
// =================================================================

// ---------------------------------------------------------------
// T01-T04: strength=0.0 → identity
// ---------------------------------------------------------------

#[test]
fn glaucoma_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [200, 50, 30, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&glaucoma(input, 0.0, GlaucomaMode::Vignette).unwrap()),
        original
    );
}

#[test]
fn macular_degeneration_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [200, 50, 30, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&macular_degeneration(input, 0.0).unwrap()),
        original
    );
}

#[test]
fn hemianopia_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [200, 50, 30, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&hemianopia(input, 0.0, 0.0).unwrap()),
        original
    );
}

#[test]
fn tunnel_vision_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [200, 50, 30, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(raw_rgba_vec(&tunnel_vision(input, 0.0).unwrap()), original);
}

// ---------------------------------------------------------------
// T05-T08: NaN strength → identity
// ---------------------------------------------------------------

#[test]
fn glaucoma_nan_strength_returns_identity() {
    let input = solid_rgba(32, 32, [100, 150, 200, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&glaucoma(input, f32::NAN, GlaucomaMode::Vignette).unwrap()),
        original
    );
}

#[test]
fn macular_degeneration_nan_strength_returns_identity() {
    let input = solid_rgba(32, 32, [100, 150, 200, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&macular_degeneration(input, f32::NAN).unwrap()),
        original
    );
}

#[test]
fn hemianopia_nan_strength_returns_identity() {
    let input = solid_rgba(32, 32, [100, 150, 200, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&hemianopia(input, f32::NAN, 0.0).unwrap()),
        original
    );
}

#[test]
fn tunnel_vision_nan_strength_returns_identity() {
    let input = solid_rgba(32, 32, [100, 150, 200, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&tunnel_vision(input, f32::NAN).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// T09: glaucoma strength=2.0 is clamped to 1.0
// ---------------------------------------------------------------

#[test]
fn glaucoma_strength_above_one_clamped() {
    let input = solid_rgba(64, 64, [200, 100, 50, 255]);
    let out2 = raw_rgba_vec(&glaucoma(input.clone(), 2.0, GlaucomaMode::Vignette).unwrap());
    let out1 = raw_rgba_vec(&glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap());
    assert_eq!(out2, out1);
}

// ---------------------------------------------------------------
// T10: alpha preserved for all 4 visual field filters
// ---------------------------------------------------------------

#[test]
fn visual_field_filters_preserve_alpha() {
    // alpha=200 のピクセル（alpha != 255 で確認）
    let input = solid_rgba(32, 32, [80, 90, 100, 200]);
    let check_alpha = |img: DynamicImage| {
        for px in img.to_rgba8().pixels() {
            assert_eq!(px[3], 200, "alpha must be preserved");
        }
    };
    check_alpha(glaucoma(input.clone(), 0.8, GlaucomaMode::Vignette).unwrap());
    check_alpha(macular_degeneration(input.clone(), 0.8).unwrap());
    check_alpha(hemianopia(input.clone(), 0.8, 0.0).unwrap());
    check_alpha(tunnel_vision(input, 0.8).unwrap());
}

// ---------------------------------------------------------------
// T11: output dimensions preserved for all 4 visual field filters
// ---------------------------------------------------------------

#[test]
fn visual_field_filters_preserve_dimensions() {
    let input = solid_rgba(47, 31, [100, 100, 100, 255]);
    let (w, h) = (47, 31);
    let out = glaucoma(input.clone(), 0.5, GlaucomaMode::Vignette).unwrap();
    assert_eq!((out.width(), out.height()), (w, h));
    let out = macular_degeneration(input.clone(), 0.5).unwrap();
    assert_eq!((out.width(), out.height()), (w, h));
    let out = hemianopia(input.clone(), 0.5, 0.5).unwrap();
    assert_eq!((out.width(), out.height()), (w, h));
    let out = tunnel_vision(input, 0.5).unwrap();
    assert_eq!((out.width(), out.height()), (w, h));
}

// ---------------------------------------------------------------
// T12: glaucoma center pixel unchanged at strength=1.0
// ---------------------------------------------------------------

#[test]
fn glaucoma_center_pixel_unchanged_at_full_strength() {
    // 白画像で中心（r < inner_r=0.3）は変化なし
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 100, 50, 255]);
    let out = glaucoma(input, 1.0, GlaucomaMode::Vignette)
        .unwrap()
        .to_rgba8();
    let cx = size / 2;
    let cy = size / 2;
    let center = out.get_pixel(cx, cy);
    // 中心画素は元のまま (mul=1.0)
    assert_eq!([center[0], center[1], center[2]], [200, 100, 50]);
}

// ---------------------------------------------------------------
// T13: glaucoma corner pixel becomes black at full strength
// ---------------------------------------------------------------

#[test]
fn glaucoma_corner_pixel_becomes_black_at_full_strength() {
    // コーナー (r=1.0 > outer_r=0.5) → mul=0.0 → 黒
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 100, 50, 255]);
    let out = glaucoma(input, 1.0, GlaucomaMode::Vignette)
        .unwrap()
        .to_rgba8();
    let corner = out.get_pixel(0, 0);
    assert_eq!([corner[0], corner[1], corner[2]], [0, 0, 0]);
}

// ---------------------------------------------------------------
// T14: glaucoma monotonic peripheral darkening
// ---------------------------------------------------------------

#[test]
fn glaucoma_strength_monotonic_peripheral_darkening() {
    // コーナー付近では strength=0.5 の方が strength=1.0 より明るい
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out05 = glaucoma(input.clone(), 0.5, GlaucomaMode::Vignette)
        .unwrap()
        .to_rgba8();
    let out10 = glaucoma(input, 1.0, GlaucomaMode::Vignette)
        .unwrap()
        .to_rgba8();
    // コーナー (0,0) での輝度比較
    let r05 = out05.get_pixel(0, 0)[0] as i32;
    let r10 = out10.get_pixel(0, 0)[0] as i32;
    assert!(
        r05 > r10,
        "strength=0.5 corner must be brighter than strength=1.0: {r05} vs {r10}"
    );
}

// ---------------------------------------------------------------
// T15: macular_degeneration center darkened at full strength
// ---------------------------------------------------------------

#[test]
fn macular_degeneration_center_darkened_at_full_strength() {
    // 中心画素: darkened = lum * 0.05 なので元より暗くなる
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out = macular_degeneration(input, 1.0).unwrap().to_rgba8();
    let cx = size / 2;
    let cy = size / 2;
    let center = out.get_pixel(cx, cy)[0] as i32;
    // 200 より大幅に暗いはず (strength=1.0, darkened = lum * 0.05)
    assert!(
        center < 200,
        "center must be darkened at full strength, got {center}"
    );
}

// ---------------------------------------------------------------
// T16: macular_degeneration periphery unchanged at full strength
// ---------------------------------------------------------------

#[test]
fn macular_degeneration_periphery_unchanged_at_full_strength() {
    // 周辺 (r > outer_r=0.4) は t=0.0 → continue → 変化なし
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 100, 50, 255]);
    let out = macular_degeneration(input, 1.0).unwrap().to_rgba8();
    // コーナーは周辺なので変化なし
    let corner = out.get_pixel(0, 0);
    assert_eq!([corner[0], corner[1], corner[2]], [200, 100, 50]);
}

// ---------------------------------------------------------------
// T17: macular_degeneration monotonic center darkening
// ---------------------------------------------------------------

#[test]
fn macular_degeneration_strength_monotonic_center_darkening() {
    // 中心では strength が大きいほど暗い
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out05 = macular_degeneration(input.clone(), 0.5).unwrap().to_rgba8();
    let out10 = macular_degeneration(input, 1.0).unwrap().to_rgba8();
    let cx = size / 2;
    let cy = size / 2;
    let r05 = out05.get_pixel(cx, cy)[0] as i32;
    let r10 = out10.get_pixel(cx, cy)[0] as i32;
    assert!(
        r05 > r10,
        "strength=0.5 center must be brighter than strength=1.0: {r05} vs {r10}"
    );
}

// ---------------------------------------------------------------
// T18: hemianopia left side darkened when side=0.0
// ---------------------------------------------------------------

#[test]
fn hemianopia_left_side_darkened_when_side_zero() {
    // side=0.0, strength=1.0: 左端 (x=0) は x < split_x - blur_w → fade=1.0 → 黒
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out = hemianopia(input, 1.0, 0.0).unwrap().to_rgba8();
    let left = out.get_pixel(0, size / 2);
    assert_eq!(
        [left[0], left[1], left[2]],
        [0, 0, 0],
        "left edge must be black when side=0.0"
    );
}

// ---------------------------------------------------------------
// T19: hemianopia right side darkened when side=1.0
// ---------------------------------------------------------------

#[test]
fn hemianopia_right_side_darkened_when_side_one() {
    // side=1.0, strength=1.0: 右端 (x=size-1) は x > split_x + blur_w → fade=1.0 → 黒
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out = hemianopia(input, 1.0, 1.0).unwrap().to_rgba8();
    let right = out.get_pixel(size - 1, size / 2);
    assert_eq!(
        [right[0], right[1], right[2]],
        [0, 0, 0],
        "right edge must be black when side=1.0"
    );
}

// ---------------------------------------------------------------
// T20: hemianopia side=0.0 and side=1.0 are left-right symmetric
// ---------------------------------------------------------------

#[test]
fn hemianopia_side_left_right_symmetry() {
    // side=0.0 と side=1.0 の対称性を境界から十分離れた領域（端部）で確認する。
    // 境界付近の blur_w ゾーンでは整数ピクセルの離散化により非対称が生じうるが、
    // 境界から遠い領域（左 25%、右 25%）では完全に対称であるべき。
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out_left = hemianopia(input.clone(), 1.0, 0.0).unwrap().to_rgba8();
    let out_right = hemianopia(input, 1.0, 1.0).unwrap().to_rgba8();
    // 境界から遠い端部（左 1/4 と右 1/4）の対称性を確認
    for y in 0..size {
        for x in 0..size / 4 {
            let pl = out_left.get_pixel(x, y)[0] as i32;
            let pr = out_right.get_pixel(size - 1 - x, y)[0] as i32;
            assert_eq!(
                pl, pr,
                "far-end symmetry failed at x={x}: side=0 left={pl}, side=1 mirrored={pr}"
            );
        }
    }
}

// ---------------------------------------------------------------
// T21: hemianopia boundary center is intermediate
// ---------------------------------------------------------------

#[test]
fn hemianopia_boundary_center_is_intermediate() {
    // x = split_x (中央) は境界内にあり、完全黒でも完全白でもない
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out = hemianopia(input, 1.0, 0.0).unwrap().to_rgba8();
    let cx = size / 2;
    let cy = size / 2;
    let center = out.get_pixel(cx, cy)[0] as i32;
    // 完全黒 (0) でも元画像 (≈200) でもない中間値
    assert!(
        center > 0 && center < 200,
        "boundary center must be intermediate, got {center}"
    );
}

// ---------------------------------------------------------------
// T22: tunnel_vision corner becomes black at full strength
// ---------------------------------------------------------------

#[test]
fn tunnel_vision_corner_becomes_black_at_full_strength() {
    // strength=1.0: inner_r=0.0, outer_r=0.05。コーナー r≈1.0 > 0.05 → 黒
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 100, 50, 255]);
    let out = tunnel_vision(input, 1.0).unwrap().to_rgba8();
    let corner = out.get_pixel(0, 0);
    assert_eq!([corner[0], corner[1], corner[2]], [0, 0, 0]);
}

// ---------------------------------------------------------------
// T23: tunnel_vision monotonic peripheral darkening
// ---------------------------------------------------------------

#[test]
fn tunnel_vision_strength_monotonic_peripheral_darkening() {
    let size = 64_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let out05 = tunnel_vision(input.clone(), 0.5).unwrap().to_rgba8();
    let out10 = tunnel_vision(input, 1.0).unwrap().to_rgba8();
    let r05 = out05.get_pixel(0, 0)[0] as i32;
    let r10 = out10.get_pixel(0, 0)[0] as i32;
    assert!(
        r05 > r10,
        "strength=0.5 corner must be brighter than strength=1.0: {r05} vs {r10}"
    );
}

// ---------------------------------------------------------------
// T24: tunnel_vision darker area is wider than glaucoma at same strength
// ---------------------------------------------------------------

#[test]
fn tunnel_vision_narrower_than_glaucoma_at_same_strength() {
    // tunnel_vision の中心保持領域は glaucoma より狭い（暗化エリアが広い）。
    // 同一の strength=1.0 で、中心から少し離れた点を比較する。
    // glaucoma: inner_r=0.3, outer_r=0.5 → 中心近くは保存
    // tunnel: inner_r=0.0, outer_r=0.05 → ほぼ全体が暗化
    // 中心から 30% 離れた点での輝度比較（glaucoma は保存, tunnel は暗化済み）
    let size = 100_u32;
    let input = solid_rgba(size, size, [200, 200, 200, 255]);
    let g_out = glaucoma(input.clone(), 1.0, GlaucomaMode::Vignette)
        .unwrap()
        .to_rgba8();
    let t_out = tunnel_vision(input, 1.0).unwrap().to_rgba8();
    // (50, 65) は中心から dy=15, normalized ≈ 0.15 → glaucoma ではinner_r=0.3 内で保存
    let cx = 50_u32;
    let test_y = 65_u32; // 中心y=50, dy=15
    let g_px = g_out.get_pixel(cx, test_y)[0] as i32;
    let t_px = t_out.get_pixel(cx, test_y)[0] as i32;
    assert!(
        g_px > t_px,
        "glaucoma must preserve more than tunnel_vision at same strength: \
         glaucoma={g_px}, tunnel={t_px}"
    );
}

// ---------------------------------------------------------------
// T25-T26: lerp tests
// ---------------------------------------------------------------

#[test]
fn lerp_basic_interpolation() {
    assert_eq!(super::lerp(0.0, 10.0, 0.0), 0.0);
    assert_eq!(super::lerp(0.0, 10.0, 1.0), 10.0);
    assert_eq!(super::lerp(0.0, 10.0, 0.5), 5.0);
    assert_eq!(super::lerp(2.0, 8.0, 0.5), 5.0);
}

#[test]
fn lerp_extrapolation_beyond_range() {
    // t=2.0 → clamp しない: a + (b-a)*2 = 0 + 10*2 = 20
    let result = super::lerp(0.0, 10.0, 2.0);
    assert!((result - 20.0).abs() < 1e-5, "expected 20.0, got {result}");
}

// ---------------------------------------------------------------
// T27-T30: 1x1 image does not panic
// ---------------------------------------------------------------

#[test]
fn glaucoma_1x1_does_not_panic() {
    let input = pixel(128, 128, 128, 255);
    let _ = glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap();
}

#[test]
fn macular_degeneration_1x1_does_not_panic() {
    let input = pixel(128, 128, 128, 255);
    let _ = macular_degeneration(input, 1.0).unwrap();
}

#[test]
fn hemianopia_1x1_does_not_panic() {
    let input = pixel(128, 128, 128, 255);
    let _ = hemianopia(input, 1.0, 0.5).unwrap();
}

#[test]
fn tunnel_vision_1x1_does_not_panic() {
    let input = pixel(128, 128, 128, 255);
    let _ = tunnel_vision(input, 1.0).unwrap();
}

// ---------------------------------------------------------------
// T31-T33: color-specific pixel behavior
// ---------------------------------------------------------------

#[test]
fn glaucoma_white_image_center_stays_white_corner_goes_black() {
    let size = 64_u32;
    let input = solid_rgba(size, size, [255, 255, 255, 255]);
    let out = glaucoma(input, 1.0, GlaucomaMode::Vignette)
        .unwrap()
        .to_rgba8();
    let cx = size / 2;
    let cy = size / 2;
    let center = out.get_pixel(cx, cy);
    assert_eq!(
        [center[0], center[1], center[2]],
        [255, 255, 255],
        "center of white image must stay white"
    );
    let corner = out.get_pixel(0, 0);
    assert_eq!(
        [corner[0], corner[1], corner[2]],
        [0, 0, 0],
        "corner of white image must become black"
    );
}

#[test]
fn glaucoma_black_image_stays_black() {
    let size = 32_u32;
    let input = solid_rgba(size, size, [0, 0, 0, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&glaucoma(input, 1.0, GlaucomaMode::Vignette).unwrap()),
        original
    );
}

#[test]
fn macular_degeneration_black_image_stays_black() {
    let size = 32_u32;
    let input = solid_rgba(size, size, [0, 0, 0, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&macular_degeneration(input, 1.0).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// 性能リグレッションガード (--ignored)
// ---------------------------------------------------------------

// =================================================================
// Phase 3 (#6): light / transparency tests
// =================================================================

// ---------------------------------------------------------------
// P01-P04: strength = 0.0 で 4 フィルタすべて identity
// ---------------------------------------------------------------

#[test]
fn cataract_strength_zero_is_identity() {
    let input = solid_rgba(16, 16, [200, 100, 50, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(raw_rgba_vec(&cataract(input, 0.0, 42).unwrap()), original);
}

#[test]
fn photophobia_strength_zero_is_identity() {
    let input = solid_rgba(16, 16, [200, 100, 50, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(raw_rgba_vec(&photophobia(input, 0.0).unwrap()), original);
}

#[test]
fn nyctalopia_strength_zero_is_identity() {
    let input = solid_rgba(16, 16, [200, 100, 50, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(raw_rgba_vec(&nyctalopia(input, 0.0).unwrap()), original);
}

#[test]
fn floaters_size_scales_coverage() {
    // #110: size を大きくすると blob/糸くずが太くなり、マスクの被覆面積が増える
    // （= 平均マスク値が下がる）。同一 seed/density で比較する。
    let mean = |m: &image::GrayImage| {
        m.as_raw().iter().map(|&v| v as u32).sum::<u32>() as f32 / m.as_raw().len() as f32
    };
    let small = floaters_mask(64, 64, 0.5, 42, 0.5, 0.5, 0.5);
    let large = floaters_mask(64, 64, 0.5, 42, 0.5, 0.5, 2.0);
    assert!(
        mean(&large) < mean(&small),
        "larger size must increase floater coverage: mean(large)={}, mean(small)={}",
        mean(&large),
        mean(&small)
    );
    // size は 1.0 が既定（恒等的に効かないわけではないが、0/NaN は 1.0 フォールバック）
    let nan = floaters_mask(64, 64, 0.5, 42, 0.5, 0.5, f32::NAN);
    let one = floaters_mask(64, 64, 0.5, 42, 0.5, 0.5, 1.0);
    assert_eq!(nan.as_raw(), one.as_raw(), "NaN size must fall back to 1.0");
}

#[test]
fn floaters_strength_zero_is_identity() {
    let input = solid_rgba(16, 16, [200, 100, 50, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&floaters(input, 0.0, 0.5, 42, 0.5, 0.5, 1.0).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// P05-P06: NaN strength は identity
// ---------------------------------------------------------------

#[test]
fn cataract_nan_strength_returns_identity() {
    let input = solid_rgba(16, 16, [200, 100, 50, 200]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&cataract(input, f32::NAN, 42).unwrap()),
        original
    );
}

#[test]
fn cataract_reduces_brightness_and_contrast() {
    // #106: VIP-Sim 二段モデルの輝度・コントラスト低下。
    // 1x1 の白と黒（同一座標なので白濁ノイズは共通）で輝度差が縮むことを確認する。
    let white = cataract(solid_rgba(1, 1, [255, 255, 255, 255]), 1.0, 42).unwrap();
    let black = cataract(solid_rgba(1, 1, [0, 0, 0, 255]), 1.0, 42).unwrap();
    let w = raw_rgba_vec(&white);
    let b = raw_rgba_vec(&black);
    let w_sum: i32 = w[0..3].iter().map(|&v| v as i32).sum();
    let b_sum: i32 = b[0..3].iter().map(|&v| v as i32).sum();
    // 白は最大未満まで下がる（コントラスト収縮 + 輝度低下）
    assert!(
        w_sum < 255 * 3,
        "cataract should dim pure white: sum={w_sum}"
    );
    // 黒は白濁ヴェールで持ち上がる
    assert!(
        b_sum > 0,
        "cataract veil should lift pure black: sum={b_sum}"
    );
    // 白黒の輝度差（コントラスト）が元の最大 (255*3) より縮む
    assert!(
        (w_sum - b_sum) < 255 * 3,
        "cataract should compress contrast: white_sum={w_sum}, black_sum={b_sum}"
    );
}

#[test]
fn nyctalopia_nan_strength_returns_identity() {
    let input = solid_rgba(16, 16, [200, 100, 50, 200]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&nyctalopia(input, f32::NAN).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// P07: floaters density=0.0 → blob_count=0 → identity
// ---------------------------------------------------------------

#[test]
fn floaters_density_zero_returns_identity() {
    let input = solid_rgba(16, 16, [100, 150, 200, 255]);
    let original = raw_rgba_vec(&input);
    // density=0.0 なので blob_count=0 → early return で identity
    assert_eq!(
        raw_rgba_vec(&floaters(input, 1.0, 0.0, 42, 0.5, 0.5, 1.0).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// P08: 4 フィルタ alpha 保持（alpha != 255 の入力）
// ---------------------------------------------------------------

#[test]
fn light_filters_preserve_alpha() {
    let input = solid_rgba(16, 16, [200, 100, 50, 128]);
    let check_alpha = |img: &DynamicImage| {
        for px in img.to_rgba8().pixels() {
            assert_eq!(px[3], 128, "alpha must be preserved");
        }
    };
    check_alpha(&cataract(input.clone(), 1.0, 42).unwrap());
    check_alpha(&photophobia(input.clone(), 1.0).unwrap());
    check_alpha(&nyctalopia(input.clone(), 1.0).unwrap());
    check_alpha(&floaters(input, 1.0, 0.5, 42, 0.5, 0.5, 1.0).unwrap());
}

// ---------------------------------------------------------------
// P09: 4 フィルタ 出力サイズ同一
// ---------------------------------------------------------------

#[test]
fn light_filters_preserve_dimensions() {
    let input = solid_rgba(31, 17, [80, 90, 100, 255]);
    let check_dims = |img: &DynamicImage| {
        assert_eq!((img.width(), img.height()), (31, 17));
    };
    check_dims(&cataract(input.clone(), 1.0, 42).unwrap());
    check_dims(&photophobia(input.clone(), 1.0).unwrap());
    check_dims(&nyctalopia(input.clone(), 1.0).unwrap());
    check_dims(&floaters(input, 1.0, 0.5, 42, 0.5, 0.5, 1.0).unwrap());
}

// ---------------------------------------------------------------
// P10: cataract yellowing reduces B channel more than R/G
// ---------------------------------------------------------------

#[test]
fn cataract_yellowing_reduces_blue() {
    // strength=1.0: R係数 0.7, G係数 0.7, B係数 0.4
    // 白画像で out_B < out_R かつ out_B < out_G になるはず
    // （ただしwhite_blendノイズの影響を避けるため、
    //   すべてのピクセルで B < R を確認する）
    let input = solid_rgba(32, 32, [255, 255, 255, 255]);
    let out = cataract(input, 1.0, 0).unwrap().to_rgba8();
    // 少なくとも中心ピクセルで確認
    let px = out.get_pixel(16, 16);
    let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
    assert!(
        b < r,
        "cataract yellowing: expected B < R, got R={r}, G={g}, B={b}"
    );
    // 全ピクセルで B <= R を確認（白濁ノイズがあっても基本的に B が最小）
    for px in out.pixels() {
        let (pr, pb) = (px[0] as i32, px[2] as i32);
        assert!(
            pb <= pr,
            "cataract: expected B <= R at every pixel, got R={pr}, B={pb}"
        );
    }
}

// ---------------------------------------------------------------
// P11: nyctalopia darkens and desaturates
// ---------------------------------------------------------------

#[test]
fn nyctalopia_darkens_and_desaturates() {
    // strength=1.0 で白画像 [255,255,255] が暗くなる
    // Purkinje shift 適用後: R < B（青チャネル微増、赤チャネル微減）
    // dark_factor = 1.0 - 1.0 * 0.7 = 0.3
    let input = solid_rgba(8, 8, [255, 255, 255, 255]);
    let out = nyctalopia(input, 1.0).unwrap().to_rgba8();
    for px in out.pixels() {
        let (r, g, b) = (px[0], px[1], px[2]);
        // 暗化: 255 より大幅に低い
        assert!(r < 200, "nyctalopia must darken: R={r}");
        assert!(g < 200, "nyctalopia must darken: G={g}");
        assert!(b < 200, "nyctalopia must darken: B={b}");
        // Purkinje shift: B >= R（青チャネルが赤チャネル以上）
        assert!(b >= r, "Purkinje shift: B={b} should be >= R={r}");
    }
}

// ---------------------------------------------------------------
// P12: floaters same seed → byte-exact reproducible
// ---------------------------------------------------------------

#[test]
fn floaters_same_seed_is_reproducible() {
    let input = solid_rgba(32, 32, [200, 150, 100, 255]);
    let out1 = raw_rgba_vec(&floaters(input.clone(), 0.8, 0.3, 12345, 0.5, 0.5, 1.0).unwrap());
    let out2 = raw_rgba_vec(&floaters(input, 0.8, 0.3, 12345, 0.5, 0.5, 1.0).unwrap());
    assert_eq!(
        out1, out2,
        "same seed must produce byte-exact identical output"
    );
}

// ---------------------------------------------------------------
// P13: floaters different seed → different output
// ---------------------------------------------------------------

#[test]
fn floaters_different_seed_differs() {
    let input = solid_rgba(32, 32, [200, 150, 100, 255]);
    let out1 = raw_rgba_vec(&floaters(input.clone(), 0.8, 0.5, 111, 0.5, 0.5, 1.0).unwrap());
    let out2 = raw_rgba_vec(&floaters(input, 0.8, 0.5, 999, 0.5, 0.5, 1.0).unwrap());
    assert_ne!(out1, out2, "different seeds must produce different output");
}

// ---------------------------------------------------------------
// P14-P17: 1x1 でクラッシュなし
// ---------------------------------------------------------------

#[test]
fn cataract_1x1_does_not_panic() {
    let input = pixel(128, 64, 32, 255);
    let _ = cataract(input, 1.0, 42).unwrap();
}

#[test]
fn photophobia_1x1_does_not_panic() {
    let input = pixel(255, 255, 255, 255);
    let _ = photophobia(input, 1.0).unwrap();
}

#[test]
fn nyctalopia_1x1_does_not_panic() {
    let input = pixel(128, 64, 32, 255);
    let _ = nyctalopia(input, 1.0).unwrap();
}

#[test]
fn floaters_1x1_does_not_panic() {
    let input = pixel(128, 64, 32, 255);
    let _ = floaters(input, 1.0, 0.5, 42, 0.5, 0.5, 1.0).unwrap();
}

// ---------------------------------------------------------------
// tetrachromacy テスト
// ---------------------------------------------------------------

#[test]
fn tetrachromacy_strength_zero_is_identity() {
    let input = pixel(200, 100, 50, 255);
    let out = tetrachromacy(input.clone(), 0.0).unwrap();
    assert_eq!(read_rgba(&out), [200, 100, 50, 255]);
}

#[test]
fn tetrachromacy_nan_strength_returns_identity() {
    let input = pixel(200, 100, 50, 200);
    let out = tetrachromacy(input.clone(), f32::NAN).unwrap();
    assert_eq!(read_rgba(&out), [200, 100, 50, 200]);
}

#[test]
fn tetrachromacy_alpha_preserved() {
    let input = pixel(200, 100, 50, 77);
    let out = tetrachromacy(input, 1.0).unwrap();
    assert_eq!(read_rgba(&out)[3], 77);
}

#[test]
fn tetrachromacy_negative_strength_is_identity() {
    let input = pixel(200, 100, 50, 255);
    let out = tetrachromacy(input.clone(), -1.0).unwrap();
    assert_eq!(read_rgba(&out), [200, 100, 50, 255]);
}

#[test]
fn tetrachromacy_above_one_clamped_same_as_one() {
    let input = pixel(200, 100, 50, 255);
    let a = tetrachromacy(input.clone(), 2.0).unwrap();
    let b = tetrachromacy(input, 1.0).unwrap();
    assert_eq!(read_rgba(&a), read_rgba(&b));
}

#[test]
fn tetrachromacy_gray_unchanged() {
    // 純グレー (R==G==B) は rg=0, yb=0 なので変化しない
    let input = pixel(128, 128, 128, 255);
    let out = tetrachromacy(input, 1.0).unwrap();
    let [r, g, b, _] = read_rgba(&out);
    // 1px round-trip で ±1 以内の誤差を許容
    assert!(r.abs_diff(128) <= 1);
    assert!(g.abs_diff(128) <= 1);
    assert!(b.abs_diff(128) <= 1);
}

#[test]
fn tetrachromacy_pure_red_amplifies_rg() {
    // 純赤: rg > 0 なので R が増え G が減る方向に誇張される
    let input = pixel(200, 0, 0, 255);
    let out = tetrachromacy(input, 1.0).unwrap();
    let [r, g, _b, _] = read_rgba(&out);
    // R は変化なし or 上昇（既に高い）、G は 0 から下はいかない（clamp）
    assert!(r >= 200 || r == 255); // clamp で飽和することもある
    assert_eq!(g, 0); // G は既に 0、下がっても 0 のまま
}

#[test]
fn tetrachromacy_preserves_dimensions() {
    // 出力サイズが入力と同一
    let mut img = RgbaImage::new(13, 7);
    for (_, _, px) in img.enumerate_pixels_mut() {
        *px = Rgba([100, 150, 80, 255]);
    }
    let input = DynamicImage::ImageRgba8(img);
    let out = tetrachromacy(input, 1.0).unwrap();
    assert_eq!((out.width(), out.height()), (13, 7));
}

#[test]
fn tetrachromacy_white_pixel_is_unchanged() {
    // (255,255,255,255): rg=0, yb=0 → 変化なし
    let input = pixel(255, 255, 255, 255);
    let out = tetrachromacy(input, 1.0).unwrap();
    let [r, g, b, a] = read_rgba(&out);
    assert_eq!(r, 255);
    assert_eq!(g, 255);
    assert_eq!(b, 255);
    assert_eq!(a, 255);
}

#[test]
fn tetrachromacy_black_pixel_is_unchanged() {
    // (0,0,0,255): rg=0, yb=0 → 変化なし
    let input = pixel(0, 0, 0, 255);
    let out = tetrachromacy(input, 1.0).unwrap();
    let [r, g, b, a] = read_rgba(&out);
    assert_eq!(r, 0);
    assert_eq!(g, 0);
    assert_eq!(b, 0);
    assert_eq!(a, 255);
}

#[test]
fn tetrachromacy_strength_monotonic() {
    // strength=1.0 の方が strength=0.5 よりも R-G 差が大きい
    // 赤みある画素 (200, 100, 0, 255): rg = R - G > 0
    let input = pixel(200, 100, 0, 255);
    let out05 = tetrachromacy(input.clone(), 0.5).unwrap();
    let out10 = tetrachromacy(input, 1.0).unwrap();
    let [r05, g05, _, _] = read_rgba(&out05);
    let [r10, g10, _, _] = read_rgba(&out10);
    let diff05 = r05 as i32 - g05 as i32;
    let diff10 = r10 as i32 - g10 as i32;
    assert!(
        diff10 > diff05,
        "strength=1.0 R-G diff ({diff10}) must be greater than strength=0.5 ({diff05})"
    );
}

// ---------------------------------------------------------------
// #38: floaters seed=0 と seed=1 で出力が異なること
// ---------------------------------------------------------------

#[test]
fn floaters_seed_0_ne_seed_1() {
    let input = solid_rgba(32, 32, [200, 150, 100, 255]);
    let out0 = raw_rgba_vec(&floaters(input.clone(), 0.8, 0.5, 0, 0.5, 0.5, 1.0).unwrap());
    let out1 = raw_rgba_vec(&floaters(input, 0.8, 0.5, 1, 0.5, 0.5, 1.0).unwrap());
    assert_ne!(
        out0, out1,
        "seed=0 and seed=1 must produce different output"
    );
}

// ---------------------------------------------------------------
// #39: tetrachromacy メタメリック領域で色差が誇張されること
// ---------------------------------------------------------------

#[test]
fn tetrachromacy_metameric_regions_enhanced() {
    // グレーに近い画素（R≈G≈B）は LMS で delta≈0 となりメタメリックペア候補
    // strength=1.0 で Cb/Cr 誇張が適用され、元画像からの変化が大きくなるはず
    // ただし純グレー(R==G==B)はCb=Cr=0なので変化なし。
    // わずかに色差のある画素でテストする
    let input_neutral = pixel(128, 128, 128, 255); // 純グレー: 変化なし
    let out_neutral = tetrachromacy(input_neutral, 1.0).unwrap();
    let [r, g, b, _] = read_rgba(&out_neutral);
    // 純グレーは変化なし（メタメリックだが Cb/Cr=0）
    assert!(
        (r as i32 - g as i32).abs() <= 2,
        "neutral gray should stay near-gray after tetrachromacy"
    );
    let _ = b;

    // 赤みのある画素: LMS delta が大きくメタメリックペアでないため
    // opponent channel による誇張が適用される
    let input_red = pixel(200, 100, 50, 255);
    let out_s0 = tetrachromacy(input_red.clone(), 0.0).unwrap();
    let out_s1 = tetrachromacy(input_red, 1.0).unwrap();
    let [r0, g0, _, _] = read_rgba(&out_s0);
    let [r1, g1, _, _] = read_rgba(&out_s1);
    assert_ne!(
        (r0 as i32 - g0 as i32),
        (r1 as i32 - g1 as i32),
        "strength=1.0 should differ from strength=0.0 on colored pixels"
    );
}

// ---------------------------------------------------------------
// #40: cataract 黄変マトリクス - 青チャネル平均が入力より低いこと
// ---------------------------------------------------------------

#[test]
fn cataract_yellowing_blue_mean_reduced() {
    // strength=1.0 で B * 0.85 となるため、青い画素で B が低下する
    let input = solid_rgba(16, 16, [128, 128, 255, 255]);
    let out = cataract(input, 1.0, 0).unwrap().to_rgba8();
    let orig_b_mean: f64 = 255.0;
    let out_b_mean: f64 =
        out.pixels().map(|p| p[2] as f64).sum::<f64>() / (out.width() * out.height()) as f64;
    assert!(
        out_b_mean < orig_b_mean,
        "cataract yellowing: blue channel mean ({out_b_mean:.1}) should be below input ({orig_b_mean:.1})"
    );
}

#[test]
#[ignore = "perf check; run with `cargo test -- --ignored`"]
fn myopia_1024_full_strength_under_5s() {
    use std::time::Instant;
    let img = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        1024,
        1024,
        image::Rgba([128, 128, 128, 255]),
    ));
    let start = Instant::now();
    let _ = myopia(img, 1.0).unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs_f32() < 5.0,
        "1024×1024 myopia s=1.0 took {elapsed:?}, target < 5s"
    );
}

// =================================================================
// Phase 4 (#9): めまいフィルタ tests
// =================================================================

// ---------------------------------------------------------------
// TC-V-01: vertigo strength=0.0 は identity
// ---------------------------------------------------------------

#[test]
fn vertigo_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [200, 100, 50, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(raw_rgba_vec(&vertigo(input, 0.0, 1.0).unwrap()), original);
}

// ---------------------------------------------------------------
// TC-V-03: vertigo 1x1 image does not panic
// ---------------------------------------------------------------

#[test]
fn vertigo_1x1_image_does_not_panic() {
    let input = pixel(128, 128, 128, 255);
    let _ = vertigo(input, 1.0, 0.5).unwrap();
}

// ---------------------------------------------------------------
// TC-V-05: bppv_rotation strength=0.0 は identity
// ---------------------------------------------------------------

#[test]
fn bppv_rotation_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [200, 100, 50, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&bppv_rotation(input, 0.0, 1.0).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// TC-V-07: bppv_rotation 1x1 image does not panic
// ---------------------------------------------------------------

#[test]
fn bppv_rotation_1x1_image_does_not_panic() {
    let input = pixel(128, 128, 128, 255);
    let _ = bppv_rotation(input, 1.0, 0.5).unwrap();
}

// ---------------------------------------------------------------
// TC-V-11: bppv_rotation time_t=-1.0 does not panic
// ---------------------------------------------------------------

#[test]
fn bppv_rotation_time_t_negative_does_not_panic() {
    let input = solid_rgba(32, 32, [100, 150, 200, 255]);
    // rem_euclid により -1.0 → 1.0 (mod 2.0) になる。角度は適正範囲に収まる。
    let _ = bppv_rotation(input, 1.0, -1.0).unwrap();
}

// ---------------------------------------------------------------
// TC-V-12: vestibular_neuritis strength=0.0 は identity
// ---------------------------------------------------------------

#[test]
fn vestibular_neuritis_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [200, 100, 50, 255]);
    let original = raw_rgba_vec(&input);
    assert_eq!(
        raw_rgba_vec(&vestibular_neuritis(input, 0.0).unwrap()),
        original
    );
}

// ---------------------------------------------------------------
// TC-V-14: vestibular_neuritis 1x1 image does not panic
// ---------------------------------------------------------------

#[test]
fn vestibular_neuritis_1x1_image_does_not_panic() {
    let input = pixel(128, 128, 128, 255);
    let _ = vestibular_neuritis(input, 1.0).unwrap();
}

// =================================================================
// Phase N (#19): depth-aware blur tests
// =================================================================

#[allow(dead_code)]
/// 32x32 の2段グラデーション深度マップを作るヘルパー。
/// 左半分 = 暗い (0), 右半分 = 明るい (255)。
fn depth_map_half(size: u32, left_val: u8, right_val: u8) -> DynamicImage {
    use image::GrayImage;
    let mut d = GrayImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let v = if x < size / 2 { left_val } else { right_val };
            d.put_pixel(x, y, image::Luma([v]));
        }
    }
    DynamicImage::ImageLuma8(d)
}

/// 単色 depth map（全面同じ深度値）を作るヘルパー。
fn depth_map_solid(size: u32, val: u8) -> DynamicImage {
    use image::GrayImage;
    DynamicImage::ImageLuma8(GrayImage::from_pixel(size, size, image::Luma([val])))
}

// ---------------------------------------------------------------
// DA-01: Myopia — 遠方（depth < focus）がボケる
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_myopia_far_is_blurred() {
    // 64x64 の中央に white dot。
    // depth_map: 全画素 depth=0.0 (最遠方)。focus=1.0。
    // Myopia → d < focus なのでボケる。max_radius_ratio=0.1 で radius = 1.0 * 0.1 * 64 = 6.4px
    let size = 64_u32;
    let input = center_white_dot(size);
    let depth_far = depth_map_solid(size, 0); // depth≈0.0 (遠方)

    let out_blurred =
        depth_aware_blur(input.clone(), &depth_far, 1.0, 0.1, DepthBlurKind::Myopia).unwrap();

    // focus と同深度（depth=1.0, val=255）はボケない
    let depth_focus = depth_map_solid(size, 255); // depth≈1.0 (focus と同深度)
    let out_sharp = depth_aware_blur(input, &depth_focus, 1.0, 0.1, DepthBlurKind::Myopia).unwrap();

    let cx = size / 2;
    let cy = size / 2;
    let blurred_center = out_blurred.to_rgba8().get_pixel(cx, cy)[0];
    let sharp_center = out_sharp.to_rgba8().get_pixel(cx, cy)[0];
    assert!(
        blurred_center < sharp_center,
        "far pixel (depth=0.0, focus=1.0) must be more blurred than focus pixel: \
         blurred_center={blurred_center}, sharp_center={sharp_center}"
    );
}

// ---------------------------------------------------------------
// DA-02: Myopia — 近方（depth > focus）はシャープ
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_myopia_near_is_sharp() {
    // 32x32 の中央に white dot。
    // depth_map: 全画素 depth=1.0 (最近方)。focus=0.0。
    // Myopia → d > focus なのでボケない（radius=0）。
    let size = 32_u32;
    let input = center_white_dot(size);
    let depth = depth_map_solid(size, 255); // depth≈1.0 (近方)

    let out = depth_aware_blur(input.clone(), &depth, 0.0, 0.1, DepthBlurKind::Myopia).unwrap();

    // ボケなし: 中心は元の白 (255) のまま
    let cx = size / 2;
    let cy = size / 2;
    let center = out.to_rgba8().get_pixel(cx, cy)[0];
    assert_eq!(
        center, 255,
        "near pixel (depth=1.0 > focus=0.0) must stay sharp"
    );
}

// ---------------------------------------------------------------
// DA-03: DepthOfField — 両側がボケる
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_dof_both_blurred() {
    // focus=0.5。depth=0.0 (遠方) と depth=1.0 (近方) の両方がボケる。
    // max_radius_ratio=0.1, size=64 → ビン0の radius = 0.5 * 0.1 * 64 = 3.2px
    let size = 64_u32;
    let input = center_white_dot(size);

    // 遠方 depth=0 (ビン0, center=0.0, delta=-0.5)
    let depth_far = depth_map_solid(size, 0);
    let out_far = depth_aware_blur(
        input.clone(),
        &depth_far,
        0.5,
        0.1,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    // 近方 depth=255 (ビン7, center=1.0, delta=0.5)
    let depth_near = depth_map_solid(size, 255);
    let out_near = depth_aware_blur(
        input.clone(),
        &depth_near,
        0.5,
        0.1,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    // focus と同じ depth=128 (ビン3 or 4, delta≈0)
    let depth_focus = depth_map_solid(size, 128);
    let out_focus =
        depth_aware_blur(input, &depth_focus, 0.5, 0.1, DepthBlurKind::DepthOfField).unwrap();

    let cx = size / 2;
    let cy = size / 2;
    let far_center = out_far.to_rgba8().get_pixel(cx, cy)[0];
    let near_center = out_near.to_rgba8().get_pixel(cx, cy)[0];
    let focus_center = out_focus.to_rgba8().get_pixel(cx, cy)[0];

    assert!(
        far_center < focus_center,
        "DoF: far must be more blurred than focus: far={far_center}, focus={focus_center}"
    );
    assert!(
        near_center < focus_center,
        "DoF: near must be more blurred than focus: near={near_center}, focus={focus_center}"
    );
}

// ---------------------------------------------------------------
// DA-04: depth_map のサイズが異なっても動作する（リサイズされる）
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_wrong_size_depth_map_does_not_panic() {
    // 32x32 の画像に対して 16x16 の depth_map を渡す
    let size = 32_u32;
    let input = solid_rgba(size, size, [100, 150, 200, 255]);
    let depth = depth_map_solid(16, 128); // 異なるサイズ

    let result = depth_aware_blur(input, &depth, 0.5, 0.023, DepthBlurKind::DepthOfField);
    assert!(result.is_ok(), "mismatched depth map size must not panic");
    let out = result.unwrap();
    assert_eq!((out.width(), out.height()), (size, size));
}

// ---------------------------------------------------------------
// DA-05: Hyperopia — 近方（depth > focus）がボケる
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_hyperopia_near_is_blurred() {
    // 64x64 の中央に white dot。
    // depth_map: 全画素 depth=1.0 (最近方)。focus=0.0。
    // Hyperopia → d > focus なのでボケる。
    let size = 64_u32;
    let input = center_white_dot(size);
    let depth_near = depth_map_solid(size, 255); // depth≈1.0 (近方)

    let out_blurred = depth_aware_blur(
        input.clone(),
        &depth_near,
        0.0,
        0.1,
        DepthBlurKind::Hyperopia,
    )
    .unwrap();

    // focus と同深度（depth=0.0, val=0）はボケない
    let depth_far = depth_map_solid(size, 0); // depth≈0.0 (遠方 = focus と同深度)
    let out_sharp =
        depth_aware_blur(input, &depth_far, 0.0, 0.1, DepthBlurKind::Hyperopia).unwrap();

    let cx = size / 2;
    let cy = size / 2;
    let blurred_center = out_blurred.to_rgba8().get_pixel(cx, cy)[0];
    let sharp_center = out_sharp.to_rgba8().get_pixel(cx, cy)[0];
    assert!(
        blurred_center < sharp_center,
        "near pixel (depth=1.0 > focus=0.0) must be more blurred than focus pixel: \
         blurred_center={blurred_center}, sharp_center={sharp_center}"
    );
}

// ---------------------------------------------------------------
// DA-06: strength=0 → identity（blur なし）
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_zero_strength_is_identity() {
    // max_radius_ratio=0.0 のとき radius=0 → どの画素もボケない。
    // 出力が入力と画素単位で一致することを確認。
    let size = 32_u32;
    let input = center_white_dot(size);
    let depth = depth_map_solid(size, 0); // 深度任意

    let out = depth_aware_blur(
        input.clone(),
        &depth,
        1.0,
        0.0, // max_radius_ratio=0 → radius=0
        DepthBlurKind::Myopia,
    )
    .unwrap();

    let in_bytes = input.to_rgba8().into_raw();
    let out_bytes = out.to_rgba8().into_raw();
    assert_eq!(
        in_bytes, out_bytes,
        "max_radius_ratio=0.0 must produce identical output (identity)"
    );
}

// ---------------------------------------------------------------
// DA-07: d=1.0 → scaled=7.0, fract=0.0, 最終ビンが正しく処理される
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_d1_uses_last_bin() {
    // d=1.0 のとき scaled=7.0, floor=7（N_BINS-1）→ 最終ビン専用パスで処理される。
    // DepthOfField, focus=0.0 → d=1.0 は最大 delta=1.0 → 最大ボケ。
    // 中央 white dot が拡散して中心輝度が下がるはず。
    let size = 64_u32;
    let input = center_white_dot(size);
    let depth_max = depth_map_solid(size, 255); // d=1.0 → scaled=7.0 → 最終ビン

    let out_blurred = depth_aware_blur(
        input.clone(),
        &depth_max,
        0.0,
        0.1,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    // d=0.0（focus=0.0 と一致）はシャープ
    let depth_zero = depth_map_solid(size, 0);
    let out_sharp =
        depth_aware_blur(input, &depth_zero, 0.0, 0.1, DepthBlurKind::DepthOfField).unwrap();

    let cx = size / 2;
    let cy = size / 2;
    let blurred_center = out_blurred.to_rgba8().get_pixel(cx, cy)[0];
    let sharp_center = out_sharp.to_rgba8().get_pixel(cx, cy)[0];
    assert!(
        blurred_center < sharp_center,
        "d=1.0 (last bin) must be more blurred than d=0.0 (focus): \
         blurred={blurred_center}, sharp={sharp_center}"
    );
}

// ---------------------------------------------------------------
// DA-08: 線形補間 — ビン境界中間の深度が両端の中間的なボケ量になる
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_lerp_intermediate_depth_is_between_endpoints() {
    // DepthOfField, focus=0.0。ビン0とビン1の境界付近を使う。
    // depth=0/255 と depth=36/255（ビン0とビン1の中間付近）と depth=18/255（その中間）を比較。
    // ボケ量が単調増加（depth が大きい → delta が大きい → blur が強い）かを確認。
    let size = 64_u32;
    let input = center_white_dot(size);

    // depth val=0  → d≈0.000 → delta=0.000 → radius≈0   → シャープ
    let out_near = depth_aware_blur(
        input.clone(),
        &depth_map_solid(size, 0),
        0.0,
        0.1,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    // depth val=18 → d≈0.071 → scaled≈0.496 → ビン0/1境界手前
    let out_mid = depth_aware_blur(
        input.clone(),
        &depth_map_solid(size, 18),
        0.0,
        0.1,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    // depth val=36 → d≈0.141 → scaled≈0.988 → ビン0/1境界ほぼ手前
    let out_far = depth_aware_blur(
        input,
        &depth_map_solid(size, 36),
        0.0,
        0.1,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    let cx = size / 2;
    let cy = size / 2;
    let c_near = out_near.to_rgba8().get_pixel(cx, cy)[0];
    let c_mid = out_mid.to_rgba8().get_pixel(cx, cy)[0];
    let c_far = out_far.to_rgba8().get_pixel(cx, cy)[0];

    // blur が強いほど中心輝度が下がる（単調減少）
    assert!(
        c_near >= c_mid,
        "depth=0 must be at least as sharp as depth=18: near={c_near}, mid={c_mid}"
    );
    assert!(
        c_mid >= c_far,
        "depth=18 must be at least as sharp as depth=36: mid={c_mid}, far={c_far}"
    );
}

// ---------------------------------------------------------------
// DA-09: 異なる深度が混在する画像でも画素ごとに正しいビンが適用される
// ---------------------------------------------------------------

#[test]
fn depth_aware_blur_per_pixel_bin_assignment() {
    // 左半分 depth=0（シャープ）, 右半分 depth=255（ボケ）の depth_map を作成。
    // 中央に white dot（左端付近）。Myopia, focus=1.0。
    // 左の dot 領域（depth=0, 遠方）はボケ、右半分のピクセルは depth=255（近方）→ シャープ。
    use image::{GrayImage, Luma};

    let size = 64_u32;

    // 左半分白 dot の入力画像
    let mut rgba_img = image::RgbaImage::from_pixel(size, size, image::Rgba([0, 0, 0, 255]));
    rgba_img.put_pixel(size / 4, size / 2, image::Rgba([255, 255, 255, 255]));
    let input = DynamicImage::ImageRgba8(rgba_img);

    // 左半分 depth=0, 右半分 depth=255 の depth_map
    let mut depth_img = GrayImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let val = if x < size / 2 { 0u8 } else { 255u8 };
            depth_img.put_pixel(x, y, Luma([val]));
        }
    }
    let depth = DynamicImage::ImageLuma8(depth_img);

    let out = depth_aware_blur(
        input,
        &depth,
        1.0, // focus=1.0
        0.1,
        DepthBlurKind::Myopia,
    )
    .unwrap();

    // 左の dot（depth=0, 遠方）はボケるので (size/4, size/2) 中心輝度が下がる
    let dot_center = out.to_rgba8().get_pixel(size / 4, size / 2)[0];
    // 右エリア（depth=255, 近方）は元々黒なので変化しない（ボケない）
    let right_px = out.to_rgba8().get_pixel(3 * size / 4, size / 2)[0];

    assert!(
        dot_center < 255,
        "left dot (depth=0, far from focus=1.0) must be blurred: dot_center={dot_center}"
    );
    assert_eq!(
        right_px, 0,
        "right area (depth=255, near=focus) must stay black (no blur source): right={right_px}"
    );
}

// ---------------------------------------------------------------
// #166: ビン中心の off-by-one 修正の回帰防止
//
// 旧実装は bin_center=(bin+0.5)/8（0.0625..0.9375）で計算しており、
// 補間側の scaled=d*7（定義域 0.0..=1.0）とズレていたため、focus_depth と
// 厳密に一致する深度でも blur 半径が 0 にならなかった。
// bin_center=bin/(N_BINS-1) に統一し、以下で焦点面のズレが解消されたことを固定する。
// ---------------------------------------------------------------

// DA-10: focus=1.0 のとき depth=1.0（均一領域）は画素単位で完全一致（identity）
//
// size/ratio は旧実装（bin_center=(bin+0.5)/8）の誤差が確実に可視化される値を選ぶ。
// size=32, ratio=0.1 だと旧誤差半径が 0.0625*0.1*32=0.2px となり
// MIN_BLUR_RADIUS_PX(0.5px) 未満に隠れて旧実装でも pass してしまう（PR #177
// レビュー指摘、main へのグラフトで実証済み）。size=64, ratio=0.5 なら
// 旧誤差半径 = 0.0625*0.5*64 = 2.0px となり、MIN_BLUR_RADIUS_PX はもちろん
// build_ellipse_spans の縮退カーネル閾値（半径 <1.0px で退化）も超えるため、
// 旧実装なら確実に FAIL する（PR #177 レビュー対応で `(bin+0.5)/N_BINS` に
// 一時復元して実測済み）。
#[test]
fn depth_aware_blur_focus_at_max_depth_is_exact_identity() {
    // depth=255 (d=1.0) は最終ビン（bin7, center=1.0）を直接使う経路。
    // focus=1.0 と bin7 の center が一致するため delta=0 → radius=0 → 補間なしで
    // raw pixel がそのまま出力されるはず。
    let size = 64_u32;
    let max_radius_ratio = 0.5_f32;
    let input = center_white_dot(size);
    let depth = depth_map_solid(size, 255); // d=1.0

    let out = depth_aware_blur(
        input.clone(),
        &depth,
        1.0,
        max_radius_ratio,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    assert_eq!(
        input.to_rgba8().into_raw(),
        out.to_rgba8().into_raw(),
        "focus_depth=1.0 と一致する depth=1.0 の均一領域は完全一致(identity)のはず（#166）"
    );
}

// DA-11: focus=0.0 のとき depth=0.0（均一領域）は画素単位で完全一致（identity）
//
// DA-10 と同じ理由で size=64, ratio=0.5（旧誤差半径 2.0px）を使う。
#[test]
fn depth_aware_blur_focus_at_min_depth_is_exact_identity() {
    // depth=0 (d=0.0) はビン0/1 ペアの t=0（補間係数ゼロ）経路。
    // bin0 の center=0.0 と focus=0.0 が一致するため delta=0 → radius=0。
    // t=0 により ceil 側の値は寄与せず、raw pixel がそのまま出力されるはず。
    let size = 64_u32;
    let max_radius_ratio = 0.5_f32;
    let input = center_white_dot(size);
    let depth = depth_map_solid(size, 0); // d=0.0

    let out = depth_aware_blur(
        input.clone(),
        &depth,
        0.0,
        max_radius_ratio,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    assert_eq!(
        input.to_rgba8().into_raw(),
        out.to_rgba8().into_raw(),
        "focus_depth=0.0 と一致する depth=0.0 の均一領域は完全一致(identity)のはず（#166）"
    );
}

// DA-12: focus=0.5 のとき、focus に最も近い深度ほど半径が最小（中心輝度が最も高い）
//
// 一般 sanity（#166 の off-by-one 回帰防止は DA-13/14 が担う）。旧実装の
// (bin+0.5)/8 でも新実装の bin/(N_BINS-1) でも「focus に近いほど半径が小さい」
// という単調性自体は成り立つため、この off-by-one を検出する目的では使わない。
#[test]
fn depth_aware_blur_focus_mid_radius_is_minimal_near_focus() {
    let size = 64_u32;
    let input = center_white_dot(size);

    let center_at = |depth_val: u8| -> u8 {
        let depth = depth_map_solid(size, depth_val);
        let out =
            depth_aware_blur(input.clone(), &depth, 0.5, 0.1, DepthBlurKind::DepthOfField).unwrap();
        out.to_rgba8().get_pixel(size / 2, size / 2)[0]
    };

    let c_near_focus = center_at(128); // d≈0.502, |delta|≈0.002（ほぼ焦点）
    let c_mid = center_at(192); // d≈0.753, |delta|≈0.253
    let c_far = center_at(0); // d=0.0, |delta|=0.5（最遠）

    assert!(
        c_near_focus >= c_mid,
        "focus=0.5 に最も近い深度が最もボケが弱い(輝度が高い)はず: near_focus={c_near_focus}, mid={c_mid}"
    );
    assert!(
        c_mid >= c_far,
        "focus から遠い深度ほどボケが強い(輝度が低い)はず: mid={c_mid}, far={c_far}"
    );
}

// DA-13/DA-14: 深度 0.0 / 1.0 の両端で半径が意図の設計式
// `max_radius_ratio * min_dim * |depth - focus|` と一致することを、
// ellipse_blur を直接呼んだ参照結果とのバイト一致で確認する。

#[test]
fn depth_aware_blur_extreme_depth_one_radius_matches_design_formula() {
    // d=1.0（最終ビン専用パス）。focus=0.0 → |delta|=1.0 → 意図半径 = ratio*min_dim。
    let size = 40_u32;
    let max_radius_ratio = 0.1_f32;
    let min_dim = size as f32;
    let input = center_white_dot(size);
    let depth = depth_map_solid(size, 255); // d=1.0

    let out = depth_aware_blur(
        input.clone(),
        &depth,
        0.0,
        max_radius_ratio,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    let rgba = input.to_rgba8();
    let (linear, alpha) = rgba_to_linear_planes(&rgba);
    let expected_radius = max_radius_ratio * min_dim;
    let expected_linear = ellipse_blur(&linear, size, size, expected_radius, expected_radius, 0.0);
    let expected = linear_planes_to_rgba(&expected_linear, &alpha, size, size);

    assert_eq!(
        out.to_rgba8().into_raw(),
        expected.into_raw(),
        "d=1.0 の半径は max_radius_ratio*min_dim(意図値)に一致するはず（#166）"
    );
}

#[test]
fn depth_aware_blur_extreme_depth_zero_radius_matches_design_formula() {
    // d=0.0（ビン0, t=0 の非補間パス）。focus=1.0 → |delta|=1.0 → 意図半径 = ratio*min_dim。
    let size = 40_u32;
    let max_radius_ratio = 0.1_f32;
    let min_dim = size as f32;
    let input = center_white_dot(size);
    let depth = depth_map_solid(size, 0); // d=0.0

    let out = depth_aware_blur(
        input.clone(),
        &depth,
        1.0,
        max_radius_ratio,
        DepthBlurKind::DepthOfField,
    )
    .unwrap();

    let rgba = input.to_rgba8();
    let (linear, alpha) = rgba_to_linear_planes(&rgba);
    let expected_radius = max_radius_ratio * min_dim;
    let expected_linear = ellipse_blur(&linear, size, size, expected_radius, expected_radius, 0.0);
    let expected = linear_planes_to_rgba(&expected_linear, &alpha, size, size);

    assert_eq!(
        out.to_rgba8().into_raw(),
        expected.into_raw(),
        "d=0.0 の半径は max_radius_ratio*min_dim(意図値)に一致するはず（#166）"
    );
}

// ---------------------------------------------------------------
// #29: diplopia / nystagmus / starbursts
// ---------------------------------------------------------------

#[test]
fn diplopia_shifts_ghost_image() {
    // 32x32、左半分を白、右半分を黒にして右に少しずらす
    let size = 32_u32;
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    // 左半分を白
    for y in 0..size {
        for x in 0..(size / 2) {
            img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }
    // 右半分の左端（x = size/2）の元の値は 0
    let check_x = size / 2;
    let check_y = size / 2;
    let orig_px = img.get_pixel(check_x, check_y)[0];
    assert_eq!(orig_px, 0, "original should be black at check point");

    let input = DynamicImage::ImageRgba8(img);
    // offset_x=0.1 → dx = 0.1 * 32 = 3px 右シフト → 幽霊は左の白領域から来る
    let out = diplopia(input, 1.0, 0.1, 0.0, 1.0).unwrap();
    let out_px_val = out.to_rgba8().get_pixel(check_x, check_y)[0];
    assert!(
        out_px_val > orig_px,
        "diplopia should show ghost (alpha blend): orig={orig_px}, out={out_px_val}"
    );
}

#[test]
fn diplopia_strength_zero_is_identity() {
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 5) as u8, (y * 7) as u8, 128, 255]);
    }
    let orig = img.clone().into_raw();
    let out = diplopia(DynamicImage::ImageRgba8(img), 0.0, 0.1, 0.1, 0.7).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    let max_err = orig
        .iter()
        .zip(out_raw.iter())
        .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
        .max()
        .unwrap_or(0);
    assert!(
        max_err <= 1,
        "strength=0 should be identity, max_err={max_err}"
    );
}

#[test]
fn diplopia_white_on_white_no_overflow() {
    // 白飛び防止: orig=white, ghost=white, strength=1, ghost_strength=1 → 全ピクセル 255 のまま
    let size = 16_u32;
    let img = RgbaImage::from_pixel(size, size, Rgba([255, 255, 255, 255]));
    let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.1, 0.0, 1.0).unwrap();
    let out_rgba = out.to_rgba8();
    for px in out_rgba.pixels() {
        assert_eq!(px[0], 255, "R channel must remain 255");
        assert_eq!(px[1], 255, "G channel must remain 255");
        assert_eq!(px[2], 255, "B channel must remain 255");
    }
}

#[test]
fn diplopia_blend_ratio_at_half_strength() {
    // 中間値の混合比: orig=黒(0), ghost=白(255), strength=1, ghost_strength=0.5 → 出力が≒127±2
    // ghost_alpha = ghost_strength * strength = 0.5 * 1.0 = 0.5
    // alpha blend: out = orig * 0.5 + ghost * 0.5 → 中間値になるはず
    let size = 16_u32;
    // 左半分白・右半分黒の画像で、オフセットなし（dx=0）→ 各ピクセルで orig=ghost=同じ色
    // なので別の方法: 全ピクセル黒の画像に offset=0（幽霊も黒）ではなく、
    // orig=黒で ghost=白 を得るために 2 枚の画像を使う必要があるが diplopia は 1 枚から作る。
    // 代わりに: 左半分白・右半分黒の画像で、右端のチェック点を使う。
    // offset_x=0.5 → dx = 0.5 * 16 = 8px。右半分の任意点(x=12)の ghost は左半分(x=4)白。
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    for y in 0..size {
        for x in 0..(size / 2) {
            img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }
    // check_x=12: orig=black(0), ghost(12-8=4)=white(255)
    let check_x = 12_u32;
    let check_y = size / 2;
    let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.5, 0.0, 0.5).unwrap();
    let val = out.to_rgba8().get_pixel(check_x, check_y)[0];
    // linear sRGB 空間で 0.5 blendすると sRGB変換後は約 188 になる（ガンマ補正の影響）
    // 単純な加算合成なら 255 になっていたが、alpha blend では中間値に抑えられる
    assert!(
        (183..=193).contains(&val),
        "half ghost_strength alpha blend should produce ≈188 (sRGB of linear 0.5), got {val}"
    );
    // また、orig(0) と ghost(255) の単純平均 127 より大きいはず（linear→sRGB変換で増加）
    assert!(
        val > 50,
        "blend result should be clearly above black, got {val}"
    );
}

#[test]
fn diplopia_ghost_strength_zero_is_identity() {
    // ghost_strength=0 の identity: strength=1.0 でも ghost_strength=0 なら orig と一致
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 7) as u8, (y * 5) as u8, 100, 255]);
    }
    let orig = img.clone().into_raw();
    let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.1, 0.1, 0.0).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    let max_err = orig
        .iter()
        .zip(out_raw.iter())
        .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
        .max()
        .unwrap_or(0);
    assert!(
        max_err <= 1,
        "ghost_strength=0 should be identity, max_err={max_err}"
    );
}

#[test]
fn diplopia_output_never_exceeds_max() {
    // グラデーション画像で strength=0.7, ghost_strength=0.8 → 関数がパニックせず正常に返ること
    // (alpha blend で overflow しないことの確認。u8 の範囲は型保証済み)
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 8) as u8, (y * 8) as u8, 200, 255]);
    }
    let result = diplopia(DynamicImage::ImageRgba8(img), 0.7, 0.2, 0.1, 0.8);
    assert!(
        result.is_ok(),
        "diplopia should not panic on gradient image"
    );
    // alpha blend の数学的性質から出力画像が正しく生成されること（u8 の上限は型保証）
    let _ = result.unwrap().to_rgba8();
}

#[test]
fn diplopia_luminance_preserved_vs_additive() {
    // 輝度保存: orig=グレー(128), ghost=グレー(128), strength=1, ghost_strength=1 → 出力が≒128
    // (旧加算合成なら 255 になっていた)
    // offset=0 → orig=ghost=同じピクセル、alpha blend でも同じ値が出力されるはず
    let size = 16_u32;
    let img = RgbaImage::from_pixel(size, size, Rgba([128, 128, 128, 255]));
    let out = diplopia(DynamicImage::ImageRgba8(img), 1.0, 0.0, 0.0, 1.0).unwrap();
    let out_rgba = out.to_rgba8();
    for px in out_rgba.pixels() {
        let val = px[0] as i32;
        assert!(
            (val - 128).abs() <= 2,
            "alpha blend of gray+gray should preserve luminance ≈128, got {val}"
        );
    }
}

#[test]
fn nystagmus_blurs_image() {
    let size = 32_u32;
    let input = center_white_dot(size);
    let cx = size / 2;
    let cy = size / 2;
    let orig_center = input.to_rgba8().get_pixel(cx, cy)[0];

    let out = nystagmus(input, 1.0, 0.1, 0.0).unwrap();
    let out_center = out.to_rgba8().get_pixel(cx, cy)[0];
    assert!(
        out_center < orig_center,
        "nystagmus should blur white dot: orig={orig_center}, out={out_center}"
    );
}

#[test]
fn nystagmus_zero_amplitude_is_identity() {
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 6) as u8, (y * 8) as u8, 100, 255]);
    }
    let orig = img.clone().into_raw();
    let out = nystagmus(DynamicImage::ImageRgba8(img), 1.0, 0.0, 0.0).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    let max_err = orig
        .iter()
        .zip(out_raw.iter())
        .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
        .max()
        .unwrap_or(0);
    assert!(
        max_err <= 1,
        "amplitude=0 should be identity, max_err={max_err}"
    );
}

#[test]
fn starbursts_brightens_near_bright_pixels() {
    let size = 32_u32;
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    img.put_pixel(size / 2, size / 2, Rgba([255, 255, 255, 255]));

    // 中央から 3px 離れた画素の元の値
    let nearby_x = size / 2 + 3;
    let nearby_y = size / 2;
    let orig_nearby = img.get_pixel(nearby_x, nearby_y)[0];

    let out = starbursts(DynamicImage::ImageRgba8(img), 1.0, 8, 0.2, 0.5, 0.0).unwrap();
    let out_nearby = out.to_rgba8().get_pixel(nearby_x, nearby_y)[0];

    assert!(
        out_nearby > orig_nearby,
        "starbursts should brighten pixels near bright source: orig={orig_nearby}, out={out_nearby}"
    );
}

#[test]
fn starbursts_strength_zero_is_identity() {
    let size = 32_u32;
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    img.put_pixel(size / 2, size / 2, Rgba([255, 255, 255, 255]));
    let orig = img.clone().into_raw();
    let out = starbursts(DynamicImage::ImageRgba8(img), 0.0, 6, 0.1, 0.5, 0.0).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    let max_err = orig
        .iter()
        .zip(out_raw.iter())
        .map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs())
        .max()
        .unwrap_or(0);
    // strength=0 は early return するため byte-exact 一致するはず
    assert!(
        max_err == 0,
        "strength=0 should be byte-exact identity, max_err={max_err}"
    );
}

#[test]
fn starbursts_dispersion_one_produces_rainbow() {
    // 中央に白い輝点を置き、dispersion=1.0 で光芒を生成する。
    // 虹色光芒なのでRGB チャネルが互いに異なる値を持つことを確認する。
    let size = 64_u32;
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    img.put_pixel(size / 2, size / 2, Rgba([255, 255, 255, 255]));
    let out = starbursts(DynamicImage::ImageRgba8(img), 1.0, 8, 0.3, 0.5, 1.0)
        .unwrap()
        .to_rgba8();

    // 全ピクセルの R/G/B の最大値を収集し、チャネル間で差があることを確認
    let mut any_diff = false;
    for px in out.pixels() {
        if px[0] != px[1] || px[1] != px[2] {
            any_diff = true;
            break;
        }
    }
    assert!(
        any_diff,
        "dispersion=1.0 should produce colored (non-gray) pixels"
    );
}

#[test]
fn eye_strain_strength_zero_is_identity() {
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 7) as u8, (y * 7) as u8, 128, 255]);
    }
    let orig = img.clone().into_raw();
    let out = eye_strain(DynamicImage::ImageRgba8(img), 0.0).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    assert_eq!(
        orig, out_raw,
        "eye_strain strength=0 should be byte-exact identity"
    );
}

#[test]
fn dry_eye_strength_zero_is_identity() {
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 7) as u8, (y * 7) as u8, 128, 255]);
    }
    let orig = img.clone().into_raw();
    let out = dry_eye(DynamicImage::ImageRgba8(img), 0.0).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    assert_eq!(
        orig, out_raw,
        "dry_eye strength=0 should be byte-exact identity"
    );
}

#[test]
fn eye_strain_reduces_contrast() {
    // 真っ白と真っ黒が混在する画像で strength=1 の分散が strength=0 より小さいことを確認
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, _y, px) in img.enumerate_pixels_mut() {
        let v = if x < size / 2 { 0u8 } else { 255u8 };
        *px = Rgba([v, v, v, 255]);
    }
    let out = eye_strain(DynamicImage::ImageRgba8(img), 1.0).unwrap();
    let out_raw = out.to_rgba8();
    // 最大値 - 最小値がコントラスト圧縮で小さくなっているはず
    let min_r = out_raw.pixels().map(|p| p[0]).min().unwrap_or(0);
    let max_r = out_raw.pixels().map(|p| p[0]).max().unwrap_or(255);
    assert!(
        (max_r as i32 - min_r as i32) < 255,
        "eye_strain should reduce contrast: min={min_r} max={max_r}"
    );
}

// =================================================================
// Issue #55: Metamorphopsia（歪視）テスト
// =================================================================

#[test]
fn metamorphopsia_strength_zero_is_identity() {
    // strength=0 → byte-exact identity（max_err ≤ 1 を許容するが実際は完全一致）
    let size = 64_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
    }
    let orig_raw = img.clone().into_raw();
    let out = metamorphopsia(DynamicImage::ImageRgba8(img), 0.0, 4.0, 42).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    // strength=0 では byte-exact identity
    assert_eq!(
        orig_raw, out_raw,
        "metamorphopsia strength=0 must be byte-exact identity"
    );
}

#[test]
fn metamorphopsia_strength_one_changes_pixels() {
    // strength=1 → 少なくとも一部のピクセルが元画像と異なること
    let size = 64_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
    }
    let orig_raw = img.clone().into_raw();
    let out = metamorphopsia(DynamicImage::ImageRgba8(img), 1.0, 4.0, 42).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    // 少なくとも 1 バイト異なることを確認
    let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
    assert!(
        differs,
        "metamorphopsia strength=1 must change at least some pixels"
    );
}

#[test]
fn metamorphopsia_preserves_image_size() {
    let img = solid_rgba(48, 32, [200, 100, 50, 255]);
    let out = metamorphopsia(img, 0.8, 4.0, 123).unwrap();
    assert_eq!(out.width(), 48);
    assert_eq!(out.height(), 32);
}

#[test]
fn metamorphopsia_preserves_alpha() {
    // alpha チャンネルは sample_bilinear が保持するので確認
    let size = 32_u32;
    let mut img = RgbaImage::new(size, size);
    for px in img.pixels_mut() {
        *px = Rgba([128, 64, 32, 200]);
    }
    let out = metamorphopsia(DynamicImage::ImageRgba8(img), 1.0, 4.0, 1).unwrap();
    for px in out.to_rgba8().pixels() {
        assert_eq!(px[3], 200, "alpha must be preserved through metamorphopsia");
    }
}

#[test]
fn metamorphopsia_different_seeds_give_different_results() {
    // 異なる seed では異なる歪みパターンになること
    let size = 64_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
    }
    let dyn_img = DynamicImage::ImageRgba8(img);
    let out1 = metamorphopsia(dyn_img.clone(), 1.0, 4.0, 1)
        .unwrap()
        .to_rgba8()
        .into_raw();
    let out2 = metamorphopsia(dyn_img, 1.0, 4.0, 99999)
        .unwrap()
        .to_rgba8()
        .into_raw();
    let differs = out1.iter().zip(out2.iter()).any(|(a, b)| a != b);
    assert!(
        differs,
        "different seeds must produce different distortion patterns"
    );
}

// ---------------------------------------------------------------
// Issue #60: vertigo / bppv_rotation / vestibular_neuritis テスト
// ---------------------------------------------------------------

#[test]
fn vertigo_strength_one_differs_from_input() {
    // グラデーション画像を使う（均一色だと回転後も同一になるため）
    let size = 64_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
    }
    let orig_raw = img.clone().into_raw();
    let out = vertigo(DynamicImage::ImageRgba8(img), 1.0, 0.25).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
    assert!(
        differs,
        "vertigo strength=1 must change at least some pixels"
    );
}

#[test]
fn bppv_rotation_strength_one_differs_from_input() {
    let size = 64_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
    }
    let orig_raw = img.clone().into_raw();
    // time_t=0.1 は急速相（angle_norm > 0）
    let out = bppv_rotation(DynamicImage::ImageRgba8(img), 1.0, 0.1).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
    assert!(
        differs,
        "bppv_rotation strength=1 must change at least some pixels"
    );
}

#[test]
fn vestibular_neuritis_strength_one_differs_from_input() {
    let size = 64_u32;
    let mut img = RgbaImage::new(size, size);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 100, 255]);
    }
    let orig_raw = img.clone().into_raw();
    let out = vestibular_neuritis(DynamicImage::ImageRgba8(img), 1.0).unwrap();
    let out_raw = out.to_rgba8().into_raw();
    let differs = orig_raw.iter().zip(out_raw.iter()).any(|(a, b)| a != b);
    assert!(
        differs,
        "vestibular_neuritis strength=1 must change at least some pixels"
    );
}

// ---------------------------------------------------------------
// Issue #51: nyctalopia Purkinje shift
// ---------------------------------------------------------------

#[test]
fn nyctalopia_purkinje_shift_blue_channel_increases() {
    // Purkinje shift: strength=1 で青チャネル平均が入力より高いことを確認
    // 白色画像を使用（すべてのチャンネルが同一値なので青の増加を検出しやすい）
    let mut img = RgbaImage::new(16, 16);
    for px in img.pixels_mut() {
        *px = Rgba([200, 200, 200, 255]);
    }
    let orig_b_sum: u32 = img.pixels().map(|p| p[2] as u32).sum();
    let orig_r_sum: u32 = img.pixels().map(|p| p[0] as u32).sum();

    let out = nyctalopia(DynamicImage::ImageRgba8(img), 1.0).unwrap();
    let out_rgba = out.to_rgba8();
    let out_b_sum: u32 = out_rgba.pixels().map(|p| p[2] as u32).sum();
    let out_r_sum: u32 = out_rgba.pixels().map(|p| p[0] as u32).sum();

    // strength=1 では全体が暗化するため絶対値は下がるが、
    // 青/赤 の比率で Purkinje shift（青↑赤↓相対）を確認する。
    // 暗化後: R = orig * (1 - 0.2) * dark_factor, B = orig * (1 + 0.1) * dark_factor
    // B / R = 1.1 / 0.8 = 1.375 > 1 になるはず
    assert!(
        out_b_sum > out_r_sum,
        "Purkinje shift: blue channel sum ({out_b_sum}) should exceed red ({out_r_sum}) at strength=1"
    );
    // 全体が暗化していることも確認
    assert!(
        out_b_sum < orig_b_sum,
        "nyctalopia darkens: blue sum {out_b_sum} < orig {orig_b_sum}"
    );
    assert!(
        out_r_sum < orig_r_sum,
        "nyctalopia darkens: red sum {out_r_sum} < orig {orig_r_sum}"
    );
}

// ---------------------------------------------------------------
// Issue #52: glaucoma 弧状暗点オプション
// ---------------------------------------------------------------

#[test]
fn glaucoma_vignette_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [180, 120, 60, 255]);
    let out = glaucoma(input.clone(), 0.0, GlaucomaMode::Vignette).unwrap();
    assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
}

#[test]
fn glaucoma_arcuate_superior_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [180, 120, 60, 255]);
    let out = glaucoma(input.clone(), 0.0, GlaucomaMode::ArcuateSuperior).unwrap();
    assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
}

#[test]
fn glaucoma_arcuate_inferior_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [180, 120, 60, 255]);
    let out = glaucoma(input.clone(), 0.0, GlaucomaMode::ArcuateInferior).unwrap();
    assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
}

#[test]
fn glaucoma_biarcuate_strength_zero_is_identity() {
    let input = solid_rgba(32, 32, [180, 120, 60, 255]);
    let out = glaucoma(input.clone(), 0.0, GlaucomaMode::Biarcuate).unwrap();
    assert_eq!(input.to_rgba8().into_raw(), out.to_rgba8().into_raw());
}

#[test]
fn glaucoma_vignette_strength_one_darkens() {
    // 十分大きな画像で周辺部が暗化することを確認
    let mut img = RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
    }
    let orig_sum: u32 = img
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    let out = glaucoma(DynamicImage::ImageRgba8(img), 1.0, GlaucomaMode::Vignette).unwrap();
    let out_sum: u32 = out
        .to_rgba8()
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    assert!(
        out_sum < orig_sum,
        "glaucoma Vignette strength=1 must darken: {out_sum} < {orig_sum}"
    );
}

#[test]
fn glaucoma_arcuate_superior_strength_one_darkens() {
    let mut img = RgbaImage::new(64, 64);
    for px in img.pixels_mut() {
        *px = Rgba([200, 200, 200, 255]);
    }
    let orig_sum: u32 = img
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    let out = glaucoma(
        DynamicImage::ImageRgba8(img),
        1.0,
        GlaucomaMode::ArcuateSuperior,
    )
    .unwrap();
    let out_sum: u32 = out
        .to_rgba8()
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    assert!(
        out_sum < orig_sum,
        "glaucoma ArcuateSuperior strength=1 must darken"
    );
}

#[test]
fn glaucoma_arcuate_inferior_strength_one_darkens() {
    let mut img = RgbaImage::new(64, 64);
    for px in img.pixels_mut() {
        *px = Rgba([200, 200, 200, 255]);
    }
    let orig_sum: u32 = img
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    let out = glaucoma(
        DynamicImage::ImageRgba8(img),
        1.0,
        GlaucomaMode::ArcuateInferior,
    )
    .unwrap();
    let out_sum: u32 = out
        .to_rgba8()
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    assert!(
        out_sum < orig_sum,
        "glaucoma ArcuateInferior strength=1 must darken"
    );
}

#[test]
fn glaucoma_biarcuate_strength_one_darkens() {
    let mut img = RgbaImage::new(64, 64);
    for px in img.pixels_mut() {
        *px = Rgba([200, 200, 200, 255]);
    }
    let orig_sum: u32 = img
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    let out = glaucoma(DynamicImage::ImageRgba8(img), 1.0, GlaucomaMode::Biarcuate).unwrap();
    let out_sum: u32 = out
        .to_rgba8()
        .pixels()
        .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
        .sum();
    assert!(
        out_sum < orig_sum,
        "glaucoma Biarcuate strength=1 must darken"
    );
}

// -------------------------------------------------------
// contrast_sensitivity tests
// -------------------------------------------------------

#[test]
fn contrast_sensitivity_strength_zero_identity() {
    let mut img = RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 3 + y * 7) as u8, (y * 4) as u8, 128, 255]);
    }
    let orig = img.clone();
    let out = contrast_sensitivity(DynamicImage::ImageRgba8(img), 0.0)
        .unwrap()
        .to_rgba8();
    // PSNR >= 60 dB
    let mse: f64 = orig
        .pixels()
        .zip(out.pixels())
        .map(|(a, b)| {
            (0..3)
                .map(|i| {
                    let d = a[i] as f64 - b[i] as f64;
                    d * d
                })
                .sum::<f64>()
        })
        .sum::<f64>()
        / (64.0 * 64.0 * 3.0);
    if mse > 0.0 {
        let psnr = 10.0 * (255.0_f64 * 255.0 / mse).log10();
        assert!(psnr >= 60.0, "PSNR={psnr:.1} dB, expected >= 60 dB");
    }
}

#[test]
fn contrast_sensitivity_strength_one_reduces_variance() {
    let mut img = RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
    }
    let orig = img.clone();
    let out = contrast_sensitivity(DynamicImage::ImageRgba8(img), 1.0)
        .unwrap()
        .to_rgba8();

    let luma = |p: &image::Rgba<u8>| -> f64 {
        0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64
    };
    let variance = |pixels: &RgbaImage| -> f64 {
        let vals: Vec<f64> = pixels.pixels().map(luma).collect();
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64
    };

    let var_in = variance(&orig);
    let var_out = variance(&out);
    assert!(var_out < var_in, "contrast_sensitivity strength=1 must reduce luminance variance (in={var_in:.2}, out={var_out:.2})");
}

// -------------------------------------------------------
// detail_loss tests
// -------------------------------------------------------

#[test]
fn detail_loss_strength_zero_identity() {
    let mut img = RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 3 + y * 7) as u8, (y * 4) as u8, 128, 255]);
    }
    let orig = img.clone().into_raw();
    let out = detail_loss(DynamicImage::ImageRgba8(img), 0.0)
        .unwrap()
        .to_rgba8()
        .into_raw();
    assert_eq!(orig, out, "detail_loss strength=0 must be identity");
}

#[test]
fn detail_loss_strength_one_reduces_stddev() {
    let mut img = RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255]);
    }
    let orig = img.clone();
    let out = detail_loss(DynamicImage::ImageRgba8(img), 1.0)
        .unwrap()
        .to_rgba8();

    let luma = |p: &image::Rgba<u8>| -> f64 {
        0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64
    };
    let stddev = |pixels: &RgbaImage| -> f64 {
        let vals: Vec<f64> = pixels.pixels().map(luma).collect();
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64).sqrt()
    };

    let sd_in = stddev(&orig);
    let sd_out = stddev(&out);
    assert!(
        sd_out < sd_in,
        "detail_loss strength=1 must reduce stddev (in={sd_in:.2}, out={sd_out:.2})"
    );
}

// -------------------------------------------------------
// teichopsia tests
// -------------------------------------------------------

#[test]
fn teichopsia_strength_zero_identity() {
    let mut img = RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 3 + y * 7) as u8, (y * 4) as u8, 128, 255]);
    }
    let orig = img.clone();
    let out = teichopsia(DynamicImage::ImageRgba8(img), 0.0)
        .unwrap()
        .to_rgba8();
    // PSNR >= 60 dB
    let mse: f64 = orig
        .pixels()
        .zip(out.pixels())
        .map(|(a, b)| {
            (0..3)
                .map(|i| {
                    let d = a[i] as f64 - b[i] as f64;
                    d * d
                })
                .sum::<f64>()
        })
        .sum::<f64>()
        / (64.0 * 64.0 * 3.0);
    if mse > 0.0 {
        let psnr = 10.0 * (255.0_f64 * 255.0 / mse).log10();
        assert!(psnr >= 60.0, "PSNR={psnr:.1} dB expected >= 60 dB");
    }
}

#[test]
fn teichopsia_strength_one_darkens_center() {
    let mut img = RgbaImage::new(64, 64);
    for px in img.pixels_mut() {
        *px = Rgba([200, 200, 200, 255]);
    }
    let out = teichopsia(DynamicImage::ImageRgba8(img), 1.0)
        .unwrap()
        .to_rgba8();
    // 中心ピクセル（scotoma）が暗化されているか
    let center = out.get_pixel(32, 32);
    let brightness = center[0] as u32 + center[1] as u32 + center[2] as u32;
    assert!(
        brightness < 600,
        "teichopsia strength=1 must darken center (got {brightness})"
    );
}

#[test]
fn teichopsia_upper_half_ring_never_darker_than_input() {
    // #168: ジグザグリング（dist 0.2..=0.5）は加算光（brightness = saw*s*fade*0.6 >= 0）
    // のはず。修正前は atan2() が uy<0（画像上半分）で常に負角度を返し、
    // f32::fract() が負入力に負を返すため saw ∈ (-1,1) になり、上半分のリング
    // 全体が暗化していた（意図した加算光と逆）。
    let w = 64u32;
    let h = 64u32;
    let mut img = RgbaImage::new(w, h);
    for px in img.pixels_mut() {
        *px = Rgba([128, 128, 128, 255]);
    }
    let input = img.clone();
    let out = teichopsia(DynamicImage::ImageRgba8(img), 1.0)
        .unwrap()
        .to_rgba8();

    let w_f = w as f32;
    let h_f = h as f32;
    let aspect = w_f / h_f;
    let mut checked = 0u32;
    for y in 0..(h / 2) {
        // uy < 0 の上半分のみ
        for x in 0..w {
            let ux = (x as f32 / w_f) - 0.5;
            let uy = ((y as f32 / h_f) - 0.5) / aspect;
            let dist = (ux * ux + uy * uy).sqrt();
            if !(0.2..=0.5).contains(&dist) {
                continue;
            }
            let src = input.get_pixel(x, y);
            let dst = out.get_pixel(x, y);
            checked += 1;
            for c in 0..3 {
                assert!(
                    dst[c] >= src[c],
                    "teichopsia ring darkened at uy<0 pixel ({x},{y}): channel {c} {}\u{2192}{} \
                     (加算光のはずが暗化した)",
                    src[c],
                    dst[c]
                );
            }
        }
    }
    assert!(checked > 0, "テスト前提: リング領域に uy<0 の画素が無い");
}

// -------------------------------------------------------
// flickering_stars tests
// -------------------------------------------------------

#[test]
fn flickering_stars_strength_zero_identity() {
    let mut img = RgbaImage::new(64, 64);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgba([(x * 3 + y * 7) as u8, (y * 4) as u8, 100, 255]);
    }
    let orig = img.clone().into_raw();
    let out = flickering_stars(DynamicImage::ImageRgba8(img), 0.0, 42)
        .unwrap()
        .to_rgba8()
        .into_raw();
    assert_eq!(orig, out, "flickering_stars strength=0 must be identity");
}

#[test]
fn flickering_stars_strength_one_increases_max_brightness() {
    let mut img = RgbaImage::new(64, 64);
    for px in img.pixels_mut() {
        // 暗めの画像で始める（最大輝度が additive で上がることを確認）
        *px = Rgba([50, 50, 50, 255]);
    }
    let orig_max: u8 = img
        .pixels()
        .map(|p| p[0].max(p[1]).max(p[2]))
        .max()
        .unwrap_or(0);
    let out = flickering_stars(DynamicImage::ImageRgba8(img), 1.0, 42)
        .unwrap()
        .to_rgba8();
    let out_max: u8 = out
        .pixels()
        .map(|p| p[0].max(p[1]).max(p[2]))
        .max()
        .unwrap_or(0);
    assert!(
        out_max > orig_max,
        "flickering_stars strength=1 must increase max brightness (orig={orig_max}, out={out_max})"
    );
}
