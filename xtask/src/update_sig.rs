use std::path::Path;

use anyhow::{Context, Result, bail};

pub(crate) fn run(rest: &[String]) -> Result<bool> {
    let [command, asset, signature, version] = rest else {
        bail!("usage: cargo xtask update-sig verify <asset> <sig> <version>");
    };
    if command != "verify" {
        bail!("unknown update-sig command {command}");
    }
    let data = std::fs::read(Path::new(asset)).with_context(|| format!("reading {asset}"))?;
    let signature = std::fs::read_to_string(Path::new(signature))
        .with_context(|| format!("reading {signature}"))?;
    let version = version.strip_prefix("beta-").unwrap_or(version);
    match mivia_update_sig::verify(&data, &signature, version) {
        Ok(slot) => {
            println!("update-sig: {asset} is signed for {version} with key {slot:?}");
            Ok(true)
        }
        Err(refusal) => {
            eprintln!("update-sig: {asset} refused for {version}: {refusal:?}");
            Ok(false)
        }
    }
}
