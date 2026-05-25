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
    /// diplopia 水平ずれ（min(W,H) 比）。デフォルト: 0.02
    pub offset_x: f32,
    /// diplopia 垂直ずれ（min(W,H) 比）。デフォルト: 0.01
    pub offset_y: f32,
    /// diplopia 幽霊像強度。デフォルト: 0.7
    pub ghost_strength: f32,
    /// nystagmus 振幅（min(W,H) 比）。デフォルト: 0.03
    pub amplitude: f32,
    /// nystagmus 方向（0°=水平, 90°=垂直）。デフォルト: 0.0
    pub direction_deg: f32,
    /// starbursts 光芒数。デフォルト: 6
    pub num_rays: u32,
    /// starbursts 光芒長（min(W,H) 比）。デフォルト: 0.1
    pub ray_length_ratio: f32,
    /// starbursts 輝度閾値。デフォルト: 0.8
    pub threshold: f32,
    /// starbursts 波長分散（虹色光芒）。デフォルト: 0.0（白）
    pub dispersion: f32,
    /// metamorphopsia 空間周波数。デフォルト: 4.0
    pub meta_freq: f32,
    /// metamorphopsia LCG シード。デフォルト: 0
    pub meta_seed: u64,
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
            offset_x: 0.02,
            offset_y: 0.01,
            ghost_strength: 0.7,
            amplitude: 0.03,
            direction_deg: 0.0,
            num_rays: 6,
            ray_length_ratio: 0.1,
            threshold: 0.8,
            dispersion: 0.0,
            meta_freq: 4.0,
            meta_seed: 0,
        }
    }

    fn apply(&self, img: DynamicImage) -> Result<DynamicImage> {
        match self.filter {
            Filter::Astigmatism { axis_deg } => vision::astigmatism(img, self.strength, axis_deg),
            Filter::Cataract => vision::cataract(img, self.strength, self.seed),
            Filter::Floaters { seed, density, .. } => {
                vision::floaters(img, self.strength, density, seed, self.gaze_x, self.gaze_y)
            }
            Filter::Photophobia => vision::photophobia(img, self.strength),
            Filter::NightBlindness => vision::nyctalopia(img, self.strength),
            Filter::Hemianopia { side } => vision::hemianopia(img, self.strength, side),
            Filter::Glaucoma { mode } => vision::glaucoma(img, self.strength, mode),
            Filter::Diplopia => vision::diplopia(img, self.strength, self.offset_x, self.offset_y, self.ghost_strength),
            Filter::Nystagmus => vision::nystagmus(img, self.strength, self.amplitude, self.direction_deg),
            Filter::Starbursts { num_rays, ray_length_ratio, threshold, dispersion } => {
                vision::starbursts(img, self.strength, num_rays, ray_length_ratio, threshold, dispersion)
            }
            Filter::Metamorphopsia => vision::metamorphopsia(img, self.strength, self.meta_freq, self.meta_seed),
            Filter::FlickeringStars { seed } => vision::flickering_stars(img, self.strength, seed),
            Filter::DetailLoss { cell_size } => vision::detail_loss_with_cell_size(img, self.strength, cell_size),
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

// ---------------------------------------------------------------
// AudioPipeline (Issue #66): 聴覚フィルタの多段合成
// ---------------------------------------------------------------

use crate::{HearingFilter, hearing::AudioBuffer};

/// 1つの聴覚フィルタ適用単位。
pub struct AudioFilterStep {
    pub filter: HearingFilter,
    pub strength: f32,
}

/// 複数の [`AudioFilterStep`] を順番に適用するパイプライン。
///
/// # 例
///
/// ```rust,no_run
/// use sensus_core::{AudioPipeline, HearingFilter, hearing::AudioBuffer};
///
/// let buf = AudioBuffer { samples: vec![0.0; 1000], sample_rate: 44100, channels: 1 };
/// let out = AudioPipeline::new()
///     .push(HearingFilter::HearingLoss, 0.5)
///     .apply(&buf)
///     .unwrap();
/// ```
pub struct AudioPipeline {
    steps: Vec<AudioFilterStep>,
}

impl AudioPipeline {
    /// 空のパイプラインを生成する。
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// ステップを末尾に追加し、`self` を返す（builder パターン）。
    pub fn push(mut self, filter: HearingFilter, strength: f32) -> Self {
        self.steps.push(AudioFilterStep { filter, strength });
        self
    }

    /// すべてのステップを順番に適用する。
    ///
    /// エラーが発生した場合は [`crate::Error`] を返す。
    pub fn apply(&self, buf: &AudioBuffer) -> Result<AudioBuffer> {
        let mut current = buf.clone();
        for step in &self.steps {
            current = crate::apply_hearing(step.filter.clone(), current, step.strength)?;
        }
        Ok(current)
    }
}

impl Default for AudioPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbaImage, RgbImage};

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
            .push(FilterStep::new(Filter::Glaucoma { mode: crate::vision::GlaucomaMode::Vignette }, 0.8));
        assert!(pipeline.apply(img).is_ok());
    }

    #[test]
    fn filter_step_with_custom_params() {
        let img = make_image();
        let step = FilterStep::new(Filter::Astigmatism { axis_deg: 45.0 }, 0.7);
        let pipeline = Pipeline::new().push(step);
        assert!(pipeline.apply(img).is_ok());
    }

    #[test]
    fn floaters_step_with_params() {
        let img = make_image();
        let mut step = FilterStep::new(Filter::Floaters { seed: 42, density: 0.3, size: 1.0 }, 0.6);
        step.gaze_x = 0.4;
        step.gaze_y = 0.6;
        let pipeline = Pipeline::new().push(step);
        assert!(pipeline.apply(img).is_ok());
    }

    /// 空のパイプラインを通しても画素値が変化しない（byte-exact）。
    #[test]
    fn empty_pipeline_is_identity() {
        let src = {
            let mut img = RgbImage::new(4, 4);
            for (x, y, px) in img.enumerate_pixels_mut() {
                *px = image::Rgb([(x * 17) as u8, (y * 31) as u8, 128]);
            }
            DynamicImage::ImageRgb8(img)
        };
        let before = src.to_rgb8().into_raw();
        let after = Pipeline::new().apply(src).unwrap().to_rgb8().into_raw();
        assert_eq!(before, after);
    }

    /// Pipeline::apply() が sensus_core::apply() と同一結果を返す。
    #[test]
    fn single_step_matches_direct_apply() {
        let img = make_image();
        let img2 = img.clone();

        let step = FilterStep::new(Filter::Protanopia, 1.0);
        let via_pipeline = Pipeline::new().push(step).apply(img).unwrap().to_rgb8().into_raw();
        let direct = crate::apply(Filter::Protanopia, img2, 1.0).unwrap().to_rgb8().into_raw();

        assert_eq!(via_pipeline, direct);
    }

    /// A→B と B→A で結果が異なることを確認する（順序依存性のテスト）。
    #[test]
    fn two_step_order_matters() {
        let img = {
            let mut base = RgbImage::new(32, 32);
            for (x, y, px) in base.enumerate_pixels_mut() {
                *px = image::Rgb([x as u8 * 7, y as u8 * 5, 200]);
            }
            DynamicImage::ImageRgb8(base)
        };
        let img2 = img.clone();

        let ab = Pipeline::new()
            .push(FilterStep::new(Filter::Protanopia, 1.0))
            .push(FilterStep::new(Filter::Glaucoma { mode: crate::vision::GlaucomaMode::Vignette }, 1.0))
            .apply(img)
            .unwrap()
            .to_rgb8()
            .into_raw();

        let ba = Pipeline::new()
            .push(FilterStep::new(Filter::Glaucoma { mode: crate::vision::GlaucomaMode::Vignette }, 1.0))
            .push(FilterStep::new(Filter::Protanopia, 1.0))
            .apply(img2)
            .unwrap()
            .to_rgb8()
            .into_raw();

        assert_ne!(ab, ba, "protanopia→glaucoma and glaucoma→protanopia should differ");
    }

    /// alpha チャンネル付き画像で alpha が保持されること。
    #[test]
    fn pipeline_preserves_alpha() {
        use image::GenericImageView;
        let src = {
            let mut img = RgbaImage::new(8, 8);
            for px in img.pixels_mut() {
                *px = image::Rgba([100, 150, 200, 128]);
            }
            DynamicImage::ImageRgba8(img)
        };
        let out = Pipeline::new()
            .push(FilterStep::new(Filter::Protanopia, 1.0))
            .apply(src)
            .unwrap();
        for (_x, _y, px) in out.pixels() {
            assert_eq!(px[3], 128, "alpha channel must be preserved");
        }
    }

    // ---------------------------------------------------------------
    // AudioPipeline テスト
    // ---------------------------------------------------------------

    fn silence_buf(frames: usize) -> AudioBuffer {
        AudioBuffer {
            samples: vec![0.0; frames],
            sample_rate: 44100,
            channels: 1,
        }
    }

    #[test]
    fn audio_pipeline_single_step_returns_ok() {
        let buf = silence_buf(1000);
        let result = AudioPipeline::new()
            .push(HearingFilter::HearingLoss, 0.5)
            .apply(&buf);
        assert!(result.is_ok(), "single step AudioPipeline should return Ok");
    }

    #[test]
    fn audio_pipeline_two_steps_returns_ok() {
        let buf = silence_buf(1000);
        let result = AudioPipeline::new()
            .push(HearingFilter::HearingLoss, 0.5)
            .push(HearingFilter::Tinnitus { freq_hz: 4000.0 }, 0.3)
            .apply(&buf);
        assert!(result.is_ok(), "two-step AudioPipeline should return Ok");
    }

    #[test]
    fn audio_pipeline_empty_returns_same_buffer() {
        let buf = silence_buf(100);
        let out = AudioPipeline::new().apply(&buf).unwrap();
        assert_eq!(out.samples, buf.samples);
        assert_eq!(out.sample_rate, buf.sample_rate);
        assert_eq!(out.channels, buf.channels);
    }

    #[test]
    fn audio_pipeline_preserves_sample_rate_and_channels() {
        let buf = AudioBuffer {
            samples: vec![0.1, -0.1, 0.2, -0.2],
            sample_rate: 48000,
            channels: 2,
        };
        let out = AudioPipeline::new()
            .push(HearingFilter::HearingLoss, 0.3)
            .apply(&buf)
            .unwrap();
        assert_eq!(out.sample_rate, 48000);
        assert_eq!(out.channels, 2);
    }
}
