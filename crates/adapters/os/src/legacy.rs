use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use mivia_domain::legacy::{CONFIG_NAME, EXE_NAME};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use crate::ffi::{self, KnownFolder};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const TASKBAR: &str = r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar";
const FOLDER_DEPTH: usize = 3;
const FOLDER_ENTRIES: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyInstall {
    pub folder: PathBuf,
    pub exe: PathBuf,
    pub config: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupEntry {
    UserLink(PathBuf),
    UserRun(String),
    MachineLink(PathBuf),
    MachineRun(String),
}

pub fn legacy_at(folder: &Path) -> Option<LegacyInstall> {
    let exe = folder.join(EXE_NAME);
    let config = folder.join(CONFIG_NAME);
    (exe.is_file() && config.is_file()).then(|| LegacyInstall {
        folder: folder.to_path_buf(),
        exe,
        config,
    })
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

fn command_path(command: &str) -> PathBuf {
    let expanded = ffi::expand_environment(command.trim());
    let path = match expanded.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or_default().to_owned(),
        None => expanded
            .find(EXE_NAME)
            .and_then(|end| expanded.get(..end + EXE_NAME.len()))
            .unwrap_or(&expanded)
            .to_owned(),
    };
    PathBuf::from(path)
}

fn running_folders() -> Vec<PathBuf> {
    let own = ffi::current_user_sid().ok();
    ffi::processes_named(EXE_NAME)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|pid| ffi::process_identity(pid).ok())
        .filter(|identity| own.as_deref() == Some(identity.sid.as_str()))
        .filter_map(|identity| identity.image.parent().map(Path::to_path_buf))
        .collect()
}

fn links_in(folder: &Path, depth: usize) -> Vec<PathBuf> {
    walkdir::WalkDir::new(folder)
        .max_depth(depth)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
        })
        .map(walkdir::DirEntry::into_path)
        .collect()
}

fn link_folders() -> Vec<PathBuf> {
    let places = [
        (KnownFolder::Desktop, 1),
        (KnownFolder::PublicDesktop, 1),
        (KnownFolder::Programs, 2),
        (KnownFolder::CommonPrograms, 2),
        (KnownFolder::Startup, 1),
        (KnownFolder::CommonStartup, 1),
    ];
    let mut links: Vec<PathBuf> = places
        .iter()
        .filter_map(|(place, depth)| ffi::known_folder(*place).map(|dir| links_in(&dir, *depth)))
        .flatten()
        .collect();
    if let Some(roaming) = ffi::known_folder(KnownFolder::RoamingAppData) {
        links.extend(links_in(&roaming.join(TASKBAR), 1));
    }
    links
        .iter()
        .filter_map(|link| ffi::read_link(link).ok())
        .filter_map(|(target, _)| command_path(&target).parent().map(Path::to_path_buf))
        .collect()
}

fn run_values(hive: &RegKey) -> Vec<(String, String)> {
    hive.open_subkey(RUN_KEY)
        .map(|key| {
            key.enum_values()
                .filter_map(Result::ok)
                .filter_map(|(name, _)| {
                    key.get_value::<String, _>(&name)
                        .ok()
                        .map(|value| (name, value))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn run_folders() -> Vec<PathBuf> {
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .flat_map(|hive| run_values(&RegKey::predef(hive)))
        .filter(|(_, value)| {
            value
                .to_ascii_lowercase()
                .contains(&EXE_NAME.to_ascii_lowercase())
        })
        .filter_map(|(_, value)| command_path(&value).parent().map(Path::to_path_buf))
        .collect()
}

fn named_folders(deadline: Instant) -> Vec<PathBuf> {
    let roots = [
        KnownFolder::Downloads,
        KnownFolder::Desktop,
        KnownFolder::Documents,
    ];
    roots
        .into_iter()
        .filter_map(ffi::known_folder)
        .flat_map(|root| {
            walkdir::WalkDir::new(root)
                .min_depth(1)
                .max_depth(FOLDER_DEPTH)
                .into_iter()
                .take(FOLDER_ENTRIES)
                .take_while(|_| Instant::now() < deadline)
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_dir())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .to_lowercase()
                        .contains("mivia")
                })
                .map(walkdir::DirEntry::into_path)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn modified(install: &LegacyInstall) -> SystemTime {
    std::fs::metadata(&install.config)
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

pub fn find_legacy(exclude: &Path, budget: Duration) -> Option<LegacyInstall> {
    let deadline = Instant::now() + budget;
    let mut folders = running_folders();
    folders.extend(link_folders());
    folders.extend(run_folders());
    if Instant::now() < deadline {
        folders.extend(named_folders(deadline));
    }
    folders
        .iter()
        .filter(|folder| !folder.starts_with(exclude))
        .filter_map(|folder| legacy_at(folder))
        .max_by_key(modified)
}

pub fn legacy_processes(exe: &Path) -> Vec<u32> {
    let own = ffi::current_user_sid().ok();
    let session = ffi::current_session_id().ok();
    ffi::processes_named(EXE_NAME)
        .unwrap_or_default()
        .into_iter()
        .filter(|pid| ffi::process_session(*pid).ok() == session)
        .filter(|pid| {
            ffi::process_identity(*pid).is_ok_and(|identity| {
                own.as_deref() == Some(identity.sid.as_str()) && same(&identity.image, exe)
            })
        })
        .collect()
}

pub fn close_legacy(exe: &Path) -> io::Result<()> {
    legacy_processes(exe)
        .into_iter()
        .try_for_each(ffi::terminate)
}

pub fn is_network(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.get(..3).is_some_and(ffi::drive_is_remote)
}

fn links_to(folder: Option<PathBuf>, exe: &Path) -> Vec<PathBuf> {
    folder
        .map(|dir| links_in(&dir, 1))
        .unwrap_or_default()
        .into_iter()
        .filter(|link| {
            ffi::read_link(link).is_ok_and(|(target, _)| same(&command_path(&target), exe))
        })
        .collect()
}

fn runs_to(hive: &RegKey, exe: &Path) -> Vec<String> {
    run_values(hive)
        .into_iter()
        .filter(|(_, value)| same(&command_path(value), exe))
        .map(|(name, _)| name)
        .collect()
}

pub fn legacy_startup(exe: &Path) -> Vec<StartupEntry> {
    let user = links_to(ffi::known_folder(KnownFolder::Startup), exe)
        .into_iter()
        .map(StartupEntry::UserLink);
    let common = links_to(ffi::known_folder(KnownFolder::CommonStartup), exe)
        .into_iter()
        .map(StartupEntry::MachineLink);
    let user_runs = runs_to(&RegKey::predef(HKEY_CURRENT_USER), exe)
        .into_iter()
        .map(StartupEntry::UserRun);
    let machine_runs = runs_to(&RegKey::predef(HKEY_LOCAL_MACHINE), exe)
        .into_iter()
        .map(StartupEntry::MachineRun);
    user.chain(common)
        .chain(user_runs)
        .chain(machine_runs)
        .collect()
}

pub fn remove_user_startup(entry: &StartupEntry) -> io::Result<bool> {
    match entry {
        StartupEntry::UserLink(link) => std::fs::remove_file(link).map(|()| true),
        StartupEntry::UserRun(name) => RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, winreg::enums::KEY_SET_VALUE)
            .and_then(|key| key.delete_value(name))
            .map(|()| true),
        StartupEntry::MachineLink(_) | StartupEntry::MachineRun(_) => Ok(false),
    }
}
