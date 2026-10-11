use std::fs;
use std::path::{Path, PathBuf};

use crate::{DISPLAY_EDGE, Rendering, VIEW_EDGE, render_file};

fn long_edge(path: &Path) -> Result<u32, String> {
    let (width, height) = image::image_dimensions(path).map_err(|error| error.to_string())?;
    Ok(width.max(height))
}

fn check(rendered: &Rendering) -> Result<(), String> {
    let long = rendered.width.max(rendered.height);
    if long == 0 {
        return Err("empty source".to_owned());
    }
    if long_edge(&rendered.display)? != long.min(DISPLAY_EDGE) {
        return Err("display tier has the wrong size".to_owned());
    }
    match &rendered.view {
        Some(view) if long_edge(view)? == long.min(VIEW_EDGE) => Ok(()),
        Some(_) => Err("view tier has the wrong size".to_owned()),
        None => Ok(()),
    }
}

pub fn render(data: &[u8]) -> Result<(), String> {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("mivia-preview-fuzz-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let input = dir.join("input.bin");
    fs::write(&input, data).map_err(|error| error.to_string())?;
    let verdict = render_file(&dir.join("previews"), &input).map_or(Ok(()), |found| check(&found));
    fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
    verdict
}
