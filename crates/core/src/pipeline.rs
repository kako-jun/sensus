//! 多段フィルタ合成（Issue #10）。
//!
//! 注意: 現状 vision フィルタは f32 で実装。pipeline で複数フィルタを連鎖すると
//! 8bit ↔ f32 round-trip が累積するため、Issue #10 着手時に f32 → f64 への
//! 切り替えを再検討する余地がある。

use image::DynamicImage;

use crate::{Filter, Result};

/// 1つのフィルタ適用単位。
///
/// フィルタ固有のパラメータはすべて [`Filter`] enum の payload に持たせる方針なので、
/// `FilterStep` は「どのフィルタを」「どの強度で」適用するかだけを保持する薄いラッパー。
/// 適用は [`crate::apply`] に委譲するため、単体適用と pipeline 適用の挙動は常に一致する。
pub struct FilterStep {
    pub filter: Filter,
    pub strength: f32,
}

impl FilterStep {
    /// `FilterStep` を生成する。フィルタ固有のパラメータは `filter` の payload で指定する。
    pub fn new(filter: Filter, strength: f32) -> Self {
        Self { filter, strength }
    }

    fn apply(&self, img: DynamicImage) -> Result<DynamicImage> {
        crate::apply(self.filter, img, self.strength)
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
            img = step.apply(img).map_err(|e| crate::Error::Pipeline {
                step: i,
                filter: format!("{:?}", step.filter),
                source: Box::new(e),
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

use crate::{hearing::AudioBuffer, HearingFilter};

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
    use image::{DynamicImage, RgbImage, RgbaImage};

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
            .push(FilterStep::new(
                Filter::Glaucoma {
                    mode: crate::vision::GlaucomaMode::Vignette,
                },
                0.8,
            ));
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
        let step = FilterStep::new(
            Filter::Floaters {
                seed: 42,
                density: 0.3,
                size: 1.0,
                gaze_x: 0.4,
                gaze_y: 0.6,
            },
            0.6,
        );
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
        let via_pipeline = Pipeline::new()
            .push(step)
            .apply(img)
            .unwrap()
            .to_rgb8()
            .into_raw();
        let direct = crate::apply(Filter::Protanopia, img2, 1.0)
            .unwrap()
            .to_rgb8()
            .into_raw();

        assert_eq!(via_pipeline, direct);
    }

    /// パラメータ付きバリアントでも Pipeline::apply() == crate::apply()。
    /// （パラメータが payload から正しく伝わることの byte-exact 保証。）
    #[test]
    fn parameterized_step_matches_direct_apply() {
        let make = || {
            let mut img = RgbImage::new(48, 48);
            for (x, y, px) in img.enumerate_pixels_mut() {
                *px = image::Rgb([(x * 5) as u8, (y * 5) as u8, 90]);
            }
            DynamicImage::ImageRgb8(img)
        };
        for filter in [
            Filter::Astigmatism { axis_deg: 45.0 },
            Filter::Diplopia {
                offset_x: 0.05,
                offset_y: 0.03,
                ghost_strength: 0.6,
            },
            Filter::Nystagmus {
                amplitude: 0.08,
                direction_deg: 30.0,
            },
            Filter::Floaters {
                seed: 3,
                density: 0.7,
                size: 1.5,
                gaze_x: 0.3,
                gaze_y: 0.8,
            },
            Filter::Metamorphopsia { freq: 6.0, seed: 9 },
        ] {
            let via_pipeline = Pipeline::new()
                .push(FilterStep::new(filter, 0.9))
                .apply(make())
                .unwrap()
                .to_rgba8()
                .into_raw();
            let direct = crate::apply(filter, make(), 0.9)
                .unwrap()
                .to_rgba8()
                .into_raw();
            assert_eq!(
                via_pipeline, direct,
                "pipeline must match direct apply for {filter:?}"
            );
        }
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
            .push(FilterStep::new(
                Filter::Glaucoma {
                    mode: crate::vision::GlaucomaMode::Vignette,
                },
                1.0,
            ))
            .apply(img)
            .unwrap()
            .to_rgb8()
            .into_raw();

        let ba = Pipeline::new()
            .push(FilterStep::new(
                Filter::Glaucoma {
                    mode: crate::vision::GlaucomaMode::Vignette,
                },
                1.0,
            ))
            .push(FilterStep::new(Filter::Protanopia, 1.0))
            .apply(img2)
            .unwrap()
            .to_rgb8()
            .into_raw();

        assert_ne!(
            ab, ba,
            "protanopia→glaucoma and glaucoma→protanopia should differ"
        );
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
    fn audio_pipeline_hearing_loss_on_silence_stays_silent() {
        // silence に HearingLoss のみを適用しても出力はゼロのまま（ゲイン低下のみ）
        let buf = silence_buf(1000);
        let out = AudioPipeline::new()
            .push(HearingFilter::HearingLoss, 1.0)
            .apply(&buf)
            .unwrap();
        assert!(
            out.samples.iter().all(|&s| s == 0.0),
            "HearingLoss on silence must keep output silent"
        );
    }

    #[test]
    fn audio_pipeline_hearing_loss_changes_nonzero_buffer() {
        // 非ゼロバッファに HearingLoss(strength=1.0) を適用すると出力が入力と異なる
        let buf = AudioBuffer {
            samples: vec![1.0_f32; 1000],
            sample_rate: 44100,
            channels: 1,
        };
        let out = AudioPipeline::new()
            .push(HearingFilter::HearingLoss, 1.0)
            .apply(&buf)
            .unwrap();
        let changed = out
            .samples
            .iter()
            .zip(buf.samples.iter())
            .any(|(a, b)| (a - b).abs() > 1e-6);
        assert!(
            changed,
            "HearingLoss(strength=1.0) must attenuate non-zero input"
        );
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

    #[test]
    fn audio_pipeline_matches_sequential_apply_hearing() {
        // #114: AudioPipeline は apply_hearing を順に適用したものと bit 一致する。
        let buf = AudioBuffer {
            samples: (0..44100)
                .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
                .collect(),
            sample_rate: 44100,
            channels: 1,
        };

        let pipelined = AudioPipeline::new()
            .push(HearingFilter::HearingLoss, 0.7)
            .push(HearingFilter::Tinnitus { freq_hz: 4000.0 }, 0.3)
            .apply(&buf)
            .unwrap();

        let s1 = crate::apply_hearing(HearingFilter::HearingLoss, buf.clone(), 0.7).unwrap();
        let manual =
            crate::apply_hearing(HearingFilter::Tinnitus { freq_hz: 4000.0 }, s1, 0.3).unwrap();

        assert_eq!(
            pipelined.samples, manual.samples,
            "pipeline must equal sequential apply_hearing"
        );
        assert_eq!(pipelined.sample_rate, manual.sample_rate);
        assert_eq!(pipelined.channels, manual.channels);
    }
}
