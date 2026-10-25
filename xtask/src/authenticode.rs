use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};

const UNBUNDLED: &[u8] = b"__TAURI_BUNDLE_TYPE_VAR_UNK";
const NSIS: &[u8] = b"__TAURI_BUNDLE_TYPE_VAR_NSS";

pub(crate) fn run(rest: &[String]) -> Result<bool> {
    match rest {
        [command, files @ ..] if command == "verify" && !files.is_empty() => verify(files),
        [command, installed, compiled] if command == "same-app" => {
            same_app(Path::new(installed), Path::new(compiled))
        }
        _ => bail!(
            "usage: cargo xtask authenticode (verify <file>... | same-app <installed exe> <compiled exe>)"
        ),
    }
}

#[cfg(windows)]
fn verify(files: &[String]) -> Result<bool> {
    let pin = mivia_os::SignerPin {
        organization: mivia_update_sig::SIGNER_ORGANIZATION,
        common_name: mivia_update_sig::SIGNER_COMMON_NAME,
        intermediate_key: mivia_update_sig::SIGNER_INTERMEDIATE_KEY,
    };
    let mut accepted = true;
    for name in files {
        let path = Path::new(name);
        let file = std::fs::File::open(path).with_context(|| format!("opening {name}"))?;
        match mivia_os::signer_facts(&file, path) {
            Err(error) => {
                eprintln!("authenticode: {name} refused: {error:?}");
                accepted = false;
            }
            Ok(facts) => match pin.accepts(&facts) {
                Ok(()) => println!(
                    "authenticode: {name} accepted, signed by {} with intermediate key {}",
                    facts.common_name, facts.intermediate_key
                ),
                Err(error) => {
                    eprintln!("authenticode: {name} refused: {error:?} {facts:?}");
                    accepted = false;
                }
            },
        }
    }
    Ok(accepted)
}

#[cfg(not(windows))]
fn verify(_: &[String]) -> Result<bool> {
    bail!("authenticode verify needs Windows")
}

fn same_app(installed: &Path, compiled: &Path) -> Result<bool> {
    let read =
        |path: &Path| std::fs::read(path).with_context(|| format!("reading {}", path.display()));
    let left = image_hash(&read(installed)?)?;
    let right = image_hash(&bundled(read(compiled)?)?)?;
    if left == right {
        println!(
            "authenticode: {} holds the image of {} ({})",
            installed.display(),
            compiled.display(),
            hex(&left)
        );
        return Ok(true);
    }
    eprintln!(
        "authenticode: {} ({}) differs from {} ({})",
        installed.display(),
        hex(&left),
        compiled.display(),
        hex(&right)
    );
    Ok(false)
}

fn bundled(mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    let mut found = bytes
        .windows(UNBUNDLED.len())
        .enumerate()
        .filter(|(_, window)| *window == UNBUNDLED)
        .map(|(at, _)| at);
    let (Some(at), None) = (found.next(), found.next()) else {
        bail!("the compiled exe must hold the bundle type marker exactly once");
    };
    bytes
        .get_mut(at..at + NSIS.len())
        .context("bundle type marker out of range")?
        .copy_from_slice(NSIS);
    Ok(bytes)
}

fn image_hash(bytes: &[u8]) -> Result<[u8; 32]> {
    let pe = usize::try_from(u32_at(bytes, 0x3c)?)?;
    ensure!(
        bytes.get(..2) == Some(b"MZ".as_slice())
            && bytes.get(pe..pe + 4) == Some(b"PE\0\0".as_slice()),
        "not a PE file"
    );
    let optional = pe + 24;
    let (count, directories) = match u16_at(bytes, optional)? {
        0x10b => (optional + 92, optional + 96),
        0x20b => (optional + 108, optional + 112),
        magic => bail!("unknown optional header magic {magic:#x}"),
    };
    ensure!(
        u32_at(bytes, count)? > 4,
        "the PE file has no security directory"
    );
    let checksum = optional + 64;
    let security = directories + 4 * 8;
    let table = usize::try_from(u32_at(bytes, security)?)?;
    let size = usize::try_from(u32_at(bytes, security + 4)?)?;
    let end = image_end(bytes.len(), table, size)?;
    let mut hasher = Sha256::new();
    for range in [0..checksum, checksum + 4..security, security + 8..end] {
        hasher.update(bytes.get(range).context("truncated PE file")?);
    }
    Ok(hasher.finalize().into())
}

fn image_end(len: usize, table: usize, size: usize) -> Result<usize> {
    if size == 0 {
        return Ok(len);
    }
    ensure!(
        table.checked_add(size) == Some(len),
        "the certificate table is not at the end of the file"
    );
    Ok(table)
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let field = bytes.get(at..at + 4).context("truncated PE file")?;
    Ok(u32::from_le_bytes(field.try_into()?))
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    let field = bytes.get(at..at + 2).context("truncated PE file")?;
    Ok(u16::from_le_bytes(field.try_into()?))
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .try_fold(String::new(), |mut text, byte| {
            write!(text, "{byte:02x}").map(|()| text)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{NSIS, UNBUNDLED, bundled, image_hash};

    const OPTIONAL: usize = 0x40 + 24;
    const SECURITY: usize = OPTIONAL + 112 + 32;

    fn put(bytes: &mut [u8], at: usize, value: &[u8]) {
        let (_, tail) = bytes.split_at_mut(at);
        let (field, _) = tail.split_at_mut(value.len());
        field.copy_from_slice(value);
    }

    fn unsigned() -> Vec<u8> {
        let mut bytes = vec![0_u8; 0x400];
        put(&mut bytes, 0, b"MZ");
        put(&mut bytes, 0x3c, &0x40_u32.to_le_bytes());
        put(&mut bytes, 0x40, b"PE\0\0");
        put(&mut bytes, OPTIONAL, &0x20b_u16.to_le_bytes());
        put(&mut bytes, OPTIONAL + 108, &16_u32.to_le_bytes());
        put(&mut bytes, 0x300, UNBUNDLED);
        bytes
    }

    fn signed(mut bytes: Vec<u8>) -> Vec<u8> {
        let table = u32::try_from(bytes.len()).unwrap();
        put(&mut bytes, OPTIONAL + 64, &0x1234_u32.to_le_bytes());
        put(&mut bytes, SECURITY, &table.to_le_bytes());
        put(&mut bytes, SECURITY + 4, &16_u32.to_le_bytes());
        bytes.extend_from_slice(&[7; 16]);
        bytes
    }

    #[test]
    fn the_signature_does_not_change_the_image_hash() {
        let compiled = unsigned();
        let installed = signed(bundled(compiled.clone()).unwrap());
        assert_eq!(
            image_hash(&installed).unwrap(),
            image_hash(&bundled(compiled).unwrap()).unwrap()
        );
    }

    #[test]
    fn any_other_change_does() {
        let compiled = bundled(unsigned()).unwrap();
        let mut installed = signed(compiled.clone());
        put(&mut installed, 0x200, b"x");
        assert_ne!(
            image_hash(&installed).unwrap(),
            image_hash(&compiled).unwrap()
        );
        let unpatched = signed(unsigned());
        assert_ne!(
            image_hash(&unpatched).unwrap(),
            image_hash(&compiled).unwrap()
        );
    }

    #[test]
    fn data_after_the_certificate_table_is_refused() {
        let mut installed = signed(unsigned());
        installed.push(0);
        assert!(image_hash(&installed).is_err());
        assert!(image_hash(b"not a PE file").is_err());
    }

    #[test]
    fn the_bundle_marker_must_appear_exactly_once() {
        let patched = bundled(unsigned()).unwrap();
        assert_eq!(patched.get(0x300..0x300 + NSIS.len()), Some(NSIS));
        assert!(bundled(patched).is_err());
        let mut twice = unsigned();
        put(&mut twice, 0x380, UNBUNDLED);
        assert!(bundled(twice).is_err());
    }
}
