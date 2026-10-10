use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mivia_os::{
    Desired, RUN_NAME, SEND_TO_ARGS, ShellIntegration, ShellPlaces, VERB_KEY, read_link,
};
use mivia_ports::ipc::setup::StartupState;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, REG_BINARY, REG_EXPAND_SZ};

const TEST_ROOT: &str = r"Software\MiViA Desktop Test";

struct Sandbox {
    prefix: String,
    dir: tempfile::TempDir,
    exe: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        drop(user().delete_subkey_all(self.prefix.trim_end_matches('\\')));
        let parent_empty = user()
            .open_subkey(TEST_ROOT)
            .is_ok_and(|key| key.enum_keys().next().is_none());
        if parent_empty {
            drop(user().delete_subkey(TEST_ROOT));
        }
    }
}

fn user() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

fn sandbox() -> Sandbox {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let prefix = format!(r"{TEST_ROOT}\{}-{nanos}\", std::process::id());
    let dir = tempfile::tempdir().unwrap();
    let install = dir.path().join("MiViA Desktop");
    std::fs::create_dir_all(&install).unwrap();
    std::fs::create_dir_all(dir.path().join("SendTo")).unwrap();
    let exe = install.join("mivia-desktop.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    let sandbox = Sandbox { prefix, dir, exe };
    let (uninstall, _) = user().create_subkey(&sandbox.places().uninstall).unwrap();
    uninstall
        .set_value("InstallLocation", &install.display().to_string())
        .unwrap();
    sandbox
}

impl Sandbox {
    fn places(&self) -> ShellPlaces {
        ShellPlaces::under(&self.prefix, self.dir.path().join("SendTo"))
    }

    fn integration(&self) -> ShellIntegration {
        ShellIntegration::for_install(self.places(), &self.exe).unwrap()
    }

    fn link(&self, name: &str) -> PathBuf {
        self.dir.path().join("SendTo").join(format!("{name}.lnk"))
    }

    fn verb(&self, ext: &str) -> String {
        format!(
            r"{}\SystemFileAssociations\.{ext}\shell\{VERB_KEY}",
            self.places().classes
        )
    }
}

fn desired([autostart, explorer]: [bool; 2], name: &str) -> Desired {
    Desired {
        autostart,
        explorer,
        verb_label: "Analyze with MiViA".to_owned(),
        send_to_name: name.to_owned(),
        old_send_to_names: vec!["MiViA analysis".to_owned(), "MiViA-Auswertung".to_owned()],
    }
}

fn expanded(text: &str) -> String {
    let local = std::env::var("LOCALAPPDATA").unwrap();
    text.replace("%LOCALAPPDATA%", &local)
}

fn same_file(text: &str, path: &Path) -> bool {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap();
    canonical(Path::new(&expanded(text))) == canonical(path)
}

#[test]
fn the_guard_refuses_copies_outside_the_install_folder() {
    let sandbox = sandbox();
    let stray = sandbox.dir.path().join("elsewhere.exe");
    assert!(ShellIntegration::for_install(sandbox.places(), &stray).is_none());
    let other = ShellPlaces::under(r"Software\MiViA Desktop Test\missing\", PathBuf::new());
    assert!(ShellIntegration::for_install(other, &sandbox.exe).is_none());
}

#[test]
fn applying_writes_the_run_value_verbs_and_send_to_link_once() {
    let sandbox = sandbox();
    let shell = sandbox.integration();
    let first = shell.apply(&desired([true, true], "MiViA analysis"));
    assert!(first.changed && first.autostart.is_ok() && first.explorer.is_ok());

    let run = user().open_subkey(&sandbox.places().run).unwrap();
    let raw = run.get_raw_value(RUN_NAME).unwrap();
    assert_eq!(raw.vtype, REG_EXPAND_SZ);
    let value: String = run.get_value(RUN_NAME).unwrap();
    let quoted = value.strip_suffix(" --minimized").unwrap();
    assert!(same_file(quoted.trim_matches('"'), &sandbox.exe), "{value}");

    for ext in ["jpg", "jpeg", "png", "tif", "tiff", "bmp"] {
        let verb = user().open_subkey(sandbox.verb(ext)).unwrap();
        assert_eq!(
            verb.get_value::<String, _>("MUIVerb").unwrap(),
            "Analyze with MiViA"
        );
        assert_eq!(
            verb.get_value::<String, _>("MultiSelectModel").unwrap(),
            "Player"
        );
        assert_eq!(verb.get_value::<String, _>("NeverDefault").unwrap(), "");
        let command: String = verb.open_subkey("command").unwrap().get_value("").unwrap();
        let exe = command.strip_suffix(" --open-paths -- \"%L\"").unwrap();
        assert!(same_file(exe.trim_matches('"'), &sandbox.exe), "{command}");
    }
    let folder = user()
        .open_subkey(format!(
            r"{}\Directory\shell\{VERB_KEY}",
            sandbox.places().classes
        ))
        .unwrap();
    assert!(folder.get_value::<String, _>("MultiSelectModel").is_err());

    let (target, arguments) = read_link(&sandbox.link("MiViA analysis")).unwrap();
    assert!(same_file(&target, &sandbox.exe), "{target}");
    assert_eq!(arguments, SEND_TO_ARGS);
    assert_eq!(shell.startup_state(), StartupState::On);

    let again = shell.apply(&desired([true, true], "MiViA analysis"));
    assert!(!again.changed);
}

#[test]
fn a_language_change_renames_the_send_to_link() {
    let sandbox = sandbox();
    let shell = sandbox.integration();
    shell.apply(&desired([false, true], "MiViA analysis"));
    let renamed = shell.apply(&desired([false, true], "MiViA-Auswertung"));
    assert!(renamed.changed);
    assert!(!sandbox.link("MiViA analysis").exists());
    assert!(sandbox.link("MiViA-Auswertung").exists());
}

#[test]
fn turning_off_removes_only_our_entries_and_prunes_empty_parents() {
    let sandbox = sandbox();
    let shell = sandbox.integration();
    shell.apply(&desired([true, true], "MiViA analysis"));
    let (foreign, _) = user()
        .create_subkey(format!(
            r"{}\SystemFileAssociations\.png\shell\Other",
            sandbox.places().classes
        ))
        .unwrap();
    foreign.set_value("MUIVerb", &"Other").unwrap();

    let off = shell.apply(&desired([false, false], "MiViA analysis"));
    assert!(off.changed);
    let run = user().open_subkey(&sandbox.places().run).unwrap();
    assert!(run.get_raw_value(RUN_NAME).is_err());
    assert!(user().open_subkey(sandbox.verb("jpg")).is_err());
    let jpg = format!(r"{}\SystemFileAssociations\.jpg", sandbox.places().classes);
    assert!(user().open_subkey(jpg).is_err());
    assert!(user().open_subkey(sandbox.verb("png")).is_err());
    let png = format!(
        r"{}\SystemFileAssociations\.png\shell\Other",
        sandbox.places().classes
    );
    assert!(user().open_subkey(png).is_ok());
    assert!(!sandbox.link("MiViA analysis").exists());
    assert_eq!(shell.startup_state(), StartupState::Off);
}

#[test]
fn a_value_of_another_program_is_never_replaced_or_removed() {
    let sandbox = sandbox();
    let other = sandbox.dir.path().join("other.exe");
    std::fs::write(&other, b"MZ").unwrap();
    let foreign = format!("\"{}\" --minimized", other.display());
    let (run, _) = user().create_subkey(&sandbox.places().run).unwrap();
    run.set_value(RUN_NAME, &foreign).unwrap();
    let shell = sandbox.integration();
    shell.apply(&desired([true, false], "MiViA analysis"));
    shell.apply(&desired([false, false], "MiViA analysis"));
    let value: String = run.get_value(RUN_NAME).unwrap();
    assert_eq!(value, foreign);
}

#[test]
fn startup_turned_off_in_task_manager_is_read_and_never_rewritten() {
    let sandbox = sandbox();
    let shell = sandbox.integration();
    let (approved, _) = user()
        .create_subkey(&sandbox.places().startup_approved)
        .unwrap();
    let disabled = winreg::RegValue {
        bytes: vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        vtype: REG_BINARY,
    };
    approved.set_raw_value(RUN_NAME, &disabled).unwrap();
    shell.apply(&desired([true, false], "MiViA analysis"));
    assert_eq!(shell.startup_state(), StartupState::TurnedOffInTaskManager);
    assert_eq!(
        approved.get_raw_value(RUN_NAME).unwrap().bytes,
        disabled.bytes
    );
}

#[test]
fn setup_completion_is_marked_for_it_inventory_and_the_sandbox_is_removed() {
    let prefix;
    {
        let sandbox = sandbox();
        sandbox.integration().mark_setup_completed("2.0.0").unwrap();
        let app = user().open_subkey(&sandbox.places().app).unwrap();
        assert_eq!(app.get_value::<u32, _>("SetupCompleted").unwrap(), 1);
        assert_eq!(app.get_value::<String, _>("SetupVersion").unwrap(), "2.0.0");
        prefix = sandbox.prefix.clone();
    }
    assert!(user().open_subkey(prefix.trim_end_matches('\\')).is_err());
}
