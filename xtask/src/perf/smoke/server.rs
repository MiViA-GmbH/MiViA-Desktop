use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};

use super::super::evidence::{job_ids, missing_ops};
use super::{Checks, Cleanup, POLICY, POLL, Smoke, infra, kill, powershell, tool, wait};

const KEY: &str = "stub-access-key-0123456789-abcdefghijkl";
const MODEL: &str = "00000009-0000-4000-8000-000000000001";
const SENT: [&str; 2] = ["probe_002.jpg", "probe_001.jpg"];
const REPORT_SUFFIX: &str = " probe_001 +1 - Grain size.pdf";
const RESULT: &str = r#"{"job_id":"ID","results":[{"type":"data-table","data":[{"label":"GRAIN_MEAN","unit":"um","value":12.5}]},{"type":"image","label":"GRAIN_OVERLAY","filename":"overlay.png"},{"type":"message","label":""}]}"#;
const PNG: [u8; 69] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x48, 0x48, 0x48, 0x00,
    0x00, 0x02, 0x44, 0x01, 0x21, 0xe1, 0x19, 0x42, 0x34, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

impl Smoke {
    pub(super) fn serve(&mut self, cleanup: &mut Cleanup) -> Result<()> {
        let ca = self.root.join("crates/testing/testkit/tls/ca.der");
        tool(
            "certutil",
            &["-addstore", "-f", "Root", &ca.to_string_lossy()],
        )?;
        let mut stub = Command::new(&self.stub)
            .env("MIVIA_STUB_TLS", "1")
            .env("MIVIA_STUB_KEY", KEY)
            .stdout(Stdio::piped())
            .spawn()
            .map_err(infra)?;
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(stub.stdout.take().context("stub output")?),
            &mut line,
        )?;
        cleanup.stub = Some(stub);
        line.trim().clone_into(&mut self.origin);
        if !self.origin.starts_with("https://") {
            return Err(infra(format!("the stub did not start over https: {line}")));
        }
        tool(
            "reg",
            &[
                "add",
                POLICY,
                "/v",
                "ServerUrl",
                "/t",
                "REG_SZ",
                "/d",
                &self.origin,
                "/f",
            ],
        )?;
        Ok(())
    }

    pub(super) fn seed(&self) -> Result<()> {
        let output = Command::new("cargo")
            .args([
                "test",
                "--profile",
                "e2e",
                "--features",
                "custom-protocol",
                "-p",
                "mivia-desktop",
                "--test",
                "smoke_seed",
                "--locked",
                "--",
                "--ignored",
            ])
            .env("MIVIA_SMOKE_ORIGIN", &self.origin)
            .env("MIVIA_SMOKE_KEY", KEY)
            .env("MIVIA_SMOKE_MODEL", MODEL)
            .current_dir(&self.root)
            .output()
            .map_err(infra)?;
        let log = [output.stdout, output.stderr].concat();
        std::fs::write(self.out.join("seed.log"), &log)?;
        if !output.status.success() {
            return Err(infra(format!(
                "seeding the profile failed, see {}",
                self.out.join("seed.log").display()
            )));
        }
        Ok(())
    }

    pub(super) fn send_to(&self, cleanup: &mut Cleanup, checks: &mut Checks) -> Result<()> {
        std::fs::create_dir_all(&self.sent)?;
        let paths: Vec<PathBuf> = SENT.iter().map(|name| self.sent.join(name)).collect();
        for (seed, path) in (20_u8..).zip(&paths) {
            std::fs::write(
                path,
                [0xff, 0xd8, 0xff, 0xe0, seed, 0x00, 0x10, seed, 0xff, 0xd9],
            )?;
        }
        let exe = self.install.join("mivia-desktop.exe");
        let primary = Command::new(&exe)
            .arg("--minimized")
            .spawn()
            .map_err(infra)?;
        let pid = primary.id();
        cleanup.app = Some(primary);
        let mut second = Command::new(&exe)
            .args(["--open-paths", "--"])
            .args(&paths)
            .spawn()
            .map_err(infra)?;
        let status = wait(&mut second, Duration::from_secs(60))?;
        if status.is_none() {
            kill(&mut second);
        }
        checks.check(
            "Send to hands the images to the running app",
            status.is_some_and(|status| status.success()),
            format!("{status:?}"),
        );
        self.analyze(pid, checks)
    }

    fn analyze(&self, pid: u32, checks: &mut Checks) -> Result<()> {
        let (clicked, text) = powershell(
            &self.out,
            "click.ps1",
            include_str!("click.ps1"),
            &[
                "-ProcessId",
                &pid.to_string(),
                "-Name",
                "Analyze 2 images",
                "-Seconds",
                "120",
            ],
        )?;
        checks.check("the one-off dialog offers Analyze 2 images", clicked, text);
        if clicked {
            Ok(())
        } else {
            Err(anyhow!("the analysis was not started"))
        }
    }

    fn curl(&self, args: &[&str]) -> Result<String> {
        let mut all = vec!["-sS", "--fail", "--ssl-no-revoke", "--max-time", "20"];
        all.extend_from_slice(args);
        tool(&self.curl, &all)
    }

    fn publish(&self, id: &str) -> Result<()> {
        let body = self.out.join(format!("result-{id}.json"));
        std::fs::write(&body, RESULT.replace("ID", id))?;
        let image = self.out.join("overlay.png");
        std::fs::write(&image, PNG)?;
        self.curl(&[
            "-H",
            "content-type: application/json",
            "--data-binary",
            &format!("@{}", body.display()),
            &format!("{}/__stub/result", self.origin),
        ])?;
        self.curl(&[
            "--data-binary",
            &format!("@{}", image.display()),
            &format!(
                "{}/__stub/result-image?job_id={id}&name=overlay.png",
                self.origin
            ),
        ])?;
        Ok(())
    }

    pub(super) fn report(&self, checks: &mut Checks) -> Result<()> {
        let reports = self.sent.join("MiViA reports");
        let listing = format!("{}/api/v2/jobs?modelId={MODEL}&page=1", self.origin);
        let header = format!("authorization: {KEY}");
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut published = Vec::new();
        let found = loop {
            for id in job_ids(&self.curl(&["-H", &header, &listing])?) {
                if !published.contains(&id) {
                    self.publish(&id)?;
                    published.push(id);
                }
            }
            let pdfs = pdfs(&reports);
            if !pdfs.is_empty() || Instant::now() > deadline {
                break pdfs;
            }
            std::thread::sleep(POLL);
        };
        checks.check(
            "the report is saved beside the sent images",
            found.len() == 1 && found.iter().all(|name| name.ends_with(REPORT_SUFFIX)),
            format!(
                "{found:?} in {}, {} jobs",
                reports.display(),
                published.len()
            ),
        );
        let journal = self.curl(&[&format!("{}/__stub/journal", self.origin)])?;
        std::fs::write(self.out.join("journal.json"), &journal)?;
        let missing = missing_ops(&journal, &["Upload", "Submit", "Status"]);
        checks.check(
            "the stub journal shows upload, submit and status over https from the policy origin",
            missing.is_empty(),
            format!("missing {missing:?}"),
        );
        Ok(())
    }
}

fn pdfs(folder: &Path) -> Vec<String> {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        })
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}
