# E2E on WebdriverIO 9 + tauri-service against a ServerSim stub

E2E drives the real Tauri binary built with the Cargo feature `e2e` through `@wdio/tauri-service` (embedded provider) against `mivia-stub`, an axum router over `ServerSim` with a fault API. Real-server contact is limited to a live smoke against the test server, run from a developer workstation on the VPN, which also records the golden fixtures. A release-artifact smoke without the feature reaches the stub through the HKLM `ServerUrl` policy.

## Considered Options

- tauri-driver/msedgedriver or Playwright CDP: broken on GitHub's elevated Windows runners since WebView2 150 (wry#1782).
- Self-hosted runner against the test server: slow, uses license, non-deterministic, and not acceptable for a public repository.

## Consequences

- Depends on a small pre-1.0 service pinned to WDIO 9; the exit plan is Playwright `connectOverCDP` on a de-elevated process, exercised by a weekly canary.
