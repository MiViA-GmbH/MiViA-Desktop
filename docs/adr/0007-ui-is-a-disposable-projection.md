# The UI is a disposable projection of Rust state

The webview is not created at autostart. Everything that must survive lives in Rust and SQLite; every window open rebuilds from queries, and events carry a revision so a missed event triggers a refetch.

Closing the window hides it: the WebView2 controller is made invisible, its memory target set to Low and `TrySuspend` called, which stays only if a one-time measurement shows it halves hidden CPU without slowing re-show. The window is destroyed 10 minutes after hide, after 2 minutes if it showed a preview, and at once when Windows signals low memory.

## Considered Options

- Destroy after a short fixed timeout: frees about 155 MB of commit charge, but every return after a short break pays a 0.5 to 0.9 s reopen and a main-thread freeze of up to 0.8 s.
- Pre-warm a hidden window: makes that memory permanent for users who only need the tray.

## Consequences

- No client-state library; view state lives in the route, persistent preferences in Rust settings.
- The idle footprint of the tray app is Rust only; the first open pays a WebView2 cold start (1.0 s at p90).
