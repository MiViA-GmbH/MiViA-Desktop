use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{PixelType, Resizer};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, RgbImage};
use mivia_ports::preview::Unavailable;

pub const RASTER_CAP: u64 = 256 << 20;
const DCT_FROM: u32 = 8192;
const DCT_KEEP: u16 = 4096;

pub struct Header {
    pub format: Option<ImageFormat>,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub orientation: Orientation,
}

impl Header {
    pub fn long_edge(&self) -> u32 {
        self.width.max(self.height)
    }
}

pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

fn unavailable(error: &ImageError) -> Unavailable {
    match error {
        ImageError::IoError(io) if io.kind() == std::io::ErrorKind::NotFound => {
            Unavailable::Missing
        }
        ImageError::Unsupported(_) => Unavailable::Unsupported,
        ImageError::Limits(_) => Unavailable::TooLarge,
        ImageError::Decoding(_)
        | ImageError::Encoding(_)
        | ImageError::Parameter(_)
        | ImageError::IoError(_) => Unavailable::Corrupt,
    }
}

const fn swaps(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

const EOI_WINDOW: u64 = 64 << 10;

fn jpeg_ends(path: &Path) -> bool {
    let tail = || {
        let mut file = File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        let start = len.saturating_sub(EOI_WINDOW);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = vec![0; usize::try_from(len - start).ok()?];
        file.read_exact(&mut bytes).ok()?;
        Some(bytes.windows(2).any(|pair| pair == [0xFF, 0xD9]))
    };
    tail().unwrap_or(false)
}

pub fn header(path: &Path) -> Result<Header, Unavailable> {
    let reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| unavailable(&ImageError::IoError(error)))?;
    let format = reader.format().ok_or(Unavailable::Unsupported)?;
    if format == ImageFormat::Jpeg && !jpeg_ends(path) {
        return Err(Unavailable::Corrupt);
    }
    let mut decoder = reader.into_decoder().map_err(|e| unavailable(&e))?;
    let (width, height) = decoder.dimensions();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let bytes =
        u64::from(width) * u64::from(height) * u64::from(decoder.color_type().bytes_per_pixel());
    let (width, height) = if swaps(orientation) {
        (height, width)
    } else {
        (width, height)
    };
    Ok(Header {
        format: Some(format),
        width,
        height,
        bytes,
        orientation,
    })
}

pub fn decode(path: &Path, header: &Header) -> Result<Rgb, Unavailable> {
    if header.format == Some(ImageFormat::Jpeg) && header.long_edge() >= DCT_FROM {
        return scaled_jpeg(path);
    }
    if header.bytes > RASTER_CAP {
        return Err(Unavailable::TooLarge);
    }
    let reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| unavailable(&ImageError::IoError(error)))?;
    let decoder = reader.into_decoder().map_err(|e| unavailable(&e))?;
    let image = DynamicImage::from_decoder(decoder).map_err(|e| unavailable(&e))?;
    let rgb = image.into_rgb8();
    Ok(Rgb {
        width: rgb.width(),
        height: rgb.height(),
        data: rgb.into_raw(),
    })
}

fn scaled_jpeg(path: &Path) -> Result<Rgb, Unavailable> {
    let file = File::open(path).map_err(|_| Unavailable::Missing)?;
    let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(file));
    let (width, height) = decoder
        .scale(DCT_KEEP, DCT_KEEP)
        .map_err(|_| Unavailable::Corrupt)?;
    let info = decoder.info().ok_or(Unavailable::Corrupt)?;
    let bytes = u64::from(width) * u64::from(height) * info.pixel_format.pixel_bytes() as u64;
    if bytes > RASTER_CAP {
        return Err(Unavailable::TooLarge);
    }
    let pixels = decoder.decode().map_err(|_| Unavailable::Corrupt)?;
    let data = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels,
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&v| [v, v, v]).collect(),
        jpeg_decoder::PixelFormat::CMYK32 => pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[c, m, y, k]| {
                [c, m, y].map(|channel| {
                    u8::try_from(u16::from(channel) * u16::from(k) / 255).unwrap_or(u8::MAX)
                })
            })
            .collect(),
        jpeg_decoder::PixelFormat::L16 => return Err(Unavailable::Unsupported),
    };
    Ok(Rgb {
        width: u32::from(width),
        height: u32::from(height),
        data,
    })
}

pub fn fit(width: u32, height: u32, edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= edge {
        return (width, height);
    }
    let side = |value: u32| {
        let scaled = (u64::from(value) * u64::from(edge) + u64::from(long) / 2) / u64::from(long);
        u32::try_from(scaled).unwrap_or(edge).max(1)
    };
    (side(width), side(height))
}

pub fn resize(source: &Rgb, edge: u32) -> Result<Rgb, Unavailable> {
    let (width, height) = fit(source.width, source.height, edge);
    if (width, height) == (source.width, source.height) {
        return Ok(Rgb {
            width,
            height,
            data: source.data.clone(),
        });
    }
    let from = ImageRef::new(source.width, source.height, &source.data, PixelType::U8x3)
        .map_err(|_| Unavailable::Corrupt)?;
    let mut to = Image::new(width, height, PixelType::U8x3);
    Resizer::new()
        .resize(&from, &mut to, None)
        .map_err(|_| Unavailable::Corrupt)?;
    Ok(Rgb {
        width,
        height,
        data: to.into_vec(),
    })
}

pub fn orient(image: Rgb, orientation: Orientation) -> Result<Rgb, Unavailable> {
    if orientation == Orientation::NoTransforms {
        return Ok(image);
    }
    let buffer =
        RgbImage::from_raw(image.width, image.height, image.data).ok_or(Unavailable::Corrupt)?;
    let mut turned = DynamicImage::ImageRgb8(buffer);
    turned.apply_orientation(orientation);
    let rgb = turned.into_rgb8();
    Ok(Rgb {
        width: rgb.width(),
        height: rgb.height(),
        data: rgb.into_raw(),
    })
}

pub fn encode(image: &Rgb, path: &Path) -> std::io::Result<u64> {
    let file = File::create(path)?;
    let mut encoder = jpeg_encoder::Encoder::new(std::io::BufWriter::new(file), 90);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
    let size = |edge: u32| u16::try_from(edge).map_err(|_| std::io::Error::other("tier too large"));
    encoder
        .encode(
            &image.data,
            size(image.width)?,
            size(image.height)?,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(std::io::Error::other)?;
    std::fs::metadata(path).map(|meta| meta.len())
}
