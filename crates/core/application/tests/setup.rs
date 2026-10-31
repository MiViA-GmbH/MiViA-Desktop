use mivia_application::legacy::{LegacyConfig, LegacyError, blank_access_token, parse};
use mivia_application::policy::{self, AutoUpdate, IssueReporting};
use mivia_ports::ipc::screens::LanguageSetting;
use mivia_ports::ipc::support::PolicySource;
use mivia_ports::policy::{Hive, PolicyValue, RegistryValue};

const CONFIG: &str = "\u{FEFF}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n\
<configuration>\r\n  <appSettings>\r\n\
    <add key=\"Models\" value=\"GRAIN, PHASES\" />\r\n\
    <add key=\"AccessToken\" value=\"mv-old-key-AbCdEfGhIjKlMnOpQrStUvWxYz012345\" />\r\n\
    <add key=\"InputDirectory\" value=\"E:\\Mikroskop &amp; Export\" />\r\n\
    <add key=\"Customizations\" value=\"GRAIN:11111111-2222-4333-8444-555555555555\" />\r\n\
    <add key=\"LogLevel\" value=\"Error\" />\r\n\
  </appSettings>\r\n</configuration>\r\n";

fn value(hive: Hive, name: &str, value: RegistryValue) -> PolicyValue {
    PolicyValue {
        hive,
        name: name.to_owned(),
        value,
    }
}

fn text(text: &str) -> RegistryValue {
    RegistryValue::Text(text.to_owned())
}

#[test]
fn the_old_config_yields_key_folder_models_and_customizations() {
    let config = parse(CONFIG.as_bytes()).unwrap();
    assert_eq!(
        config,
        LegacyConfig {
            access_token: Some("mv-old-key-AbCdEfGhIjKlMnOpQrStUvWxYz012345".to_owned()),
            input_directory: Some(r"E:\Mikroskop & Export".to_owned()),
            models: vec!["GRAIN".to_owned(), "PHASES".to_owned()],
            customizations: vec![(
                "GRAIN".to_owned(),
                "11111111-2222-4333-8444-555555555555".to_owned()
            )],
            server_url: None,
        }
    );
}

#[test]
fn blanking_the_key_keeps_every_other_setting_and_the_bom() {
    let blanked = blank_access_token(CONFIG.as_bytes()).unwrap();
    assert!(blanked.starts_with(b"\xEF\xBB\xBF"));
    let text = String::from_utf8(blanked.clone()).unwrap();
    assert!(!text.contains("mv-old-key"));
    assert!(text.contains("<add key=\"LogLevel\" value=\"Error\" />"));
    let again = parse(&blanked).unwrap();
    let before = parse(CONFIG.as_bytes()).unwrap();
    assert_eq!(again.access_token, None);
    assert_eq!(again.models, before.models);
    assert_eq!(again.input_directory, before.input_directory);
    assert_eq!(again.customizations, before.customizations);
}

#[test]
fn hostile_or_huge_configs_are_refused() {
    let dtd = "<?xml version=\"1.0\"?><!DOCTYPE c [<!ENTITY x \"boom\">]>\
               <configuration><appSettings><add key=\"Models\" value=\"&x;\"/></appSettings></configuration>";
    assert_eq!(parse(dtd.as_bytes()), Err(LegacyError::Unreadable));
    assert_eq!(parse(b"<configuration><add"), Err(LegacyError::Unreadable));
    assert_eq!(parse(&[0xFF, 0xFE, 0x00]), Err(LegacyError::Unreadable));
    assert_eq!(
        parse(br#"<add key="AccessToken" value="mv-a" value="b"/>"#),
        Err(LegacyError::Unreadable)
    );
    let huge = vec![b' '; 1024 * 1024 + 1];
    assert_eq!(parse(&huge), Err(LegacyError::TooLarge));
}

#[test]
fn the_legacy_fuzz_entry_holds_for_the_sample() {
    mivia_application::fuzz::legacy_config(CONFIG.as_bytes()).unwrap();
}

#[test]
fn machine_policies_win_and_wrong_types_are_ignored() {
    let values = [
        value(Hive::User, "Language", text("de")),
        value(Hive::Machine, "Language", text("en")),
        value(Hive::User, "Autostart", RegistryValue::Number(0)),
        value(Hive::Machine, "Autostart", text("sometimes")),
        value(Hive::User, "ExplorerIntegration", text("OFF")),
        value(Hive::Machine, "Telemetry", RegistryValue::Number(1)),
        value(Hive::User, "ServerUrl", text("https://evil.example")),
        value(Hive::Machine, "IssueReporting", text("mailto-only")),
        value(Hive::User, "AutoUpdate", text("notify-only")),
        value(Hive::User, "MaxUploadConcurrency", text("3")),
        value(
            Hive::User,
            "DefaultInputFolder",
            RegistryValue::Expandable(r"C:\Lab".to_owned()),
        ),
        value(Hive::User, "Unknown", RegistryValue::Number(1)),
    ];
    let (policies, ignored) = policy::parse(&values);
    assert_eq!(policies.language, Some(LanguageSetting::En));
    assert_eq!(policies.autostart, Some(false));
    assert_eq!(policies.explorer, Some(false));
    assert_eq!(policies.telemetry, Some(true));
    assert_eq!(policies.server_url, None);
    assert_eq!(policies.issue_reporting, Some(IssueReporting::MailtoOnly));
    assert_eq!(policies.auto_update, Some(AutoUpdate::NotifyOnly));
    assert_eq!(policies.max_upload_concurrency, None);
    assert_eq!(policies.default_input_folder.as_deref(), Some(r"C:\Lab"));
    let names: Vec<&str> = ignored.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        names,
        ["ServerUrl", "MaxUploadConcurrency", "Unknown", "Autostart"]
    );
}

#[test]
fn server_url_needs_https_and_the_machine_hive() {
    let accepted = policy::parse(&[value(
        Hive::Machine,
        "ServerUrl",
        text("https://Lab.Example.com/"),
    )]);
    assert_eq!(
        accepted.0.server_url.as_deref(),
        Some("https://lab.example.com")
    );
    for bad in [
        "http://lab.example.com",
        "https://",
        "https://a/b",
        "https://u@h",
    ] {
        let (policies, _) = policy::parse(&[value(Hive::Machine, "ServerUrl", text(bad))]);
        assert_eq!(policies.server_url, None, "{bad}");
    }
}

#[test]
fn the_policy_template_covers_every_value_the_app_reads() {
    let admx = include_str!("../../../../packaging/policies/MiViA.admx");
    let translations = [
        include_str!("../../../../packaging/policies/en-US/MiViA.adml"),
        include_str!("../../../../packaging/policies/de-DE/MiViA.adml"),
    ];
    assert!(admx.contains(&format!("key=\"{}\"", policy::KEY)));
    for name in policy::NAMES {
        assert!(
            admx.contains(&format!("valueName=\"{name}\"")),
            "{name} missing in MiViA.admx"
        );
        for language in translations {
            assert!(
                language.contains(&format!("id=\"{name}\"")),
                "{name} missing in an ADML file"
            );
        }
    }
    let declared = admx.matches("<policy ").count();
    assert_eq!(
        declared,
        policy::NAMES.len(),
        "MiViA.admx declares a policy the app does not read"
    );
}

mod draft {
    use mivia_application::setup::{DraftConnection, KnownRoots, SetupDraft, classify_folder};
    use mivia_domain::path::WinPath;
    use mivia_ports::ipc::folders::{FolderBlock, FolderNote};
    use mivia_ports::ipc::setup::{ModelChoiceView, SetupOption};

    fn win(text: &str) -> WinPath {
        WinPath::parse(text).unwrap()
    }

    fn connection(account: &str) -> DraftConnection {
        DraftConnection {
            issued_key_id: None,
            origin: "https://app.mivia.ai".to_owned(),
            account_id: account.to_owned(),
            name: "Lab".to_owned(),
            email: "lab@example.invalid".to_owned(),
            notes: Vec::new(),
            remaining_uploads: None,
        }
    }

    #[test]
    fn a_new_draft_has_the_recommended_choices_and_round_trips() {
        let mut draft = SetupDraft::default();
        assert!(draft.has(SetupOption::Autostart) && draft.has(SetupOption::Explorer));
        assert!(!draft.has(SetupOption::Telemetry) && draft.has(SetupOption::LegacyStop));
        assert!(!draft.has(SetupOption::IncludeExisting));
        draft.models = vec!["m1:".to_owned(), "m2:c7".to_owned()];
        let json = serde_json::to_string(&draft).unwrap();
        assert_eq!(SetupDraft::parse(Some(&json)), Some(draft.clone()));
        assert_eq!(
            draft.model_views(),
            [
                ModelChoiceView {
                    model_id: "m1".to_owned(),
                    customization_id: None
                },
                ModelChoiceView {
                    model_id: "m2".to_owned(),
                    customization_id: Some("c7".to_owned())
                },
            ]
        );
        assert_eq!(SetupDraft::parse(Some("{broken")), None);
        assert!(!json.contains("key\""));
    }

    #[test]
    fn a_key_of_another_account_resets_the_chosen_models() {
        let mut draft = SetupDraft::default();
        assert!(!draft.connect(connection("a")));
        draft.models = vec!["m1:".to_owned()];
        assert!(!draft.connect(connection("a")));
        assert_eq!(draft.models.len(), 1);
        assert!(draft.connect(connection("b")));
        assert_eq!(draft.models, Vec::<String>::new());
    }

    #[test]
    fn folder_checks_block_broad_and_nested_folders_and_warn_on_general_ones() {
        let roots = KnownRoots {
            windows: vec![win(r"C:\Windows"), win(r"C:\Program Files")],
            profile: Some(win(r"C:\Users\lab")),
            general: vec![win(r"C:\Users\lab\Desktop")],
            one_drive: vec![win(r"C:\Users\lab\OneDrive - Lab")],
        };
        let watched = [win(r"E:\Mikroskop")];
        let check = |path: &str| classify_folder(&win(path), &roots, &watched);
        assert_eq!(check(r"D:\").0, Some(FolderBlock::TooBroad));
        assert_eq!(check(r"C:\Users\lab").0, Some(FolderBlock::TooBroad));
        assert_eq!(
            check(r"C:\Program Files\Tool").0,
            Some(FolderBlock::TooBroad)
        );
        assert_eq!(
            check(r"E:\Mikroskop\Probe").0,
            Some(FolderBlock::Nested {
                name: "Mikroskop".to_owned()
            })
        );
        assert_eq!(
            check(r"C:\Users\lab\Desktop"),
            (None, vec![FolderNote::General])
        );
        assert_eq!(
            check(r"C:\Users\lab\OneDrive - Lab\Bilder"),
            (None, vec![FolderNote::OneDrive])
        );
        assert_eq!(
            check(r"\\server\share\lab"),
            (None, vec![FolderNote::Network])
        );
        assert_eq!(check(r"D:\Labor\Bilder"), (None, Vec::new()));
    }
}

#[test]
fn policy_rows_list_every_name_with_the_effective_value_and_source() {
    let values = [
        value(Hive::User, "IssueReporting", text("off")),
        value(Hive::Machine, "IssueReporting", text("mailto-only")),
        value(Hive::User, "SupportContact", text("mailto:it@lab.example")),
        value(Hive::User, "ServerUrl", text("https://lab.example")),
        value(Hive::Machine, "Autostart", text("maybe")),
        value(Hive::User, "Autostart", RegistryValue::Number(0)),
    ];
    let rows = policy::rows(&values);
    assert_eq!(rows.len(), policy::NAMES.len());
    let row = |name: &str| rows.iter().find(|row| row.name == name).unwrap().clone();
    let issue = row("IssueReporting");
    assert_eq!(issue.value.as_deref(), Some("mailto-only"));
    assert_eq!(issue.source, Some(PolicySource::Machine));
    assert_eq!(row("SupportContact").source, Some(PolicySource::User));
    assert_eq!(row("ServerUrl").source, None);
    let autostart = row("Autostart");
    assert_eq!(autostart.value.as_deref(), Some("0"));
    assert_eq!(autostart.source, Some(PolicySource::User));
    assert_eq!(row("Language").value, None);
}
