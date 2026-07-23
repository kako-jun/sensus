//! sensus CLI integration tests.
//!
//! 実行ファイルを直接呼び出し、exit code と出力 PNG を検証する。
//! `CARGO_BIN_EXE_sensus` は cargo がテスト実行時に自動でセットする
//! 環境変数で、対象 bin の絶対パスが入る。

use std::process::Command;

use tempfile::TempDir;

/// Helper: build a `Command` pointing at the just-built `sensus` binary.
fn cargo_run() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sensus"))
}

/// 1×1 の単色 RGBA PNG をテスト用に書き出すヘルパー。
fn write_pixel_png(path: &std::path::Path, rgba: [u8; 4]) {
    let img = image::RgbaImage::from_pixel(1, 1, image::Rgba(rgba));
    img.save(path).expect("write test PNG");
}

#[test]
fn cli_deuteranopia_writes_output_png() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");

    write_pixel_png(&input, [255, 0, 0, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "deuteranopia",
            "--strength",
            "1.0",
        ])
        .status()
        .unwrap();

    assert!(status.success(), "expected exit 0 for implemented filter");
    assert!(output.exists(), "expected output PNG to be written");
}

#[test]
fn cli_strength_zero_is_identity() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    write_pixel_png(&input, [200, 50, 30, 255]);
    let original = std::fs::read(&input).unwrap();
    // image crate の PNG 出力 byte 列が encoder バージョンに依存する可能性を
    // 排除するため、比較は decode 後の RGBA バイト列で行う。
    let original_rgba = image::open(&input).unwrap().to_rgba8().into_raw();

    for filter in ["protanopia", "deuteranopia", "tritanopia", "achromatopsia"] {
        let output = dir.path().join(format!("out-{filter}.png"));
        let status = cargo_run()
            .args([
                "-i",
                input.to_str().unwrap(),
                "-o",
                output.to_str().unwrap(),
                "--filter",
                filter,
                "--strength",
                "0.0",
            ])
            .status()
            .unwrap();
        assert!(status.success(), "expected exit 0 for filter {filter}");
        let out_rgba = image::open(&output).unwrap().to_rgba8().into_raw();
        assert_eq!(
            out_rgba, original_rgba,
            "strength=0 must be byte-exact identity for filter {filter}"
        );
    }
    // 入力ファイル自体が書き換えられていないことも一応確認
    assert_eq!(std::fs::read(&input).unwrap(), original);
}

#[test]
fn cli_strength_above_one_is_rejected_at_cli_layer() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    write_pixel_png(&input, [255, 0, 0, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "deuteranopia",
            "--strength",
            "2.0",
        ])
        .status()
        .unwrap();

    assert!(
        !status.success(),
        "expected non-zero exit for out-of-range strength"
    );
    assert!(
        !output.exists(),
        "expected no output PNG when CLI rejects args"
    );
}

// ----------------------------------------------------------------------
// Phase 2 (#4): focus / refraction integration tests
// ----------------------------------------------------------------------

/// Helper: write a small RGBA PNG large enough to give the disk blur
/// kernel something to chew on.
fn write_solid_png(path: &std::path::Path, w: u32, h: u32, rgba: [u8; 4]) {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba(rgba));
    img.save(path).expect("write test PNG");
}

#[test]
fn cli_myopia_writes_output_png() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    write_solid_png(&input, 32, 32, [200, 50, 30, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "myopia",
            "--strength",
            "1.0",
        ])
        .status()
        .unwrap();

    assert!(status.success(), "expected exit 0 for myopia filter");
    assert!(output.exists(), "expected output PNG to be written");
}

#[test]
fn cli_astigmatism_with_axis_writes_output_png() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    write_solid_png(&input, 32, 32, [200, 50, 30, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "astigmatism",
            "--strength",
            "1.0",
            "--axis",
            "45",
        ])
        .status()
        .unwrap();

    assert!(
        status.success(),
        "expected exit 0 for astigmatism with --axis"
    );
    assert!(output.exists(), "expected output PNG to be written");
}

#[test]
fn cli_axis_actually_changes_astigmatism_output() {
    // 回帰 (B2): --axis が pipeline まで届かず固定値 90° が使われていた問題の再発防止。
    // 横長グラデーション画像に axis=0 と axis=90 の astigmatism をかけると、
    // ボケ方向が直交するので出力 RGBA は一致しないはず。
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    // 大きめ画像にして方向性ブラー半径 (≈ min(W,H)·1.1%) を 1px 以上確保する。
    let img = image::RgbaImage::from_fn(200, 200, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    });
    img.save(&input).unwrap();

    let run_axis = |axis: &str, out: &std::path::Path| {
        let status = cargo_run()
            .args([
                "-i",
                input.to_str().unwrap(),
                "-o",
                out.to_str().unwrap(),
                "--filter",
                "astigmatism",
                "--strength",
                "1.0",
                "--axis",
                axis,
            ])
            .status()
            .unwrap();
        assert!(status.success(), "astigmatism --axis {axis} should succeed");
        image::open(out).unwrap().to_rgba8().into_raw()
    };

    let a0 = run_axis("0", &dir.path().join("a0.png"));
    let a90 = run_axis("90", &dir.path().join("a90.png"));
    assert_ne!(
        a0, a90,
        "--axis must change astigmatism blur direction (regression: axis was pinned to 90)"
    );
}

#[test]
fn cli_field_loss_mode_actually_changes_glaucoma_output() {
    // #171 / PR #180 should4: --field-loss-mode darken/blur が実際に pipeline
    // まで届いて出力へ反映されることの回帰防止（--axis の前例と同型）。
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    // 大きめのグラデーション画像で disk blur の効果を確保する。
    let img = image::RgbaImage::from_fn(200, 200, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    });
    img.save(&input).unwrap();

    let run_mode = |mode: &str, out: &std::path::Path| {
        let status = cargo_run()
            .args([
                "-i",
                input.to_str().unwrap(),
                "-o",
                out.to_str().unwrap(),
                "--filter",
                "glaucoma",
                "--strength",
                "1.0",
                "--field-loss-mode",
                mode,
            ])
            .status()
            .unwrap();
        assert!(
            status.success(),
            "glaucoma --field-loss-mode {mode} should succeed"
        );
        image::open(out).unwrap().to_rgba8().into_raw()
    };

    let darken = run_mode("darken", &dir.path().join("darken.png"));
    let blur = run_mode("blur", &dir.path().join("blur.png"));
    assert_ne!(
        darken, blur,
        "--field-loss-mode must change glaucoma output (darken vs blur)"
    );
}

#[test]
fn cli_field_loss_mode_with_non_field_loss_filter_warns() {
    // myopia + --field-loss-mode blur は glaucoma/macular-degeneration/
    // hemianopia/tunnel-vision 以外なので silent ignore せず stderr に
    // warning を出す（--axis / --side と同じパターン）。実行自体は成功する。
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    write_pixel_png(&input, [120, 120, 120, 255]);

    let out = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "myopia",
            "--strength",
            "0.5",
            "--field-loss-mode",
            "blur",
        ])
        .output()
        .unwrap();

    assert!(out.status.success(), "expected exit 0");
    assert!(output.exists(), "expected output PNG to be written");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--field-loss-mode"),
        "expected warning about --field-loss-mode on stderr, got: {stderr}"
    );
}

#[test]
fn cli_newly_exposed_filters_run() {
    // 回帰 (S1): core にあるのに CLI から選べなかった 5 フィルタが疎通すること。
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    write_solid_png(&input, 32, 32, [200, 120, 60, 255]);

    for filter in [
        "metamorphopsia",
        "contrast-sensitivity",
        "detail-loss",
        "teichopsia",
        "flickering-stars",
    ] {
        let output = dir.path().join(format!("out-{filter}.png"));
        let status = cargo_run()
            .args([
                "-i",
                input.to_str().unwrap(),
                "-o",
                output.to_str().unwrap(),
                "--filter",
                filter,
                "--strength",
                "1.0",
            ])
            .status()
            .unwrap();
        assert!(
            status.success(),
            "expected exit 0 for newly exposed filter {filter}"
        );
        assert!(output.exists(), "expected output PNG for filter {filter}");
    }
}

#[test]
fn cli_axis_out_of_range_is_rejected() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    write_solid_png(&input, 16, 16, [255, 0, 0, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "astigmatism",
            "--strength",
            "1.0",
            "--axis",
            "200",
        ])
        .status()
        .unwrap();

    assert!(
        !status.success(),
        "expected non-zero exit for out-of-range axis"
    );
    assert!(
        !output.exists(),
        "expected no output PNG when CLI rejects args"
    );
}

#[test]
fn cli_axis_with_non_astigmatism_filter_warns() {
    // myopia + --axis 45 は astigmatism でないので silent ignore せず
    // stderr に warning を出す。実行自体は成功 (exit 0 + 出力 PNG 生成)。
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    write_pixel_png(&input, [120, 120, 120, 255]);

    let out = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "myopia",
            "--strength",
            "0.5",
            "--axis",
            "45",
        ])
        .output()
        .unwrap();

    assert!(out.status.success(), "expected exit 0");
    assert!(output.exists(), "expected output PNG to be written");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--axis"),
        "expected warning about --axis on stderr, got: {stderr}"
    );
}
