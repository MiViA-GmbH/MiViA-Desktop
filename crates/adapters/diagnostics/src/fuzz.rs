use serde_json::Value;

use crate::{Known, Redactor};

const CANARY: &str = "mvkFuzzCanaryKey0123456789abcdefXYZ";

pub fn redaction(data: &[u8]) -> Result<(), String> {
    let known = Known {
        literals: vec![
            (CANARY.to_owned(), "<access-key>"),
            ("fuzzuser".to_owned(), "<user>"),
        ],
        roots: vec![(r"C:\Users\fuzzuser".to_owned(), "%USERPROFILE%")],
    };
    let redactor = Redactor::new([1; 32], &known, false).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(data);
    let line = redactor.line(&text);
    let outputs = [
        redactor.text(&text),
        redactor.path(&text),
        redactor.description(&text),
    ];
    let leaked = outputs
        .iter()
        .find(|output| leaks(output))
        .or_else(|| holds_leak(&line).then_some(&line));
    leaked.map_or(Ok(()), |output| Err(format!("canary left in {output:?}")))
}

fn leaks(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains(&CANARY.to_lowercase()) || lower.contains("fuzzuser")
}

fn holds_leak(line: &str) -> bool {
    serde_json::from_str::<Value>(line).map_or_else(|_| leaks(line), |value| value_leaks(&value))
}

fn value_leaks(value: &Value) -> bool {
    match value {
        Value::String(text) => leaks(text),
        Value::Array(items) => items.iter().any(value_leaks),
        Value::Object(map) => map
            .iter()
            .any(|(key, item)| leaks(key) || value_leaks(item)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::holds_leak;

    #[test]
    fn the_oracle_reads_decoded_values_not_their_escaped_text() {
        assert!(holds_leak(r#"{"a":"x fuzzuser y"}"#));
        assert!(holds_leak(r#"{"fuzzuser":1}"#));
        assert!(holds_leak(r#"{"a":"fuzzuser"}"#));
        assert!(holds_leak("not json mvkFuzzCanaryKey0123456789abcdefXYZ"));
        assert!(!holds_leak(r#"{"a":"\fuzzuser"}"#));
    }
}
