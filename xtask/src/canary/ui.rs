use anyhow::Result;
use serde_json::Value;

use super::{Probe, Work};

const FEATURES: &str = "ui/src/features/";
const BIOME_RULES: &[(&str, &str)] = &[
    ("shape.ts", "lint/complexity/noExcessiveLinesPerFunction"),
    ("shape.ts", "lint/complexity/noExcessiveCognitiveComplexity"),
    ("shape.ts", "lint/complexity/useMaxParams"),
    ("shape.ts", "lint/style/noNestedTernary"),
    ("shape.ts", "plugin"),
    ("shape.ts", "lint/suspicious/noConsole"),
    ("shape.ts", "lint/style/noDefaultExport"),
    ("shape.ts", "lint/style/noRestrictedImports"),
    ("shape.ts", "lint/suspicious/noExplicitAny"),
    ("shape.ts", "lint/suspicious/noTsIgnore"),
];

pub(crate) fn run(work: &Work) -> Result<Vec<Probe>> {
    plant(work)?;
    let mut probes = biome(work)?;
    let (passed, output) = work.output(
        "node",
        &[
            "ui/node_modules/typescript/bin/tsc",
            "-p",
            "ui/tsconfig.json",
        ],
    )?;
    let unchecked = !passed && output.contains("alpha/index.ts") && output.contains("TS2322");
    probes.push(Probe::expect("tsc", "noUncheckedIndexedAccess", unchecked));
    probes.extend(fallow(work)?);
    Ok(probes)
}

fn plant(work: &Work) -> Result<()> {
    work.write(
        &format!("{FEATURES}alpha/shape.ts"),
        include_str!("../../canary/shape.ts.txt"),
    )?;
    work.write(
        &format!("{FEATURES}alpha/simple.ts"),
        include_str!("../../canary/simple.ts.txt"),
    )?;
    work.write(
        &format!("{FEATURES}alpha/index.ts"),
        include_str!("../../canary/alpha.ts.txt"),
    )?;
    work.write(
        &format!("{FEATURES}beta/index.ts"),
        include_str!("../../canary/beta.ts.txt"),
    )?;
    work.write(
        &format!("{FEATURES}beta/copy.ts"),
        include_str!("../../canary/copy.ts.txt"),
    )?;
    work.write("ui/src/main.ts", include_str!("../../canary/main.ts.txt"))
}

fn biome(work: &Work) -> Result<Vec<Probe>> {
    let biome = "node_modules/@biomejs/biome/bin/biome";
    let (_, output) = work.output(
        "node",
        &[
            biome,
            "lint",
            "--reporter=github",
            "--max-diagnostics=500",
            FEATURES,
        ],
    )?;
    let lines: Vec<&str> = output
        .lines()
        .filter(|line| line.starts_with("::"))
        .collect();
    let fired = |file: &str, rule: &str| {
        let title = format!("title={rule},");
        lines
            .iter()
            .any(|line| line.contains(&title) && line.contains(file))
    };
    let mut probes: Vec<Probe> = BIOME_RULES
        .iter()
        .map(|(file, rule)| Probe::expect("biome", format!("{rule} ({file})"), fired(file, rule)))
        .collect();
    let simple_clean = !lines.iter().any(|line| line.contains("simple.ts"));
    probes.push(Probe::expect(
        "biome",
        "simple ternary passes",
        simple_clean,
    ));
    Ok(probes)
}

fn fallow(work: &Work) -> Result<Vec<Probe>> {
    let fallow = "node_modules/fallow/bin/fallow";
    let ui = work.path("ui");
    let (passed, output) = work.output_in(
        &ui,
        "node",
        &[fallow, "--format", "json", "--fail-on-issues"],
    )?;
    let json_start = output.find('{').unwrap_or_default();
    let report: Value =
        serde_json::Deserializer::from_str(output.get(json_start..).unwrap_or_default())
            .into_iter()
            .next()
            .and_then(Result::ok)
            .unwrap_or_default();
    let count = |pointer: &str| {
        report
            .pointer(pointer)
            .and_then(Value::as_u64)
            .unwrap_or_default()
    };
    let clones = report
        .pointer("/dupes/clone_groups")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    Ok(vec![
        Probe::expect(
            "fallow",
            "unused-exports",
            !passed && count("/check/summary/unused_exports") > 0,
        ),
        Probe::expect(
            "fallow",
            "boundary-violation",
            !passed && count("/check/summary/boundary_violations") > 0,
        ),
        Probe::expect("fallow", "duplicates", !passed && clones > 0),
    ])
}
