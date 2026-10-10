use std::collections::BTreeSet;

use mivia_domain::ids::{MINUTE_MS, Timestamp};

const GAP_MS: i64 = 10 * MINUTE_MS;

#[derive(Debug, Default)]
pub struct OfferGate {
    offered: BTreeSet<String>,
    last: Option<Timestamp>,
}

impl OfferGate {
    pub fn claim(&mut self, signature: &str, now: Timestamp) -> bool {
        let quiet = self.last.is_none_or(|last| now.0 - last.0 >= GAP_MS);
        let offer = quiet && !self.offered.contains(signature);
        if offer {
            self.offered.insert(signature.to_owned());
            self.last = Some(now);
        }
        offer
    }
}

#[cfg(test)]
mod tests {
    use super::{GAP_MS, OfferGate};
    use mivia_domain::ids::Timestamp;

    #[test]
    fn offers_once_per_signature_and_at_most_once_per_ten_minutes() {
        let mut gate = OfferGate::default();
        assert!(gate.claim("internal:-:a", Timestamp(0)));
        assert!(!gate.claim("internal:-:b", Timestamp(GAP_MS - 1)));
        assert!(gate.claim("internal:-:b", Timestamp(GAP_MS)));
        assert!(!gate.claim("internal:-:a", Timestamp(10 * GAP_MS)));
    }
}
