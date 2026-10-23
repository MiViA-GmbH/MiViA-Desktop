# Performance decisions

One entry per budget breach, waiver or scenario that is not built yet. Each entry names the metric, the measured value, the machine, why, and what happens next.

## Machines

- **workstation**: AMD Ryzen Threadripper 7960X (24 cores, 48 threads), 128 GB, Windows 11 26200, WebView2 154, e2e profile (release without LTO, `custom-protocol,e2e` features). Never CPU-quiet: the harness ran with `--allow-noise`, background load stayed between 5 and 10 % of all cores. Not the reference laptop, so no budget moves on these numbers.
- **reference laptop / VM**: pending (the VM image does not exist yet). Every entry below that says "repeat on the VM" waits for it.

## idle.floor.threads

- Measured: 18 threads at the 20 s settle (0 folders, empty DB, e2e build), 10 threads after about 2 minutes. Budget 12, PR tier.
- Machine: workstation.
- Thread table at the 20 s settle:

| Thread | Created by | Count | Ours | Notes |
|---|---|---|---|---|
| main | tao event loop | 1 | yes | |
| tokio-rt-worker | tokio runtime, `worker_threads(2)` | 2 | yes | spec 4 sets two workers |
| tracing-appender | non-blocking log writer | 1 | yes | |
| mivia-gate | single-instance pipe accept loop | 1 | yes | |
| mivia-session | session, power and network notification window | 1 | yes | |
| unnamed, `tao::event_loop::wait_thread` | tao, timer helper for `ControlFlow::WaitUntil` | 1 | library | started by the event loop, no option to avoid it |
| unnamed, `tokio_rusqlite::event_loop` | store writer connection | 1 | yes | spec 4: one writer on its own thread |
| unnamed, `ntdll!TppWorkerThread` | Windows thread pool workers | 9 | no | 5 at process start (loader and early system calls), 4 at the first network request (WinHTTP proxy lookup and name resolution) |
| unnamed, `mswsock` | Winsock async helper | 1 | no | started by the first socket |

- What changed: the two store reader connections (one thread each) used to open at startup for background reads and stay open for the whole session. Readers are now kept only after the window reported `ui_ready` and are dropped after each read otherwise, as spec 4 describes ("closed when the window is destroyed"). The floor scenario now runs with 0 folders as the budget row defines (the notify thread is gone). Together: 21 to 18 at 20 s, 13 to 10 at steady state.
- Why the rest stays: 10 of the 18 threads are created and retired by Windows. Pool workers retire after 30 s to 2 minutes without work, the count at 20 s depends on that timer, not on the app. Our own threads are 8, inside the budget with margin.
- Decision (owner, 2026-10-11): the budget counts only threads started by the app image. A thread counts when its Win32 start address lies inside the main module of the process; Windows pool workers and the Winsock helper start in system DLLs and do not count. Statically linked crates (tokio, tao, tokio_rusqlite) start inside the exe and count. A thread whose start address cannot be read counts, so the check errs strict. This matches spec D47 (9 expected, including the instance gate accept thread). The budget stays at 12 and the waiver is removed; the run notes keep the total.
- Measured with own-thread counting (workstation, not CPU-quiet): 8 own of 18 total at the 20 s settle. Green.
- Next: repeat on the VM.

## idle.cpu and idle.switches

- Measured (30 min, tray-only, one watched folder): idle.cpu 0.375 s per 30 min (budget 1.8 s), idle.switches 10.47/s (budget 12/s). Green.
- Per thread: notify loop 9.2/s, the two tokio workers 0.7/s and 0.6/s, everything else at most 0.1/s. The row also asks for non-notify threads at 1/s or less; they sum to about 1.4/s.
- What changed: the telemetry writer woke every second even with an empty queue (12.29/s before, tokio workers 1.7/s and 1.3/s). It now sleeps until the first queued event, then persists after 1 s or at 200 events, as the telemetry sink section of the spec describes.
- Next: the remaining tokio wakeups come from the 5 s telemetry sender tick and the pipeline timers; look at them if the VM shows non-notify threads above 1/s. The notify loop rate is inside the notify crate.

## start.tray.median

- Measured: median 211.5 ms, p90 229 ms, n=30 (start.db 11 ms). Budget median 150 ms (two-night), p90 300 ms. First-night breach.
- Machine: workstation, e2e profile without LTO, machine not CPU-quiet (about 7 % of all cores busy).
- Why: about 15 ms pass between process creation and the first log line and about 185 ms between it and `tray_ready`; the Tauri builder (event loop, plugins, tray) runs in that span. The e2e build has no LTO and 16 codegen units; the release profile could not be built here (it requires the production updater keys and signer EKU, a compile-time assert).
- Next: repeat on the VM with the nightly release build. If it stays above 150 ms, split the span with extra markers (builder start, plugins, tray build) before changing code.

## hist.page

- Measured: p95 3.5 ms (max of plain and search pages, 100k batches of 10 files). Budget 5 ms, green.
- Note: with the readers change a history read without a shown window opens and closes its own connection, which costs about 3 ms (6.5 ms p95 when the probe did not keep readers). The probe now keeps readers, like the app once the window reported `ui_ready`. Tray-only reads are rare (startup, backups), so the extra cost there is accepted.

## win.hidden window length

The budget row measures tree CPU over 10 minutes starting 60 s after hide, but the window lifecycle destroys a hidden window 10 minutes after hide. A 10-minute sample therefore ends after the destroy, and the first A/B attempt (30 minutes hidden) failed because the window no longer existed at reshow. `--hidden-minutes` now defaults to 8 (60 s settle plus 8 minutes ends before the destroy). The 30-minute A/B in the ticket cannot run against the product as specified; it ran for 8 minutes per arm.

## TrySuspend A/B

- Method: `cargo xtask perf suspend-ab` (e2e build, seeded profile, window opened through the instance gate, 20 s settle, hidden through `WM_CLOSE`, 60 s settle, then tree CPU time from the process table over 8 minutes; then 10 reshow cycles, request to `shown_painted`). Arm A: TrySuspend on hide (default). Arm B: `MIVIA_E2E_NO_SUSPEND=1`. One run per arm, workstation not CPU-quiet.
- Numbers:

| Arm | Tree CPU while hidden | Tree commit hidden | Reshow p90 |
|---|---|---|---|
| TrySuspend | 0.016 % of one core | 176.6 MB | 29 ms |
| no suspend | 0.042 % of one core | 180.2 MB | 50 ms |

- Decision: keep TrySuspend. CPU drops by 61 % (rule: at least 50 %) and reshow p90 stays at 29 ms (rule: at most 100 ms). Absolute values are tiny (about 80 ms against 200 ms of CPU over 8 minutes), so one noisy run can flip the ratio.
- Next: repeat on the VM once it exists (X12), with three runs per arm.

## ui.scroll.dom and ui.scroll.heap

- Measured (Playwright against the mock build, History with 9 rows): 533 DOM nodes in total, 31 to 36 nodes per row, JS heap 6.6 MB after a forced GC.
- Estimate: about 250 nodes of page chrome plus about 34 per row. One history page holds 100 rows, so the first page alone reaches about 3,650 nodes and every "Show more" adds about 3,400. The 1,500-node budget allows about 36 rows. Heap stays far below 80 MB at these sizes.
- Why not fixed now: both rows are `tbc` (report-only), and windowing the History table is a UI change with focus, keyboard and screen reader work, not a small one.
- UI review (2026-10-12): a smaller page does not meet the budget, because the U2 scenario scrolls through 10,000 rows and every "Show more" keeps the earlier rows in the DOM. Only a windowed list meets it. The windowed History list is planned after release 1, so release 1 ships with "Show more" paging and the budget stays unmet.
- Next: virtualise the History rows after release 1, then build the U2 scenario (mock History with 10,000 rows, scroll to the end, read node count and heap) and move the row out of `tbc`.

## size.exe

Measured by the nightly. The update notice (#117) took the exe from 21.99 to 22.15 MiB at opt-level s. At opt-level z it is 19.95 MiB (local build, same command as the nightly). Hashing, JPEG, resize, preview and SQLite stay at opt-level 3. Local io run at z: burst.5000.ready 18.1 s (18.3 s at s), hist.page 3.2 ms, report.save 56 ms, one run each.

## Scenarios and metrics not built yet

Each row below has no measurement. They stay report-only (`tbc`) or pending until the named work lands; none of them can move a budget.

| Metric | Why not measured | Next |
|---|---|---|
| start.tray_cold | needs a cold file cache (reboot or standby list purge), not possible on a shared workstation | VM nightly with `RAMMap -Et` before the run |
| start.tray_logon | release tier, needs a logon cycle | release checklist on the VM |
| start.armed | needs 10 folders x 10k files seeded through `cargo xtask testdata`; the harness seeds one folder | add multi-folder seeding to the harness |
| idle.exercised | same seeding gap (20 folders x 10k, 100 Computing polled, history read, window destroy) | with the multi-folder seeding |
| idle.timer | `NtQueryTimerResolution` probe not written | add to `mivia_os` probe |
| idle.log_io | needs an hour of tray-only idle and per-file byte counts | read log sizes in the 30 min idle window and scale |
| win.ui_stall, win.build_block telemetry | the `run_on_main_thread` stall watchdog and the `build_block_ms` / `ui_stall_ms` fields in `window.shown` are not built; the harness measures build_block from markers instead | build the watchdog with the window lifecycle work |
| win.ipc_payload, win.ipc_rate | need IPC byte and rate counters in the e2e build | add counters behind the `e2e` feature |
| active.switches | needs a scripted active session (upload plus open window) | with the upload scenario |
| upload.* | the stub has no large-file mode and no bandwidth shaping | extend `mivia-stub` |
| db.commit, db.catchup, db.checkpoint | no store probe for single commits and catch-up yet | ignored store test like the history probe |
| hist.page_cold | needs a cold page cache for the DB file | VM run with a standby purge |
| hist.ipc | needs an IPC round-trip probe through the webview | with the IPC counters |
| io.catchup | needs the catch-up scenario (files added while the app is closed) | with the multi-folder seeding |
| prev.gpu | the preview row "shared GPU memory of the tree" needs GPU counters on an iGPU laptop | release checklist |
| gpu.mem, frame.60hz | release tier, manual on the reference laptop | release checklist |
| bench.instr | gungraun runs on Linux under valgrind; not run on this Windows machine | the `perf-instr` CI job |
| size.installer, size.install | no local NSIS bundle (no Tauri CLI in the proto toolchain); the nightly workflow's `pnpm tauri build` step has no matching script either | nightly or release build job |

## prev.render, prev.cached, prev.mem, prev.soak

- Measured (workstation, `cargo xtask perf preview`, 60 distinct 8192 x 8192 PNGs, 20 of them also reopened): prev.render p90 595 ms (budget 1500), prev.cached p90 160 ms (budget 500), prev.mem max 6.4 MB (budget 250), prev.soak.plateau 130.6 MB (budget 700). Green.
- prev.soak.slope 1.70 MB per open against a budget of 1 (report-only): the last 30 of 60 opens still climb. The private bytes of the tree keep growing while Chromium fills its decoded image cache; the plateau budget has a wide margin at 60 opens.
- How it is measured: a Node helper (`ui/scripts/perf-preview.mjs`) connects to the running app over CDP, clicks the eye icon of an image row and waits for a decoded `asset.localhost` image; the clock is the click to that moment, so it includes the CDP round trip. Memory is the private bytes of the app process tree (`xtask` samples it on request of the helper, before and after each measured open and after every open for the soak).
- Limits: the generated pictures are smooth gradients, so the PNG files are small and the PNG decode is cheaper than for noisy microscope images. The Criterion bench of the renderer alone (release, same machine) gives 848 ms uncached and 35 ms cached for an 8192 x 8192 PNG; the reference laptop is expected to be 3 to 6 times slower, so prev.render is likely red there.
- Next: repeat on the VM and on a 4-core / 8 GB iGPU laptop with a noisy 8k image; if the slope stays above 1 MB per open there, try `createImageBitmap` with `close()` on unmount before raising the budget.

## SetDefaultDllDirectories

`harden_process` keeps `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)` plus the `/DEPENDENTLOADFLAG:0x800` link flag. The idle, startup and window scenarios ran with it and WebView2 loaded normally (win.open measured, webview 0 before the first open), so the smoke passes and the call stays.

## intake_burst `lost: -1`

Closed. The negative loss came from a real double count (a late watcher event for a waiting file of a full series detected it again); `LIVE_MEMBERS` was fixed with a regression test in M8, and the burst runs since report `lost: 0`.
