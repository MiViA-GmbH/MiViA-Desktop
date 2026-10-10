# Session end starts shutdown on WM_QUERYENDSESSION and finishes it inside RunEvent::Exit

On Windows logoff, shutdown or a Restart Manager close, tao never raises `ExitRequested`: it delivers `WM_ENDSESSION` as a synchronous `RunEvent::Exit` and then calls `std::process::exit(0)`, so destructors, the tracing guard and any async grace period never run, and the order in which our other windows see the message is not fixed. The app therefore starts its shutdown on `WM_QUERYENDSESSION` from its own hidden top-level window and, inside the `RunEvent::Exit` handler, waits for it with a 3 s cap before checkpointing the WAL and dropping the log guard by hand.

## Considered Options

- Rely on `RunEvent::ExitRequested` and the normal 5 s + 2 s shutdown: never runs on session end.
- Block or veto in `WM_QUERYENDSESSION` (`ShutdownBlockReasonCreate`): delays the user's logoff for a tray app with no unsaved user data.

## Consequences

- Session-end shutdown is a bounded best effort; correctness still rests on the crash-safe states (Submitting rows, per-state recovery, session marker).
- It is verified per release with a real logoff, shutdown, restart and Restart Manager close in a VM.
