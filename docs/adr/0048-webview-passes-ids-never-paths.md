# The webview passes ids, never file paths or URLs

No IPC command takes a file path or URL from the UI. Paths reach Rust only as ids: spool ids from the instance gate, drop ids from the native drag-and-drop event, and pick ids from file and folder dialogs that Rust opens itself. Opening a report, its folder, an original image or a help link takes an id or a fixed link name, and Rust resolves the target. Switching to a key of another account is confirmed in a native message box raised from Rust. The webview gets no opener, dialog, file system, shell, HTTP or process permission. A build check fails when a command input type contains a path or a field named like one.

Any script in the webview (a trojaned npm package, or injected markup) can call every command the UI can. Without this rule such a script could connect the attacker's access key and add the user's documents as a watched folder, uploading lab images to the attacker's account.

## Considered Options

- Tauri isolation pattern: an extra script validates every message, but it runs beside the same compromised bundle, duplicates the Rust validation in JavaScript and encrypts every progress message.
- Validating paths in Rust against a list of allowed folders: the folders the user chooses are arbitrary, so a script could still name any of them.

## Consequences

- File and folder pickers, drag and drop and "open" actions go through small Rust commands instead of plugin calls from the UI.
- The UI cannot be reused against a plain web backend without these commands.
- Adding a command that needs a path is a deliberate, reviewed exception.
