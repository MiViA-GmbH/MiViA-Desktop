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

pub struct SeededIdGen {
    state: AtomicU64,
}

impl SeededIdGen {
    pub const fn new(seed: u64) -> Self {
        Self {
            state: AtomicU64::new(seed),
        }
    }
}

impl IdGen for SeededIdGen {
    fn uuid(&self) -> Uuid {
        let step = 0x9E37_79B9_7F4A_7C15_u64;
        let mut z = self
            .state
            .fetch_add(step, Ordering::Relaxed)
            .wrapping_add(step);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Uuid::from_u64_pair(z ^ (z >> 31), z.rotate_left(17))
    }
}
