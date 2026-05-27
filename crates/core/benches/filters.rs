use criterion::{criterion_group, criterion_main, Criterion};
use image::{DynamicImage, RgbImage};
use sensus_core::{apply, vision, Filter};

fn make_image(w: u32, h: u32) -> DynamicImage {
    // グラデーション画像を生成
    let mut img = RgbImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 128]);
    }
    DynamicImage::ImageRgb8(img)
}

fn bench_filters(c: &mut Criterion) {
    let img_512 = make_image(512, 512);

    // matrix フィルタ（軽量、ベースライン）
    c.bench_function("protanopia_512", |b| {
        b.iter(|| apply(Filter::Protanopia, img_512.clone(), 1.0).unwrap())
    });

    // disk blur（重い）
    c.bench_function("myopia_512", |b| {
        b.iter(|| apply(Filter::Myopia, img_512.clone(), 1.0).unwrap())
    });
    c.bench_function("hyperopia_512", |b| {
        b.iter(|| apply(Filter::Hyperopia, img_512.clone(), 1.0).unwrap())
    });
    c.bench_function("presbyopia_512", |b| {
        b.iter(|| apply(Filter::Presbyopia, img_512.clone(), 1.0).unwrap())
    });

    // directional blur
    c.bench_function("astigmatism_512", |b| {
        b.iter(|| vision::astigmatism(img_512.clone(), 1.0, 90.0).unwrap())
    });

    // ray-marching
    c.bench_function("starbursts_512", |b| {
        b.iter(|| vision::starbursts(img_512.clone(), 1.0, 6, 0.1, 0.8, 0.0).unwrap())
    });

    // floaters
    c.bench_function("floaters_512", |b| {
        b.iter(|| vision::floaters(img_512.clone(), 1.0, 0.5, 0, 0.5, 0.5, 1.0).unwrap())
    });

    // eye_strain / dry_eye
    c.bench_function("eye_strain_512", |b| {
        b.iter(|| apply(Filter::EyeStrain, img_512.clone(), 1.0).unwrap())
    });
    c.bench_function("dry_eye_512", |b| {
        b.iter(|| apply(Filter::DryEye, img_512.clone(), 1.0).unwrap())
    });
}

criterion_group!(benches, bench_filters);
criterion_main!(benches);
