use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

const MARKER: &str = "plugins-signed";

#[derive(Debug, PartialEq, Eq)]
enum Plan {
    Sign(Vec<PathBuf>),
    Skip,
}

pub(super) fn run(file: &Path) -> Result<bool> {
    let env = |name: &str| std::env::var(name).with_context(|| format!("{name} is not set"));
    let signtool = env("RELEASE_SIGNTOOL")?;
    let thumbprint = env("RELEASE_THUMBPRINT")?;
    let timestamp = env("RELEASE_TIMESTAMP")?;
    let files = match plan(file)? {
        Plan::Skip => {
            println!(
                "authenticode: {} was signed with the plugins",
                file.display()
            );
            return Ok(true);
        }
        Plan::Sign(files) => files,
    };
    let status = Command::new(signtool)
        .args(["sign", "/fd", "sha256", "/sha1", &thumbprint])
        .args(["/tr", &timestamp, "/td", "sha256", "/d", "MiViA Desktop"])
        .args(&files)
        .status()
        .context("starting signtool")?;
    if !status.success() {
        eprintln!("authenticode: signtool failed ({status}), the release stops here");
        return Ok(false);
    }
    if let Some(marker) = plugin_marker(file) {
        std::fs::write(&marker, "").with_context(|| format!("writing {}", marker.display()))?;
    }
    Ok(true)
}

fn plugin_root(file: &Path) -> Option<&Path> {
    file.ancestors().find(|dir| {
        dir.file_name().is_some_and(|name| name == "x86-unicode")
            && dir
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "Plugins")
    })
}

fn plugin_marker(file: &Path) -> Option<PathBuf> {
    Some(plugin_root(file)?.parent()?.parent()?.join(MARKER))
}

fn plan(file: &Path) -> Result<Plan> {
    let (Some(root), Some(marker)) = (plugin_root(file), plugin_marker(file)) else {
        return Ok(Plan::Sign(vec![file.to_path_buf()]));
    };
    if marker.exists() {
        return Ok(Plan::Skip);
    }
    let mut dlls = Vec::new();
    collect_dlls(root, &mut dlls)?;
    dlls.sort();
    Ok(Plan::Sign(dlls))
}

fn collect_dlls(dir: &Path, into: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_dlls(&path, into)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        {
            into.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{MARKER, Plan, plan};

    fn layout(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-sign-{name}-{}", std::process::id()));
        let root = dir.join("x64").join("Plugins").join("x86-unicode");
        std::fs::create_dir_all(root.join("additional")).unwrap();
        for file in [
            "System.dll",
            "nsDialogs.dll",
            "additional/nsis_tauri_utils.dll",
        ] {
            std::fs::write(root.join(file), "").unwrap();
        }
        std::fs::write(root.join("readme.txt"), "").unwrap();
        dir
    }

    #[test]
    fn other_files_are_signed_alone() {
        let file = PathBuf::from(r"C:\build\target\release\mivia-desktop.exe");
        assert_eq!(plan(&file).unwrap(), Plan::Sign(vec![file]));
    }

    #[test]
    fn the_first_plugin_signs_every_plugin_and_the_rest_are_skipped() {
        let dir = layout("plugins");
        let root = dir.join("x64").join("Plugins").join("x86-unicode");
        let mut all = vec![
            root.join("System.dll"),
            root.join("nsDialogs.dll"),
            root.join("additional").join("nsis_tauri_utils.dll"),
        ];
        all.sort();
        assert_eq!(plan(&root.join("System.dll")).unwrap(), Plan::Sign(all));
        std::fs::write(dir.join("x64").join(MARKER), "").unwrap();
        assert_eq!(plan(&root.join("nsDialogs.dll")).unwrap(), Plan::Skip);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
