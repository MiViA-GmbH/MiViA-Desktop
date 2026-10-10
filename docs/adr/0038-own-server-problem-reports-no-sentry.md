# Problem reports go to our own server, identifying data only in them, no Sentry

The client writes rich structured logs locally. Pseudonymous telemetry (a stable `machine_id` and `install_id` pair) is on by default with an opt-out. Identifying data (file names, paths) leaves the machine only inside a problem report: the user sends it, previews it first, and has to opt in to include names. The report goes to an access-key endpoint under `/api/client-diagnostics` on app.mivia.ai, which keeps it for 90 days and mails support. Clients never get a third-party SDK or an ingestion key.

## Considered Options

- Sentry SaaS EU. Rejected: a third-party processor, attachments are not scrubbed, its crash-reporter process clashes with Send to argv, and it adds idle cost in the tray.
- Direct OTLP export to HyperDX. Rejected: the ingestion key would ship in every installer, and the collector would have to be public.
- GitHub issues. Rejected: public, needs a GitHub account and a token shipped in the client.
- mailto only. Kept as the offline, no-key and no-endpoint fallback.

## Consequences

- Native crashes produce no symbolicated minidumps. They are detected through crash files, WebView2 `ProcessFailed` and an unclean-shutdown marker.
- MachineGuid is read only while telemetry is on or when a report is sent. The telemetry default follows the legal assessment under TDDDG §25; the code supports on by default, on by organisational consent, or off with separate switches for diagnostics and usage.
