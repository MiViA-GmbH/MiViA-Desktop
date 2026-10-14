#[cfg(test)]
mod tests;

pub(crate) const CANARIES: [&str; 12] = [
    "version",
    "uxtheme",
    "dwmapi",
    "cryptbase",
    "cryptsp",
    "userenv",
    "profapi",
    "propsys",
    "textshaping",
    "winmm",
    "netapi32",
    "wininet",
];

pub(crate) const I386: u16 = 0x14c;
pub(crate) const AMD64: u16 = 0x8664;

pub(crate) fn pe_machine(bytes: &[u8]) -> Option<u16> {
    let at = |offset: usize, len: usize| bytes.get(offset..offset + len);
    if at(0, 2)? != b"MZ" {
        return None;
    }
    let header = usize::try_from(u32::from_le_bytes(at(0x3c, 4)?.try_into().ok()?)).ok()?;
    if at(header, 4)? != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(at(header + 4, 2)?.try_into().ok()?))
}

pub(crate) fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xff, 0xfe, rest @ ..] => {
            let units: Vec<u16> = rest
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect();
            String::from_utf16_lossy(&units)
        }
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

pub(crate) fn canary_hits(trace: &str, folders: &[&str]) -> Vec<String> {
    let trace = trace.to_ascii_lowercase();
    folders
        .iter()
        .flat_map(|folder| {
            CANARIES
                .iter()
                .map(move |name| format!("\\{}\\{name}.dll", folder.to_ascii_lowercase()))
        })
        .filter(|path| {
            trace.match_indices(path.as_str()).any(|(at, _)| {
                trace
                    .get(at + path.len()..)
                    .and_then(|rest| rest.chars().next())
                    .is_none_or(|next| !next.is_ascii_alphanumeric() && next != '.')
            })
        })
        .collect()
}

pub(crate) fn absent<'a>(text: &str, needles: &[&'a str]) -> Vec<&'a str> {
    let text = text.to_ascii_lowercase();
    needles
        .iter()
        .copied()
        .filter(|needle| !text.contains(&needle.to_ascii_lowercase()))
        .collect()
}

pub(crate) fn job_ids(listing: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(listing)
        .ok()
        .and_then(|value| value.get("data")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|job| Some(job.get("id")?.as_str()?.to_owned()))
        .collect()
}

pub(crate) fn missing_ops<'a>(journal: &str, expected: &[&'a str]) -> Vec<&'a str> {
    let ops: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(journal)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| Some(entry.get("op")?.as_str()?.to_owned()))
        .collect();
    expected
        .iter()
        .copied()
        .filter(|op| !ops.iter().any(|seen| seen == op))
        .collect()
}
