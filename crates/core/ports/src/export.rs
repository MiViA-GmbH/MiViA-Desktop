use mivia_domain::history::csv_line;
use mivia_domain::ids::{BatchId, Timestamp};
use mivia_domain::path::WinPath;

use crate::fs::FsError;
use crate::ipc::history::HistoryFilter;
use crate::time::iso_with_offset;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExportTable {
    Images,
    Reports,
    Changes,
}

impl ExportTable {
    pub const ALL: [Self; 3] = [Self::Images, Self::Reports, Self::Changes];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Reports => "reports",
            Self::Changes => "changes",
        }
    }

    pub const fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Images => IMAGE_COLUMNS,
            Self::Reports => REPORT_COLUMNS,
            Self::Changes => CHANGE_COLUMNS,
        }
    }
}

pub const IMAGE_COLUMNS: &[&str] = &[
    "account",
    "server",
    "install_id",
    "computer_name",
    "series_id",
    "series_label",
    "origin",
    "watched_folder_path",
    "series_opened_at",
    "series_closed_at",
    "closure_reason",
    "source_path",
    "source_file_name",
    "source_size",
    "source_mtime",
    "image_sha256",
    "source_state",
    "image_id",
    "computation_id",
    "job_id",
    "result_id",
    "model_id",
    "model_name",
    "model_version",
    "result_model_version",
    "customization_id",
    "customization_name",
    "computation_state",
    "failure_reason",
    "reused_from_computation_id",
    "reused_result_date",
    "superseded_by_computation_id",
    "submitted_at",
    "done_at",
    "app_version",
    "server_version",
    "report_ids",
];

pub const REPORT_COLUMNS: &[&str] = &[
    "report_id",
    "server_report_id",
    "kind",
    "custom_name",
    "series_id",
    "model_name",
    "customization_name",
    "replaces_server_report_id",
    "replaced_by_report_id",
    "part",
    "parts",
    "format",
    "language",
    "decimal_separator",
    "timezone",
    "included_count",
    "omitted_count",
    "requested_at",
    "saved_at",
    "saved_path",
    "current_path",
    "file_sha256",
    "file_size",
    "state",
    "stale",
    "moved_to_replaced_at",
    "app_version",
    "server_version",
];

pub const CHANGE_COLUMNS: &[&str] = &[
    "event_id",
    "at",
    "tz",
    "windows_user",
    "account",
    "series_id",
    "computation_id",
    "report_id",
    "action",
    "trigger",
    "before",
    "after",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    Text(String),
    Number(i64),
    Time(Timestamp),
    Empty,
}

pub const BOM: &str = "\u{feff}";

impl Cell {
    pub fn text(&self, offset_minutes: &dyn Fn(Timestamp) -> i64) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Number(number) => number.to_string(),
            Self::Time(at) => iso_with_offset(*at, offset_minutes(*at)),
            Self::Empty => String::new(),
        }
    }
}

pub fn header_line(table: ExportTable, separator: char) -> String {
    let names: Vec<String> = table
        .columns()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    format!("{BOM}{}", csv_line(&names, separator))
}

pub fn row_line(
    cells: &[Cell],
    separator: char,
    offset_minutes: &dyn Fn(Timestamp) -> i64,
) -> String {
    let texts: Vec<String> = cells.iter().map(|cell| cell.text(offset_minutes)).collect();
    csv_line(&texts, separator)
}

impl From<Option<String>> for Cell {
    fn from(value: Option<String>) -> Self {
        value.map_or(Self::Empty, Self::Text)
    }
}

impl From<Option<i64>> for Cell {
    fn from(value: Option<i64>) -> Self {
        value.map_or(Self::Empty, Self::Number)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportSelection {
    All,
    Filtered(HistoryFilter),
    Series(BatchId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportRequest {
    pub selection: ExportSelection,
    pub install_id: String,
    pub computer_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedFile {
    pub table: ExportTable,
    pub path: WinPath,
    pub sha256: String,
    pub rows: u64,
}

pub trait ExportSink: Send {
    fn row(&mut self, table: ExportTable, cells: Vec<Cell>) -> Result<(), FsError>;
    fn finish(self: Box<Self>) -> Result<Vec<ExportedFile>, FsError>;
}
