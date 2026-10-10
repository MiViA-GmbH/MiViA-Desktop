use mivia_domain::ids::Timestamp;
use mivia_domain::path::WinPath;
use mivia_ports::store::{AccountScope, SentImages, SettingsScope, TxError, WriteTx};

use crate::setup::{SetupDraft, read_draft, write_draft};

fn merge(sent: &mut SentImages, spool: Option<&str>, paths: &[WinPath]) -> bool {
    if let Some(id) = spool {
        if sent.spool_ids.iter().any(|known| known == id) {
            return false;
        }
        sent.spool_ids.push(id.to_owned());
    }
    for path in paths {
        if !sent.paths.iter().any(|known| fold_eq(known, path)) {
            sent.paths.push(path.to_string());
        }
    }
    true
}

fn fold_eq(known: &str, path: &WinPath) -> bool {
    WinPath::parse(known).is_ok_and(|known| known.key() == path.key())
}

fn absorb_draft(tx: &mut dyn WriteTx, pending: &mut SentImages) -> Result<(), TxError> {
    let mut draft = read_draft(tx)?;
    if draft.sent == SentImages::default() {
        return Ok(());
    }
    let paths: Vec<WinPath> = draft
        .sent
        .paths
        .iter()
        .filter_map(|text| WinPath::parse(text).ok())
        .collect();
    merge(pending, None, &paths);
    pending.spool_ids.append(&mut draft.sent.spool_ids);
    draft.sent = SentImages::default();
    if draft == SetupDraft::default() {
        tx.clear_settings(SettingsScope::SetupDraft)
    } else {
        write_draft(tx, &draft)
    }
}

pub fn pending(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    now: Timestamp,
) -> Result<SentImages, TxError> {
    let mut pending = tx.pending_one_off(scope)?.unwrap_or_default();
    let before = pending.clone();
    absorb_draft(tx, &mut pending)?;
    if pending != before {
        tx.set_pending_one_off(scope, Some(&pending), now)?;
    }
    Ok(pending)
}

pub fn stage(
    tx: &mut dyn WriteTx,
    scope: Option<AccountScope>,
    sent: (Option<&str>, &[WinPath]),
    now: Timestamp,
) -> Result<bool, TxError> {
    let (spool, paths) = sent;
    let Some(scope) = scope else {
        let mut draft = read_draft(tx)?;
        let staged = merge(&mut draft.sent, spool, paths);
        write_draft(tx, &draft)?;
        return Ok(staged);
    };
    let mut current = pending(tx, scope, now)?;
    let staged = merge(&mut current, spool, paths);
    tx.set_pending_one_off(scope, Some(&current), now)?;
    Ok(staged)
}

pub fn discard(tx: &mut dyn WriteTx, scope: AccountScope, now: Timestamp) -> Result<(), TxError> {
    pending(tx, scope, now)?;
    tx.set_pending_one_off(scope, None, now)
}

pub(super) fn in_group(text: &str, folder: &WinPath) -> bool {
    WinPath::parse(text).is_ok_and(|path| {
        path.key() == folder.key() || path.parent().is_some_and(|p| p.key() == folder.key())
    })
}

pub(super) fn keep_pending(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    left: &SentImages,
    now: Timestamp,
) -> Result<(), TxError> {
    let left = (!left.paths.is_empty()).then_some(left);
    tx.set_pending_one_off(scope, left, now)
}
