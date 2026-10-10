use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer, XmlVersion};

pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;

const BOM: &[u8] = b"\xEF\xBB\xBF";
const TOKEN: &str = "AccessToken";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LegacyConfig {
    pub access_token: Option<String>,
    pub input_directory: Option<String>,
    pub models: Vec<String>,
    pub customizations: Vec<(String, String)>,
    pub server_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyError {
    TooLarge,
    Unreadable,
}

fn text(bytes: &[u8]) -> Result<&str, LegacyError> {
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(LegacyError::TooLarge);
    }
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    std::str::from_utf8(body).map_err(|_| LegacyError::Unreadable)
}

fn attribute(element: &BytesStart<'_>, name: &str) -> Result<Option<String>, LegacyError> {
    let found = element
        .try_get_attribute(name)
        .map_err(|_| LegacyError::Unreadable)?;
    found
        .map(|value| {
            value
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| value.trim().to_owned())
                .map_err(|_| LegacyError::Unreadable)
        })
        .transpose()
}

fn setting(element: &BytesStart<'_>) -> Result<Option<(String, String)>, LegacyError> {
    if element.name().as_ref() != "add" {
        return Ok(None);
    }
    let key = attribute(element, "key")?;
    let value = attribute(element, "value")?.unwrap_or_default();
    Ok(key.map(|key| (key, value)))
}

fn list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn record(config: &mut LegacyConfig, key: &str, value: String) {
    match key {
        TOKEN => config.access_token = non_empty(value),
        "InputDirectory" => config.input_directory = non_empty(value),
        "ServerUrl" => config.server_url = non_empty(value),
        "Models" => config.models = list(&value),
        "Customizations" => {
            config.customizations = list(&value)
                .into_iter()
                .filter_map(|pair| {
                    let (model, id) = pair.split_once(':')?;
                    Some((model.trim().to_owned(), id.trim().to_owned()))
                })
                .collect();
        }
        _ => {}
    }
}

pub fn parse(bytes: &[u8]) -> Result<LegacyConfig, LegacyError> {
    let mut reader = Reader::from_str(text(bytes)?);
    let mut config = LegacyConfig::default();
    loop {
        match reader.read_event().map_err(|_| LegacyError::Unreadable)? {
            Event::Eof => return Ok(config),
            Event::DocType(_) | Event::GeneralRef(_) => return Err(LegacyError::Unreadable),
            Event::Start(element) | Event::Empty(element) => {
                if let Some((key, value)) = setting(&element)? {
                    record(&mut config, &key, value);
                }
            }
            Event::End(_)
            | Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_) => {}
        }
    }
}

fn blanked(element: &BytesStart<'_>) -> Result<Option<BytesStart<'static>>, LegacyError> {
    if setting(element)?.is_none_or(|(key, _)| key != TOKEN) {
        return Ok(None);
    }
    let mut copy = BytesStart::new("add");
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| LegacyError::Unreadable)?;
        if attribute.key.as_ref() == "value" {
            copy.push_attribute(("value", ""));
        } else {
            copy.push_attribute(attribute);
        }
    }
    Ok(Some(copy))
}

pub fn blank_access_token(bytes: &[u8]) -> Result<Vec<u8>, LegacyError> {
    let mut reader = Reader::from_str(text(bytes)?);
    let prefix = if bytes.starts_with(BOM) { BOM } else { &[] };
    let mut writer = Writer::new(prefix.to_vec());
    loop {
        let event = reader.read_event().map_err(|_| LegacyError::Unreadable)?;
        let event = match event {
            Event::Eof => return Ok(writer.into_inner()),
            Event::DocType(_) | Event::GeneralRef(_) => return Err(LegacyError::Unreadable),
            Event::Empty(element) => blanked(&element)?.map_or(Event::Empty(element), Event::Empty),
            Event::Start(element) => blanked(&element)?.map_or(Event::Start(element), Event::Start),
            other @ (Event::End(_)
            | Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)) => other,
        };
        writer
            .write_event(event)
            .map_err(|_| LegacyError::Unreadable)?;
    }
}
