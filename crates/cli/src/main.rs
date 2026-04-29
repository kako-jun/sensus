//! sensus CLI — simulate sensory perception on images.
//!
//! Phase 1 (Issue #2) 以降で各フィルタを実装する。本 scaffold (#1) では
//! 引数を受け取り、未実装フィルタの場合は stderr に通知して exit(2) する。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Filter {
    // Phase 1: 色覚特性 (Issue #2)
    Protanopia,
    Deuteranopia,
    Tritanopia,
    Achromatopsia,
    // Phase 1+: 四色型色覚 (Issue #3)
    Tetrachromacy,
    // Phase 2: 焦点・屈折 (Issue #4)
    Myopia,
    Hyperopia,
    Astigmatism,
    Presbyopia,
    // Phase 3: 視野異常 (Issue #5)
    Glaucoma,
    MacularDegeneration,
    Hemianopia,
    TunnelVision,
    // Phase 3: 光・透明度 (Issue #6)
    Cataract,
    Floaters,
    Photophobia,
    NightBlindness,
}

/// sensus — simulate sensory perception on images.
#[derive(Debug, Parser)]
#[command(name = "sensus", version, about, long_about = None)]
struct Cli {
    /// Input image path (PNG / JPEG / WebP, etc.).
    #[arg(short, long)]
    input: PathBuf,

    /// Output image path. Format is inferred from the extension.
    #[arg(short, long)]
    output: PathBuf,

    /// Filter to apply.
    #[arg(short, long, value_enum)]
    filter: Filter,

    /// Filter strength in 0.0..=1.0 (0.0 = original, 1.0 = full effect).
    #[arg(short, long, default_value_t = 1.0)]
    strength: f32,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // #1 scaffold: filters are not implemented yet. Read the input so the
    // path is validated, then bail out with a clear "not implemented" message.
    let img = match image::open(&cli.input) {
        Ok(img) => img,
        Err(err) => {
            eprintln!("sensus: failed to open input {:?}: {err}", cli.input);
            return ExitCode::from(1);
        }
    };

    eprintln!(
        "sensus: filter {:?} (strength {:.2}) is not implemented yet (scaffold #1).",
        cli.filter, cli.strength
    );
    eprintln!(
        "sensus: input {}x{} {:?} -> output {:?}",
        img.width(),
        img.height(),
        cli.input,
        cli.output
    );
    eprintln!("sensus: see https://github.com/kako-jun/sensus for roadmap.");

    ExitCode::from(2)
}
