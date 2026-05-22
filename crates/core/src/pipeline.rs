//! 多段フィルタ合成（Issue #10）。
//!
//! 注意: 現状 vision フィルタは f32 で実装。pipeline で複数フィルタを連鎖すると
//! 8bit ↔ f32 round-trip が累積するため、Issue #10 着手時に f32 → f64 への
//! 切り替えを再検討する余地がある。

use image::DynamicImage;

use crate::{vision, Filter, Result};

/// 1つのフィルタ適用単位。
pub struct FilterStep {
    pub filter: Filter,
    pub strength: f32,
    /// astigmatism 用軸（度）。デフォルト: 90.0
    pub axis: f32,
    /// cataract / floaters 用ランダムシード。デフォルト: 0
    pub seed: u64,
    /// floaters 用密度。デフォルト: 0.5
    pub density: f32,
    /// floaters 用視線 X 位置（0=左, 1=右）。デフォルト: 0.5
    pub gaze_x: f32,
    /// floaters 用視線 Y 位置（0=上, 1=下）。デフォルト: 0.5
    pub gaze_y: f32,
    /// hemianopia 用側（0.0=左視野消失, 1.0=右視野消失）。デフォルト: 0.0
    pub side: f32,
}

impl FilterStep {
    /// デフォルトパラメータで `FilterStep` を生成する。
    pub fn new(filter: Filter, strength: f32) -> Self {
        Self {
            filter,
            strength,
            axis: 90.0,
            seed: 0,
            density: 0.5,
            gaze_x: 0.5,
            gaze_y: 0.5,
            side: 0.0,
        }
    }

    fn apply(&self, img: DynamicImage) -> Result<DynamicImage> {
        match self.filter {
            Filter::Astigmatism => vision::astigmatism(img, self.strength, self.axis),
            Filter::Cataract => vision::cataract(img, self.strength, self.seed),
            Filter::Floaters => {
                vision::floaters(img, self.strength, self.density, self.seed, self.gaze_x, self.gaze_y)
            }
            Filter::Photophobia => vision::photophobia(img, self.strength),
            Filter::NightBlindness => vision::nyctalopia(img, self.strength),
            Filter::Hemianopia => vision::hemianopia(img, self.strength, self.side),
            f => crate::apply(f, img, self.strength),
        }
    }
}

/// 複数の [`FilterStep`] を順番に適用するパイプライン。
pub struct Pipeline {
    steps: Vec<FilterStep>,
}

impl Pipeline {
    /// 空のパイプラインを生成する。
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// ステップを末尾に追加し、`self` を返す（builder パターン）。
    pub fn push(mut self, step: FilterStep) -> Self {
        self.steps.push(step);
        self
    }

    /// すべてのステップを順番に適用する。
    ///
    /// エラーが発生した場合は、ステップのインデックスとフィルタ名を含む
    /// メッセージを持つ [`crate::Error`] を返す。
    pub fn apply(&self, mut img: DynamicImage) -> Result<DynamicImage> {
        for (i, step) in self.steps.iter().enumerate() {
            img = step.apply(img).map_err(|e| {
                crate::Error::Pipeline {
                    step: i,
                    filter: format!("{:?}", step.filter),
                    source: Box::new(e),
                }
            })?;
        }
        Ok(img)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbImage};

    fn make_image() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::new(64, 64))
    }

    #[test]
    fn empty_pipeline_returns_image() {
        let img = make_image();
        let result = Pipeline::new().apply(img);
        assert!(result.is_ok());
    }

    #[test]
    fn single_step_pipeline() {
        let img = make_image();
        let pipeline = Pipeline::new().push(FilterStep::new(Filter::Myopia, 0.5));
        assert!(pipeline.apply(img).is_ok());
    }

    #[test]
    fn multi_step_pipeline() {
        let img = make_image();
        let pipeline = Pipeline::new()
            .push(FilterStep::new(Filter::Myopia, 0.5))
            .push(FilterStep::new(Filter::Protanopia, 1.0))
            .push(FilterStep::new(Filter::Glaucoma, 0.8));
        assert!(pipeline.apply(img).is_ok());
    }

    #[test]
    fn filter_step_with_custom_params() {
        let img = make_image();
        let mut step = FilterStep::new(Filter::Astigmatism, 0.7);
        step.axis = 45.0;
        let pipeline = Pipeline::new().push(step);
        assert!(pipeline.apply(img).is_ok());
    }

    #[test]
    fn floaters_step_with_params() {
        let img = make_image();
        let mut step = FilterStep::new(Filter::Floaters, 0.6);
        step.seed = 42;
        step.density = 0.3;
        step.gaze_x = 0.4;
        step.gaze_y = 0.6;
        let pipeline = Pipeline::new().push(step);
        assert!(pipeline.apply(img).is_ok());
    }
}
