use std::io::Cursor;

use tauri::image::Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    Idle,
    Working,
    Attention,
}

impl Mark {
    pub(super) const fn working(working: bool) -> Self {
        if working { Self::Working } else { Self::Idle }
    }

    pub(super) const fn raised(self, attention: bool) -> Self {
        if attention { Self::Attention } else { self }
    }

    pub(super) const fn tooltip(self) -> &'static str {
        match self {
            Self::Idle => "MiViA Desktop",
            Self::Working => "MiViA Desktop: working",
            Self::Attention => "MiViA Desktop: needs attention",
        }
    }
}

type Frames = [(u32, &'static [u8]); 6];

macro_rules! frames {
    ($name:literal) => {
        [
            (
                16,
                include_bytes!(concat!("../../../icons/tray/", $name, "-16.png")),
            ),
            (
                20,
                include_bytes!(concat!("../../../icons/tray/", $name, "-20.png")),
            ),
            (
                24,
                include_bytes!(concat!("../../../icons/tray/", $name, "-24.png")),
            ),
            (
                32,
                include_bytes!(concat!("../../../icons/tray/", $name, "-32.png")),
            ),
            (
                40,
                include_bytes!(concat!("../../../icons/tray/", $name, "-40.png")),
            ),
            (
                48,
                include_bytes!(concat!("../../../icons/tray/", $name, "-48.png")),
            ),
        ]
    };
}

fn frames(light: bool, mark: Mark) -> Frames {
    match (light, mark) {
        (true, Mark::Idle) => frames!("light-idle"),
        (true, Mark::Working) => frames!("light-working"),
        (true, Mark::Attention) => frames!("light-attention"),
        (false, Mark::Idle) => frames!("dark-idle"),
        (false, Mark::Working) => frames!("dark-working"),
        (false, Mark::Attention) => frames!("dark-attention"),
    }
}

fn nearest(frames: &Frames, wanted: u32) -> Option<&'static [u8]> {
    frames
        .iter()
        .find(|(size, _)| *size >= wanted)
        .or_else(|| frames.last())
        .map(|(_, bytes)| *bytes)
}

pub(super) fn image(light: bool, mark: Mark, wanted: u32) -> Option<Image<'static>> {
    let mut reader = png::Decoder::new(Cursor::new(nearest(&frames(light, mark), wanted)?))
        .read_info()
        .ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(info.buffer_size());
    Some(Image::new_owned(rgba, info.width, info.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attention_wins_over_working() {
        assert_eq!(Mark::working(true).raised(true), Mark::Attention);
        assert_eq!(Mark::working(true).raised(false), Mark::Working);
        assert_eq!(Mark::working(false).raised(false), Mark::Idle);
    }

    #[test]
    fn tooltips_stay_within_the_shell_limit() {
        for mark in [Mark::Idle, Mark::Working, Mark::Attention] {
            assert!(mark.tooltip().chars().count() <= 127);
        }
    }

    #[test]
    fn the_next_larger_frame_is_chosen_and_the_largest_caps_it() {
        let set = frames(false, Mark::Idle);
        assert_eq!(nearest(&set, 16), Some(set[0].1));
        assert_eq!(nearest(&set, 21), Some(set[2].1));
        assert_eq!(nearest(&set, 96), Some(set[5].1));
    }

    #[test]
    fn every_theme_and_state_decodes_at_the_requested_size() {
        for light in [false, true] {
            for mark in [Mark::Idle, Mark::Working, Mark::Attention] {
                for size in [16, 20, 24, 32, 40, 48] {
                    let image = image(light, mark, size).expect("frame decodes");
                    assert_eq!((image.width(), image.height()), (size, size));
                }
            }
        }
    }
}
