//! CLI→core 変換層。
//!
//! `--filter` / `--hearing` の CLI enum（[`crate::arguments::Filter`] /
//! [`crate::arguments::Hearing`]）を core の [`CoreFilter`] / [`HearingFilter`]
//! に写し、フィルタ固有パラメータを `Cli` フラグから payload に詰める。併せて、
//! 単一フィルタ指定時に使われないフラグを警告する [`warn_unused_flags`] を持つ。

use sensus_core::{vision::DepthBlurKind, Filter as CoreFilter, HearingFilter};

use crate::arguments::{Cli, Filter, Hearing};

impl Filter {
    pub(crate) fn is_depth_filter(self) -> bool {
        matches!(
            self,
            Filter::MyopiaDepth | Filter::HyperopiaDepth | Filter::DepthOfField
        )
    }

    pub(crate) fn depth_kind(self) -> Option<DepthBlurKind> {
        match self {
            Filter::MyopiaDepth => Some(DepthBlurKind::Myopia),
            Filter::HyperopiaDepth => Some(DepthBlurKind::Hyperopia),
            Filter::DepthOfField => Some(DepthBlurKind::DepthOfField),
            _ => None,
        }
    }

    /// Map the CLI-facing enum (clap derive) to the core enum, pulling
    /// filter-specific parameters from the parsed `cli` flags.
    ///
    /// すべてのフィルタ固有パラメータは core `Filter` の payload に詰める。
    /// pipeline / apply はこの payload だけを読むため、CLI フラグが無視されることはない。
    pub(crate) fn to_core(self, cli: &Cli) -> CoreFilter {
        match self {
            Filter::Protanopia => CoreFilter::Protanopia,
            Filter::Deuteranopia => CoreFilter::Deuteranopia,
            Filter::Tritanopia => CoreFilter::Tritanopia,
            Filter::Achromatopsia => CoreFilter::Achromatopsia,
            Filter::Tetrachromacy => CoreFilter::Tetrachromacy,
            Filter::Myopia => CoreFilter::Myopia,
            Filter::Hyperopia => CoreFilter::Hyperopia,
            Filter::Astigmatism => CoreFilter::Astigmatism { axis_deg: cli.axis },
            Filter::Presbyopia => CoreFilter::Presbyopia,
            Filter::Glaucoma => CoreFilter::Glaucoma {
                mode: sensus_core::vision::GlaucomaMode::Vignette,
            },
            Filter::MacularDegeneration => CoreFilter::MacularDegeneration,
            Filter::Hemianopia => CoreFilter::Hemianopia { side: cli.side },
            Filter::TunnelVision => CoreFilter::TunnelVision,
            Filter::Cataract => CoreFilter::Cataract { seed: cli.seed },
            Filter::Floaters => CoreFilter::Floaters {
                seed: cli.seed,
                density: cli.density,
                size: cli.size,
                gaze_x: cli.gaze_x,
                gaze_y: cli.gaze_y,
            },
            Filter::Photophobia => CoreFilter::Photophobia,
            Filter::NightBlindness => CoreFilter::NightBlindness,
            Filter::Vertigo => CoreFilter::Vertigo,
            Filter::BppvRotation => CoreFilter::BppvRotation,
            Filter::VestibularNeuritis => CoreFilter::VestibularNeuritis,
            Filter::Diplopia => CoreFilter::Diplopia {
                offset_x: cli.offset_x,
                offset_y: cli.offset_y,
                ghost_strength: cli.ghost_strength,
            },
            Filter::Nystagmus => CoreFilter::Nystagmus {
                amplitude: cli.amplitude,
                direction_deg: cli.direction_deg,
            },
            Filter::Starbursts => CoreFilter::Starbursts {
                num_rays: cli.num_rays,
                ray_length_ratio: cli.ray_length,
                threshold: cli.threshold,
                dispersion: cli.dispersion,
            },
            Filter::EyeStrain => CoreFilter::EyeStrain,
            Filter::DryEye => CoreFilter::DryEye,
            Filter::Metamorphopsia => CoreFilter::Metamorphopsia {
                freq: cli.meta_freq,
                seed: cli.meta_seed,
            },
            Filter::ContrastSensitivity => CoreFilter::ContrastSensitivity,
            Filter::DetailLoss => CoreFilter::DetailLoss {
                cell_size: cli.cell_size,
            },
            Filter::Teichopsia => CoreFilter::Teichopsia,
            Filter::FlickeringStars => CoreFilter::FlickeringStars { seed: cli.seed },
            Filter::MyopiaDepth | Filter::HyperopiaDepth | Filter::DepthOfField => {
                // depth フィルタは pipeline を通さないため、ここには来ない
                unreachable!("depth filters must be handled separately")
            }
        }
    }
}

impl Hearing {
    /// Map the CLI-facing enum to the core enum, pulling parameters from `cli`.
    pub(crate) fn to_core(self, cli: &Cli) -> HearingFilter {
        match self {
            Hearing::HearingLoss => HearingFilter::HearingLoss,
            Hearing::SuddenHearingLoss => HearingFilter::SuddenHearingLoss { freq_hz: cli.freq },
            Hearing::NoiseInducedHearingLoss => HearingFilter::NoiseInducedHearingLoss,
            Hearing::Tinnitus => HearingFilter::Tinnitus { freq_hz: cli.freq },
            Hearing::Hyperacusis => HearingFilter::Hyperacusis,
            Hearing::Misophonia => HearingFilter::Misophonia { freq_hz: cli.freq },
            Hearing::Paracusis => HearingFilter::Paracusis,
            Hearing::Amusia => HearingFilter::Amusia,
            Hearing::Dysmelodia => HearingFilter::Dysmelodia,
            Hearing::PitchShift => HearingFilter::PitchShift {
                semitones: cli.semitones,
            },
            Hearing::Diplacusis => HearingFilter::Diplacusis,
            Hearing::AuditoryProcessingDisorder => HearingFilter::AuditoryProcessingDisorder,
            Hearing::Meniere => HearingFilter::Meniere,
            Hearing::Labyrinthitis => HearingFilter::Labyrinthitis,
        }
    }
}

/// 単一フィルタ指定時に、そのフィルタが使わないパラメータフラグが
/// 明示的に変更されていれば警告する（best-effort な UX 補助。挙動には影響しない）。
pub(crate) fn warn_unused_flags(cli: &Cli, core_filter: CoreFilter) {
    if !matches!(core_filter, CoreFilter::Astigmatism { .. }) && cli.axis != 90.0 {
        eprintln!(
            "sensus: warning: --axis is only used with --filter astigmatism (ignored for {core_filter:?})"
        );
    }
    let uses_seed = matches!(
        core_filter,
        CoreFilter::Cataract { .. }
            | CoreFilter::Floaters { .. }
            | CoreFilter::FlickeringStars { .. }
    );
    if !uses_seed && cli.seed != 0 {
        eprintln!(
            "sensus: warning: --seed is only used with --filter cataract / floaters / flickering-stars (ignored for {core_filter:?})"
        );
    }
    let uses_floater_params = matches!(core_filter, CoreFilter::Floaters { .. });
    if !uses_floater_params
        && (cli.density != 0.5 || cli.gaze_x != 0.5 || cli.gaze_y != 0.5 || cli.size != 1.0)
    {
        eprintln!(
            "sensus: warning: --density/--gaze-x/--gaze-y/--size are only used with --filter floaters (ignored for {core_filter:?})"
        );
    }
    if !matches!(core_filter, CoreFilter::Hemianopia { .. }) && cli.side != 0.0 {
        eprintln!(
            "sensus: warning: --side is only used with --filter hemianopia (ignored for {core_filter:?})"
        );
    }
}
