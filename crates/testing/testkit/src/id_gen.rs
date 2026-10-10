use std::sync::atomic::{AtomicU64, Ordering};

use mivia_ports::id_gen::IdGen;
use uuid::Uuid;

#[derive(Default)]
pub struct SeqIdGen {
    next: AtomicU64,
}

impl IdGen for SeqIdGen {
    fn uuid(&self) -> Uuid {
        Uuid::from_u128(u128::from(self.next.fetch_add(1, Ordering::Relaxed) + 1))
    }
}
