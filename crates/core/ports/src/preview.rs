use async_trait::async_trait;
use mivia_domain::path::WinPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    Missing,
    TooLarge,
    Unsupported,
    Corrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewError {
    Unavailable(Unavailable),
    Cancelled,
    Io,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub width: u32,
    pub height: u32,
    pub display: WinPath,
    pub view: Option<WinPath>,
}

#[async_trait]
pub trait PreviewRenderer: Send + Sync {
    async fn render(&self, input: &WinPath) -> Result<Rendered, PreviewError>;
    fn cancel_queued(&self);
    async fn maintain(&self);
    fn tmp_dir(&self) -> WinPath;
}
