# File readiness is decided by an open probe and a format check, not by quiet time or size

Windows copy engines set the final size before writing and emit no events mid-copy, and an interrupted copy leaves a full-size file with a zero tail. A file in a watched folder is therefore Ready only when an open with `FILE_SHARE_READ` succeeds, its handle metadata is unchanged across hashing, and a JPEG/PNG/TIFF/BMP completeness check passes; quiet time is only a 2 s floor.

## Considered Options

- Quiet period per path: fails on any slow copy.
- Size and mtime stability: fails because of preallocation.
- Exclusive open with share mode 0: blocked by any viewer holding the file.

## Consequences

- The app never holds a blocking handle longer than one probe; hashing and upload share read, write and delete.
- A hand-written format checker must be maintained and property-tested.
