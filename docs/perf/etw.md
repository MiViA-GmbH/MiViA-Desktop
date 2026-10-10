# Idle wakeups with Windows Performance Recorder

Manual recipe for the idle and hidden-window checks. Run it from an elevated prompt on the test machine.

1. Start recording: `wpr -start GeneralProfile -start CPU -start Power -filemode`.
2. Run the scenario: 5 minutes tray-only, then 5 minutes with the window hidden.
3. Stop: `wpr -stop idle.etl`.
4. In Windows Performance Analyzer open CPU Usage (Precise), group by Process and NewThreadId. Count context switches and ready-thread wakeups per thread of the app. Only the folder watcher thread may wake the process while idle.
5. Open Generic Events and look for Microsoft-Windows-Kernel-Power timer-resolution requests from the app. There must be none while tray-only or hidden.
6. `powercfg /energy /duration 60` lists timer and power-request offenders. On the laptop run `powercfg /sleepstudy` as well.

For the GPU use `wpr -start GPU` plus the counters `\GPU Process Memory(*)\Dedicated Usage` and `Shared Usage`, filtered to the pids of the app tree.
