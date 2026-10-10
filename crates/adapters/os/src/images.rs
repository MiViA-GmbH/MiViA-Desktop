use std::path::Path;
use std::time::{Duration, Instant};

use mivia_domain::image_format::has_accepted_extension;

pub struct ImageCount {
    pub count: u32,
    pub more: bool,
}

pub fn count_images(folder: &Path, skip: &str, cap: u32, budget: Duration) -> ImageCount {
    let deadline = Instant::now() + budget;
    let mut count = 0_u32;
    let walk = walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() != 1
                || !entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(skip)
        });
    for entry in walk.filter_map(Result::ok) {
        if count >= cap || Instant::now() >= deadline {
            return ImageCount { count, more: true };
        }
        let image = entry.file_type().is_file()
            && has_accepted_extension(&entry.file_name().to_string_lossy());
        count += u32::from(image);
    }
    ImageCount { count, more: false }
}
