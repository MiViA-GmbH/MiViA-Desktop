# Own named-pipe instance gate instead of tauri-plugin-single-instance on Windows

The first statement of `main()` elects the primary by creating the first instance of a per-user named pipe (`FILE_FLAG_FIRST_PIPE_INSTANCE`). Every other process writes its paths to a spool file, hands the spool id over the pipe and exits only after the primary acknowledges it; the primary also ingests leftover spool files at start-up, so no path is lost if it dies. The plugin was rejected because a prototype reproduced its Windows race (plugin issue #3587: 2 of 30 cold 15-file bursts started several full app instances) and lost every forwarded path when the primary died during start-up.

## Considered Options

- tauri-plugin-single-instance as is: simplest, but several full instances (two SQLite writers, two trays) and silent path loss.
- Plugin plus an own pre-mutex, or a fork with its pending fix: keeps the window message transport and its unbounded `SendMessageW`, depends on plugin internals.

## Consequences

- One primary per Windows user across sessions; a second session gets a clear message.
- A small amount of our own code, covered by a burst test in CI; the Win32 calls it needs live in the one module allowed to use unsafe code.
- Secondaries never start Tauri, logging or the database.
- macOS uses `RunEvent::Opened`; Linux gets a Unix socket plus `flock` behind the same port when it ships.
- Pipe names are machine-wide and a user SID is public, so the name also carries a random per-user nonce kept in the app data folder; another Windows user cannot create our pipe first. The pipe rejects remote clients, secondaries connect at identification level only, and the spool inbox lives in the app data folder, not in the install folder.
