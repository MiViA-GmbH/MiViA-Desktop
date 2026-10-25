pub const MIN_LEN: usize = 32;
pub const MAX_LEN: usize = 256;

const BEARER: &str = "bearer ";
const QUOTES: &[char] = &[
    '"', '\'', '\u{201C}', '\u{201D}', '\u{201E}', '\u{2018}', '\u{2019}',
];

const fn invisible(c: char) -> bool {
    matches!(
        c,
        '\r' | '\n' | '\t' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
    )
}

const fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn strip_bearer(text: &str) -> &str {
    match text.get(..BEARER.len()) {
        Some(prefix) if prefix.eq_ignore_ascii_case(BEARER) => {
            text.get(BEARER.len()..).unwrap_or(text)
        }
        _ => text,
    }
}

pub fn normalise(raw: &str) -> Option<String> {
    let visible: String = raw.chars().filter(|c| !invisible(*c)).collect();
    let unquoted = visible.trim().trim_matches(QUOTES).trim();
    let key = strip_bearer(unquoted);
    let valid = (MIN_LEN..=MAX_LEN).contains(&key.len()) && key.chars().all(allowed);
    valid.then(|| key.to_owned())
}

pub const REQUIRED_SCOPES: [&str; 5] = [
    "images:write",
    "images:delete",
    "jobs:read",
    "jobs:write",
    "jobs:delete",
];

pub fn missing_scopes(granted: &[String]) -> Vec<&'static str> {
    let has_resource_scopes = granted.iter().any(|scope| scope.contains(':'));
    if !has_resource_scopes {
        return Vec::new();
    }
    REQUIRED_SCOPES
        .into_iter()
        .filter(|scope| !granted.iter().any(|given| given == scope))
        .collect()
}
