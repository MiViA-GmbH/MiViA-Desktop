use std::collections::BTreeMap;
use std::os::windows::io::AsHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ROUNDS: usize = 100;
const SECONDARIES: usize = 15;
const PARALLEL_ROUNDS: usize = 4;
const KILL_WINDOW_MS: u64 = 3_000;
const SETTLE: Duration = Duration::from_secs(40);

struct Round {
    index: usize,
    local_app_data: PathBuf,
    children: Vec<Child>,
}

impl Drop for Round {
    fn drop(&mut self) {
        for child in &mut self.children {
            drop(child.kill());
            drop(child.wait());
        }
        drop(std::fs::remove_dir_all(&self.local_app_data));
    }
}

fn data_dir(local_app_data: &Path) -> PathBuf {
    local_app_data.join("ai.mivia.desktop")
}

fn sent_path(round: usize, file: usize) -> String {
    format!(r"C:\burst\r{round:03}\f{file:02}.png")
}

fn primary_pid(local_app_data: &Path) -> Option<u32> {
    let nonce = std::fs::read_to_string(data_dir(local_app_data).join("instance.nonce")).ok()?;
    let sid = mivia_os::current_user_sid().ok()?;
    let name = format!(r"\\.\pipe\ai.mivia.desktop-{sid}-{nonce}");
    let pipe = mivia_os::open_pipe_client(&name).ok()?;
    mivia_os::pipe_server_pid(pipe.as_handle()).ok()
}

fn start(index: usize) -> Round {
    let local_app_data =
        std::env::temp_dir().join(format!("mivia-burst-{}-{index}", std::process::id()));
    drop(std::fs::remove_dir_all(&local_app_data));
    std::fs::create_dir_all(&local_app_data).unwrap();
    let children = (0..SECONDARIES)
        .map(|file| {
            Command::new(env!("CARGO_BIN_EXE_mivia-desktop"))
                .args(["--open-paths", "--", &sent_path(index, file)])
                .env("LOCALAPPDATA", &local_app_data)
                .spawn()
                .unwrap()
        })
        .collect();
    Round {
        index,
        local_app_data,
        children,
    }
}

fn kill_primary(round: &mut Round, after: Duration) -> Option<u32> {
    let started = Instant::now();
    std::thread::sleep(after);
    while started.elapsed() < after + Duration::from_secs(5) {
        if let Some(pid) = primary_pid(&round.local_app_data)
            && let Some(child) = round.children.iter_mut().find(|child| child.id() == pid)
        {
            child.kill().unwrap();
            child.wait().unwrap();
            return Some(pid);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn settle(round: &mut Round, killed: Option<u32>) -> Vec<u32> {
    let deadline = Instant::now() + SETTLE;
    loop {
        let alive: Vec<u32> = round
            .children
            .iter_mut()
            .filter_map(|child| child.try_wait().unwrap().is_none().then(|| child.id()))
            .collect();
        let primary = primary_pid(&round.local_app_data);
        let settled = alive.is_empty() || (alive.len() == 1 && primary == alive.first().copied());
        if settled || Instant::now() > deadline {
            assert!(
                alive.len() <= 1,
                "round {} ends with {} live processes (killed {killed:?})",
                round.index,
                alive.len()
            );
            return alive;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn staged_paths(local_app_data: &Path) -> BTreeMap<String, usize> {
    let mut seen = BTreeMap::new();
    let inbox = data_dir(local_app_data).join("inbox");
    for entry in std::fs::read_dir(inbox).unwrap() {
        let path = entry.unwrap().path();
        assert_eq!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("json"),
            "{}",
            path.display()
        );
        let spool = mivia_application::spool::parse(&std::fs::read(&path).unwrap()).unwrap();
        for staged in spool.paths {
            *seen.entry(staged.to_string()).or_insert(0) += 1;
        }
    }
    seen
}

fn check(mut round: Round, kill_after: Duration) {
    let killed = kill_primary(&mut round, kill_after);
    assert!(
        killed.is_some(),
        "round {} found no primary to kill",
        round.index
    );
    let alive = settle(&mut round, killed);
    for child in &mut round.children {
        if alive.contains(&child.id()) || killed == Some(child.id()) {
            continue;
        }
        if let Some(status) = child.try_wait().unwrap()
            && !status.success()
        {
            panic!(
                "round {} child {} exited with {status}",
                round.index,
                child.id()
            );
        }
    }
    let seen = staged_paths(&round.local_app_data);
    let expected: BTreeMap<String, usize> = (0..SECONDARIES)
        .map(|file| (sent_path(round.index, file), 1))
        .collect();
    assert_eq!(
        seen, expected,
        "round {} after kill at {kill_after:?}",
        round.index
    );
    let failures: Vec<_> = std::fs::read_dir(data_dir(&round.local_app_data).join("logs"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name())
                .filter(|name| name.to_string_lossy().starts_with("mivia-"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        failures.is_empty(),
        "round {} gate failures {failures:?}",
        round.index
    );
}

fn random_delays() -> Vec<Duration> {
    let state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let mut state = u64::from(state) << 1 | 1;
    (0..ROUNDS)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            Duration::from_millis(state % KILL_WINDOW_MS)
        })
        .collect()
}

#[test]
fn bursts_of_secondaries_survive_a_killed_primary() {
    let delays = random_delays();
    for batch in delays.chunks(PARALLEL_ROUNDS).enumerate() {
        let (batch_index, batch_delays) = batch;
        std::thread::scope(|scope| {
            for (offset, delay) in batch_delays.iter().enumerate() {
                let round = start(batch_index * PARALLEL_ROUNDS + offset);
                scope.spawn(move || check(round, *delay));
            }
        });
    }
}
