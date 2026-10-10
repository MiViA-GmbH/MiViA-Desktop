use mivia_domain::ids::Timestamp;
use mivia_ports::export::{Cell, ExportRequest, ExportSelection, ExportSink, ExportTable};
use mivia_ports::store::{AccountScope, StoreError};
use rusqlite::{Connection, Row, Transaction};

use crate::history::filtered_ids;

const SERIES: &str = "SELECT b.id, b.label, b.origin, f.path, b.opened_at, b.closed_at, \
     b.closure_reason FROM batch b LEFT JOIN watched_folder f ON f.id = b.folder_id \
     WHERE b.id = ?1 AND b.account_id = ?2";

const ALL_SERIES: &str = "SELECT id FROM batch INDEXED BY batch_hist_time \
     WHERE account_id = ?1 ORDER BY sort_at, id";

const IMAGES: &str = "SELECT s.path, s.label, s.size, s.mtime, lower(hex(s.fingerprint)), \
     s.state || ifnull('/' || s.reason, ''), s.server_image_id, c.id, c.server_job_id, \
     c.result_id, m.model_id, json_extract(m.names, '$.en'), c.model_version, \
     c.result_model_version, m.customization_id, \
     (SELECT json_extract(k.names, '$.en') FROM model_catalogue k \
     WHERE k.account_id = c.account_id AND k.model_id = m.model_id \
     AND k.customization_id = m.customization_id), \
     c.state, c.reason, c.reused_from, o.done_at, c.superseded_by, c.submitted_at, c.done_at, \
     c.app_version, c.server_version, \
     (SELECT group_concat(rm.report_id, ' ') FROM report_member rm WHERE rm.computation_id = c.id) \
     FROM source_file s INDEXED BY source_file_by_batch \
     LEFT JOIN computation c ON c.source_file_id = s.id \
     LEFT JOIN batch_model_choice m ON m.batch_id = c.batch_id \
     AND m.model_choice_key = c.model_choice_key \
     LEFT JOIN computation o ON o.id = c.reused_from \
     WHERE s.batch_id = ?1 ORDER BY s.arrival_seq, c.model_choice_key, c.id";

const REPORT_COLUMNS: &str = "r.id, p.server_report_id, r.kind, r.custom_name, r.batch_id, \
     (SELECT json_extract(x.names, '$.en') FROM batch_model_choice x \
     WHERE x.model_choice_key = r.model_choice_key AND x.batch_id = COALESCE(r.batch_id, \
     (SELECT c.batch_id FROM report_member rm JOIN computation c ON c.id = rm.computation_id \
     WHERE rm.report_id = r.id ORDER BY rm.seq LIMIT 1))), \
     substr(r.model_choice_key, instr(r.model_choice_key, ':') + 1), \
     (SELECT o.server_report_id FROM report_part o WHERE o.report_id = r.replaces \
     AND o.part_no = 1 AND o.format = p.format), \
     r.replaced_by, p.part_no, p.part_count, p.format, r.language, r.decimal_separator, \
     r.timezone, p.member_to - p.member_from, \
     (SELECT count(*) FROM report_member m WHERE m.report_id = r.id AND m.omitted IS NOT NULL), \
     p.requested_at, p.saved_at, p.saved_path, p.file_path, lower(hex(p.sha256)), p.size, \
     p.state || ifnull('/' || p.error, ''), r.stale, p.moved_to_replaced_at, p.app_version, \
     p.server_version";

const SERIES_EVENTS: &str = "SELECT e.id, e.at, e.tz, e.windows_user, e.batch_id, \
     e.computation_id, e.report_id, e.action, e.trigger, e.before_json, e.after_json \
     FROM history_event e INDEXED BY history_event_batch WHERE e.batch_id = ?1 ORDER BY e.id";

const ACCOUNT_EVENTS: &str = "SELECT e.id, e.at, e.tz, e.windows_user, e.batch_id, \
     e.computation_id, e.report_id, e.action, e.trigger, e.before_json, e.after_json \
     FROM history_event e INDEXED BY history_event_time WHERE e.account_id = ?1 \
     AND e.batch_id IS NULL ORDER BY e.id";

type Fail = Box<dyn std::error::Error + Send + Sync>;

fn text(row: &Row<'_>, at: usize) -> rusqlite::Result<Cell> {
    row.get::<_, Option<String>>(at).map(Cell::from)
}

fn number(row: &Row<'_>, at: usize) -> rusqlite::Result<Cell> {
    row.get::<_, Option<i64>>(at).map(Cell::from)
}

fn time(row: &Row<'_>, at: usize) -> rusqlite::Result<Cell> {
    Ok(row
        .get::<_, Option<i64>>(at)?
        .map_or(Cell::Empty, |ms| Cell::Time(Timestamp(ms))))
}

fn cells(row: &Row<'_>, kinds: &[u8]) -> rusqlite::Result<Vec<Cell>> {
    kinds
        .iter()
        .enumerate()
        .map(|(at, kind)| match kind {
            b'n' => number(row, at),
            b't' => time(row, at),
            _ => text(row, at),
        })
        .collect()
}

struct Writer<'a> {
    tx: &'a Transaction<'a>,
    sink: &'a mut dyn ExportSink,
    account: [Cell; 4],
}

impl Writer<'_> {
    fn emit(&mut self, table: ExportTable, cells: Vec<Cell>) -> Result<(), Fail> {
        self.sink
            .row(table, cells)
            .map_err(|error| format!("export sink {error:?}").into())
    }

    fn series(&mut self, batch: i64, scope: AccountScope) -> Result<(), Fail> {
        let head = self
            .tx
            .query_row(SERIES, [batch, scope.0.0], |row| cells(row, b"nssstts"))?;
        let rows = self
            .tx
            .prepare_cached(IMAGES)?
            .query_map([batch], |row| cells(row, b"ssntsssnssssssssssntnttsss"))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for images in rows {
            let mut line = self.account.to_vec();
            line.extend(head.iter().cloned());
            line.extend(images);
            self.emit(ExportTable::Images, line)?;
        }
        self.reports(&format!("SELECT {REPORT_COLUMNS} FROM report r INDEXED BY report_by_batch JOIN report_part p ON p.report_id = r.id WHERE r.batch_id = ?1 ORDER BY r.id, p.format DESC, p.part_no"), batch)?;
        self.events(SERIES_EVENTS, batch)
    }

    fn reports(&mut self, query: &str, key: i64) -> Result<(), Fail> {
        let rows = self
            .tx
            .prepare_cached(query)?
            .query_map([key], |row| cells(row, b"nsssnsssnnnssssnnttsssnsntss"))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .try_for_each(|line| self.emit(ExportTable::Reports, line))
    }

    fn events(&mut self, query: &str, key: i64) -> Result<(), Fail> {
        let account = self.account.first().cloned().unwrap_or(Cell::Empty);
        let rows = self
            .tx
            .prepare_cached(query)?
            .query_map([key], |row| cells(row, b"ntssnnnssss"))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for mut line in rows {
            line.insert(4, account.clone());
            self.emit(ExportTable::Changes, line)?;
        }
        Ok(())
    }
}

fn series_ids(
    tx: &Transaction<'_>,
    scope: AccountScope,
    request: &ExportRequest,
) -> rusqlite::Result<Vec<i64>> {
    match &request.selection {
        ExportSelection::All => tx
            .prepare_cached(ALL_SERIES)?
            .query_map([scope.0.0], |row| row.get(0))?
            .collect(),
        ExportSelection::Series(id) => Ok(vec![id.0]),
        ExportSelection::Filtered(filter) => filtered_ids(tx, scope, filter),
    }
}

pub(crate) fn export(
    conn: &mut Connection,
    scope: AccountScope,
    request: &ExportRequest,
    sink: &mut dyn ExportSink,
) -> Result<(), Fail> {
    let tx = conn.transaction()?;
    let (name, server): (String, String) = tx.query_row(
        "SELECT display_name, server_url FROM account WHERE id = ?1",
        [scope.0.0],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let ids = series_ids(&tx, scope, request)?;
    let mut writer = Writer {
        tx: &tx,
        sink,
        account: [
            Cell::Text(name),
            Cell::Text(server),
            Cell::Text(request.install_id.clone()),
            Cell::Text(request.computer_name.clone()),
        ],
    };
    for id in ids {
        writer.series(id, scope)?;
    }
    if request.selection == ExportSelection::All {
        writer.reports(&format!("SELECT {REPORT_COLUMNS} FROM report r INDEXED BY report_custom JOIN report_part p ON p.report_id = r.id WHERE r.account_id = ?1 AND r.kind = 'custom' ORDER BY r.created_at, r.id, p.format DESC, p.part_no"), scope.0.0)?;
        writer.events(ACCOUNT_EVENTS, scope.0.0)?;
    }
    drop(writer);
    tx.commit()?;
    Ok(())
}

pub(crate) fn failure(error: &Fail) -> StoreError {
    StoreError::Sql(error.to_string())
}
