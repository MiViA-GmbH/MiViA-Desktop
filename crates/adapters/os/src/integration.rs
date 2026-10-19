use std::io;
use std::path::{Path, PathBuf};

use mivia_domain::image_format::ACCEPTED_EXTENSIONS;
use mivia_domain::path::WinPath;
use mivia_ports::ipc::setup::StartupState;
use winreg::enums::{HKEY_CURRENT_USER, REG_EXPAND_SZ};
use winreg::{RegKey, RegValue};

use crate::ffi::{self, KnownFolder};
use verbs::remove_verb;

mod verbs;

pub const RUN_NAME: &str = "MiViA Desktop";

pub const VERB_KEY: &str = "MiViA.Desktop";

pub const SEND_TO_ARGS: &str = "--open-paths --";

const LOCAL_APP_DATA: &str = "%LOCALAPPDATA%";

pub struct ShellPlaces {
    pub classes: String,
    pub run: String,
    pub startup_approved: String,
    pub uninstall: String,
    pub app: String,
    pub send_to: PathBuf,
}

impl ShellPlaces {
    pub fn under(prefix: &str, send_to: PathBuf) -> Self {
        let at = |path: &str| format!("{prefix}{path}");
        Self {
            classes: at(r"Software\Classes"),
            run: at(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            startup_approved: at(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
            ),
            uninstall: at(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\MiViA Desktop"),
            app: at(r"Software\MiViA\Desktop"),
            send_to,
        }
    }

    pub fn user() -> Option<Self> {
        ffi::known_folder(KnownFolder::SendTo).map(|send_to| Self::under("", send_to))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desired {
    pub autostart: bool,
    pub explorer: bool,
    pub verb_label: String,
    pub send_to_name: String,
    pub old_send_to_names: Vec<String>,
}

#[derive(Debug)]
pub struct Applied {
    pub changed: bool,
    pub autostart: io::Result<()>,
    pub explorer: io::Result<()>,
}

pub struct ShellIntegration {
    places: ShellPlaces,
    exe: PathBuf,
    install: WinPath,
    expandable: String,
}

fn user() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

fn win(path: &Path) -> Option<WinPath> {
    WinPath::parse(path.to_str()?).ok()
}

fn canonical(path: &Path) -> Option<WinPath> {
    std::fs::canonicalize(path).ok().as_deref().and_then(win)
}

fn expandable(exe: &Path) -> String {
    let local = ffi::known_folder(KnownFolder::LocalAppData).and_then(|dir| win(&dir));
    let rest = local
        .zip(win(exe))
        .and_then(|(local, path)| local.relative_components(&path).map(|rest| rest.join("\\")))
        .filter(|rest| !rest.is_empty());
    rest.map_or_else(
        || exe.display().to_string(),
        |rest| format!("{LOCAL_APP_DATA}\\{rest}"),
    )
}

fn expand_sz(text: &str) -> RegValue {
    let bytes = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    RegValue {
        bytes,
        vtype: REG_EXPAND_SZ,
    }
}

fn set_text(key: &RegKey, name: &str, value: &str, changed: &mut bool) -> io::Result<()> {
    if key.get_value::<String, _>(name).ok().as_deref() != Some(value) {
        key.set_value(name, &value)?;
        *changed = true;
    }
    Ok(())
}

pub fn register_app_id(places: &ShellPlaces, name: &str, icon: &Path) -> io::Result<()> {
    let path = format!(r"{}\AppUserModelId\{}", places.classes, crate::APP_ID);
    let (key, _) = user().create_subkey(path)?;
    key.set_raw_value("DisplayName", &expand_sz(name))?;
    key.set_raw_value("IconUri", &expand_sz(&icon.display().to_string()))
}

impl ShellIntegration {
    pub fn for_install(places: ShellPlaces, exe: &Path) -> Option<Self> {
        let location: String = user()
            .open_subkey(&places.uninstall)
            .ok()?
            .get_value("InstallLocation")
            .ok()?;
        let install = canonical(Path::new(location.trim().trim_matches('"')))?;
        let exe = canonical(exe).filter(|path| install.contains(path))?;
        let exe = PathBuf::from(exe.to_string());
        let expandable = expandable(&exe);
        Some(Self {
            places,
            exe,
            install,
            expandable,
        })
    }

    fn ours(&self, command: &str) -> bool {
        let expanded = ffi::expand_environment(command);
        let target = expanded
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or_default();
        let target = Path::new(target.trim());
        !target.exists() || canonical(target).is_some_and(|path| self.install.contains(&path))
    }

    fn run_value(&self) -> String {
        format!("\"{}\" --minimized", self.expandable)
    }

    fn verb_command(&self) -> String {
        format!("\"{}\" --open-paths -- \"%L\"", self.exe.display())
    }

    pub fn apply(&self, desired: &Desired) -> Applied {
        let mut changed = false;
        let autostart = self.apply_autostart(desired.autostart, &mut changed);
        let explorer = if desired.explorer {
            self.add_explorer(desired, &mut changed)
        } else {
            self.remove_explorer(desired, &mut changed)
        };
        if changed {
            ffi::notify_associations_changed();
        }
        Applied {
            changed,
            autostart,
            explorer,
        }
    }

    pub fn autostart(&self, on: bool) -> io::Result<()> {
        self.apply_autostart(on, &mut false)
    }

    fn apply_autostart(&self, on: bool, changed: &mut bool) -> io::Result<()> {
        let (run, _) = user().create_subkey(&self.places.run)?;
        let current: Option<String> = run.get_value(RUN_NAME).ok();
        let wanted = self.run_value();
        match (on, current) {
            (true, Some(value)) if value == wanted => Ok(()),
            (true, Some(value)) if !self.ours(&value) => Ok(()),
            (true, _) => {
                *changed = true;
                run.set_raw_value(RUN_NAME, &expand_sz(&wanted))
            }
            (false, Some(value)) if self.ours(&value) => {
                *changed = true;
                run.delete_value(RUN_NAME)
            }
            (false, _) => Ok(()),
        }
    }

    fn verb_keys(&self) -> Vec<(String, bool)> {
        ACCEPTED_EXTENSIONS
            .iter()
            .map(|ext| {
                let key = format!(
                    r"{}\SystemFileAssociations\.{ext}\shell\{VERB_KEY}",
                    self.places.classes
                );
                (key, true)
            })
            .chain(std::iter::once((
                format!(r"{}\Directory\shell\{VERB_KEY}", self.places.classes),
                false,
            )))
            .collect()
    }

    fn write_verb(
        &self,
        (path, multi): &(String, bool),
        label: &str,
        changed: &mut bool,
    ) -> io::Result<()> {
        let (key, _) = user().create_subkey(path)?;
        let existing: Option<String> = key
            .open_subkey("command")
            .ok()
            .and_then(|command| command.get_value("").ok());
        if existing.is_some_and(|command| !self.ours(&command)) {
            return Ok(());
        }
        set_text(&key, "MUIVerb", label, changed)?;
        set_text(
            &key,
            "Icon",
            &format!("\"{}\",0", self.exe.display()),
            changed,
        )?;
        set_text(&key, "NeverDefault", "", changed)?;
        if *multi {
            set_text(&key, "MultiSelectModel", "Player", changed)?;
        }
        let (command, _) = key.create_subkey("command")?;
        set_text(&command, "", &self.verb_command(), changed)
    }

    fn link(&self, name: &str) -> PathBuf {
        self.places.send_to.join(format!("{name}.lnk"))
    }

    fn our_link(&self, file: &Path) -> bool {
        ffi::read_link(file).is_ok_and(|(target, _)| self.ours(&target))
    }

    fn add_explorer(&self, desired: &Desired, changed: &mut bool) -> io::Result<()> {
        for verb in self.verb_keys() {
            self.write_verb(&verb, &desired.verb_label, changed)?;
        }
        for old in &desired.old_send_to_names {
            let file = self.link(old);
            if *old != desired.send_to_name && self.our_link(&file) {
                std::fs::remove_file(&file)?;
                *changed = true;
            }
        }
        let file = self.link(&desired.send_to_name);
        let current = ffi::read_link(&file).ok();
        let fresh = current.as_ref().is_some_and(|(target, args)| {
            target.eq_ignore_ascii_case(&self.expandable) && args == SEND_TO_ARGS
        });
        if fresh
            || current
                .as_ref()
                .is_some_and(|(target, _)| !self.ours(target))
        {
            return Ok(());
        }
        *changed = true;
        ffi::save_link(&file, &self.expandable, SEND_TO_ARGS)
    }

    fn remove_explorer(&self, desired: &Desired, changed: &mut bool) -> io::Result<()> {
        for (path, _) in self.verb_keys() {
            *changed |= remove_verb(&path, |command| self.ours(command))?;
        }
        let names = desired
            .old_send_to_names
            .iter()
            .chain(std::iter::once(&desired.send_to_name));
        for name in names {
            let file = self.link(name);
            if self.our_link(&file) {
                std::fs::remove_file(&file)?;
                *changed = true;
            }
        }
        Ok(())
    }

    pub fn startup_state(&self) -> StartupState {
        let approved = user()
            .open_subkey(&self.places.startup_approved)
            .and_then(|key| key.get_raw_value(RUN_NAME))
            .ok()
            .and_then(|value| value.bytes.first().copied());
        if approved.is_some_and(|first| first % 2 == 1) {
            return StartupState::TurnedOffInTaskManager;
        }
        let run: Option<String> = user()
            .open_subkey(&self.places.run)
            .and_then(|key| key.get_value(RUN_NAME))
            .ok();
        if run.is_some_and(|value| self.ours(&value)) {
            StartupState::On
        } else {
            StartupState::Off
        }
    }

    pub fn mark_setup_completed(&self, version: &str) -> io::Result<()> {
        let (key, _) = user().create_subkey(&self.places.app)?;
        key.set_value("SetupCompleted", &1_u32)?;
        key.set_value("SetupVersion", &version)
    }
}
