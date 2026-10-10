use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::Ctx;
use crate::structured::strings;

const STRICT_ON: &[&str] = &[
    "strict",
    "noUncheckedIndexedAccess",
    "exactOptionalPropertyTypes",
    "noImplicitOverride",
    "noImplicitReturns",
    "noFallthroughCasesInSwitch",
    "noUnusedLocals",
    "noUnusedParameters",
    "verbatimModuleSyntax",
    "isolatedModules",
    "erasableSyntaxOnly",
    "forceConsistentCasingInFileNames",
];
const STRICT_OFF: &[&str] = &["allowUnreachableCode", "allowUnusedLabels"];
const MODES: &[&str] = &["report-only", "trend", "two-night", "block"];

pub(crate) fn frozen(ctx: &Ctx<'_>) -> Vec<String> {
    let changed = ctx.base_text.map(normalized) != ctx.head_text.map(normalized);
    changed
        .then(|| "any change needs the human".to_owned())
        .into_iter()
        .collect()
}

fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub(crate) fn member_lints(ctx: &Ctx<'_>) -> Vec<String> {
    let lints = ctx.head.get("lints");
    let workspace_only = lints.is_none_or(|lints| *lints == json!({ "workspace": true }));
    (!workspace_only)
        .then(|| "[lints] other than workspace = true".to_owned())
        .into_iter()
        .collect()
}

pub(crate) fn aliases(ctx: &Ctx<'_>) -> Vec<String> {
    let Some(base) = ctx.base.pointer("/alias").and_then(Value::as_object) else {
        return Vec::new();
    };
    base.iter()
        .filter(|(name, old)| !only_added_denies(old, ctx.head.pointer(&format!("/alias/{name}"))))
        .map(|(name, _)| format!("alias {name} changed other than by added -D flags"))
        .collect()
}

fn only_added_denies(old: &Value, new: Option<&Value>) -> bool {
    let old_tokens = tokens(old);
    let new_tokens = new.map(tokens).unwrap_or_default();
    let old_denies = denies(&old_tokens);
    let kept = old_denies
        .iter()
        .all(|deny| denies(&new_tokens).contains(deny));
    kept && without_denies(&old_tokens) == without_denies(&new_tokens)
}

fn tokens(value: &Value) -> Vec<String> {
    value
        .as_str()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn denies(tokens: &[String]) -> Vec<String> {
    tokens
        .windows(2)
        .filter_map(|pair| match pair {
            [flag, lint] if flag == "-D" => Some(lint.clone()),
            _ => None,
        })
        .collect()
}

fn without_denies(tokens: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut skip_next = false;
    for token in tokens {
        if skip_next {
            skip_next = false;
        } else if token == "-D" {
            skip_next = true;
        } else {
            kept.push(token.clone());
        }
    }
    kept
}

pub(crate) fn biome_includes(ctx: &Ctx<'_>) -> Vec<String> {
    strings(&ctx.head, "/files/includes")
        .into_iter()
        .filter_map(|entry| entry.strip_prefix('!').map(str::to_owned))
        .filter(|negation| !ctx.generated.contains(negation))
        .map(|negation| {
            format!("files.includes negation {negation} is not in quality/generated.toml")
        })
        .collect()
}

pub(crate) fn ts_strict(ctx: &Ctx<'_>) -> Vec<String> {
    let flag =
        |value: &Value, name: &str| value.pointer(&format!("/compilerOptions/{name}")).cloned();
    let loosened_on = STRICT_ON
        .iter()
        .filter(|name| flag(&ctx.base, name) == Some(Value::Bool(true)))
        .filter(|name| flag(&ctx.head, name) != Some(Value::Bool(true)));
    let loosened_off = STRICT_OFF
        .iter()
        .filter(|name| flag(&ctx.base, name) == Some(Value::Bool(false)))
        .filter(|name| flag(&ctx.head, name) != Some(Value::Bool(false)));
    loosened_on
        .chain(loosened_off)
        .map(|name| format!("compilerOptions.{name} loosened"))
        .collect()
}

pub(crate) fn budgets(ctx: &Ctx<'_>) -> Vec<String> {
    let Some(rows) = ctx.base.as_object() else {
        return Vec::new();
    };
    rows.iter()
        .flat_map(|(id, old)| budget_row(id, old, ctx.head.get(id)))
        .collect()
}

fn budget_row(id: &str, old: &Value, new: Option<&Value>) -> Vec<String> {
    let Some(new) = new else {
        return vec![format!("budget {id} removed")];
    };
    let mut found = Vec::new();
    let max = old.get("direction").and_then(Value::as_str) != Some("min");
    let (before, after) = (number_of(old, "budget"), number_of(new, "budget"));
    let looser = if max { after > before } else { after < before };
    if looser {
        found.push(format!("budget {id} loosened from {before} to {after}"));
    }
    if mode_rank(new) < mode_rank(old) {
        found.push(format!("budget {id} mode moved toward report-only"));
    }
    found
}

fn number_of(row: &Value, key: &str) -> f64 {
    row.get(key).and_then(Value::as_f64).unwrap_or_default()
}

fn mode_rank(row: &Value) -> Option<usize> {
    let mode = row.get("mode").and_then(Value::as_str)?;
    MODES.iter().position(|known| *known == mode)
}

pub(crate) fn caps(ctx: &Ctx<'_>, cap: usize) -> Vec<String> {
    let entries = ctx
        .head
        .get("entry")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let today = today();
    let mut found = Vec::new();
    if entries.len() > cap {
        found.push(format!("{} entries, cap {cap}", entries.len()));
    }
    for entry in &entries {
        let expires = entry
            .get("expires")
            .and_then(Value::as_str)
            .and_then(days_from_date);
        if expires.is_none_or(|day| day > today + 14) {
            found.push(format!(
                "entry {entry} expires more than 14 days ahead or has no valid expires"
            ));
        }
    }
    found
}

pub(crate) fn nextest(ctx: &Ctx<'_>) -> Vec<String> {
    super::flatten::flatten(&ctx.head)
        .into_iter()
        .filter(|(key, value)| {
            key.last().is_some_and(|last| last == "retries") && value.as_u64() != Some(0)
        })
        .map(|(key, _)| format!("{} must be 0", key.join(".")))
        .collect()
}

pub(crate) fn playwright(ctx: &Ctx<'_>) -> Vec<String> {
    let text = ctx.head_text.unwrap_or_default();
    let literal = text.split("retries:").nth(1).map(|rest| {
        rest.trim_start()
            .split(|c: char| !c.is_ascii_digit())
            .next()
    });
    match literal {
        None => Vec::new(),
        Some(Some(digits)) if digits.parse::<u32>().is_ok_and(|retries| retries <= 1) => Vec::new(),
        Some(_) => vec!["retries must be a literal 0 or 1".to_owned()],
    }
}

pub(crate) fn today() -> i64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    i64::try_from(seconds / 86_400).unwrap_or_default()
}

pub(crate) fn days_from_date(date: &str) -> Option<i64> {
    let mut parts = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let year_of_era = shifted - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}
