use mivia_domain::fuzz::image_format_check;
use mivia_domain::image_format::{FormatCheck, ImageFormat, check_format, has_accepted_extension};
use proptest::prelude::*;

const PNG_IEND: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];

fn jpeg() -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
    bytes.extend([0xFF, 0xD9]);
    bytes
}

fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 9, 9, 9];
    bytes.extend(PNG_IEND);
    bytes
}

fn bmp(len: u32) -> Vec<u8> {
    let mut bytes = b"BM".to_vec();
    bytes.extend(len.to_le_bytes());
    bytes.resize(usize::try_from(len).expect("small"), 0);
    bytes
}

fn entry(big_endian: bool, tag: u16, kind: u16, count: u32, value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let push16 = |out: &mut Vec<u8>, v: u16| {
        out.extend(if big_endian {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        });
    };
    let push32 = |out: &mut Vec<u8>, v: u32| {
        out.extend(if big_endian {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        });
    };
    push16(&mut out, tag);
    push16(&mut out, kind);
    push32(&mut out, count);
    if kind == 3 && count == 1 {
        push16(&mut out, u16::try_from(value).expect("short"));
        push16(&mut out, 0);
    } else {
        push32(&mut out, value);
    }
    out
}

fn tiff(big_endian: bool, strip_len: u32) -> Vec<u8> {
    let u16b = |v: u16| {
        if big_endian {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    };
    let u32b = |v: u32| {
        if big_endian {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    };
    let mut bytes = if big_endian {
        b"MM".to_vec()
    } else {
        b"II".to_vec()
    };
    bytes.extend(u16b(42));
    bytes.extend(u32b(8));
    bytes.extend(u16b(2));
    let data_at = 8 + 2 + 2 * 12 + 4;
    bytes.extend(entry(big_endian, 273, 4, 1, data_at));
    bytes.extend(entry(big_endian, 279, 3, 1, strip_len));
    bytes.extend(u32b(0));
    bytes.extend(vec![7; usize::try_from(strip_len).expect("small")]);
    bytes
}

fn tiff_with_out_of_line_strips() -> Vec<u8> {
    let mut bytes = b"II".to_vec();
    bytes.extend(42_u16.to_le_bytes());
    bytes.extend(8_u32.to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    let arrays_at = 8 + 2 + 2 * 12 + 4;
    bytes.extend(entry(false, 273, 4, 3, arrays_at));
    bytes.extend(entry(false, 279, 4, 3, arrays_at + 12));
    bytes.extend(0_u32.to_le_bytes());
    let data_at = arrays_at + 24;
    for value in [data_at, data_at + 4, data_at + 8] {
        bytes.extend(value.to_le_bytes());
    }
    for _ in 0..3 {
        bytes.extend(4_u32.to_le_bytes());
    }
    bytes.extend([1; 12]);
    bytes
}

fn big_tiff(strip_len: u64) -> Vec<u8> {
    let mut bytes = b"II".to_vec();
    bytes.extend(43_u16.to_le_bytes());
    bytes.extend(8_u16.to_le_bytes());
    bytes.extend(0_u16.to_le_bytes());
    bytes.extend(16_u64.to_le_bytes());
    bytes.extend(2_u64.to_le_bytes());
    let data_at = 16 + 8 + 2 * 20 + 8;
    for (tag, value) in [(273_u16, data_at), (279, strip_len)] {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(1_u64.to_le_bytes());
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(0_u64.to_le_bytes());
    bytes.extend(vec![5; usize::try_from(strip_len).expect("small")]);
    bytes
}

#[test]
fn complete_files_pass() {
    assert_eq!(
        check_format(&jpeg()),
        FormatCheck::Complete(ImageFormat::Jpeg)
    );
    assert_eq!(
        check_format(&png()),
        FormatCheck::Complete(ImageFormat::Png)
    );
    assert_eq!(
        check_format(&bmp(40)),
        FormatCheck::Complete(ImageFormat::Bmp)
    );
    assert_eq!(
        check_format(&tiff(false, 16)),
        FormatCheck::Complete(ImageFormat::Tiff)
    );
    assert_eq!(
        check_format(&tiff(true, 16)),
        FormatCheck::Complete(ImageFormat::Tiff)
    );
    assert_eq!(
        check_format(&tiff_with_out_of_line_strips()),
        FormatCheck::Complete(ImageFormat::Tiff)
    );
    assert_eq!(
        check_format(&big_tiff(20)),
        FormatCheck::Complete(ImageFormat::BigTiff)
    );
}

#[test]
fn truncated_or_broken_files_are_incomplete() {
    let cut = |bytes: Vec<u8>| bytes.get(..bytes.len() - 3).expect("long enough").to_vec();
    assert_eq!(
        check_format(&cut(jpeg())),
        FormatCheck::Incomplete(ImageFormat::Jpeg)
    );
    assert_eq!(
        check_format(&cut(png())),
        FormatCheck::Incomplete(ImageFormat::Png)
    );
    assert_eq!(
        check_format(&cut(bmp(40))),
        FormatCheck::Incomplete(ImageFormat::Bmp)
    );
    assert_eq!(
        check_format(&cut(tiff(false, 16))),
        FormatCheck::Incomplete(ImageFormat::Tiff)
    );
    assert_eq!(
        check_format(&cut(tiff_with_out_of_line_strips())),
        FormatCheck::Incomplete(ImageFormat::Tiff)
    );
    assert_eq!(
        check_format(&cut(big_tiff(20))),
        FormatCheck::Incomplete(ImageFormat::BigTiff)
    );
    let mut looping = tiff(false, 4);
    looping.splice(34..38, 8_u32.to_le_bytes());
    assert_eq!(
        check_format(&looping),
        FormatCheck::Incomplete(ImageFormat::Tiff)
    );
    let mut zero_first = tiff(false, 4);
    zero_first.splice(4..8, 0_u32.to_le_bytes());
    assert_eq!(
        check_format(&zero_first),
        FormatCheck::Incomplete(ImageFormat::Tiff)
    );
    let mut lonely_offsets = tiff(false, 4);
    lonely_offsets.splice(22..24, 300_u16.to_le_bytes());
    assert_eq!(
        check_format(&lonely_offsets),
        FormatCheck::Incomplete(ImageFormat::Tiff)
    );
    let mut odd_type = tiff(false, 4);
    odd_type.splice(12..14, 7_u16.to_le_bytes());
    assert_eq!(
        check_format(&odd_type),
        FormatCheck::Incomplete(ImageFormat::Tiff)
    );
}

#[test]
fn unknown_content_is_reported() {
    assert_eq!(check_format(b""), FormatCheck::Unknown);
    assert_eq!(check_format(b"GIF89a"), FormatCheck::Unknown);
    assert_eq!(check_format(b"II\x2b"), FormatCheck::Unknown);
    assert_eq!(check_format(b"MM\x00\x07"), FormatCheck::Unknown);
    let png_named_jpg = png();
    assert_eq!(
        check_format(&png_named_jpg),
        FormatCheck::Complete(ImageFormat::Png)
    );
}

#[test]
fn accepted_extensions() {
    for name in ["a.jpg", "a.JPEG", "b.Png", "c.tif", "d.TIFF", "e.bmp"] {
        assert!(has_accepted_extension(name), "{name}");
    }
    for name in ["a.gif", "a", "a.pdf", "jpg", "a.tif.tmp"] {
        assert!(!has_accepted_extension(name), "{name}");
    }
}

proptest! {
    #[test]
    fn any_truncation_of_a_complete_file_is_detected(len in 1_u32..64, keep in 0_usize..200, kind in 0_u8..5) {
        let file = match kind {
            0 => jpeg(),
            1 => png(),
            2 => bmp(len + 6),
            3 => tiff(len % 2 == 0, len),
            _ => big_tiff(u64::from(len)),
        };
        let keep = keep % file.len();
        let truncated = file.get(..keep).expect("prefix");
        prop_assert!(!matches!(check_format(truncated), FormatCheck::Complete(_)));
    }

    #[test]
    fn arbitrary_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..512)) {
        prop_assert_eq!(image_format_check(&data), Ok(()));
    }

    #[test]
    fn mutated_tiff_headers_never_panic(position in 0_usize..60, value in any::<u8>()) {
        let mut file = tiff(false, 16);
        if let Some(byte) = file.get_mut(position) {
            *byte = value;
        }
        prop_assert_eq!(image_format_check(&file), Ok(()));
    }
}
