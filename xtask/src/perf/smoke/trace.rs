use std::path::Path;

use anyhow::{Context, Result};

use super::super::evidence::{CANARIES, I386, absent, canary_hits, decode};
use super::{Checks, Smoke, TRACE, infra, quiet, tool, windows_dir};

const KERNEL_PROCESS: &str = "{22FB2CD6-0E7B-422B-A0C7-2FAD1FD0E716}";
const IMAGE_KEYWORD: &str = "0x40";

impl Smoke {
    pub(super) fn start_trace(&self) -> Result<()> {
        let etl = self.out.join("images.etl");
        tool(
            "logman",
            &[
                "create",
                "trace",
                TRACE,
                "-ets",
                "-p",
                KERNEL_PROCESS,
                IMAGE_KEYWORD,
                "-o",
                &etl.to_string_lossy(),
            ],
        )
        .map(drop)
        .map_err(|error| {
            infra(format!(
                "image-load tracing needs an elevated shell: {error:#}"
            ))
        })
    }

    pub(super) fn trace(&self, checks: &mut Checks) -> Result<()> {
        quiet("logman", &["stop", TRACE, "-ets"]);
        let etl = self.out.join("images.etl");
        if !etl.is_file() {
            checks.check("image-load trace", false, "no trace was recorded");
            return Ok(());
        }
        let xml = self.out.join("images.xml");
        tool(
            "tracerpt",
            &[
                &etl.to_string_lossy(),
                "-o",
                &xml.to_string_lossy(),
                "-of",
                "XML",
                "-y",
            ],
        )?;
        let trace = decode(&std::fs::read(&xml)?);
        let setup = self.setup.file_name().unwrap_or_default().to_string_lossy();
        let unseen = absent(&trace, &[&setup, "mivia-desktop.exe", "un_a.exe"]);
        checks.check(
            "the image-load trace covers install, first start and uninstall",
            unseen.is_empty(),
            format!("not seen {unseen:?}"),
        );
        let hits = canary_hits(&trace, &["downloads", "install"]);
        checks.check(
            "no planted DLL was loaded",
            hits.is_empty(),
            format!("{} canaries in 2 folders, loaded {hits:?}", CANARIES.len()),
        );
        Ok(())
    }
}

pub(super) fn plant(folder: &Path, machine: u16) -> Result<()> {
    let system = if machine == I386 {
        "SysWOW64"
    } else {
        "System32"
    };
    let source = windows_dir().join(system).join("version.dll");
    for name in CANARIES {
        std::fs::copy(&source, folder.join(format!("{name}.dll")))
            .with_context(|| format!("planting {name}.dll in {}", folder.display()))?;
    }
    Ok(())
}
