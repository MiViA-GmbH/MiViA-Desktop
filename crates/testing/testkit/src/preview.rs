use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use mivia_domain::path::WinPath;
use mivia_ports::preview::{PreviewError, PreviewRenderer, Rendered};
use parking_lot::Mutex;

pub struct FakeRenderer {
    tmp: WinPath,
    rendered: Mutex<Vec<WinPath>>,
    outcome: Mutex<Result<(u32, u32), PreviewError>>,
    cancels: AtomicU32,
}

impl FakeRenderer {
    pub fn new(tmp: WinPath) -> Self {
        Self {
            tmp,
            rendered: Mutex::new(Vec::new()),
            outcome: Mutex::new(Ok((2048, 1536))),
            cancels: AtomicU32::new(0),
        }
    }

    pub fn answer(&self, outcome: Result<(u32, u32), PreviewError>) {
        *self.outcome.lock() = outcome;
    }

    pub fn rendered(&self) -> Vec<WinPath> {
        self.rendered.lock().clone()
    }

    pub fn cancels(&self) -> u32 {
        self.cancels.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PreviewRenderer for FakeRenderer {
    async fn render(&self, input: &WinPath) -> Result<Rendered, PreviewError> {
        self.rendered.lock().push(input.clone());
        let (width, height) = (*self.outcome.lock())?;
        let tier = |name: &str| {
            self.tmp
                .parent()
                .and_then(|root| root.join("v1").ok())
                .and_then(|dir| dir.join(name).ok())
                .ok_or(PreviewError::Io)
        };
        Ok(Rendered {
            width,
            height,
            display: tier("fake-display.jpg")?,
            view: (width.max(height) > 2560)
                .then(|| tier("fake-view.jpg"))
                .transpose()?,
        })
    }

    fn cancel_queued(&self) {
        self.cancels.fetch_add(1, Ordering::SeqCst);
    }

    async fn maintain(&self) {}

    fn tmp_dir(&self) -> WinPath {
        self.tmp.clone()
    }
}
