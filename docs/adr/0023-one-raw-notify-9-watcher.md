# One raw notify 9 watcher with our own quiet-period logic

All watched folders share one `notify` watcher using raw events; the intake worker keeps its own per-path quiet-period map with one timer, which batch closing needs anyway. The watcher callback only pushes into a bounded channel of 65,536 entries; a full channel, an overflow reported as `Flag::Rescan`, a watcher error or a resume from sleep each trigger a catch-up scan of that folder, and a periodic reconciliation scan catches the rest.

notify 8.2 ignores ReadDirectoryChangesW overflow and stops watching after network errors without a callback; in a burst test it delivered 925 of 5000 events with no rescan signal. notify 9 (release candidate) reports overflow. notify-debouncer-full was rejected: it walks every watched tree at start to build its file-id cache, adds a thread and periodic wakeups per debouncer, and its rename stitching is redundant because renames are recognised by content fingerprint.

## Consequences

- Depends on a release candidate until 9.0.0 ships; the fallback is notify 8.2 with the rescan fix vendored.
