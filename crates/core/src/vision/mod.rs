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
//! Phase 2 (Issue #4) では焦点・屈折 4 種を実装する:
//!
//! - [`myopia`]      — 近視 (-6D 上限相当, 等方 disk blur)
//! - [`hyperopia`]   — 遠視 (+4D 上限相当, 等方 disk blur)
//! - [`presbyopia`]  — 老眼 (+3D add 相当, 等方 disk blur)
//! - [`astigmatism`] — 乱視 (純粋 cylinder lens, -3CD 上限相当の **方向性 blur**)
//!
//! myopia / hyperopia / presbyopia は光学的に正しい等方 **disk blur
//! (pillbox kernel)** を linear sRGB 空間で適用する。Gaussian は実際の defocus
//! blur ではないため採用しない（瞳孔は円形であり、点光源の retina 上の像は
//! circle of confusion = 円となる）。
//!
//! astigmatism は **isolated cylinder error** のシミュレーションで、純粋
//! cylinder lens は line focus (焦線) を作るため光学的には **1D directional
//! blur** が正しい。実装上は楕円カーネルの短軸を sub-pixel まで縮退させて
//! 1D box フィルタとして畳み込む。臨床現場で多い合併乱視 (cylinder + sphere)
//! は両経線にぼけがあるが、これは Phase 4 (#10) pipeline で
//! `Myopia + Astigmatism` のような合成として扱う前提で、本フィルタ単体では
//! 表現しない。
//!
//! ディオプター → 画素半径の換算は以下の前提による:
//! Smith-Helmholtz 近似 `θ_diameter (rad) ≈ pupil_diameter(m) × |D|` は
//! **角直径 (CoC 円盤の直径)** を返すので、半径は `θ_diameter / 2`。
//! pupil 4 mm = 0.004 m (mesopic 標準), 視距離 50 cm / FOV 30° を想定し、
//! 画像の `min(width, height)` に対する比率で表現する。詳細は各関数の
//! `MAX_RADIUS_RATIO` 定数のコメントを参照。
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

// 振る舞い不変の god-file 分割（Issue #157）。各サブモジュールは症状領域ごとに
// 公開フィルタ・専用 const/enum・専用 private ヘルパーを保持し、複数領域から
// 使う純粋ヘルパーは `common` に集約する。`mod.rs` は全公開アイテムを `pub use`
// で vision 直下に再エクスポートし、`crate::vision::<name>` の解決パスを分割前と
// 完全に一致させる（lib.rs の apply・tests・CLI・FFI が参照する API は不変）。

// クロスドメイン共有ヘルパー（srgb 変換・blur カーネル・bilinear 等）。
pub(crate) mod common;
// 色覚特性: protanopia/deuteranopia/tritanopia/achromatopsia/tetrachromacy。
mod color;
// 焦点・屈折: myopia/hyperopia/presbyopia/astigmatism/depth_aware_blur。
mod refraction;
// 視野欠損: glaucoma/macular_degeneration/hemianopia/tunnel_vision。
mod field;
// 光・透明度: cataract/photophobia/nyctalopia/floaters。
mod light;
// 前庭・動き: vertigo/bppv_rotation/vestibular_neuritis/diplopia/nystagmus/starbursts。
mod motion;
// 眼精疲労: eye_strain/dry_eye。
mod fatigue;
// 知覚低下・歪み・閃輝・光視症: contrast_sensitivity/detail_loss(+with_cell_size)/
// metamorphopsia/teichopsia/flickering_stars。
mod phenomena;

// 全公開アイテムを vision 直下へ再エクスポートし、`crate::vision::<name>` を不変に保つ。
pub use color::{achromatopsia, deuteranopia, protanopia, tetrachromacy, tritanopia};
pub use fatigue::{dry_eye, eye_strain};
pub use field::{glaucoma, hemianopia, macular_degeneration, tunnel_vision, GlaucomaMode};
pub use light::{cataract, floaters, floaters_mask, nyctalopia, photophobia};
pub use motion::{
    bppv_rotation, diplopia, nystagmus, starbursts, vertigo, vestibular_neuritis,
    BPPV_STILL_TIME_S, VERTIGO_STILL_TIME_S,
};
pub use phenomena::{
    contrast_sensitivity, detail_loss, detail_loss_with_cell_size, flickering_stars,
    metamorphopsia, teichopsia,
};
pub use refraction::{astigmatism, depth_aware_blur, hyperopia, myopia, presbyopia, DepthBlurKind};

// srgb gamma 2 関数は integration test (tests/shader_equivalence.rs) が参照する
// ため公開 util として再エクスポートする（Issue #157 の二重定義統合）。
pub use common::{linear_to_srgb, srgb_to_linear};

// クロスモジュールで使う pub(crate) ヘルパーを vision 直下へ再エクスポートする。
// これにより各サブモジュールの `use super::*;` から到達でき、`super::lerp` 等の
// テスト参照（分割前と同一）もそのまま解決する。
pub(crate) use color::{LUMA_B, LUMA_G, LUMA_R};
pub(crate) use common::{
    ellipse_blur, isotropic_disk_blur_image, lerp, linear_planes_to_rgba, normalize_strength,
    pack_u8, radius_from_strength, rgba_to_linear_planes, sample_bilinear, MIN_BLUR_RADIUS_PX,
};

#[cfg(test)]
mod tests;
