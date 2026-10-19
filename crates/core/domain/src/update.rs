use std::cmp::Ordering;

use crate::ids::{HOUR_MS, MINUTE_MS, SECOND_MS, Timestamp};

pub const FIRST_CHECK_MS: i64 = 60 * SECOND_MS;
pub const CHECK_EVERY_MS: i64 = 6 * HOUR_MS;
pub const CHECK_JITTER_MS: i64 = 30 * MINUTE_MS;
pub const CHECK_GAP_MS: i64 = HOUR_MS;
pub const MANUAL_GAP_MS: i64 = MINUTE_MS;
pub const AUTO_INSTALL_AFTER_MS: i64 = 24 * HOUR_MS;
pub const NOTICE_AFTER_MS: i64 = 72 * HOUR_MS;
pub const USER_IDLE_MS: i64 = 10 * MINUTE_MS;
pub const IDLE_RECHECK_MS: i64 = 30 * MINUTE_MS;
pub const MAX_DOWNLOAD_BYTES: u64 = 300 * 1024 * 1024;
pub const DOWNLOAD_TIMEOUT_MS: i64 = 10 * MINUTE_MS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Signature,
    SignedVersion,
    Prerelease,
    UrlPrefix,
    Blocked,
    Authenticode,
    TooLarge,
    Downgrade,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    Number(u64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<Identifier>,
}

impl Version {
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    pub const fn core(&self) -> [u64; 3] {
        self.core
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn number(part: &str) -> Option<u64> {
    let plain = !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    plain.then(|| part.parse().ok()).flatten()
}

fn identifier(part: &str) -> Option<Identifier> {
    let valid = !part.is_empty()
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if !valid {
        return None;
    }
    Some(number(part).map_or_else(|| Identifier::Text(part.to_owned()), Identifier::Number))
}

pub fn parse(text: &str) -> Option<Version> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    let text = text.split_once('+').map_or(text, |(version, _)| version);
    let (core, pre) = text
        .split_once('-')
        .map_or((text, None), |(core, pre)| (core, Some(pre)));
    let mut parts = core.split('.').map(number);
    let core = [parts.next()??, parts.next()??, parts.next()??];
    if parts.next().is_some() {
        return None;
    }
    let pre = match pre {
        Some(pre) => pre.split('.').map(identifier).collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    Some(Version { core, pre })
}

fn listed(version: &Version, list: &[String]) -> bool {
    list.iter()
        .filter_map(|entry| parse(entry))
        .any(|entry| entry == *version)
}

pub struct Rules<'a> {
    pub channel: Channel,
    pub blocked: &'a [String],
    pub prefix: &'a str,
}

pub fn vet(offer: &Offer, current: &str, rules: &Rules<'_>) -> Result<Version, Refusal> {
    let offered = parse(&offer.version).ok_or(Refusal::Downgrade)?;
    if parse(current).is_some_and(|current| offered <= current) {
        return Err(Refusal::Downgrade);
    }
    if rules.channel == Channel::Stable && offered.is_prerelease() {
        return Err(Refusal::Prerelease);
    }
    let tail = offer
        .url
        .strip_prefix(rules.prefix)
        .ok_or(Refusal::UrlPrefix)?;
    if tail.is_empty() || tail.contains(['?', '#', '\\']) || tail.contains("..") {
        return Err(Refusal::UrlPrefix);
    }
    if listed(&offered, rules.blocked) {
        return Err(Refusal::Blocked);
    }
    Ok(offered)
}

pub fn too_old(current: &str, min: Option<&str>, blocked: &[String]) -> bool {
    let Some(current) = parse(current) else {
        return false;
    };
    let below = min.and_then(parse).is_some_and(|min| current < min);
    below || listed(&current, blocked)
}

pub fn satisfies(offered: &str, min: Option<&str>) -> bool {
    match (parse(offered), min.and_then(parse)) {
        (Some(offered), Some(min)) => offered >= min,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

pub fn next_check(earliest: Timestamp, last_check: Option<Timestamp>) -> Timestamp {
    last_check.map_or(earliest, |last| earliest.max(last.plus_ms(CHECK_GAP_MS)))
}

pub fn after_check(at: Timestamp, random: u64) -> Timestamp {
    let span = u64::try_from(2 * CHECK_JITTER_MS + 1).unwrap_or(1);
    let jitter = i64::try_from(random % span).unwrap_or(0) - CHECK_JITTER_MS;
    at.plus_ms(CHECK_EVERY_MS + jitter)
}
