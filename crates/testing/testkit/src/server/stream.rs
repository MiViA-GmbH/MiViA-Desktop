use mivia_ports::fs::ByteSink;
use mivia_ports::server::{Download, ServerError};

const CHUNK: usize = 16;

pub(super) async fn feed(
    bytes: &[u8],
    mut sink: Box<dyn ByteSink>,
) -> Result<Download, ServerError> {
    for chunk in bytes.chunks(CHUNK) {
        sink.write(chunk.to_vec())
            .await
            .map_err(ServerError::Source)?;
    }
    let staged = sink.finish().await.map_err(ServerError::Source)?;
    Ok(Download::Staged(staged))
}
