//! 視野欠損フィルタ。
//!
//! 緑内障・加齢黄斑変性・半盲・視野狭窄。`GlaucomaMode` enum はこの領域専用。
//! `FieldLossMode`（Darken/Blur）は 4 フィルタ共通の表現モード payload（#171）。

use super::*;
use image::DynamicImage;

// ---------------------------------------------------------------
// Phase 3a: 視野異常 (Issue #5) — glaucoma / macular_degeneration / hemianopia / tunnel_vision
// ---------------------------------------------------------------

/// 緑内障シミュレーションのモード。
///
/// `Vignette` は既存の均等 vignetting 実装（後方互換）。
/// `ArcuateSuperior` / `ArcuateInferior` / `Biarcuate` は視神経乳頭を中心とした
/// 弧状暗点を生成する。
///
/// ## 医学的背景
///
/// 緑内障の視野欠損は視神経乳頭（ON head）の損傷パターンに対応する弧状暗点
/// （arcuate scotoma）として現れることが多い。均等な周辺暗化（Vignette）は
/// 近似であり、実臨床ではBjerrumの弧状暗点が典型的。
///
/// - `Vignette`: 旧実装の中心保存 + 周辺均等暗化（近似）
/// - `ArcuateSuperior`: 上方弧状暗点（Bjerrum 上方）
/// - `ArcuateInferior`: 下方弧状暗点（Bjerrum 下方）
/// - `Biarcuate`: 両方の弧状暗点（進行した緑内障）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlaucomaMode {
    /// 既存実装（後方互換）: 中心保存 + 周辺 smoothstep vignetting
    Vignette,
    /// 上方弧状暗点（Bjerrum 上方弧状暗点）
    ArcuateSuperior,
    /// 下方弧状暗点（Bjerrum 下方弧状暗点）
    ArcuateInferior,
    /// 両弧状暗点（上下両方、進行例）
    Biarcuate,
}

impl GlaucomaMode {
    /// glaucoma.frag の `uMode` uniform に渡す GLSL モード値。
    /// 0=Vignette, 1=ArcuateSuperior, 2=ArcuateInferior, 3=Biarcuate。
    /// .frag の分岐（[`crate::shaders::glaucoma_glsl`]）と 1 対 1 対応する。
    pub fn to_glsl_mode(self) -> i32 {
        match self {
            GlaucomaMode::Vignette => 0,
            GlaucomaMode::ArcuateSuperior => 1,
            GlaucomaMode::ArcuateInferior => 2,
            GlaucomaMode::Biarcuate => 3,
        }
    }
}

/// 視野欠損の表現モード（#171）。
///
/// `Darken`（既定）は欠損部を黒方向へ暗転させる既存挙動（後方互換、golden 不変）。
/// `Blur` は VIP-Sim（`myFieldLoss.cs`）の mipmap 方式に相当し、欠損部を
/// 「ぼけ + 彩度低下」で表現する（黒には落とさない）。緑内障・黄斑変性の
/// 患者の多くは暗点を「黒い影」ではなく「ぼやけ・埋められた感じ」として
/// 知覚し、暗点の自覚がないケースも多いとされる。Blur モードはこの臨床像に
/// 寄せた表現である（提案 Issue kako-jun/sensus#171）。
///
/// glaucoma / macular_degeneration / tunnel_vision / hemianopia の 4 フィルタが
/// 共通 payload として持つ。GLSL シェーダ（`.frag`）は現状 `Darken` のみ対応で、
/// `Blur` は CPU 実装のみ（フォローアップ課題、`docs/overview.md` 参照）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldLossMode {
    /// 既定・後方互換: 欠損部を黒方向へ暗転させる。
    #[default]
    Darken,
    /// 欠損部を disk blur + 彩度低下で表現する（VIP-Sim mip 方式相当）。
    Blur,
}

/// `FieldLossMode::Blur` における最大 disk blur 半径比（min(W,H) 比）。
///
/// 導出: refraction 系（myopia 等）の disk blur はディオプター起源の defocus 量を
/// Smith-Helmholtz 近似で換算するが、視野欠損（scotoma）は光学的な defocus では
/// なく「情報の消失」であるため同じ換算式は使えない。VIP-Sim の mipmap 方式は
/// 欠損部を最下位ミップ（元解像度に対しおよそ 1 辺 1/8、面積比 1/64）まで落として
/// 「識別不能」を近似する。本実装は mip レベルの代わりに disk blur 半径を使うため、
/// 「ミップ最下位タイル 1 辺 (1/8) の半分」を最大半径として `0.125` を採用する
/// （値が大きいほど欠損部の情報量が減るという定性的な関係は保たれる）。
const FIELD_LOSS_MAX_RADIUS_RATIO: f32 = 0.125;

/// `FieldLossMode::Blur` の完全欠損部（m=1）における最大彩度低下率。
///
/// linear 空間で `lerp(color, luma, mask * FIELD_LOSS_DESATURATE_MAX)` する係数。
/// 1.0 = 完全グレースケール、0.0 = 無変化。「黒には落とさない」設計のため輝度は
/// 保ちつつ、色情報の減衰で VIP-Sim が意図する「情報の消失感」を補強する。
/// 中間 m はさらに m でスケールされるため実際の低下率はこの値未満になる。
/// 完全な脱色（1.0）にすると単なる disk blur との違いが分かりにくくなるため、
/// 「色は残るが明確に鈍る」という中間点として 50% に留める。
const FIELD_LOSS_DESATURATE_MAX: f32 = 0.5;

/// 緑内障（glaucoma）シミュレーション。
///
/// 緑内障は眼圧上昇による視神経萎縮が原因で、周辺視野から徐々に欠けていく。
/// `mode` により均等 vignetting と弧状暗点を切り替えられる。
/// `field_loss_mode` により暗転（Darken, 既定）とぼかし（Blur, #171）を切り替えられる。
///
/// ## モード: Vignette（デフォルト、後方互換）
///
/// 中心からの距離に基づく vignetted mask を使用:
/// - 中心付近 (normalized 距離 < `inner_r`): 保存
/// - 周辺 (距離 > `outer_r`): 暗化 × `strength`
/// - 中間: smoothstep で滑らかに移行
///
/// `inner_r` = `1.0 - strength * 0.7`, `outer_r` = `inner_r + 0.2`
///
/// ## モード: ArcuateSuperior / ArcuateInferior / Biarcuate
///
/// 視神経乳頭（ON head）を画像中心から水平方向 15% オフセットした位置に設定し、
/// そこから放射する Bjerrum 弧状暗点を極座標マスクで生成する。
/// 弧状領域内を strength に応じて暗化する。
///
/// > **注記**: `Vignette` モードの均等暗化は緑内障の視野欠損の近似に過ぎない。
/// > 実臨床の典型的な欠損は `ArcuateSuperior` / `ArcuateInferior` のような
/// > 弧状暗点（Bjerrum scotoma）として現れる。
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
/// - `mode`: 暗点の種類（[`GlaucomaMode`] を参照）
/// - `field_loss_mode`: 表現モード（[`FieldLossMode`] を参照。既定 `Darken`）
pub fn glaucoma(
    img: DynamicImage,
    strength: f32,
    mode: GlaucomaMode,
    field_loss_mode: FieldLossMode,
) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let cx = w_f * 0.5;
    let cy = h_f * 0.5;

    match field_loss_mode {
        // Darken（既定）: 既存実装そのまま（golden 不変。#171 で 1 ビットも変更しない）。
        FieldLossMode::Darken => match mode {
            GlaucomaMode::Vignette => {
                // 既存実装（後方互換）
                let max_r = (cx * cx + cy * cy).sqrt();
                let inner_r = 1.0 - strength * 0.7;
                let outer_r = (inner_r + 0.2).min(1.0);

                let mut out_rgba = rgba.clone();
                for y in 0..height {
                    for x in 0..width {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let r = (dx * dx + dy * dy).sqrt() / max_r;

                        let fade = if r <= inner_r {
                            0.0
                        } else if r >= outer_r {
                            1.0
                        } else {
                            let t = (r - inner_r) / (outer_r - inner_r);
                            t * t * (3.0 - 2.0 * t)
                        };

                        let mul = 1.0 - strength * fade;

                        let px = out_rgba.get_pixel_mut(x, y);
                        let rl = srgb_to_linear(px[0] as f32 / 255.0);
                        let gl = srgb_to_linear(px[1] as f32 / 255.0);
                        let bl = srgb_to_linear(px[2] as f32 / 255.0);
                        px[0] = pack_u8(linear_to_srgb(rl * mul));
                        px[1] = pack_u8(linear_to_srgb(gl * mul));
                        px[2] = pack_u8(linear_to_srgb(bl * mul));
                    }
                }
                Ok(DynamicImage::ImageRgba8(out_rgba))
            }
            mode => {
                // 弧状暗点モード（ArcuateSuperior / ArcuateInferior / Biarcuate）
                //
                // 視神経乳頭（ON head）の位置: 画像中心から水平方向 15% オフセット（耳側）
                let on_x = cx + w_f * 0.15;
                let on_y = cy;

                // 弧状暗点のパラメータ（極座標）
                // r_min..=r_max: ON head からの距離（min(W,H) 比）
                let min_dim = w_f.min(h_f);
                let r_min = min_dim * 0.20; // 内側境界
                let r_max = min_dim * 0.55 * strength.sqrt(); // 外側境界（strength に応じて拡大）

                // 弧状の角度範囲（ON head からの極角 θ）
                // 上方弧状: θ ∈ [90°, 270°]（y > on_y の半面、画像座標では y 下向き）
                // 下方弧状: θ ∈ [-90°, 90°]（y < on_y の半面）

                let apply_superior = matches!(
                    mode,
                    GlaucomaMode::ArcuateSuperior | GlaucomaMode::Biarcuate
                );
                let apply_inferior = matches!(
                    mode,
                    GlaucomaMode::ArcuateInferior | GlaucomaMode::Biarcuate
                );

                let mut out_rgba = rgba.clone();
                for y in 0..height {
                    for x in 0..width {
                        let dx = x as f32 - on_x;
                        let dy = y as f32 - on_y; // 画像座標: y 下向きが正

                        let r = (dx * dx + dy * dy).sqrt();

                        // ON head からの距離が弧状帯に入っているか
                        if r < r_min || r > r_max {
                            continue;
                        }

                        // 弧状帯の中での正規化距離（smoothstep 用）
                        // strength ≈ 0.133 付近で r_max ≈ r_min になりうるが、
                        // その場合は r_min < r < r_max が成立しないため早期 continue し、
                        // ゼロ除算・NaN は発生しない。
                        let t_r = (r - r_min) / (r_max - r_min);
                        let fade_r = t_r * t_r * (3.0 - 2.0 * t_r); // smoothstep
                                                                    // 帯の中央（t_r=0.5）が最も暗く、両端に向かって明るくなる
                        let fade_radial = 1.0 - (fade_r * 2.0 - 1.0).abs();

                        // 角度条件: dy > 0 が画像下方（inferior）、dy < 0 が上方（superior）
                        let in_superior = dy < 0.0; // 画像上半分（y が on_y より上）
                        let in_inferior = dy > 0.0; // 画像下半分

                        let in_arc =
                            (apply_superior && in_superior) || (apply_inferior && in_inferior);
                        if !in_arc {
                            continue;
                        }

                        // ON head に近い角度（x 軸付近）では暗点が弱くなる（弧状の端）
                        // |θ| が 0 や π に近いほど暗点は弱い → sin(θ) の絶対値でフェード
                        let theta = dy.atan2(dx); // -π..=π
                        let arc_fade = theta.sin().abs().sqrt().clamp(0.0, 1.0);

                        let fade = strength * fade_radial * arc_fade;

                        let mul = 1.0 - fade;
                        let px = out_rgba.get_pixel_mut(x, y);
                        let rl = srgb_to_linear(px[0] as f32 / 255.0);
                        let gl = srgb_to_linear(px[1] as f32 / 255.0);
                        let bl = srgb_to_linear(px[2] as f32 / 255.0);
                        px[0] = pack_u8(linear_to_srgb(rl * mul));
                        px[1] = pack_u8(linear_to_srgb(gl * mul));
                        px[2] = pack_u8(linear_to_srgb(bl * mul));
                    }
                }
                Ok(DynamicImage::ImageRgba8(out_rgba))
            }
        },
        // Blur（#171）: 暗転係数 m を blur 半径スケールとして再解釈する。
        // Darken 分岐と同じ式で m（0=無傷, 1=完全欠損）を計算し、disk blur +
        // 彩度低下を適用する（黒には落とさない）。
        FieldLossMode::Blur => {
            let mut mask = vec![0.0_f32; (width * height) as usize];
            match mode {
                GlaucomaMode::Vignette => {
                    let max_r = (cx * cx + cy * cy).sqrt();
                    let inner_r = 1.0 - strength * 0.7;
                    let outer_r = (inner_r + 0.2).min(1.0);

                    for y in 0..height {
                        for x in 0..width {
                            let dx = x as f32 - cx;
                            let dy = y as f32 - cy;
                            let r = (dx * dx + dy * dy).sqrt() / max_r;

                            let fade = if r <= inner_r {
                                0.0
                            } else if r >= outer_r {
                                1.0
                            } else {
                                let t = (r - inner_r) / (outer_r - inner_r);
                                t * t * (3.0 - 2.0 * t)
                            };

                            mask[(y * width + x) as usize] = strength * fade;
                        }
                    }
                }
                mode => {
                    let on_x = cx + w_f * 0.15;
                    let on_y = cy;
                    let min_dim = w_f.min(h_f);
                    let r_min = min_dim * 0.20;
                    let r_max = min_dim * 0.55 * strength.sqrt();

                    let apply_superior = matches!(
                        mode,
                        GlaucomaMode::ArcuateSuperior | GlaucomaMode::Biarcuate
                    );
                    let apply_inferior = matches!(
                        mode,
                        GlaucomaMode::ArcuateInferior | GlaucomaMode::Biarcuate
                    );

                    for y in 0..height {
                        for x in 0..width {
                            let dx = x as f32 - on_x;
                            let dy = y as f32 - on_y;

                            let r = (dx * dx + dy * dy).sqrt();
                            if r < r_min || r > r_max {
                                continue;
                            }

                            let t_r = (r - r_min) / (r_max - r_min);
                            let fade_r = t_r * t_r * (3.0 - 2.0 * t_r);
                            let fade_radial = 1.0 - (fade_r * 2.0 - 1.0).abs();

                            let in_superior = dy < 0.0;
                            let in_inferior = dy > 0.0;
                            let in_arc =
                                (apply_superior && in_superior) || (apply_inferior && in_inferior);
                            if !in_arc {
                                continue;
                            }

                            let theta = dy.atan2(dx);
                            let arc_fade = theta.sin().abs().sqrt().clamp(0.0, 1.0);

                            mask[(y * width + x) as usize] = strength * fade_radial * arc_fade;
                        }
                    }
                }
            }
            let max_radius_px = FIELD_LOSS_MAX_RADIUS_RATIO * width.min(height) as f32;
            let out =
                mask_mapped_blur_desaturate(&rgba, &mask, max_radius_px, FIELD_LOSS_DESATURATE_MAX);
            Ok(DynamicImage::ImageRgba8(out))
        }
    }
}

/// 黄斑変性（macular degeneration）シミュレーション。
///
/// 黄斑部（網膜中心）の光受容体が変性し、中心視野が失われる。
/// 周辺視野は保たれるが、読書・顔の認識が困難になる。
///
/// ## アルゴリズム
/// 中心に集中した暗いぼかし円を重ねる:
/// - 中心 (normalized 距離 < `inner_r`): 強く暗化 + 色彩低下
/// - 周辺 (距離 > `outer_r`): 変化なし
/// - 中間: smoothstep
///
/// `inner_r` = `strength * 0.25`, `outer_r` = `strength * 0.4`
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
/// - `field_loss_mode`: 表現モード（[`FieldLossMode`] を参照。既定 `Darken`）
pub fn macular_degeneration(
    img: DynamicImage,
    strength: f32,
    field_loss_mode: FieldLossMode,
) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let cx = w_f * 0.5;
    let cy = h_f * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt();

    let inner_r = strength * 0.25;
    let outer_r = strength * 0.4;

    match field_loss_mode {
        // Darken（既定）: 既存実装そのまま（golden 不変）。
        FieldLossMode::Darken => {
            let mut out_rgba = rgba.clone();
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r = (dx * dx + dy * dy).sqrt() / max_r;

                    let t = if r <= inner_r {
                        1.0
                    } else if r >= outer_r {
                        0.0
                    } else {
                        let u = (r - inner_r) / (outer_r - inner_r);
                        1.0 - u * u * (3.0 - 2.0 * u)
                    };

                    if t == 0.0 {
                        continue;
                    }

                    let px = out_rgba.get_pixel_mut(x, y);
                    let rl = srgb_to_linear(px[0] as f32 / 255.0);
                    let gl = srgb_to_linear(px[1] as f32 / 255.0);
                    let bl = srgb_to_linear(px[2] as f32 / 255.0);

                    // 中心部: 輝度を BT.709 で取り出して暗化＋脱色
                    let lum = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
                    // 強度に応じて暗化 (最大 0.05 の輝度)
                    let darkened = lum * (1.0 - strength * 0.95);
                    // 元色と脱色・暗化色を t でブレンド
                    let out_r = lerp(rl, darkened, t);
                    let out_g = lerp(gl, darkened, t);
                    let out_b = lerp(bl, darkened, t);

                    px[0] = pack_u8(linear_to_srgb(out_r));
                    px[1] = pack_u8(linear_to_srgb(out_g));
                    px[2] = pack_u8(linear_to_srgb(out_b));
                }
            }

            Ok(DynamicImage::ImageRgba8(out_rgba))
        }
        // Blur（#171）: `t`（Darken 分岐と同じ式）を disk blur 半径スケールとして使う。
        // `t` 単体は空間的な広がり（inner_r/outer_r）にしか strength を反映しない
        // （中心は strength によらず常に t=1）ため、他 3 フィルタ（glaucoma /
        // hemianopia / tunnel_vision）に合わせて `t * strength` で強度も
        // マスクに乗せる（PR #180 レビュー must1: strength 無視のバグ修正）。
        FieldLossMode::Blur => {
            let mut mask = vec![0.0_f32; (width * height) as usize];
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r = (dx * dx + dy * dy).sqrt() / max_r;

                    let t = if r <= inner_r {
                        1.0
                    } else if r >= outer_r {
                        0.0
                    } else {
                        let u = (r - inner_r) / (outer_r - inner_r);
                        1.0 - u * u * (3.0 - 2.0 * u)
                    };

                    mask[(y * width + x) as usize] = t * strength;
                }
            }
            let max_radius_px = FIELD_LOSS_MAX_RADIUS_RATIO * width.min(height) as f32;
            let out =
                mask_mapped_blur_desaturate(&rgba, &mask, max_radius_px, FIELD_LOSS_DESATURATE_MAX);
            Ok(DynamicImage::ImageRgba8(out))
        }
    }
}

/// 半盲（hemianopia）シミュレーション。
///
/// 視野の左右どちらかが完全に失われる（同名半盲）。
/// 脳卒中・脳腫瘍による視放線の損傷が主因。
///
/// ## アルゴリズム
/// `side`: `0.0` = 左側が失われる、`1.0` = 右側が失われる（中間値で移行領域を調整）
/// 境界は常に画像の水平中央 (`x = width / 2`) に固定。
/// `side` は fade 量の重み付けに使用し、0.0 = 左側を完全暗化、1.0 = 右側を完全暗化。
/// 境界付近は幅 `2%` の smoothstep でぼかす。
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
/// - `side`: 欠損側 (0.0 = 左欠損, 1.0 = 右欠損)
/// - `field_loss_mode`: 表現モード（[`FieldLossMode`] を参照。既定 `Darken`）
pub fn hemianopia(
    img: DynamicImage,
    strength: f32,
    side: f32,
    field_loss_mode: FieldLossMode,
) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let side = side.clamp(0.0, 1.0);

    // 境界 X 座標（正規化 0.5 が中心）
    let split_x = w_f * 0.5;
    // 境界のぼかし幅
    let blur_w = w_f * 0.02;

    match field_loss_mode {
        // Darken（既定）: 既存実装そのまま（golden 不変）。
        FieldLossMode::Darken => {
            let mut out_rgba = rgba.clone();
            for y in 0..height {
                for x in 0..width {
                    let xf = x as f32;

                    // 左欠損 (side=0.0): x < split_x の領域を暗化
                    // 右欠損 (side=1.0): x > split_x の領域を暗化
                    // 中間値は欠損量を按分
                    let left_fade = if xf < split_x - blur_w {
                        1.0
                    } else if xf > split_x + blur_w {
                        0.0
                    } else {
                        let t = (xf - (split_x - blur_w)) / (2.0 * blur_w);
                        1.0 - t * t * (3.0 - 2.0 * t)
                    };

                    // side=0 → left_fade を使う, side=1 → (1-left_fade) を使う
                    let fade = lerp(left_fade, 1.0 - left_fade, side);

                    if fade == 0.0 {
                        continue;
                    }

                    let mul = 1.0 - fade * strength;

                    let px = out_rgba.get_pixel_mut(x, y);
                    let rl = srgb_to_linear(px[0] as f32 / 255.0);
                    let gl = srgb_to_linear(px[1] as f32 / 255.0);
                    let bl = srgb_to_linear(px[2] as f32 / 255.0);
                    px[0] = pack_u8(linear_to_srgb(rl * mul));
                    px[1] = pack_u8(linear_to_srgb(gl * mul));
                    px[2] = pack_u8(linear_to_srgb(bl * mul));
                }
            }

            Ok(DynamicImage::ImageRgba8(out_rgba))
        }
        // Blur（#171）: `fade * strength`（Darken 分岐と同じ式）を blur 半径スケールとして使う。
        FieldLossMode::Blur => {
            let mut mask = vec![0.0_f32; (width * height) as usize];
            for y in 0..height {
                for x in 0..width {
                    let xf = x as f32;

                    let left_fade = if xf < split_x - blur_w {
                        1.0
                    } else if xf > split_x + blur_w {
                        0.0
                    } else {
                        let t = (xf - (split_x - blur_w)) / (2.0 * blur_w);
                        1.0 - t * t * (3.0 - 2.0 * t)
                    };

                    let fade = lerp(left_fade, 1.0 - left_fade, side);
                    mask[(y * width + x) as usize] = fade * strength;
                }
            }
            let max_radius_px = FIELD_LOSS_MAX_RADIUS_RATIO * width.min(height) as f32;
            let out =
                mask_mapped_blur_desaturate(&rgba, &mask, max_radius_px, FIELD_LOSS_DESATURATE_MAX);
            Ok(DynamicImage::ImageRgba8(out))
        }
    }
}

/// 視野狭窄（tunnel vision）シミュレーション。
///
/// 全般的に視野が狭窄し、極端な場合は穴を通して見るような視野になる。
/// 網膜色素変性・重度の緑内障末期などで生じる。
///
/// ## アルゴリズム
/// glaucoma と同様の vignetting だが、保存される中心領域がより小さく、
/// 移行領域が狭い（急激な境界）。
///
/// `inner_r` = `(1.0 - strength) * 0.5`, `outer_r` = `inner_r + 0.05`
///
/// # 引数
/// - `img`: 入力画像
/// - `strength`: 強度 (0.0..=1.0)
/// - `field_loss_mode`: 表現モード（[`FieldLossMode`] を参照。既定 `Darken`）
pub fn tunnel_vision(
    img: DynamicImage,
    strength: f32,
    field_loss_mode: FieldLossMode,
) -> crate::Result<DynamicImage> {
    let strength = normalize_strength(strength);
    let rgba = img.to_rgba8();
    if strength == 0.0 {
        return Ok(DynamicImage::ImageRgba8(rgba));
    }

    let width = rgba.width();
    let height = rgba.height();
    let w_f = width as f32;
    let h_f = height as f32;
    let cx = w_f * 0.5;
    let cy = h_f * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt();

    // 中心視野の半径: strength が大きいほど小さい
    let inner_r = (1.0 - strength) * 0.5;
    // tunnel_vision は急激な境界が特徴
    let outer_r = (inner_r + 0.05).min(1.0);

    match field_loss_mode {
        // Darken（既定）: 既存実装そのまま（golden 不変）。
        FieldLossMode::Darken => {
            let mut out_rgba = rgba.clone();
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r = (dx * dx + dy * dy).sqrt() / max_r;

                    let fade = if r <= inner_r {
                        0.0
                    } else if r >= outer_r {
                        1.0
                    } else {
                        let t = (r - inner_r) / (outer_r - inner_r);
                        t * t * (3.0 - 2.0 * t)
                    };

                    if fade == 0.0 {
                        continue;
                    }

                    let mul = 1.0 - strength * fade;

                    let px = out_rgba.get_pixel_mut(x, y);
                    let rl = srgb_to_linear(px[0] as f32 / 255.0);
                    let gl = srgb_to_linear(px[1] as f32 / 255.0);
                    let bl = srgb_to_linear(px[2] as f32 / 255.0);
                    px[0] = pack_u8(linear_to_srgb(rl * mul));
                    px[1] = pack_u8(linear_to_srgb(gl * mul));
                    px[2] = pack_u8(linear_to_srgb(bl * mul));
                }
            }

            Ok(DynamicImage::ImageRgba8(out_rgba))
        }
        // Blur（#171）: `strength * fade`（Darken 分岐と同じ式）を blur 半径スケールとして使う。
        FieldLossMode::Blur => {
            let mut mask = vec![0.0_f32; (width * height) as usize];
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r = (dx * dx + dy * dy).sqrt() / max_r;

                    let fade = if r <= inner_r {
                        0.0
                    } else if r >= outer_r {
                        1.0
                    } else {
                        let t = (r - inner_r) / (outer_r - inner_r);
                        t * t * (3.0 - 2.0 * t)
                    };

                    mask[(y * width + x) as usize] = strength * fade;
                }
            }
            let max_radius_px = FIELD_LOSS_MAX_RADIUS_RATIO * width.min(height) as f32;
            let out =
                mask_mapped_blur_desaturate(&rgba, &mask, max_radius_px, FIELD_LOSS_DESATURATE_MAX);
            Ok(DynamicImage::ImageRgba8(out))
        }
    }
}
