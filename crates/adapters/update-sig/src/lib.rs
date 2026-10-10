use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};
use mivia_domain::update::{Refusal, parse};

pub const KEY_A: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDlGOUU3NDdCMUI1RDNDOUQKUldTZFBGMGJlM1NlbitLY3lPcHkyZm10Wk1BVTJMQ3lVdVJWNTkrMHlYRXR0ZE1TVnZPaFpKbkMK";
pub const KEY_B: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU1QTUzMUYyRDc2RkQ3QUQKUldTdDEyL1g4akdsNWNHdlJSU2ZKMjhNRFNjL2lUaXlNVGFnY1ZYV2FjclZuM25uL1llRjVyWG0K";
pub const TEST_KEYS: bool = true;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    A,
    B,
}

fn text(base64: &str) -> Option<String> {
    String::from_utf8(STANDARD.decode(base64.trim()).ok()?).ok()
}

fn signed_version(signature: &Signature) -> Option<&str> {
    signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
}

pub fn verify_with(key: &str, data: &[u8], signature: &str, version: &str) -> Result<(), Refusal> {
    let key = text(key)
        .and_then(|key| PublicKey::decode(&key).ok())
        .ok_or(Refusal::Signature)?;
    let signature = text(signature)
        .and_then(|signature| Signature::decode(&signature).ok())
        .ok_or(Refusal::Signature)?;
    key.verify(data, &signature, true)
        .map_err(|_| Refusal::Signature)?;
    let signed = signed_version(&signature).and_then(parse);
    match (signed, parse(version)) {
        (Some(signed), Some(announced)) if signed == announced => Ok(()),
        _ => Err(Refusal::SignedVersion),
    }
}

pub fn verify(data: &[u8], signature: &str, version: &str) -> Result<Slot, Refusal> {
    match verify_with(KEY_A, data, signature, version) {
        Ok(()) => Ok(Slot::A),
        Err(Refusal::Signature) => verify_with(KEY_B, data, signature, version).map(|()| Slot::B),
        Err(refusal) => Err(refusal),
    }
}
