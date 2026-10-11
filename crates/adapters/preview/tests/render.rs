use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};
use mivia_ports::preview::Unavailable;
use mivia_preview::{Rendering, render_file};

fn byte(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap()
}

fn scene(width: u32, height: u32) -> RgbImage {
    let saw = |v: u32, period: u32| i32::try_from((v % period) * 40 / period).unwrap() - 20;
    RgbImage::from_fn(width, height, |x, y| {
        let (gx, gy) = (x * 255 / width, y * 255 / height);
        let ring = saw(x, width / 20 + 1) + saw(y, height / 16 + 1);
        let base = |gain: u32| i32::try_from((gx * gain + gy * (255 - gain)) / 255).unwrap() + ring;
        let (dx, dy) = (
            i32::try_from(x * 100 / width).unwrap() - 35,
            i32::try_from(y * 100 / height).unwrap() - 50,
        );
        let inside = dx * dx + dy * dy < 400;
        let mix = |value: i32, tint: i32| {
            if inside {
                i32::midpoint(value, tint)
            } else {
                value
            }
        };
        Rgb([
            byte(mix(base(200), 230)),
            byte(mix(base(120), 40)),
            byte(mix(base(60), 40)),
        ])
    })
}

fn luma(pixel: Rgb<u8>) -> f64 {
    0.299 * f64::from(pixel.0[0]) + 0.587 * f64::from(pixel.0[1]) + 0.114 * f64::from(pixel.0[2])
}

fn ssim(left: &RgbImage, right: &RgbImage) -> f64 {
    assert_eq!(left.dimensions(), right.dimensions());
    let (c1, c2) = (6.5025, 58.5225);
    let (mut sum, mut blocks) = (0.0, 0.0);
    for by in (0..left.height() - 7).step_by(8) {
        for bx in (0..left.width() - 7).step_by(8) {
            let pairs: Vec<(f64, f64)> = (0..64)
                .map(|i| {
                    let (x, y) = (bx + i % 8, by + i / 8);
                    (luma(*left.get_pixel(x, y)), luma(*right.get_pixel(x, y)))
                })
                .collect();
            let (ma, mb) = (
                pairs.iter().map(|p| p.0).sum::<f64>() / 64.0,
                pairs.iter().map(|p| p.1).sum::<f64>() / 64.0,
            );
            let va = pairs.iter().map(|p| (p.0 - ma).powi(2)).sum::<f64>() / 63.0;
            let vb = pairs.iter().map(|p| (p.1 - mb).powi(2)).sum::<f64>() / 63.0;
            let cov = pairs.iter().map(|p| (p.0 - ma) * (p.1 - mb)).sum::<f64>() / 63.0;
            sum += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            blocks += 1.0;
        }
    }
    sum / blocks
}

fn open(path: &Path) -> RgbImage {
    image::open(path).unwrap().into_rgb8()
}

fn jpeg_of(image: &RgbImage, path: &Path, exif: Option<Vec<u8>>) {
    let file = BufWriter::new(fs::File::create(path).unwrap());
    let mut encoder = jpeg_encoder::Encoder::new(file, 92);
    if let Some(exif) = exif {
        encoder.add_app_segment(1, exif).unwrap();
    }
    let side = |v: u32| u16::try_from(v).unwrap();
    encoder
        .encode(
            image.as_raw(),
            side(image.width()),
            side(image.height()),
            jpeg_encoder::ColorType::Rgb,
        )
        .unwrap();
}

fn exif_orientation(value: u8) -> Vec<u8> {
    let mut exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0".to_vec();
    exif.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0, value, 0, 0, 0, 0, 0, 0, 0]);
    exif
}

struct Bench {
    dir: tempfile::TempDir,
}

impl Bench {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("previews")
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn render(&self, input: &Path) -> Result<Rendering, Unavailable> {
        render_file(&self.root(), input)
    }
}

fn dims(path: &Path) -> (u32, u32) {
    image::image_dimensions(path).unwrap()
}

#[test]
fn overlay_of_two_thousand_pixels_keeps_its_structure() {
    let bench = Bench::new();
    let source = scene(2048, 1536);
    let input = bench.file("overlay.png");
    source.save(&input).unwrap();
    let out = bench.render(&input).unwrap();
    assert_eq!((out.width, out.height), (2048, 1536));
    assert!(out.view.is_none());
    assert_eq!(dims(&out.display), (2048, 1536));
    assert!(ssim(&source, &open(&out.display)) >= 0.98);
}

#[test]
fn large_png_gets_both_tiers_from_one_decode() {
    let bench = Bench::new();
    let input = bench.file("big.png");
    scene(8192, 4096).save(&input).unwrap();
    let out = bench.render(&input).unwrap();
    assert_eq!(dims(&out.display), (2048, 1024));
    assert_eq!(dims(&out.view.unwrap()), (4096, 2048));
}

#[test]
fn eight_k_jpeg_uses_the_scaled_decode() {
    let bench = Bench::new();
    let input = bench.file("big.jpg");
    jpeg_of(&scene(8192, 4096), &input, None);
    let out = bench.render(&input).unwrap();
    assert_eq!((out.width, out.height), (8192, 4096));
    assert_eq!(dims(&out.display), (2048, 1024));
    assert_eq!(dims(&out.view.unwrap()), (4096, 2048));
}

#[test]
fn very_wide_jpeg_is_scaled_while_decoding() {
    let bench = Bench::new();
    let input = bench.file("wide.jpg");
    jpeg_of(&scene(23040, 1536), &input, None);
    let out = bench.render(&input).unwrap();
    assert_eq!((out.width, out.height), (23040, 1536));
    assert_eq!(dims(&out.display), (2048, 137));
    assert_eq!(dims(&out.view.unwrap()), (4096, 273));
}

#[test]
fn sixteen_bit_tiff_becomes_eight_bit_rgb() {
    let bench = Bench::new();
    let input = bench.file("deep.tif");
    let deep = image::ImageBuffer::<Rgb<u16>, Vec<u16>>::from_fn(640, 480, |x, _| {
        Rgb([u16::try_from(x * 100).unwrap(), 30_000, 65_535])
    });
    deep.save(&input).unwrap();
    let out = bench.render(&input).unwrap();
    assert_eq!(dims(&out.display), (640, 480));
    let pixel = *open(&out.display).get_pixel(320, 100);
    assert!((i32::from(pixel.0[0]) - 124).abs() < 4, "{pixel:?}");
    assert!(pixel.0[2] > 250);
}

#[test]
fn multi_page_tiff_renders_the_first_page() {
    let bench = Bench::new();
    let input = bench.file("pages.tif");
    let file = BufWriter::new(fs::File::create(&input).unwrap());
    let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
    encoder
        .write_image::<tiff::encoder::colortype::RGB8>(300, 200, &vec![200; 300 * 200 * 3])
        .unwrap();
    encoder
        .write_image::<tiff::encoder::colortype::RGB8>(500, 500, &vec![10; 500 * 500 * 3])
        .unwrap();
    drop(encoder);
    let out = bench.render(&input).unwrap();
    assert_eq!(dims(&out.display), (300, 200));
}

#[test]
fn bmp_renders() {
    let bench = Bench::new();
    let input = bench.file("shot.bmp");
    scene(800, 600).save(&input).unwrap();
    assert_eq!(dims(&bench.render(&input).unwrap().display), (800, 600));
}

#[test]
fn cmyk_jpeg_keeps_its_colour() {
    let bench = Bench::new();
    let input = bench.file("ink.jpg");
    let ink: Vec<u8> = (0..64 * 64).flat_map(|_| [255, 0, 0, 0]).collect();
    let file = BufWriter::new(fs::File::create(&input).unwrap());
    let encoder = jpeg_encoder::Encoder::new(file, 95);
    encoder
        .encode(&ink, 64, 64, jpeg_encoder::ColorType::Cmyk)
        .unwrap();
    let out = bench.render(&input).unwrap();
    let pixel = *open(&out.display).get_pixel(32, 32);
    assert!(
        pixel.0[0] < 60 && pixel.0[1] > 200 && pixel.0[2] > 200,
        "{pixel:?}"
    );
}

#[test]
fn exif_rotation_is_applied() {
    let bench = Bench::new();
    let input = bench.file("turned.jpg");
    let mut picture = RgbImage::from_pixel(300, 200, Rgb([20, 20, 20]));
    for x in 0..60 {
        for y in 0..60 {
            picture.put_pixel(x, y, Rgb([255, 0, 0]));
        }
    }
    jpeg_of(&picture, &input, Some(exif_orientation(6)));
    let out = bench.render(&input).unwrap();
    assert_eq!((out.width, out.height), (200, 300));
    let turned = open(&out.display);
    assert_eq!(turned.dimensions(), (200, 300));
    assert!(
        turned.get_pixel(180, 20).0[0] > 200,
        "the marker moves to the top right"
    );
    assert!(turned.get_pixel(20, 20).0[0] < 80);
}

#[test]
fn one_bit_png_renders_as_grey() {
    let bench = Bench::new();
    let input = bench.file("mask.png");
    let mut encoder = png::Encoder::new(BufWriter::new(fs::File::create(&input).unwrap()), 64, 64);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::One);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0xF0; 8 * 64]).unwrap();
    writer.finish().unwrap();
    let out = bench.render(&input).unwrap();
    let grey = open(&out.display);
    assert!(grey.get_pixel(2, 2).0[0] > 200);
    assert!(grey.get_pixel(60, 2).0[0] < 50);
}

#[test]
fn raster_over_the_cap_is_not_available() {
    let bench = Bench::new();
    let input = bench.file("huge.png");
    let file = BufWriter::new(fs::File::create(&input).unwrap());
    let mut encoder = png::Encoder::new(file, 9500, 9500);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder
        .write_header()
        .unwrap()
        .into_stream_writer()
        .unwrap();
    let row = vec![0; 9500 * 3];
    for _ in 0..9500 {
        writer.write_all(&row).unwrap();
    }
    writer.finish().unwrap();
    assert_eq!(bench.render(&input).err(), Some(Unavailable::TooLarge));
    assert!(
        !bench
            .root()
            .join("v1")
            .read_dir()
            .is_ok_and(|mut found| found.next().is_some())
    );
}

#[test]
fn truncated_files_are_not_available() {
    let bench = Bench::new();
    let whole = bench.file("whole.png");
    scene(1200, 900).save(&whole).unwrap();
    let bytes = fs::read(&whole).unwrap();
    let cut = bench.file("cut.png");
    fs::write(&cut, &bytes[..bytes.len() * 6 / 10]).unwrap();
    assert_eq!(bench.render(&cut).err(), Some(Unavailable::Corrupt));
    let jpeg = bench.file("whole.jpg");
    jpeg_of(&scene(1200, 900), &jpeg, None);
    let bytes = fs::read(&jpeg).unwrap();
    let cut = bench.file("cut.jpg");
    fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(bench.render(&cut).err(), Some(Unavailable::Corrupt));
}

#[test]
fn missing_and_unsupported_files() {
    let bench = Bench::new();
    assert_eq!(
        bench.render(&bench.file("none.png")).err(),
        Some(Unavailable::Missing)
    );
    let text = bench.file("notes.png");
    fs::write(&text, b"not a picture at all").unwrap();
    assert_eq!(bench.render(&text).err(), Some(Unavailable::Corrupt));
    let other = bench.file("notes.txt");
    fs::write(&other, b"not a picture at all").unwrap();
    assert_eq!(bench.render(&other).err(), Some(Unavailable::Unsupported));
}

#[test]
fn identical_bytes_share_their_tiers() {
    let bench = Bench::new();
    let (first, second) = (bench.file("a.png"), bench.file("b.png"));
    scene(900, 700).save(&first).unwrap();
    fs::copy(&first, &second).unwrap();
    let one = bench.render(&first).unwrap();
    let two = bench.render(&second).unwrap();
    assert_eq!(one.display, two.display);
    let name = one
        .display
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(name.ends_with("-display.jpg") && name.len() == 64 + "-display.jpg".len());
    assert_eq!(bench.root().join("v1").read_dir().unwrap().count(), 1);
}
