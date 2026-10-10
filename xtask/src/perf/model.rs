use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::repo;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Budget {
    pub(crate) statistic: String,
    pub(crate) direction: String,
    #[serde(rename = "budget")]
    pub(crate) limit: f64,
    pub(crate) unit: String,
    pub(crate) mode: String,
    pub(crate) basis: String,
}

#[derive(Debug, Deserialize)]
struct Waiver {
    id: String,
    expires: String,
}

#[derive(Debug, Deserialize)]
struct Waivers {
    #[serde(default)]
    entry: Vec<Waiver>,
}

pub(crate) struct Budgets {
    rows: BTreeMap<String, Budget>,
    waived: Vec<(String, String)>,
}

impl Budgets {
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let rows = toml::from_str(&repo::read(root, "perf/budgets.toml")?)
            .context("parsing perf/budgets.toml")?;
        let waivers: Waivers = toml::from_str(&repo::read(root, "quality/perf-waivers.toml")?)
            .context("parsing quality/perf-waivers.toml")?;
        let waived = waivers
            .entry
            .into_iter()
            .map(|waiver| (waiver.id, waiver.expires))
            .collect();
        Ok(Self { rows, waived })
    }

    #[cfg(test)]
    pub(crate) fn from_parts(
        rows: BTreeMap<String, Budget>,
        waived: Vec<(String, String)>,
    ) -> Self {
        Self { rows, waived }
    }

    pub(crate) fn row(&self, id: &str) -> Result<&Budget> {
        self.rows
            .get(id)
            .with_context(|| format!("no budget row {id}"))
    }

    pub(crate) fn metric(&self, id: &str, samples: Vec<f64>) -> Result<Metric> {
        let row = self.row(id)?;
        Ok(Metric {
            id: id.to_owned(),
            value: statistic(&row.statistic, &samples)?,
            n: samples.len(),
            samples,
        })
    }

    fn waived(&self, id: &str, today: &str) -> bool {
        self.waived
            .iter()
            .any(|(waived, expires)| waived == id && expires.as_str() >= today)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Metric {
    pub(crate) id: String,
    pub(crate) samples: Vec<f64>,
    pub(crate) value: f64,
    pub(crate) n: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Run {
    pub(crate) scenario: String,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) validity: String,
    pub(crate) notes: Vec<String>,
    pub(crate) metrics: Vec<Metric>,
}

pub(crate) fn statistic(name: &str, samples: &[f64]) -> Result<f64> {
    if samples.is_empty() {
        bail!("no samples for statistic {name}");
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |index: usize| sorted.get(index).copied().unwrap_or_default();
    let rank = |percent: usize| at((sorted.len() * percent).div_ceil(100).saturating_sub(1));
    Ok(match name {
        "median" if sorted.len() % 2 == 1 => at(sorted.len() / 2),
        "median" => f64::midpoint(at(sorted.len() / 2 - 1), at(sorted.len() / 2)),
        "p90" => rank(90),
        "p95" => rank(95),
        "p99" => rank(99),
        "max" => at(sorted.len() - 1),
        "value" => samples.last().copied().unwrap_or_default(),
        "slope" => slope(samples),
        other => bail!("unknown statistic {other}"),
    })
}

fn slope(samples: &[f64]) -> f64 {
    let mut slopes = Vec::new();
    for (first, a) in samples.iter().enumerate() {
        for (gap, b) in samples.iter().skip(first + 1).enumerate() {
            slopes.push((b - a) / f64::from(u32::try_from(gap + 1).unwrap_or(u32::MAX)));
        }
    }
    slopes.sort_by(f64::total_cmp);
    slopes.get(slopes.len() / 2).copied().unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Green,
    Red,
    FirstNight,
    Report,
    Waived,
}

pub(crate) struct Verdict {
    pub(crate) id: String,
    pub(crate) value: f64,
    pub(crate) budget: f64,
    pub(crate) unit: String,
    pub(crate) status: Status,
}

fn breached(row: &Budget, value: f64) -> bool {
    if row.direction == "min" {
        value < row.limit
    } else {
        value > row.limit
    }
}

pub(crate) fn judge(
    budgets: &Budgets,
    run: &Run,
    previous: Option<&Run>,
    today: &str,
) -> Vec<Verdict> {
    run.metrics
        .iter()
        .filter_map(|metric| {
            let row = budgets.row(&metric.id).ok()?;
            let over = breached(row, metric.value);
            let before = previous
                .and_then(|run| run.metrics.iter().find(|before| before.id == metric.id))
                .is_some_and(|before| breached(row, before.value));
            let waived = budgets.waived(&metric.id, today);
            let status = status_of(row, run.validity == "ok", [over, before, waived]);
            Some(Verdict {
                id: metric.id.clone(),
                value: metric.value,
                budget: row.limit,
                unit: row.unit.clone(),
                status,
            })
        })
        .collect()
}

fn status_of(row: &Budget, valid: bool, [over, before, waived]: [bool; 3]) -> Status {
    if !valid || row.basis == "tbc" {
        return Status::Report;
    }
    if !over {
        return Status::Green;
    }
    if waived {
        return Status::Waived;
    }
    match row.mode.as_str() {
        "block" => Status::Red,
        "two-night" if before => Status::Red,
        "two-night" => Status::FirstNight,
        _ => Status::Report,
    }
}
