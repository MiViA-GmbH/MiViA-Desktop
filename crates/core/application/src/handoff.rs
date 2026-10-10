use uuid::Uuid;

pub const MAX_MESSAGE_BYTES: usize = 64;
pub const ACK: u8 = b'K';
pub const OTHER_SESSION: u8 = b'S';

const VERSION: u8 = 1;
const WAKE: u8 = 1;
const SPOOL: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handoff {
    Wake,
    Spool(Uuid),
}

pub fn encode(handoff: Handoff) -> Vec<u8> {
    match handoff {
        Handoff::Wake => vec![VERSION, WAKE],
        Handoff::Spool(id) => [&[VERSION, SPOOL][..], id.as_bytes()].concat(),
    }
}

pub fn parse(bytes: &[u8]) -> Option<Handoff> {
    match bytes {
        [VERSION, WAKE] => Some(Handoff::Wake),
        [VERSION, SPOOL, id @ ..] => Uuid::from_slice(id).ok().map(Handoff::Spool),
        _ => None,
    }
}
