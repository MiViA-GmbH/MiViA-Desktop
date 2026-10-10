use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    NotAbsolute,
    DotComponent,
    InvalidCharacter,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Root {
    Drive(char),
    Unc { server: String, share: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WinPath {
    root: Root,
    components: Vec<String>,
}

const INVALID: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

impl WinPath {
    pub fn parse(text: &str) -> Result<Self, PathError> {
        let unified = text.replace('/', "\\");
        let plain = strip_verbatim(&unified);
        let (root, rest) = split_root(&plain)?;
        let components = rest
            .split('\\')
            .filter(|part| !part.is_empty())
            .map(component)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { root, components })
    }

    pub const fn is_unc(&self) -> bool {
        matches!(self.root, Root::Unc { .. })
    }

    pub fn key(&self) -> String {
        fold(&self.to_string())
    }

    pub fn components(&self) -> &[String] {
        &self.components
    }

    pub fn file_name(&self) -> Option<&str> {
        self.components.last().map(String::as_str)
    }

    pub fn parent(&self) -> Option<Self> {
        let (_, rest) = self.components.split_last()?;
        Some(Self {
            root: self.root.clone(),
            components: rest.to_vec(),
        })
    }

    pub fn join(&self, name: &str) -> Result<Self, PathError> {
        let mut joined = self.clone();
        joined.components.push(component(name)?);
        Ok(joined)
    }

    pub fn contains(&self, other: &Self) -> bool {
        self.relative_components(other).is_some()
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.contains(other) || other.contains(self)
    }

    pub fn relative_components<'a>(&self, other: &'a Self) -> Option<&'a [String]> {
        let same_root = fold(&root_text(&self.root)) == fold(&root_text(&other.root));
        let (head, rest) = other.components.split_at_checked(self.components.len())?;
        let same_prefix = head
            .iter()
            .zip(&self.components)
            .all(|(left, right)| fold(left) == fold(right));
        (same_root && same_prefix).then_some(rest)
    }
}

impl fmt::Display for WinPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&root_text(&self.root))?;
        let separator_after_root =
            matches!(self.root, Root::Drive(_)) || !self.components.is_empty();
        if separator_after_root {
            f.write_str("\\")?;
        }
        f.write_str(&self.components.join("\\"))
    }
}

pub fn fold(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

pub(crate) fn fold_char(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

fn root_text(root: &Root) -> String {
    match root {
        Root::Drive(letter) => format!("{letter}:"),
        Root::Unc { server, share } => format!("\\\\{server}\\{share}"),
    }
}

fn strip_verbatim(text: &str) -> String {
    if let Some(rest) = text.strip_prefix("\\\\?\\UNC\\") {
        return format!("\\\\{rest}");
    }
    text.strip_prefix("\\\\?\\").unwrap_or(text).to_owned()
}

fn split_root(text: &str) -> Result<(Root, &str), PathError> {
    if let Some(rest) = text.strip_prefix("\\\\") {
        let mut parts = rest.splitn(3, '\\');
        let server = component(parts.next().unwrap_or_default())?;
        let share = component(parts.next().unwrap_or_default())?;
        return Ok((
            Root::Unc { server, share },
            parts.next().unwrap_or_default(),
        ));
    }
    let mut chars = text.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(letter), Some(':'), Some('\\')) if letter.is_ascii_alphabetic() => {
            Ok((Root::Drive(letter.to_ascii_uppercase()), chars.as_str()))
        }
        _ => Err(PathError::NotAbsolute),
    }
}

fn component(part: &str) -> Result<String, PathError> {
    if part.is_empty() {
        return Err(PathError::NotAbsolute);
    }
    if part == "." || part == ".." {
        return Err(PathError::DotComponent);
    }
    let invalid = part.chars().any(|c| c.is_control() || INVALID.contains(&c));
    if invalid {
        return Err(PathError::InvalidCharacter);
    }
    Ok(part.to_owned())
}
