# Accept cryptbase.dll loaded from the install folder

The release artifact smoke plants copies of 12 system DLLs next to the installed exe and in the Downloads folder. 11 are never loaded. `cryptbase.dll` is loaded from the install folder by the primary and the Send to process a few milliseconds after start, before `main()` calls `harden_process()`. `/DEPENDENTLOADFLAG:0x800` is set but does not cover it: advapi32 and winhttp delay-load cryptbase. A planted copy without the expected exports makes the app exit at start with 0xC0000139. We accept this and list `cryptbase` in `ACCEPTED_LOADS` of the smoke, so the other 11 names stay checked.

The install folder is `%LOCALAPPDATA%\MiViA Desktop`, writable by the user, and the beta installer is not signed (D56). Whoever can write a DLL there can replace `mivia-desktop.exe` itself, so the planted DLL gives nothing extra. The Windows loader picks a DLL by name and search order, never by content, so a hash cannot help: it could only be checked after the code has run.

## Considered Options

- Load advapi32 and winhttp only after hardening, or set a signature-only image policy before the first load: needs the exe built so that nothing loads before `main()`, which is research with unclear outcome, for a risk the writable folder already makes moot.
- Leave the nightly smoke red: blocks a check that otherwise guards 11 real cases.
- Drop `cryptbase` from the canaries: loses the trace that shows when this load changes.

## Consequences

- The nightly `Release artifact smoke` no longer fails on `cryptbase`.
- Review this decision when a per-machine install exists or when code signing is introduced, which is planned before stable.
