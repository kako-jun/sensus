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
fn cli_unimplemented_filter_returns_exit_2() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.png");
    let output = dir.path().join("out.png");
    write_pixel_png(&input, [100, 100, 100, 255]);

    let status = cargo_run()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--filter",
            "myopia",
        ])
        .status()
        .unwrap();

    assert_eq!(
        status.code(),
        Some(2),
        "expected exit code 2 for unimplemented filter"
    );
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
