# Reports go into a subfolder of the watched folder, excluded from intake by identity

By default reports are saved in `<watched folder>\MiViA reports` (name fixed at folder creation in the UI language), not next to each image and not in a central Documents folder. Because watching is recursive, the report folder is excluded from intake by canonical relative path and file ID, and an unwritable target always asks the user instead of falling back silently.

## Considered Options

- Next to the images (v1): clutter, name collisions, scattered reports; kept only as the opt-in mode "next to the images of each series".
- Central `Documents\MiViA reports`: invisible to colleagues and share backups, blocked by Controlled Folder Access, redirected by OneDrive.
- Sibling of the watched folder: needs write access to the parent.

## Consequences

- Exclusion must match paths across UNC, mapped, subst and junction spellings.
- A downloaded part stays as a staging copy in app data until it is saved. Saving writes a temporary file in the target folder, flushes it and renames it without overwriting, adding a counter on a clash, so Not saved reports survive and nothing is overwritten.
