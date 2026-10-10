use mivia_domain::ids::Timestamp;
use mivia_domain::image_format::has_accepted_extension;
use mivia_domain::intake::stem;
use mivia_domain::naming::Language;
use mivia_domain::natural;
use mivia_domain::output::ProbeResult;
use mivia_domain::path::WinPath;
use mivia_domain::sanitise::clean_text;
use mivia_ports::fs::FileSystem;
use mivia_ports::ipc::one_off::{ImageGroupView, NotIncludedReason, NotIncludedView};
use mivia_ports::store::intake::FolderRow;

const LABEL_MAX_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Image {
    pub(super) path: WinPath,
    pub(super) size: u64,
    pub(super) mtime: Timestamp,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Expanded {
    pub(super) images: Vec<Image>,
    pub(super) not_included: Vec<NotIncludedView>,
    pub(super) subfolders: u32,
}

fn name_of(path: &WinPath) -> String {
    path.file_name().unwrap_or_default().to_owned()
}

fn by_name(left: &Image, right: &Image) -> std::cmp::Ordering {
    natural::compare(&name_of(&left.path), &name_of(&right.path))
}

impl Expanded {
    fn skip(&mut self, text: &str, reason: NotIncludedReason) {
        let name = WinPath::parse(text).map_or_else(|_| text.to_owned(), |path| name_of(&path));
        self.not_included.push(NotIncludedView { name, reason });
    }

    fn add(&mut self, path: WinPath, size: u64, mtime: Timestamp) {
        if !self
            .images
            .iter()
            .any(|image| image.path.key() == path.key())
        {
            self.images.push(Image { path, size, mtime });
        }
    }

    pub(super) fn groups(&self) -> Vec<ImageGroupView> {
        let mut groups: Vec<(WinPath, Vec<&Image>)> = Vec::new();
        for image in &self.images {
            let folder = image.path.parent().unwrap_or_else(|| image.path.clone());
            match groups
                .iter_mut()
                .find(|(known, _)| known.key() == folder.key())
            {
                Some((_, members)) => members.push(image),
                None => groups.push((folder, vec![image])),
            }
        }
        groups
            .into_iter()
            .map(|(folder, mut members)| {
                members.sort_by(|left, right| by_name(left, right));
                ImageGroupView {
                    folder: folder.to_string(),
                    names: members.iter().map(|image| name_of(&image.path)).collect(),
                }
            })
            .collect()
    }

    pub(super) fn first(&self) -> Option<&Image> {
        self.images
            .iter()
            .min_by(|left, right| by_name(left, right))
    }

    pub(super) fn label(&self) -> String {
        let first = self.first().map(|image| name_of(&image.path));
        let stem = first.as_deref().map(stem).unwrap_or_default();
        let raw = match self.images.len() {
            0 | 1 => stem.to_owned(),
            count => format!("{stem} +{}", count - 1),
        };
        clean_text(&raw).chars().take(LABEL_MAX_CHARS).collect()
    }
}

async fn expand_folder(fs: &dyn FileSystem, folder: &WinPath, into: &mut Expanded) {
    let mut names = fs.list_names(folder).await.unwrap_or_default();
    names.sort_by(|left, right| natural::compare(left, right));
    for name in names {
        let Ok(path) = folder.join(&name) else {
            continue;
        };
        match fs.stat(&path).await {
            Ok(stat) if stat.is_dir => into.subfolders += 1,
            Ok(stat) if has_accepted_extension(&name) => into.add(path, stat.size, stat.mtime),
            Ok(_) | Err(_) => {}
        }
    }
}

async fn expand_one(fs: &dyn FileSystem, text: &str, temp: Option<&WinPath>, into: &mut Expanded) {
    let Ok(path) = WinPath::parse(text) else {
        return into.skip(text, NotIncludedReason::Missing);
    };
    if temp.is_some_and(|temp| temp.contains(&path)) {
        return into.skip(text, NotIncludedReason::ZipTemp);
    }
    match fs.stat(&path).await {
        Ok(stat) if stat.is_dir => expand_folder(fs, &path, into).await,
        Ok(stat) if has_accepted_extension(&name_of(&path)) => {
            into.add(path, stat.size, stat.mtime);
        }
        Ok(_) => into.skip(text, NotIncludedReason::NotImage),
        Err(_) => into.skip(text, NotIncludedReason::Missing),
    }
}

pub(super) async fn expand(
    fs: &dyn FileSystem,
    staged: &[String],
    temp: Option<&WinPath>,
) -> Expanded {
    let mut expanded = Expanded::default();
    for text in staged {
        expand_one(fs, text, temp, &mut expanded).await;
    }
    expanded
}

pub(super) struct ReportPlace<'a> {
    pub(super) folders: &'a [FolderRow],
    pub(super) language: Language,
    pub(super) fallback: Option<&'a WinPath>,
}

fn watched_target(first: &WinPath, folders: &[FolderRow]) -> Option<WinPath> {
    folders.iter().find_map(|row| {
        let relative = row.path.relative_components(first)?;
        row.report_folder.target(&row.path, relative).ok()
    })
}

pub(super) async fn report_target(
    fs: &dyn FileSystem,
    first: &WinPath,
    place: &ReportPlace<'_>,
) -> (WinPath, bool) {
    if let Some(target) = watched_target(first, place.folders) {
        return (target, false);
    }
    let parent = first.parent().unwrap_or_else(|| first.clone());
    let probe = fs.probe_folder(&parent, true).await;
    let beside = parent.join(place.language.report_folder_name()).ok();
    match (probe, beside, place.fallback) {
        (Ok(ProbeResult::Ok | ProbeResult::CreateOnly), Some(target), _) => (target, false),
        (_, _, Some(fallback)) => (fallback.clone(), true),
        (_, beside, None) => (beside.unwrap_or(parent), false),
    }
}
