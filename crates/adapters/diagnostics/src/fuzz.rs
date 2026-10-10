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
    let outputs = [
        redactor.line(&text),
        redactor.text(&text),
        redactor.path(&text),
        redactor.description(&text),
    ];
    let leaked = outputs.iter().find(|output| {
        let lower = output.to_lowercase();
        lower.contains(&CANARY.to_lowercase()) || lower.contains("fuzzuser")
    });
    leaked.map_or(Ok(()), |output| Err(format!("canary left in {output:?}")))
}
