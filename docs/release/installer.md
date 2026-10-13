# Windows installer

The release ships one per-user NSIS installer, built with `src-tauri/tauri.release.conf.json`. Our additions to the Tauri template live in `packaging/nsis/hooks.nsh`.

## Exit codes

| Code | Meaning | What to do |
|------|---------|------------|
| 0 | Installed or uninstalled | Nothing |
| 1618 | Silent install (`/S`) while MiViA Desktop runs for this user | Retry later. Intune and most deployment tools retry this code on their own |
| 1625 | Started as LocalSystem | Deploy in the user context. The installer is per user and never installs for SYSTEM |
| 1, 2 | Cancelled or failed (NSIS defaults) | Check the user's disk space and antivirus log |

## Behaviour

- `/S` installs silently for the signed-in user. It never starts the app; add `/R` only if you want that.
- The updater runs the installer with `/S /UPDATE`. In that mode a running app is closed instead of returning 1618.
- When setup has not been completed for this user (`HKCU\Software\MiViA\Desktop\SetupCompleted` is not 1), the installer writes the autostart value `HKCU\...\Run\MiViA Desktop` = `"%LOCALAPPDATA%\MiViA Desktop\mivia-desktop.exe" --minimized`, so the setup wizard appears at the next sign-in. The `Autostart` policy (HKLM first, then HKCU) set to 0 or `off` prevents it. After setup the app owns the value and the installer leaves it alone.
- Uninstalling keeps the application data. The checkbox "Delete the application data" (off by default) asks for confirmation and then removes the data folders, the saved access keys and `HKCU\Software\MiViA\Desktop`. An update never removes data.
- Uninstalling always removes the Send to shortcuts, the Explorer entries, the autostart value, the `inbox` folder and `instance.nonce`. When the installer itself runs the old uninstaller during an interactive upgrade, the autostart value is put back afterwards.

## Manual checks on a clean Windows VM

Run these before each stable release; the local test on a developer machine cannot cover them.

1. Silent install as SYSTEM (`psexec -s setup.exe /S`): exit code 1625, nothing installed.
2. Policy `HKLM\Software\Policies\MiViA\Desktop\Autostart` = 0, fresh user, `/S`: no Run value. With 1 or no policy: Run value present, wizard at next sign-in.
3. Interactive install, finish page: the app starts only when its checkbox stays ticked.
4. Interactive uninstall with "Delete the application data": a confirmation appears; No keeps the data, Yes removes the data folders, the credentials `MiViA Desktop/v1/*` and `HKCU\Software\MiViA\Desktop`.
5. Uninstall with a junction inside the data folder that points at a canary folder: the canary survives.
6. Interactive upgrade over an older version with autostart on: autostart is still on afterwards. With autostart turned off in the app: still off.
7. Update through the app (`/S /UPDATE`): data, settings and autostart unchanged, the app comes back minimized.
