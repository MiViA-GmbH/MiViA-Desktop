use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use super::{Checks, POLL, Smoke, infra, powershell, tool, wait};

const APP_KEY: &str = r"HKCU\Software\MiViA\Desktop";
const SYSTEM_REFUSED: &str = "1625";

impl Smoke {
    pub(super) fn system_install(&self, checks: &mut Checks) -> Result<()> {
        let target = self.out.join("system");
        let (_, output) = powershell(
            &self.out,
            "system.ps1",
            include_str!("system.ps1"),
            &[
                "-Installer",
                &self.setup.to_string_lossy(),
                "-Target",
                &target.to_string_lossy(),
                "-Seconds",
                "180",
            ],
        )?;
        let code = output.lines().last().unwrap_or_default().trim().to_owned();
        checks.check(
            "silent install as SYSTEM is refused",
            code == SYSTEM_REFUSED && !target.join("mivia-desktop.exe").exists(),
            format!("exit {code}"),
        );
        Ok(())
    }

    pub(super) fn install(&self, checks: &mut Checks) -> Result<()> {
        let started = Instant::now();
        let status = Command::new(&self.setup)
            .args(["/S", &format!("/D={}", self.install.display())])
            .status()
            .map_err(infra)?;
        let exe = self.install.join("mivia-desktop.exe");
        checks.check(
            "silent install from the Downloads folder",
            status.success() && exe.is_file(),
            format!("{status} in {:.1} s", started.elapsed().as_secs_f64()),
        );
        if exe.is_file() {
            Ok(())
        } else {
            Err(anyhow!("the installed app is missing at {}", exe.display()))
        }
    }

    pub(super) fn junctions(&self) -> Result<(PathBuf, PathBuf)> {
        let inbox = self.out.join("canary-inbox");
        let logs = self.out.join("canary-logs");
        for (canary, link) in [
            (&inbox, self.data.join("inbox")),
            (&logs, self.data.join("logs").join("escape")),
        ] {
            std::fs::create_dir_all(canary)?;
            std::fs::write(canary.join("keep.txt"), "canary")?;
            drop(std::fs::remove_dir_all(&link));
            tool(
                "cmd",
                &[
                    "/C",
                    "mklink",
                    "/J",
                    &link.to_string_lossy(),
                    &canary.to_string_lossy(),
                ],
            )?;
        }
        Ok((inbox, logs))
    }

    fn credential_saved(&self) -> Result<bool> {
        Ok(tool("cmdkey", &["/list"])?.contains(&format!("MiViA Desktop/v1/{}", self.origin)))
    }

    pub(super) fn update(&self, checks: &mut Checks, canaries: &[&Path]) -> Result<()> {
        let before = listing(&self.data);
        let status = Command::new(&self.setup)
            .args(["/S", "/UPDATE", &format!("/D={}", self.install.display())])
            .status()
            .map_err(infra)?;
        let after = listing(&self.data);
        let lost: Vec<&String> = before.iter().filter(|path| !after.contains(path)).collect();
        checks.check(
            "an update-mode install keeps every app data file",
            status.success() && lost.is_empty() && !before.is_empty(),
            format!("{status}, {} files before, lost {lost:?}", before.len()),
        );
        checks.check(
            "an update-mode install keeps the access key",
            self.credential_saved()?,
            "Credential Manager entry for the stub origin",
        );
        survived(checks, "after the update-mode install", canaries);
        Ok(())
    }

    pub(super) fn uninstall(&self, checks: &mut Checks, canaries: &[&Path]) -> Result<()> {
        let mut uninstaller = Command::new(self.install.join("uninstall.exe"))
            .spawn()
            .map_err(infra)?;
        let (driven, steps) = powershell(
            &self.out,
            "uninstall.ps1",
            include_str!("uninstall.ps1"),
            &["-Seconds", "180"],
        )?;
        wait(&mut uninstaller, Duration::from_secs(10))?;
        checks.check(
            "the uninstaller deletes the app data when asked",
            driven,
            steps,
        );
        self.removed(checks, canaries)
    }

    fn removed(&self, checks: &mut Checks, canaries: &[&Path]) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);
        while self.install.join("mivia-desktop.exe").exists() && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
        survived(checks, "after uninstall with app data deletion", canaries);
        checks.check(
            "uninstall removed the app data folder",
            !self.data.exists(),
            self.data.display(),
        );
        checks.check(
            "uninstall removed the app",
            !self.install.join("mivia-desktop.exe").exists(),
            self.install.display(),
        );
        checks.check(
            "uninstall removed the access key",
            !self.credential_saved()?,
            "Credential Manager entry for the stub origin",
        );
        checks.check(
            "uninstall removed the app settings key",
            tool("reg", &["query", APP_KEY]).is_err(),
            APP_KEY,
        );
        Ok(())
    }
}

fn listing(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            let link =
                std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink());
            if path.is_dir() && !link {
                pending.push(path.clone());
            }
            found.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
            );
        }
    }
    found.sort();
    found
}

fn survived(checks: &mut Checks, when: &str, canaries: &[&Path]) {
    let gone: Vec<String> = canaries
        .iter()
        .map(|canary| canary.join("keep.txt"))
        .filter(|file| !file.is_file())
        .map(|file| file.display().to_string())
        .collect();
    checks.check(
        &format!("junction targets survive {when}"),
        gone.is_empty(),
        format!("deleted {gone:?}"),
    );
}
