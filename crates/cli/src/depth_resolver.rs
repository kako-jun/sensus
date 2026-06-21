//! depth blur 統合層。
//!
//! depth フィルタ（myopia-depth / hyperopia-depth / depth-of-field）の最大ぼけ半径
//! 定数と、`--filter` 列から depth kind を抽出する [`depth_kinds`]、Pipeline を
//! 介した（非）depth フィルタの適用ヘルパー（[`apply_filters_to_image`] /
//! [`apply_non_depth_filters`]）を持つ。depth blur 本体は `run` から呼ぶ。

use sensus_core::{
    pipeline::{FilterStep, Pipeline},
    vision::DepthBlurKind,
};

use crate::arguments::Cli;
use crate::RunError;

/// depth フィルタ（myopia-depth / hyperopia-depth / depth-of-field）の `--strength 1.0`
/// に対応する最大ぼけ半径（min(W,H) 比）。
///
/// この値は非深度の近視ディスクブラー（`vision::myopia` の `MYOPIA_MAX_RADIUS_RATIO`）と
/// 同じ 0.023 で、Smith–Helmholtz の `0.5 × pupil_diameter × |D|` から導かれる「近視最大相当」。
/// `depth_aware_blur` は各画素のぼけ半径を `depth との差 × この比 × min(W,H)` で算出するため、
/// `--strength 1.0` がこの比＝**全効果**になる（縮小ではなく、これが上限）。
pub(crate) const DEPTH_BLUR_MAX_RADIUS_RATIO: f32 = 0.023;

/// 画像にフィルタパイプラインを適用する（--pipe モードと通常モードの共通処理）。
///
/// # 通常モードとの差分
/// 通常モードの `run()` では pipeline 構築後に warning 出力（--axis / --seed 等の
/// 使われていないフラグに対する注意喚起）を行うが、--pipe モードでは省略している。
/// これはフレームごとに同じ warning が大量に出力されることを防ぐためである。
pub(crate) fn apply_filters_to_image(
    img: image::DynamicImage,
    cli: &Cli,
) -> Result<image::DynamicImage, RunError> {
    let mut pipeline = Pipeline::new();
    for f in &cli.filter {
        let core_filter = f.to_core(cli);
        pipeline = pipeline.push(FilterStep::new(core_filter, cli.strength));
    }
    pipeline
        .apply(img)
        .map_err(|e| RunError::Pipeline(format!("sensus: {e}")))
}

/// cli.filter 中の depth フィルタの kind 一覧（#108）。
pub(crate) fn depth_kinds(cli: &Cli) -> Vec<DepthBlurKind> {
    cli.filter.iter().filter_map(|f| f.depth_kind()).collect()
}

/// depth **以外**のフィルタを Pipeline で `img` に適用する（#108 の合成用）。
///
/// depth フィルタは深度マップという第2入力が必要で Pipeline に載らないため、CLI 側で
/// 「非 depth フィルタを先に適用 → その結果に depth_aware_blur」と合成する。非 depth
/// フィルタが無ければ `img` をそのまま返す。
pub(crate) fn apply_non_depth_filters(
    img: image::DynamicImage,
    cli: &Cli,
) -> Result<image::DynamicImage, RunError> {
    // 非 depth フィルタだけで Pipeline を組む。空なら Pipeline::apply は恒等。
    let mut pipeline = Pipeline::new();
    for f in cli.filter.iter().filter(|f| !f.is_depth_filter()) {
        let core_filter = f.to_core(cli);
        pipeline = pipeline.push(FilterStep::new(core_filter, cli.strength));
    }
    pipeline
        .apply(img)
        .map_err(|e| RunError::Pipeline(format!("sensus: {e}")))
}
