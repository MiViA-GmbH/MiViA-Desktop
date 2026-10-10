use std::collections::{BTreeMap, BTreeSet};

use mivia_domain::history::CUSTOM_REPORT_LIMIT;
use mivia_domain::ids::{BatchId, ReportId, Timestamp};
use mivia_domain::natural;
use mivia_domain::path::WinPath;
use mivia_domain::report::{MAX_PART_MEMBERS, Membership, membership};
use mivia_ports::ipc::history::{CustomAnalysisView, CustomReportDraft, OmittedView};
use mivia_ports::ipc::screens::{AnalysisView, Language};
use mivia_ports::store::history::{CustomMember, MemberCandidate, NewCustomReport};
use mivia_ports::store::reports::ReportSpec;
use mivia_ports::store::{AccountScope, SettingsScope, Store, StoreExt};

use super::{History, HistoryError};
use crate::settings::{GlobalSettings, write_global};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomRequest {
    pub series: Vec<BatchId>,
    pub name: String,
    pub spec: ReportSpec,
    pub by_date: bool,
    pub folder: WinPath,
    pub app_version: String,
    pub now: Timestamp,
}

struct Plan {
    choice: String,
    members: Vec<CustomMember>,
    included: usize,
}

fn order(by_date: bool) -> impl Fn(&MemberCandidate, &MemberCandidate) -> std::cmp::Ordering {
    move |a, b| {
        let dates = if by_date {
            a.done_at.cmp(&b.done_at)
        } else {
            std::cmp::Ordering::Equal
        };
        dates
            .then_with(|| natural::compare(&a.label, &b.label))
            .then(a.batch.cmp(&b.batch))
            .then(a.arrival.cmp(&b.arrival))
    }
}

fn plan(choice: String, mut group: Vec<MemberCandidate>, by_date: bool) -> Plan {
    group.sort_by(order(by_date));
    let member = |candidate: &MemberCandidate, omitted| CustomMember {
        computation: candidate.computation,
        label: candidate.label.clone(),
        omitted,
    };
    let sorted: Vec<(Membership, &MemberCandidate)> = group
        .iter()
        .map(|candidate| {
            (
                membership(candidate.state, candidate.file_rejected),
                candidate,
            )
        })
        .collect();
    let included: Vec<CustomMember> = sorted
        .iter()
        .filter(|(kind, _)| *kind == Membership::Included)
        .map(|(_, candidate)| member(candidate, None))
        .collect();
    let omitted = sorted.iter().filter_map(|(kind, candidate)| match kind {
        Membership::Omitted(reason) => Some(member(candidate, Some(*reason))),
        Membership::Included | Membership::Excluded => None,
    });
    let count = included.len();
    Plan {
        choice,
        members: included.into_iter().chain(omitted).collect(),
        included: count,
    }
}

fn plans(candidates: Vec<MemberCandidate>, by_date: bool) -> Vec<Plan> {
    let mut groups: BTreeMap<String, Vec<MemberCandidate>> = BTreeMap::new();
    for candidate in candidates {
        groups
            .entry(candidate.choice_key.clone())
            .or_default()
            .push(candidate);
    }
    groups
        .into_iter()
        .map(|(choice, group)| plan(choice, group, by_date))
        .collect()
}

async fn candidates(
    store: &dyn Store,
    scope: AccountScope,
    series: Vec<BatchId>,
) -> Result<Vec<MemberCandidate>, HistoryError> {
    Ok(store
        .write(move |tx| tx.history(scope).candidates(&series))
        .await?)
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn analysis_view(plan: &Plan, known: &BTreeMap<String, AnalysisView>) -> CustomAnalysisView {
    let analysis = known
        .get(&plan.choice)
        .cloned()
        .unwrap_or_else(|| AnalysisView {
            key: plan.choice.clone(),
            model_id: plan.choice.split(':').next().unwrap_or_default().to_owned(),
            names: BTreeMap::new(),
        });
    CustomAnalysisView {
        analysis,
        included: count(plan.included),
        omitted: plan
            .members
            .iter()
            .filter_map(|member| {
                Some(OmittedView {
                    label: member.label.clone(),
                    reason: member.omitted?.code().to_owned(),
                })
            })
            .collect(),
        parts: count(plan.included.div_ceil(MAX_PART_MEMBERS)),
    }
}

impl History {
    pub async fn custom_report_draft(
        &self,
        series: Vec<BatchId>,
    ) -> Result<CustomReportDraft, HistoryError> {
        let (store, scope) = (self.store.as_ref(), self.scope);
        let mut known = BTreeMap::new();
        for batch in &series {
            if let Some(view) = self.queries.get_batch(scope, *batch).await? {
                known.extend(view.analyses.into_iter().map(|a| (a.key.clone(), a)));
            }
        }
        let found = candidates(store, scope, series).await?;
        let images: BTreeSet<(BatchId, i64)> = found.iter().map(|c| (c.batch, c.arrival)).collect();
        let plans = plans(found, false);
        let draft = CustomReportDraft {
            images: count(images.len()),
            too_many: plans.iter().any(|plan| plan.included > CUSTOM_REPORT_LIMIT),
            analyses: plans
                .iter()
                .map(|plan| analysis_view(plan, &known))
                .collect(),
            language: Language::En,
            folder: String::new(),
        };
        Ok(draft)
    }

    pub async fn create_custom_reports(
        &self,
        request: CustomRequest,
    ) -> Result<Vec<ReportId>, HistoryError> {
        let (store, scope) = (self.store.as_ref(), self.scope);
        let name = request.name.trim().to_owned();
        if name.is_empty() {
            return Err(HistoryError::NameRequired);
        }
        let plans = plans(
            candidates(store, scope, request.series.clone()).await?,
            request.by_date,
        );
        if plans.iter().any(|plan| plan.included > CUSTOM_REPORT_LIMIT) {
            return Err(HistoryError::TooMany);
        }
        if plans.iter().all(|plan| plan.included == 0) {
            return Err(HistoryError::NoImages);
        }
        Ok(store
            .write(move |tx| {
                let mut created = Vec::new();
                for plan in plans.into_iter().filter(|plan| plan.included > 0) {
                    let report = NewCustomReport {
                        name: name.clone(),
                        choice_key: plan.choice,
                        spec: request.spec.clone(),
                        folder: request.folder.clone(),
                        created_at: request.now,
                        app_version: request.app_version.clone(),
                        members: plan.members,
                    };
                    created.push(tx.history(scope).create_custom(&report)?);
                }
                let stored = tx.settings(SettingsScope::Global)?;
                let mut settings = GlobalSettings::parse(stored.as_deref());
                settings.custom_report_folder = Some(request.folder.to_string());
                write_global(tx, &settings)?;
                Ok(created)
            })
            .await?)
    }
}
