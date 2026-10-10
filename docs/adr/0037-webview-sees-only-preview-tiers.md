# The webview only ever sees Rust-rendered preview tiers, at most 4096 px

Source files and result images never reach WebView2 at full resolution. Rust renders a 2048 px display tier and, for larger pictures, a 4096 px view tier as JPEG into the preview cache, and the asset protocol serves only that directory. Measured: a full-resolution 8k PNG cost 2.3 s and +550 MB private memory that stayed after close, and a 23k JPEG froze zoom for 1.5 s, while a cached 2048 tier opens in 188 ms with +95 MB. Preview is a secondary action; nothing renders until the user opens it.

## Considered Options

- Full-resolution file through the asset protocol: no Rust code, but the memory and stalls above, and Tauri copies the whole file into memory per request.
- Tile pyramid + OpenSeadragon: true 1:1 zoom, but 3 to 4 s and about 340 files per 8k image and a second viewer stack. It stays the upgrade path if usage data shows people zooming at the largest tier.

## Consequences

- No 1:1 detail above 4096 px; "Open original" hands the file to the OS viewer.
- The result image endpoint must become public contract, because the desktop downloads result images to render tiers.
