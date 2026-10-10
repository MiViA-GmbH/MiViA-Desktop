use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::io::AsHandle;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mivia_application::handoff::OTHER_SESSION;
use mivia_os::{
    ProcessIdentity, allow_foreground, open_pipe_client, pipe_server_pid, process_identity,
};

const ERROR_PIPE_BUSY: i32 = 231;
const CONNECT_BUDGET: Duration = Duration::from_secs(5);
const ACK_WAIT: Duration = Duration::from_secs(10);

pub(super) enum Reply {
    Delivered,
    OtherSession,
}

pub(super) enum Refused {
    Unreachable,
    Unverified,
}

pub(super) fn forward(name: &str, own: &ProcessIdentity, message: &[u8]) -> Result<Reply, Refused> {
    let mut pipe = connect(name).map_err(|_| Refused::Unreachable)?;
    let server = pipe_server_pid(pipe.as_handle()).map_err(|_| Refused::Unreachable)?;
    let identity = process_identity(server).map_err(|_| Refused::Unreachable)?;
    if identity != *own {
        return Err(Refused::Unverified);
    }
    allow_foreground(server);
    pipe.write_all(message).map_err(|_| Refused::Unreachable)?;
    match await_reply(pipe) {
        Ok(Some(OTHER_SESSION)) => Ok(Reply::OtherSession),
        Ok(_) => Ok(Reply::Delivered),
        Err(_) => Err(Refused::Unreachable),
    }
}

fn connect(name: &str) -> io::Result<File> {
    let deadline = Instant::now() + CONNECT_BUDGET;
    loop {
        match open_pipe_client(name) {
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            opened => return opened,
        }
    }
}

fn await_reply(mut pipe: File) -> io::Result<Option<u8>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reply = [0u8; 1];
        drop(sender.send(pipe.read_exact(&mut reply).map(|()| reply[0])));
    });
    match receiver.recv_timeout(ACK_WAIT) {
        Ok(reply) => reply.map(Some),
        Err(_) => Ok(None),
    }
}
