use super::{Pipeline, PipelineError};
use crate::history::{blocks_every_call, remove_on_server};

impl Pipeline {
    pub(super) async fn clean_server(&self) -> Result<(), PipelineError> {
        let due = self
            .write(|tx, scope| tx.pipeline(scope).leftovers())
            .await?;
        for leftover in due {
            let removed = remove_on_server(
                self.ports.server.as_ref(),
                leftover.kind,
                &leftover.server_id,
            )
            .await;
            if let Err(error) = &removed {
                tracing::warn!(?error, "pipeline.leftoverNotDeleted");
                if blocks_every_call(error) {
                    return Ok(());
                }
            }
            let id = leftover.id;
            self.write(move |tx, scope| tx.pipeline(scope).cleaned(id))
                .await?;
        }
        Ok(())
    }
}
