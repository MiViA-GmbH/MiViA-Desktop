use mivia_domain::computation::ComputationState;
use mivia_domain::ids::Timestamp;
use mivia_domain::report::PartState;
use mivia_domain::source_file::SourceFileState;
use rusqlite::{Transaction, params};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) files: i64,
    pub(crate) done: i64,
    pub(crate) failed: i64,
    pub(crate) problems: i64,
    pub(crate) active: i64,
    pub(crate) reports_saved: i64,
    pub(crate) reports_problem: i64,
    pub(crate) reports_total: i64,
    pub(crate) reports_stale: i64,
}

pub(crate) fn source_file(state: Option<SourceFileState>) -> Counts {
    Counts {
        files: i64::from(state.is_some()),
        problems: i64::from(matches!(
            state,
            Some(SourceFileState::Rejected(_) | SourceFileState::Unreadable(_))
        )),
        ..Counts::default()
    }
}

pub(crate) fn computation(state: Option<ComputationState>) -> Counts {
    use ComputationState as S;
    Counts {
        done: i64::from(matches!(state, Some(S::Done { .. }))),
        failed: i64::from(matches!(state, Some(S::Failed(_)))),
        problems: i64::from(matches!(
            state,
            Some(S::Failed(_) | S::Unavailable | S::Lost)
        )),
        active: i64::from(matches!(
            state,
            Some(S::Queued | S::Submitting | S::Computing)
        )),
        ..Counts::default()
    }
}

pub(crate) fn part(state: Option<PartState>, in_stale_report: bool) -> Counts {
    Counts {
        reports_total: i64::from(state.is_some()),
        reports_saved: i64::from(matches!(state, Some(PartState::Saved))),
        reports_problem: i64::from(matches!(
            state,
            Some(PartState::NotSaved | PartState::Failed(_))
        )),
        reports_stale: i64::from(in_stale_report && state.is_some()),
        ..Counts::default()
    }
}

const ADD: &str = "UPDATE batch SET files = files + ?2, done = done + ?3, failed = failed + ?4, \
     problems = problems + ?5, active_count = active_count + ?6, \
     reports_saved = reports_saved + ?7, reports_problem = reports_problem + ?8, \
     reports_total = reports_total + ?9, reports_stale = reports_stale + ?10, \
     needs_attention = CASE WHEN ?5 > 0 THEN 1 ELSE needs_attention END, \
     problems_changed_at = CASE WHEN ?5 > 0 THEN ?11 ELSE problems_changed_at END \
     WHERE id = ?1";

pub(crate) fn shift(
    tx: &Transaction<'_>,
    batch_id: i64,
    change: (Counts, Counts),
    now: Timestamp,
) -> rusqlite::Result<()> {
    let (before, after) = change;
    if before == after {
        return Ok(());
    }
    tx.prepare_cached(ADD)?.execute(params![
        batch_id,
        after.files - before.files,
        after.done - before.done,
        after.failed - before.failed,
        after.problems - before.problems,
        after.active - before.active,
        after.reports_saved - before.reports_saved,
        after.reports_problem - before.reports_problem,
        after.reports_total - before.reports_total,
        after.reports_stale - before.reports_stale,
        now.0,
    ])?;
    Ok(())
}
