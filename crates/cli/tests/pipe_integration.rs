//! Integration tests for --pipe mode (Issue #42).

use std::io::Write;
use std::process::{Command, Stdio};

/// 最小の 1×1 JPEG バイト列を生成する。
/// image crate でエンコードしたものを使う。
fn make_tiny_jpeg() -> Vec<u8> {
    use image::{DynamicImage, RgbImage};
    let img = RgbImage::from_pixel(1, 1, image::Rgb([128u8, 64, 32]));
    let dyn_img = DynamicImage::ImageRgb8(img);
    let mut buf = Vec::new();
    dyn_img
        .write_to(
            &mut std::io::Cursor::new(&mut buf),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    buf
}

fn sensus_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_sensus"))
}

/// 2 フレーム分の JPEG を stdin に流し、stdout に 2 フレーム出力されることを確認する。
/// must-1 の修正により --pipe 時は --output なしで動作することも検証する。
#[test]
fn pipe_two_frames_output_two_jpegs() {
    let frame = make_tiny_jpeg();
    // 2 フレームを連結
    let mut two_frames = frame.clone();
    two_frames.extend_from_slice(&frame);

    let bin = sensus_bin();
    // must-1: --output なしで --pipe が動作することを確認
    let mut child = Command::new(&bin)
        .args(["--filter", "protanopia", "--pipe"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn sensus");

    child.stdin.take().unwrap().write_all(&two_frames).unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "sensus --pipe exited with error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 出力から FFD8...FFD9 の JPEG フレームを数える
    let frames = count_jpeg_frames(&output.stdout);
    assert_eq!(frames, 2, "expected 2 output frames, got {frames}");
}

/// --pipe と --input を同時指定するとエラーになることを確認する。
#[test]
fn pipe_conflicts_with_input() {
    let bin = sensus_bin();
    let output = Command::new(&bin)
        .args([
            "--filter",
            "protanopia",
            "--pipe",
            "--input",
            "dummy.png",
            "--output",
            "/dev/null",
        ])
        .output()
        .expect("failed to run sensus");

    assert!(
        !output.status.success(),
        "expected non-zero exit when --pipe and --input are combined"
    );
}

fn count_jpeg_frames(data: &[u8]) -> usize {
    let mut count = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD8 {
            let start = i;
            let mut j = start + 2;
            while j + 1 < data.len() {
                if data[j] == 0xFF && data[j + 1] == 0xD9 {
                    count += 1;
                    i = j + 2;
                    break;
                }
                j += 1;
            }
            if j + 1 >= data.len() {
                break;
            }
        } else {
            i += 1;
        }
    }
    count
}
