# Telemetry ids are re-keyed by the server and person links expire

The server stores `install_id` and `machine_id` only as HMACs under a secret pepper. It adds an account pseudonym that is cleared after 90 days, and clears session, trace and span ids after 30 days, while install-level rows stay 400 days for cohort analysis. Identifiers stay stable for analysis, and after 90 days nothing in telemetry links to a person. The pepper can never be rotated without breaking continuity.
