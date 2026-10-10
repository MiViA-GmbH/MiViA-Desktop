mod client;
mod listener;

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use mivia_application::handoff::{self, Handoff};
use mivia_application::launch::{self, Launch};
use mivia_application::spool;
use mivia_os::{
    ProcessIdentity, create_user_only, current_user_sid, is_elevated, process_identity,
};
use uuid::Uuid;

pub(crate) use listener::Primary;

pub(crate) const IDENTIFIER: &str = "ai.mivia.desktop";
const NONCE_FILE: &str = "instance.nonce";
const ELECTION_BUDGET: Duration = Duration::from_secs(10);
const MAX_BACKOFF: Duration = Duration::from_secs(1);

pub(crate) enum Outcome {
    Primary(Box<Primary>),
    Forwarded,
}

#[derive(Debug)]
pub(crate) enum Failure {
    Elevated,
    OtherSession,
    Unverified,
    Unreachable,
    Io(io::Error),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Failure {
    fn reason(&self) -> String {
        match self {
            Self::Io(error) => error.to_string(),
            Self::Elevated | Self::OtherSession | Self::Unverified | Self::Unreachable => {
                format!("{self:?}")
            }
        }
    }

    pub(crate) fn report(&self) -> ExitCode {
        let text = match self {
            Self::Elevated => "MiViA Desktop cannot run as administrator. Start it normally.",
            Self::OtherSession => {
                "MiViA Desktop is already running in another Windows session of your user."
            }
            Self::Unverified | Self::Unreachable | Self::Io(_) => {
                "MiViA Desktop could not receive the images. Start MiViA Desktop and send them again."
            }
        };
        if let Some(dir) = data_dir() {
            log_failure(&dir.join("logs"), self);
        }
        mivia_os::show_error("MiViA Desktop", text);
        ExitCode::from(if matches!(self, Self::Elevated) { 4 } else { 3 })
    }
}

pub(crate) fn data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join(IDENTIFIER))
}

pub(crate) fn inbox(data_dir: &Path) -> PathBuf {
    data_dir.join("inbox")
}

pub(crate) fn spool_file(data_dir: &Path, id: Uuid) -> PathBuf {
    inbox(data_dir).join(format!("{id}.json"))
}

pub(crate) fn claim() -> Result<Outcome, Failure> {
    let (args, non_unicode) = utf8_args(std::env::args_os().skip(1));
    let launch = launch::parse(&args);
    let data_dir = data_dir().ok_or(Failure::Unreachable)?;
    let sid = current_user_sid()?;
    let own = process_identity(std::process::id())?;
    let name = pipe_name(&sid, &nonce(&data_dir, &sid)?);
    let message = if launch.paths.is_empty() {
        Handoff::Wake
    } else {
        Handoff::Spool(write_spool(&data_dir, &launch, non_unicode)?)
    };
    let election = Election {
        name,
        sid,
        own,
        elevated: is_elevated()?,
    };
    election.run(&handoff::encode(message), launch, data_dir)
}

fn utf8_args(args: impl Iterator<Item = OsString>) -> (Vec<String>, u32) {
    let mut non_unicode = 0;
    let utf8 = args
        .filter_map(|arg| {
            let text = arg.into_string().ok();
            non_unicode += u32::from(text.is_none());
            text
        })
        .collect();
    (utf8, non_unicode)
}

struct Election {
    name: String,
    sid: String,
    own: ProcessIdentity,
    elevated: bool,
}

impl Election {
    fn run(self, message: &[u8], launch: Launch, data_dir: PathBuf) -> Result<Outcome, Failure> {
        let deadline = Instant::now() + ELECTION_BUDGET;
        let mut backoff = Duration::from_millis(100);
        loop {
            if !self.elevated
                && let Ok(pipe) = listener::create(&self.name, &self.sid)
            {
                let claimed = listener::Claimed {
                    pipe,
                    name: self.name,
                    sid: self.sid,
                    own: self.own,
                };
                return Ok(Outcome::Primary(Box::new(Primary::new(
                    claimed, launch, data_dir,
                )?)));
            }
            match client::forward(&self.name, &self.own, message) {
                Ok(client::Reply::Delivered) => return Ok(Outcome::Forwarded),
                Ok(client::Reply::OtherSession) => return Err(Failure::OtherSession),
                Err(client::Refused::Unverified) => return Err(Failure::Unverified),
                Err(client::Refused::Unreachable) if self.elevated => {
                    return Err(Failure::Elevated);
                }
                Err(client::Refused::Unreachable) if Instant::now() >= deadline => {
                    return Err(Failure::Unreachable);
                }
                Err(client::Refused::Unreachable) => {}
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }
}

fn pipe_name(sid: &str, nonce: &str) -> String {
    format!(r"\\.\pipe\{IDENTIFIER}-{sid}-{nonce}")
}

fn nonce(data_dir: &Path, sid: &str) -> io::Result<String> {
    let path = data_dir.join(NONCE_FILE);
    if let Some(nonce) = read_nonce(&path) {
        return Ok(nonce);
    }
    std::fs::create_dir_all(data_dir)?;
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let fresh = format!("{:032x}", u128::from_be_bytes(bytes));
    let temp = data_dir.join(format!("{NONCE_FILE}.{}.tmp", std::process::id()));
    drop(std::fs::remove_file(&temp));
    let mut file = create_user_only(&temp, sid)?;
    file.write_all(fresh.as_bytes())?;
    file.sync_all()?;
    drop(file);
    let linked = std::fs::hard_link(&temp, &path);
    std::fs::remove_file(&temp)?;
    match linked {
        Ok(()) => Ok(fresh),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            read_nonce(&path).ok_or_else(|| io::Error::other("unreadable instance nonce"))
        }
        Err(error) => Err(error),
    }
}

fn read_nonce(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(64)
        .read_to_string(&mut text)
        .ok()?;
    let valid = text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit());
    valid.then_some(text)
}

fn write_spool(data_dir: &Path, launch: &Launch, non_unicode: u32) -> io::Result<Uuid> {
    let bytes = spool::encode(std::process::id(), &launch.paths, non_unicode)?;
    std::fs::create_dir_all(inbox(data_dir))?;
    let id = Uuid::now_v7();
    let target = spool_file(data_dir, id);
    let temp = target.with_extension("tmp");
    let mut file = File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, &target)?;
    Ok(id)
}

fn log_failure(logs: &Path, failure: &Failure) {
    let line = serde_json::json!({
        "timestamp": mivia_ports::time::iso_utc(mivia_domain::ids::Timestamp(mivia_diagnostics::now_ms())),
        "level": "ERROR",
        "message": "instance.gateError",
        "reason": failure.reason(),
        "target": "mivia_desktop::instance",
    });
    let path = logs.join(format!("mivia-{}.log", std::process::id()));
    if std::fs::create_dir_all(logs).is_ok() {
        drop(std::fs::write(path, format!("{line}\n")));
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{pipe_name, read_nonce, utf8_args};

    #[test]
    fn pipe_name_carries_sid_and_nonce() {
        assert_eq!(
            pipe_name("S-1-5-21-1", "00ff"),
            r"\\.\pipe\ai.mivia.desktop-S-1-5-21-1-00ff"
        );
    }

    #[test]
    fn non_unicode_arguments_are_dropped_and_counted() {
        use std::os::windows::ffi::OsStringExt;
        let lone_surrogate = OsString::from_wide(&[0xD800]);
        let (args, dropped) = utf8_args(
            [
                OsString::from("--open-paths"),
                lone_surrogate,
                OsString::from(r"C:\a"),
            ]
            .into_iter(),
        );
        assert_eq!(args, vec!["--open-paths".to_owned(), r"C:\a".to_owned()]);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn nonce_must_be_32_hex_characters() {
        let dir = std::env::temp_dir().join(format!("mivia-nonce-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n");
        for (text, valid) in [
            ("0123456789abcdef0123456789abcdef", true),
            ("xyz", false),
            ("", false),
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(read_nonce(&path).is_some(), valid, "{text}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
