# Explorer entries are Send to plus a Player verb, registered by the app itself

Release 1 offers "Send to" (one process with all paths), an HKCU verb on the six accepted image extensions with `MultiSelectModel=Player` (one process per file, visible for up to 100 selected files, coalesced by the instance gate) and a folder verb. All carry one name, "Analyze with MiViA". The running app writes these entries and the HKCU `Run` value at every start while the setting is on, and only reads the Task Manager startup switch, never overriding it; the uninstaller only removes them. A native `IExplorerCommand` handler (Windows 11 top-level menu, no limits) stays deferred because it needs a COM server and a signed MSIX identity package.

## Consequences

- Silent IT installs get the same integration on each user's first start.
- Send to fails silently above the 32,767-character command line (about 150 to 350 files); the app accepts folders and tells users to send the folder for large sets.
- The verb is never the double-click default.
