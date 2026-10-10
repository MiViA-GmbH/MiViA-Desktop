use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mivia_domain::ids::FolderId;
use mivia_ports::watch::{FsSignal, SignalSink};
use parking_lot::Mutex;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use super::Wakers;

pub const SIGNAL_CAPACITY: usize = 65_536;

pub struct IntakeSink {
    sender: Sender<FsSignal>,
    capacity: usize,
    overflow: Mutex<BTreeSet<FolderId>>,
    wakers: Arc<Wakers>,
    high_water: AtomicUsize,
}

impl IntakeSink {
    pub fn channel(capacity: usize) -> (Arc<Self>, Receiver<FsSignal>) {
        let (sender, receiver) = channel(capacity);
        let sink = Self {
            sender,
            capacity,
            overflow: Mutex::new(BTreeSet::new()),
            wakers: Arc::new(Wakers::default()),
            high_water: AtomicUsize::new(0),
        };
        (Arc::new(sink), receiver)
    }

    pub(super) const fn wakers(&self) -> &Arc<Wakers> {
        &self.wakers
    }

    pub fn high_water(&self) -> usize {
        self.high_water.load(Ordering::Relaxed)
    }

    pub(super) fn take_overflow(&self) -> BTreeSet<FolderId> {
        std::mem::take(&mut *self.overflow.lock())
    }
}

impl SignalSink for IntakeSink {
    fn push(&self, signal: FsSignal) {
        let folder = signal.folder;
        match self.sender.try_send(signal) {
            Ok(()) => {
                let queued = self.capacity.saturating_sub(self.sender.capacity());
                self.high_water.fetch_max(queued, Ordering::Relaxed);
            }
            Err(TrySendError::Full(_)) => {
                self.overflow.lock().insert(folder);
            }
            Err(TrySendError::Closed(_)) => return,
        }
        self.wakers.intake.notify_one();
    }
}
