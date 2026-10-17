use mivia_ports::fs::ByteSink;
use mivia_ports::server::{Download, ServerError, UploadRequest};

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

pub(super) async fn drain(mut request: UploadRequest) -> Result<Vec<u8>, ServerError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = request
        .source
        .next_chunk()
        .await
        .map_err(ServerError::Source)?
    {
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
