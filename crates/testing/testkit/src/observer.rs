use mivia_ports::store::{CommitObserver, Committed};
use parking_lot::Mutex;

#[derive(Default)]
pub struct RecordingObserver {
    commits: Mutex<Vec<Committed>>,
}

impl RecordingObserver {
    pub fn commits(&self) -> Vec<Committed> {
        self.commits.lock().clone()
    }
}

impl CommitObserver for RecordingObserver {
    fn committed(&self, committed: &Committed) {
        self.commits.lock().push(committed.clone());
    }
}
