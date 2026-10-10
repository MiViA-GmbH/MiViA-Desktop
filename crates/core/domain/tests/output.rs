use mivia_domain::naming::{Language, NameParts, Stamp, report_name};
use mivia_domain::output::{
    Format, ProbeResult, ReportFolder, TooManyClashes, is_inside, next_clash,
};
use mivia_domain::path::WinPath;

fn win(text: &str) -> WinPath {
    WinPath::parse(text).unwrap()
}

fn parts(relative: &str) -> Vec<String> {
    relative.split('/').map(str::to_owned).collect()
}

#[test]
fn report_folders_resolve_per_mode() {
    let root = win(r"D:\Lab");
    let sub = ReportFolder::Subfolder("MiViA reports".to_owned());
    assert_eq!(
        sub.target(&root, &parts("a.jpg")).unwrap(),
        win(r"D:\Lab\MiViA reports")
    );
    assert_eq!(
        ReportFolder::BesideSeries
            .target(&root, &parts("Run 1/a.jpg"))
            .unwrap(),
        win(r"D:\Lab\Run 1")
    );
    assert_eq!(
        ReportFolder::BesideSeries
            .target(&root, &parts("a.jpg"))
            .unwrap(),
        root
    );
    let custom = ReportFolder::Custom(win(r"\\srv\share\Reports"));
    assert_eq!(
        custom.target(&root, &parts("a.jpg")).unwrap(),
        win(r"\\srv\share\Reports")
    );
    assert_eq!(custom.base(&root), win(r"\\srv\share\Reports"));
    assert!(sub.creates() && !custom.creates());
}

#[test]
fn report_folders_inside_the_watched_folder_are_excluded() {
    let root = win(r"D:\Lab");
    let sub = ReportFolder::Subfolder("MiViA reports".to_owned());
    let excluded = sub.excluded(&root).unwrap();
    assert!(is_inside(&parts("miVia REPORTS/a.jpg"), &excluded));
    assert!(!is_inside(&parts("MiViA reports"), &excluded));
    assert!(!is_inside(&parts("MiViA reports 2/a.jpg"), &excluded));
    let inner = ReportFolder::Custom(win(r"D:\Lab\out\pdf"));
    assert_eq!(inner.excluded(&root), Some(parts("out/pdf")));
    assert!(is_inside(&parts("out/pdf/x.tif"), &parts("out/pdf")));
    assert_eq!(ReportFolder::Custom(win(r"E:\Out")).excluded(&root), None);
    assert_eq!(ReportFolder::BesideSeries.excluded(&root), None);
}

#[test]
fn downloaded_bytes_must_match_the_format() {
    assert!(Format::Pdf.accepts(b"%PDF-1.7\n"));
    assert!(!Format::Pdf.accepts(b"<html>"));
    assert!(Format::Csv.accepts(b"PK\x03\x04rest"));
    assert!(!Format::Csv.accepts(b"%PDF-"));
    assert_eq!(Format::Csv.extension(), "zip");
    assert_eq!(Format::from_code("pdf"), Some(Format::Pdf));
    for result in [
        ProbeResult::Ok,
        ProbeResult::CreateOnly,
        ProbeResult::Denied,
    ] {
        assert_eq!(ProbeResult::from_code(result.code()), Some(result));
    }
}

#[test]
fn clash_counter_continues_after_the_highest_existing_name() {
    let name = report_name(
        &NameParts {
            stamp: Stamp {
                year: 2026,
                month: 10,
                day: 9,
                hour: 14,
                minute: 32,
            },
            label: "A1",
            analysis: "Grain",
            variant: None,
            part: None,
            update: None,
            data: false,
            language: Language::En,
        },
        20,
    );
    let plain = name.file_name(None);
    let listed = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
    assert_eq!(next_clash(&listed(&["other.pdf"]), &name), Ok(None));
    assert_eq!(
        next_clash(&listed(&[&plain.to_uppercase()]), &name),
        Ok(Some(2))
    );
    let third = name.file_name(Some(7));
    assert_eq!(next_clash(&listed(&[&plain, &third]), &name), Ok(Some(8)));
    let last = name.file_name(Some(999));
    assert_eq!(next_clash(&listed(&[&last]), &name), Err(TooManyClashes));
}
