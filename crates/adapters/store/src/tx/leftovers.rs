use mivia_ports::store::TxError;
use mivia_ports::store::pipeline::{Leftover, LeftoverKind};
use rusqlite::params;

use super::SqlTx;

const LEFTOVER: &str = "INSERT OR IGNORE INTO server_cleanup (account_id, kind, server_id) \
     SELECT ?1, ?2, ?3 WHERE ?2 = 'job' OR NOT EXISTS (SELECT 1 FROM source_file o \
     JOIN batch b ON b.id = o.batch_id WHERE o.account_id = ?1 AND o.server_image_id = ?3 \
     AND b.deleted_at IS NULL)";

const LEFTOVERS: &str = "SELECT id, kind = 'image', server_id FROM server_cleanup \
     WHERE account_id = ?1 ORDER BY id LIMIT 50";

impl SqlTx<'_> {
    pub(super) fn leftover(
        &self,
        server: bool,
        kind: LeftoverKind,
        id: &str,
    ) -> Result<(), TxError> {
        if !server {
            return Ok(());
        }
        let code = match kind {
            LeftoverKind::Job => "job",
            LeftoverKind::Image => "image",
        };
        self.run(LEFTOVER, params![self.account(), code, id])
    }

    pub(super) fn server_leftovers(&self) -> Result<Vec<Leftover>, TxError> {
        self.rows(LEFTOVERS, [self.account()], |row| {
            let kind = if row.get(1)? {
                LeftoverKind::Image
            } else {
                LeftoverKind::Job
            };
            Ok(Leftover {
                id: row.get(0)?,
                kind,
                server_id: row.get(2)?,
            })
        })
    }
}
