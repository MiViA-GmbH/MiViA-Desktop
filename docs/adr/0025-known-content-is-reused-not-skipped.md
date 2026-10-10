# Known content is reused, not skipped

A watched folder keeps a content ledger keyed by fingerprint and model choice. A renamed, moved or re-scanned file is a Known file and is not reported again. A new copy of content the folder has already computed joins its batch and reuses the earlier result, so it appears in that batch's report. Skipping all known content would have left images out of the report the user expected. Keying by path would have recomputed every rename. Neither costs license (the server counts only new uploads), so the rule exists to avoid noise and redundant work.
