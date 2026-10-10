# Desktop telemetry is relayed by the MiViA server into its own ClickHouse table

Desktop clients send typed telemetry events to `POST /api/client-diagnostics/telemetry` with their access key. The server validates each event against a closed schema and inserts the batch synchronously into a typed `desktop_telemetry` table (deduplicated by batch id), which HyperDX reads as a log source. Clients never get an OTLP ingestion key, the client has no OTel SDK, and every metric is a query over events.

## Considered Options

- OTLP logs through the server's HyperDX logger plus a materialized view: rejected, because a failing view breaks all server log ingestion, the exporter queue drops silently, and the data sits in `otel_logs` with a 14-day TTL.
- Converting events to OTel metrics: rejected, because of the 90-day TTL, per-install cardinality and fixed buckets.
- A third-party product analytics service: rejected, because it adds a processor, puts a key in the installer and cannot be joined with server traces.

## Consequences

The server needs a ClickHouse credential and a network path to the ClickHouse host, and the DDL lives outside HyperDX's schema seed. A new event needs a server deploy before the desktop release that sends it.
