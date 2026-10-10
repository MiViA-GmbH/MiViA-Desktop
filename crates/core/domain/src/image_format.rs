use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Tiff,
    BigTiff,
    Bmp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatCheck {
    Complete(ImageFormat),
    Incomplete(ImageFormat),
    Unknown,
}

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const PNG_IEND: &[u8] = &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];
const MAX_IFDS: usize = 4096;
const MAX_ARRAY_BYTES: u64 = 64 * 1024 * 1024;
const STRIP_TAGS: [(u16, u16); 2] = [(273, 279), (324, 325)];

pub const ACCEPTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff", "bmp"];

pub fn has_accepted_extension(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, extension)| {
        ACCEPTED_EXTENSIONS
            .iter()
            .any(|accepted| accepted.eq_ignore_ascii_case(extension))
    })
}

pub trait ByteSource {
    fn size(&self) -> u64;
    fn read_at(&self, at: u64, buffer: &mut [u8]) -> bool;
}

impl ByteSource for [u8] {
    fn size(&self) -> u64 {
        u64::try_from(self.len()).unwrap_or(u64::MAX)
    }

    fn read_at(&self, at: u64, buffer: &mut [u8]) -> bool {
        let window = usize::try_from(at).ok().and_then(|start| {
            let end = start.checked_add(buffer.len())?;
            self.get(start..end)
        });
        window.is_some_and(|window| {
            buffer.copy_from_slice(window);
            true
        })
    }
}

pub fn check_format(bytes: &[u8]) -> FormatCheck {
    check_source(bytes)
}

pub fn check_source<S: ByteSource + ?Sized>(source: &S) -> FormatCheck {
    let Some(format) = detect(source) else {
        return FormatCheck::Unknown;
    };
    let complete = match format {
        ImageFormat::Jpeg => ends_with(source, &[0xFF, 0xD9]),
        ImageFormat::Png => ends_with(source, PNG_IEND),
        ImageFormat::Tiff | ImageFormat::BigTiff => Tiff::new(source, format).complete(),
        ImageFormat::Bmp => read_u32_le(source, 2) == Some(source.size()),
    };
    if complete {
        FormatCheck::Complete(format)
    } else {
        FormatCheck::Incomplete(format)
    }
}

fn head<S: ByteSource + ?Sized>(source: &S) -> Vec<u8> {
    let length = usize::try_from(source.size().min(8)).unwrap_or_default();
    let mut buffer = vec![0; length];
    if source.read_at(0, &mut buffer) {
        buffer
    } else {
        Vec::new()
    }
}

fn ends_with<S: ByteSource + ?Sized>(source: &S, suffix: &[u8]) -> bool {
    let length = u64::try_from(suffix.len()).unwrap_or(u64::MAX);
    let mut buffer = vec![0; suffix.len()];
    source
        .size()
        .checked_sub(length)
        .is_some_and(|at| source.read_at(at, &mut buffer) && buffer == suffix)
}

fn detect<S: ByteSource + ?Sized>(source: &S) -> Option<ImageFormat> {
    let bytes = head(source);
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageFormat::Jpeg);
    }
    if bytes.starts_with(PNG_SIGNATURE) {
        return Some(ImageFormat::Png);
    }
    if bytes.starts_with(b"BM") {
        return Some(ImageFormat::Bmp);
    }
    let little = bytes.starts_with(b"II");
    if !little && !bytes.starts_with(b"MM") {
        return None;
    }
    let header = Tiff {
        source,
        little,
        big: false,
    };
    match header.uint(2, 2)? {
        42 => Some(ImageFormat::Tiff),
        43 => Some(ImageFormat::BigTiff),
        _ => None,
    }
}

fn read_u32_le<S: ByteSource + ?Sized>(source: &S, at: u64) -> Option<u64> {
    let mut window = [0; 4];
    source
        .read_at(at, &mut window)
        .then(|| u64::from(u32::from_le_bytes(window)))
}

struct Tiff<'a, S: ByteSource + ?Sized> {
    source: &'a S,
    little: bool,
    big: bool,
}

impl<'a, S: ByteSource + ?Sized> Tiff<'a, S> {
    fn new(source: &'a S, format: ImageFormat) -> Self {
        Self {
            source,
            little: head(source).starts_with(b"II"),
            big: format == ImageFormat::BigTiff,
        }
    }

    fn complete(&self) -> bool {
        let first = if self.big {
            self.uint(8, 8)
        } else {
            self.uint(4, 4)
        };
        if first.is_none_or(|offset| offset == 0) {
            return false;
        }
        let mut next = first;
        let mut seen = BTreeSet::new();
        while let Some(offset) = next.filter(|&offset| offset != 0) {
            if seen.len() >= MAX_IFDS || !seen.insert(offset) {
                return false;
            }
            match self.ifd(offset) {
                Some(following) => next = Some(following),
                None => return false,
            }
        }
        next.is_some()
    }

    fn ifd(&self, offset: u64) -> Option<u64> {
        let (count_size, entry_size, value_size) = if self.big { (8, 20, 8) } else { (2, 12, 4) };
        let count = self.uint(offset, count_size)?;
        let entries = offset.checked_add(count_size)?;
        let end = entries.checked_add(count.checked_mul(entry_size)?)?;
        let next = self.uint(end, value_size)?;
        let entry = |index: u64| entries + index * entry_size;
        let ok = STRIP_TAGS.iter().all(|&(offsets_tag, counts_tag)| {
            let find = |tag: u16| {
                (0..count)
                    .map(entry)
                    .find(|&at| self.uint(at, 2) == Some(u64::from(tag)))
            };
            match (find(offsets_tag), find(counts_tag)) {
                (Some(offsets), Some(counts)) => self.strips_inside(offsets, counts),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
        });
        ok.then_some(next)
    }

    fn strips_inside(&self, offsets: u64, counts: u64) -> bool {
        let (Some(starts), Some(lengths)) = (self.values(offsets), self.values(counts)) else {
            return false;
        };
        let size = self.source.size();
        starts.len() == lengths.len()
            && starts
                .iter()
                .zip(&lengths)
                .all(|(start, length)| start.checked_add(*length).is_some_and(|end| end <= size))
    }

    fn values(&self, entry: u64) -> Option<Vec<u64>> {
        let width = match self.uint(entry + 2, 2)? {
            3 => 2,
            4 => 4,
            16 => 8,
            _ => return None,
        };
        let (count_size, inline) = if self.big { (8, 8) } else { (4, 4) };
        let count = self.uint(entry + 4, count_size)?;
        let total = count.checked_mul(width)?;
        let data = if total <= inline {
            entry + 4 + count_size
        } else {
            self.uint(entry + 4 + count_size, inline)?
        };
        if total > self.source.size().min(MAX_ARRAY_BYTES) {
            return None;
        }
        let mut array = vec![0; usize::try_from(total).ok()?];
        if !self.source.read_at(data, &mut array) {
            return None;
        }
        let step = usize::try_from(width).ok()?;
        Some(
            array
                .chunks_exact(step)
                .map(|chunk| self.fold(chunk))
                .collect(),
        )
    }

    fn uint(&self, at: u64, width: u64) -> Option<u64> {
        let mut window = [0; 8];
        let window = window.get_mut(..usize::try_from(width).ok()?)?;
        self.source.read_at(at, window).then(|| self.fold(window))
    }

    fn fold(&self, window: &[u8]) -> u64 {
        let fold = |value: u64, byte: &u8| (value << 8) | u64::from(*byte);
        if self.little {
            window.iter().rev().fold(0, fold)
        } else {
            window.iter().fold(0, fold)
        }
    }
}
