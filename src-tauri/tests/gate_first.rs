use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const ACK_WAIT: Duration = Duration::from_secs(10);

struct Run {
    local_app_data: PathBuf,
    primary: Child,
}

impl Drop for Run {
    fn drop(&mut self) {
        drop(self.primary.kill());
        drop(self.primary.wait());
        drop(std::fs::remove_dir_all(&self.local_app_data));
    }
}

fn app(local_app_data: &Path, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_mivia-desktop"))
        .args(args)
        .env("LOCALAPPDATA", local_app_data)
        .spawn()
        .unwrap()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_secondary_is_answered_while_the_primary_is_still_starting() {
    let local_app_data =
        std::env::temp_dir().join(format!("mivia-gate-first-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&local_app_data));
    let data_dir = local_app_data.join("ai.mivia.desktop");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("logs"), b"not a directory").unwrap();
    let primary = app(&local_app_data, &["--minimized"]);
    let mut run = Run {
        local_app_data,
        primary,
    };
    wait_for(&data_dir.join("instance.nonce"));
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    let mut secondary = app(
        &run.local_app_data,
        &["--open-paths", "--", r"C:\gate\a.png"],
    );
    let status = loop {
        if let Some(status) = secondary.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > ACK_WAIT / 2 {
            drop(secondary.kill());
            drop(secondary.wait());
            panic!("the secondary got no answer while the primary was still starting");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "secondary exited with {status}");
    assert!(run.primary.try_wait().unwrap().is_none());
}
