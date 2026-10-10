use std::time::Duration;

use mivia_application::feed::Coalescer;
use mivia_ports::store::{CommitObserver, Committed};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::cx::Cx;
use super::events::StoreChangedEvent;
use super::window::LABEL;

const COALESCE: Duration = Duration::from_millis(100);

pub(crate) struct Feed(UnboundedSender<Committed>);

impl CommitObserver for Feed {
    fn committed(&self, committed: &Committed) {
        drop(self.0.send(committed.clone()));
    }
}

pub(crate) fn channel() -> (Feed, UnboundedReceiver<Committed>) {
    let (sender, receiver) = unbounded_channel();
    (Feed(sender), receiver)
}

pub(crate) async fn run(app: AppHandle, mut commits: UnboundedReceiver<Committed>) {
    let mut coalescer = Coalescer::default();
    while let Some(first) = commits.recv().await {
        coalescer.push(&first);
        tokio::time::sleep(COALESCE).await;
        while let Ok(next) = commits.try_recv() {
            coalescer.push(&next);
        }
        let Some(changed) = coalescer.take() else {
            continue;
        };
        if let Some(ready) = app.state::<Cx>().get() {
            ready.wake();
        }
        if app.get_webview_window(LABEL).is_some()
            && let Err(error) = StoreChangedEvent(changed).emit(&app)
        {
            tracing::warn!(%error, "ipc.emitFailed");
        }
    }
}
