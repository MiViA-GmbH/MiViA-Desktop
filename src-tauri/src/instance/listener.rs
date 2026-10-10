use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::io::AsHandle;
use std::path::PathBuf;
use std::time::Duration;

use mivia_application::handoff::{self, ACK, Handoff, MAX_MESSAGE_BYTES, OTHER_SESSION};
use mivia_application::launch::Launch;
use mivia_os::{
    ProcessIdentity, accept_pipe_client, create_pipe_instance, current_session_id, pipe_client,
    process_identity,
};
use tokio::sync::mpsc::Sender;

pub(crate) struct Primary {
    pipe: File,
    name: String,
    sid: String,
    own: ProcessIdentity,
    session: u32,
    pub(crate) launch: Launch,
    pub(crate) data_dir: PathBuf,
}

pub(super) struct Claimed {
    pub(super) pipe: File,
    pub(super) name: String,
    pub(super) sid: String,
    pub(super) own: ProcessIdentity,
}

enum Client {
    Same,
    OtherSession,
    Foreign,
}

pub(super) fn create(name: &str, sid: &str) -> io::Result<File> {
    create_pipe_instance(name, sid, true)
}

impl Primary {
    pub(super) fn new(claimed: Claimed, launch: Launch, data_dir: PathBuf) -> io::Result<Self> {
        Ok(Self {
            pipe: claimed.pipe,
            name: claimed.name,
            sid: claimed.sid,
            own: claimed.own,
            session: current_session_id()?,
            launch,
            data_dir,
        })
    }

    pub(crate) fn serve(self, handoffs: Sender<Handoff>) -> io::Result<()> {
        std::thread::Builder::new()
            .name("mivia-gate".to_owned())
            .spawn(move || self.accept_forever(&handoffs))
            .map(drop)
    }

    fn accept_forever(mut self, handoffs: &Sender<Handoff>) {
        loop {
            let connected = accept_pipe_client(&self.pipe);
            let next = create_pipe_instance(&self.name, &self.sid, false);
            match (connected, next) {
                (Ok(()), Ok(next)) => {
                    let client = std::mem::replace(&mut self.pipe, next);
                    self.serve_client(client, handoffs);
                }
                (Err(error), Ok(next)) => {
                    tracing::warn!(%error, "instance.acceptFailed");
                    self.pipe = next;
                }
                (_, Err(error)) => {
                    tracing::error!(%error, "instance.gateError");
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }

    fn serve_client(&self, mut client: File, handoffs: &Sender<Handoff>) {
        match self.classify(&client) {
            Client::Foreign => tracing::warn!(reason = "client_unverified", "instance.gateError"),
            Client::OtherSession => {
                drop(client.write_all(&[OTHER_SESSION]));
                tracing::info!("instance.otherSession");
            }
            Client::Same => {
                let Some(message) = read_message(&mut client) else {
                    tracing::debug!("instance.emptyClient");
                    return;
                };
                drop(client.write_all(&[ACK]));
                if handoffs.blocking_send(message).is_err() {
                    tracing::debug!("instance.receiverClosed");
                }
            }
        }
    }

    fn classify(&self, client: &File) -> Client {
        let Ok((pid, session)) = pipe_client(client.as_handle()) else {
            return Client::Foreign;
        };
        match process_identity(pid) {
            Ok(identity) if identity == self.own && session == self.session => Client::Same,
            Ok(identity) if identity == self.own => Client::OtherSession,
            Ok(_) | Err(_) => Client::Foreign,
        }
    }
}

fn read_message(client: &mut File) -> Option<Handoff> {
    let mut buffer = Vec::with_capacity(MAX_MESSAGE_BYTES);
    let mut chunk = [0u8; MAX_MESSAGE_BYTES];
    while buffer.len() < MAX_MESSAGE_BYTES {
        let read = client.read(&mut chunk).ok().filter(|&read| read > 0)?;
        buffer.extend(chunk.iter().take(read));
        if let Some(message) = handoff::parse(&buffer) {
            return Some(message);
        }
    }
    None
}
