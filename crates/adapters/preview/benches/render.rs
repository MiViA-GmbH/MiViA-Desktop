use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use image::{Rgb, RgbImage};
use mivia_preview::decode::{decode, encode, header, resize};

fn picture(width: u32, height: u32) -> RgbImage {
    RgbImage::from_fn(width, height, |x, y| {
        let byte = |value: u32| u8::try_from(value & 0xFF).unwrap();
        Rgb([byte(x * 255 / width), byte(y * 255 / height), byte(x ^ y)])
    })
}

fn jpeg(image: &RgbImage, path: &Path) {
    let encoder = jpeg_encoder::Encoder::new_file(path, 90).unwrap();
    encoder
        .encode(
            image.as_raw(),
            u16::try_from(image.width()).unwrap(),
            u16::try_from(image.height()).unwrap(),
            jpeg_encoder::ColorType::Rgb,
        )
        .unwrap();
}

fn fixtures(dir: &Path) -> Vec<(&'static str, PathBuf)> {
    let base = picture(4096, 3072);
    let wide = picture(12288, 2048);
    let files = vec![
        ("png_4k", dir.join("a.png")),
        ("tiff_4k", dir.join("a.tif")),
        ("bmp_4k", dir.join("a.bmp")),
        ("jpeg_4k", dir.join("a.jpg")),
        ("jpeg_12k_scaled", dir.join("wide.jpg")),
    ];
    for (name, path) in &files {
        match *name {
            "jpeg_4k" => jpeg(&base, path),
            "jpeg_12k_scaled" => jpeg(&wide, path),
            _ => base.save(path).unwrap(),
        }
    }
    files
}

fn stages(criterion: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let files = fixtures(dir.path());
    let mut group = criterion.benchmark_group("preview");
    group.sample_size(10);
    for (name, path) in &files {
        let head = header(path).unwrap();
        group.bench_function(format!("decode_{name}"), |b| {
            b.iter(|| decode(black_box(path), &head).unwrap());
        });
    }
    let (_, path) = files.first().unwrap();
    let source = decode(path, &header(path).unwrap()).unwrap();
    group.bench_function("resize_4k_to_2048", |b| {
        b.iter(|| resize(black_box(&source), 2048).unwrap());
    });
    let tier = resize(&source, 2048).unwrap();
    let out = dir.path().join("tier.jpg");
    group.bench_function("encode_2048", |b| {
        b.iter(|| encode(black_box(&tier), &out).unwrap());
    });
    group.finish();
}

criterion_group!(benches, stages);
criterion_main!(benches);
