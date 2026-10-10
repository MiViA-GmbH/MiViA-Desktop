use std::fmt::Debug;

use mivia_domain::ids::Timestamp;
use mivia_ports::ipc::change::{Change, StoreChanged};
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::views::{
    BatchCounts, BatchItem, BatchItemsPage, BatchSummary, BatchView, ComputationItem, FolderView,
    FoldersView, HistoryCursor, HistoryPage, NeedsAttentionView, OriginView,
};
use mivia_ports::time::{iso_utc, utc_date};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) -> String {
    let json = serde_json::to_string(value).unwrap();
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, value);
    json
}

fn summary(origin: OriginView) -> BatchSummary {
    BatchSummary {
        id: "7".to_owned(),
        label: "Probe 12".to_owned(),
        origin,
        open: false,
        opened_at: iso_utc(Timestamp(0)),
        closed_at: Some(iso_utc(Timestamp(1_000))),
        counts: BatchCounts {
            files: 3,
            done: 2,
            failed: 1,
            problems: 1,
            active: 0,
            reports_saved: 1,
            reports_problem: 0,
            reports_total: 1,
            reports_stale: 0,
        },
        needs_attention: true,
        closure_reason: Some("quiet_period".to_owned()),
    }
}

#[test]
fn views_round_trip_with_camel_case_fields() {
    let folder = summary(OriginView::Folder {
        folder_id: "2".to_owned(),
    });
    let json = round_trip(&folder);
    assert!(json.contains(r#""origin":{"kind":"folder","folderId":"2"}"#));
    assert!(json.contains(r#""reportsSaved":1"#));
    round_trip(&HistoryPage {
        revision: "4".to_owned(),
        head: vec![summary(OriginView::OneOff)],
        rows: vec![folder.clone()],
        next: Some(HistoryCursor {
            sort_at: "9".to_owned(),
            id: "7".to_owned(),
        }),
    });
    round_trip(&NeedsAttentionView {
        revision: "4".to_owned(),
        series: vec![folder.clone()],
        series_total: 1,
    });
    round_trip(&BatchView {
        revision: "4".to_owned(),
        summary: folder.clone(),
    });
    round_trip(&BatchItemsPage {
        revision: "4".to_owned(),
        items: vec![BatchItem {
            id: "1".to_owned(),
            label: "a.tif".to_owned(),
            state: "uploaded".to_owned(),
            computations: vec![ComputationItem {
                id: "1".to_owned(),
                model_choice_key: "m:".to_owned(),
                state: "done".to_owned(),
            }],
        }],
        next: None,
    });
    round_trip(&FoldersView {
        revision: "4".to_owned(),
        folders: vec![FolderView {
            id: "2".to_owned(),
            path: r"D:\Lab".to_owned(),
            position: 0,
            active: true,
        }],
    });
    let changed = StoreChanged {
        from_revision: "3".to_owned(),
        to_revision: "4".to_owned(),
        changes: vec![
            Change::BatchSummary {
                batch_id: "7".to_owned(),
                summary: folder,
            },
            Change::BatchItems {
                batch_id: "7".to_owned(),
            },
            Change::Folder { id: "2".to_owned() },
            Change::Reports {
                batch_id: "7".to_owned(),
            },
            Change::NeedsAttention,
            Change::Holds,
            Change::Catalogue,
            Change::Settings,
            Change::History,
            Change::All,
        ],
    };
    assert!(round_trip(&changed).contains(r#"{"type":"batchItems","batchId":"7"}"#));
}

#[test]
fn app_error_carries_signature_and_reportable_flag() {
    let error = AppError::new(
        AppErrorKind::Contract,
        Some("bad_json"),
        "unexpected body".to_owned(),
        "submit",
    );
    assert_eq!(error.signature, "contract:bad_json:submit");
    assert!(error.reportable);
    let hold = AppError::new(AppErrorKind::Hold, None, "offline".to_owned(), "upload");
    assert_eq!(hold.signature, "hold:-:upload");
    assert!(!hold.reportable);
    assert!(round_trip(&error).contains(r#""kind":"contract""#));
}

#[test]
fn timestamps_format_as_iso_utc() {
    assert_eq!(iso_utc(Timestamp(0)), "1970-01-01T00:00:00.000Z");
    assert_eq!(
        iso_utc(Timestamp(1_791_594_123_456)),
        "2026-10-10T01:02:03.456Z"
    );
    assert_eq!(
        iso_utc(Timestamp(951_782_400_000)),
        "2000-02-29T00:00:00.000Z"
    );
    assert_eq!(iso_utc(Timestamp(-1)), "1969-12-31T23:59:59.999Z");
    assert_eq!(utc_date(Timestamp(4_107_542_399_999)), "2100-02-28");
}
