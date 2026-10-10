use mivia_diagnostics::{Known, Redactor};
use proptest::prelude::*;

#[derive(Debug)]
struct Planted {
    key: String,
    user: String,
    host: String,
    email: String,
    token: String,
    segments: Vec<String>,
    filler: String,
}

fn planted() -> impl Strategy<Value = Planted> {
    (
        "[A-Za-z0-9_-]{32,48}",
        "[g-z]{2}[0-9]{2}[g-z]{2,4}",
        "[G-Z]{2}[0-9]{3}",
        "[g-z]{3,6}[0-9]{2}@[a-z]{3,8}\\.(com|de)",
        "[g-z]{2}[0-9]{6}",
        prop::collection::vec("[g-z]{2}[0-9]{2}[a-z]{2,5}", 3..6),
        "[a-z ]{0,20}",
    )
        .prop_map(
            |(key, user, host, email, token, segments, filler)| Planted {
                key,
                user,
                host,
                email,
                token,
                segments,
                filler,
            },
        )
}

fn known(planted: &Planted) -> Known {
    Known {
        literals: vec![
            (planted.key.clone(), "<access-key>"),
            (planted.user.clone(), "<user>"),
            (planted.host.clone(), "<host>"),
        ],
        roots: vec![
            (format!(r"C:\Users\{}", planted.user), "%USERPROFILE%"),
            (
                format!(r"C:\Users\{}\AppData\Local", planted.user),
                "%LOCALAPPDATA%",
            ),
        ],
    }
}

fn text(planted: &Planted) -> String {
    let [a, b, c, ..] = planted.segments.as_slice() else {
        return String::new();
    };
    format!(
        "{filler} key={key} KEY={upper} by {email} file C:\\Users\\{user}\\Documents\\{a}\\{b}.tif \
         and D:\\{c}\\{a}.jpg on \\\\{host}\\{b}\\{c}.png Authorization: Bearer {token} \
         https://{user}:{token}@lab.example/x host {host} {filler}",
        filler = planted.filler,
        key = planted.key,
        upper = planted.key.to_uppercase(),
        email = planted.email,
        user = planted.user,
        host = planted.host,
        token = planted.token,
    )
}

fn leaks(output: &str, planted: &Planted) -> Vec<String> {
    let lower = output.to_lowercase();
    let mut secrets = vec![
        planted.key.clone(),
        planted.user.clone(),
        planted.host.clone(),
        planted.email.clone(),
        planted.token.clone(),
    ];
    secrets.extend(planted.segments.iter().cloned());
    secrets
        .into_iter()
        .filter(|secret| lower.contains(&secret.to_lowercase()))
        .collect()
}

fn redactor(key: u8, planted: &Planted, names: bool) -> Redactor {
    Redactor::new([key; 32], &known(planted), names).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn redacted_text_contains_no_planted_secret_or_name(planted in planted()) {
        let redactor = redactor(7, &planted, false);
        let output = redactor.text(&text(&planted));
        prop_assert!(leaks(&output, &planted).is_empty(), "{output}");
        let line = serde_json::json!({ "message": text(&planted), "path": planted.segments[0] }).to_string();
        let output = redactor.line(&line);
        prop_assert!(leaks(&output, &planted).is_empty(), "{output}");
    }

    #[test]
    fn redaction_is_idempotent(planted in planted()) {
        let redactor = redactor(7, &planted, false);
        let once = redactor.text(&text(&planted));
        prop_assert_eq!(redactor.text(&once), once);
        let line = redactor.line(&serde_json::json!({ "file": planted.segments[1], "folder": format!(r"\\{}\{} x", planted.host, planted.segments[2]), "message": text(&planted) }).to_string());
        prop_assert_eq!(redactor.line(&line), line);
    }

    #[test]
    fn pseudonyms_are_stable_in_a_report_and_differ_across_reports(planted in planted()) {
        let path = format!(r"D:\{}\{}.tif", planted.segments[0], planted.segments[1]);
        let first = redactor(1, &planted, false);
        let second = redactor(2, &planted, false);
        prop_assert_eq!(first.text(&path).to_lowercase(), first.text(&path.to_uppercase()).to_lowercase());
        prop_assert_ne!(first.text(&path), second.text(&path));
        prop_assert!(first.text(&path).starts_with("<drive-1>\\n-"));
        prop_assert!(first.text(&path).rsplit_once('.').is_some_and(|(_, ext)| ext == "tif"));
    }

    #[test]
    fn names_included_keep_paths_but_never_secrets(planted in planted()) {
        let redactor = redactor(7, &planted, true);
        let output = redactor.text(&text(&planted));
        prop_assert!(output.contains(&planted.segments[0]));
        let lower = output.to_lowercase();
        for secret in [&planted.key, &planted.email, &planted.token, &planted.user, &planted.host] {
            prop_assert!(!lower.contains(&secret.to_lowercase()), "{output}");
        }
    }
}

#[test]
fn roots_and_drives_get_stable_tokens() {
    let planted = Planted {
        key: "k".repeat(40),
        user: "gx12ab".to_owned(),
        host: "LAB01".to_owned(),
        email: "a@b.de".to_owned(),
        token: "t".to_owned(),
        segments: Vec::new(),
        filler: String::new(),
    };
    let redactor = redactor(3, &planted, false);
    let local = redactor.text(r"C:\Users\GX12AB\AppData\Local\MiViA\x.log");
    assert!(local.starts_with(r"%LOCALAPPDATA%\n-"), "{local}");
    assert_eq!(local.rsplit_once('.').map(|(_, ext)| ext), Some("log"));
    assert_eq!(redactor.text(r"E:\a"), redactor.text(r"e:\a"));
    assert!(redactor.text(r"E:\a").starts_with("<drive-1>"));
    assert!(redactor.text(r"F:\a").starts_with("<drive-2>"));
    assert_eq!(
        redactor.text(r"C:\Users\x\OneDrive - Contoso GmbH\y"),
        redactor.text(r"C:\Users\x\OneDrive - <org>\y")
    );
    assert_eq!(
        redactor.description("mail a@b.de gx12ab"),
        "mail <email> <user>"
    );
}
