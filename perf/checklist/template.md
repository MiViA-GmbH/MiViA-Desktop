# Performance checklist for release VERSION

Filled once per stable release and committed with the release pull request. Signed off by: NAME, DATE.

## Hyper-V virtual machine (clean checkpoint, signed release candidate installed through NSIS)

1. Session end, each with (a) tray-only, (b) window open, (c) an upload in flight: `shutdown /l`; `shutdown /s /t 0` with Fast Startup on and off; `shutdown /r /t 0`. After the next logon: session marker absent, log ends with the shutdown record, `mivia.db-wal` is 0 bytes, recovery touches at most the in-flight item, logged shutdown duration is 3 s or less.
   - Result:
2. Restart Manager: the updater installs N+1 while N runs (`ENDSESSION_CLOSEAPP`). Same checks as item 1, N+1 starts and shows the tray.
   - Result:
3. Autostart: 3 reboots, `start.tray_logon` (`tray_ready` p90 at most 800 ms, process started at most 3 s after `explorer.exe`).
   - Result:
4. Memory pressure: Sysinternals `TestLimit64 -d` until about 1.5 GB is free. Hidden for 10 min, then reopen: the low-memory destroy is logged and reopen p90 is at most twice `win.open`. Preview soak of 60 images while visible: tree hard faults below 50/s sustained over any 1 min window.
   - Result:
5. 24 h soak (72 h before 2.0.0): 1 file/min into 3 folders, polling stub, open and hide every 10 min. App commit Theil-Sen slope at most 100 KB/h after hour 1, threads and handles within 2, database growth consistent with rows, logs within the roll caps.
   - Result:
6. WPR idle trace (docs/perf/etw.md): no wakeups outside the notify thread, no timer-resolution request while tray-only or hidden.
   - Result:

## Physical laptop (4C/8T 2021 mobile CPU, 8 GB, iGPU, NVMe). Until it exists the items read "laptop pending" and do not block.

7. `start.tray_cold`, `win.open`, `win.build_block`, `hist.page_cold`, `prev.render` against the budgets. Laptop to runner ratio per metric recorded in `perf/decisions.md`.
   - Result:
8. Preview on the iGPU: `gpu.mem` and `frame.60hz` on a local console at 60 Hz (not RDP).
   - Result:
9. Battery: 30 min tray-only and 30 min hidden on battery with `powercfg /energy`; `powercfg /sleepstudy` after a Modern Standby night. Sleep and resume: watchers re-armed, a burst written during sleep caught by catch-up within 15 s of resume, uploads resume.
   - Result:

## Update notice (Hyper-V virtual machine, beta N installed through NSIS, update to beta N+1)

10. With automatic updates off, beta N+1 found while the window is closed: an "Update available" toast shows once; the tray menu has "Restart and update to N+1" above "Check for updates"; Settings and the Updates section carry a dot. A later check or a restart shows no second toast.
    - Result:
11. Click the toast after the window was destroyed (hidden for 10 min): the window opens on Settings > Updates.
    - Result:
12. Language German in Settings > General: the whole tray menu and the next toast are German.
    - Result:
13. Restart and update from the tray: after the relaunch an "Updated to N+1." toast shows; "Release notes" and a click on the toast open the release page; no in-app message about the update follows.
    - Result:
14. Click a toast left in the notification center by an earlier session after quitting the app: nothing happens or the app starts plainly.
    - Result:
15. MiViA notifications turned off in Windows Settings: no toast, the tray item and the dot still show.
    - Result:
16. Install with `/NS` (no Start menu shortcut): the toast still shows with the MiViA name and icon.
    - Result:
