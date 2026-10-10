use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub(crate) fn root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask has no parent directory")
}

pub(crate) fn files(root: &Path) -> Result<Vec<String>> {
    let output = git(root, &["ls-files", "-z", "-co", "--exclude-standard"])?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty() && root.join(path).is_file())
        .map(str::to_owned)
        .collect())
}

pub(crate) fn show(root: &Path, rev: &str, path: &str) -> Option<String> {
    git(root, &["show", &format!("{rev}:{path}")]).ok()
}

pub(crate) fn tracked_at(root: &Path, rev: &str) -> Result<Vec<String>> {
    let output = git(root, &["ls-tree", "-r", "-z", "--name-only", rev])?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

pub(crate) fn read(root: &Path, path: &str) -> Result<String> {
    std::fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))
}

pub(crate) fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("running git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git output is not UTF-8")
}

pub(crate) fn extension(path: &str) -> &str {
    path.rsplit_once('.').map_or("", |(_, extension)| extension)
}

pub(crate) fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}
