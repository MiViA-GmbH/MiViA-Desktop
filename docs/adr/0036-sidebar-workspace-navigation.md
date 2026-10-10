# Sidebar workspace with registry-driven navigation

The desktop UI is a permanent sidebar workspace (Overview, History, Single images, one row per input folder, Settings) rendered from one navigation array, with one Hold banner and a derived "Needs attention" list, instead of a folder-card dashboard, an activity feed or a tray flyout. It was the strongest of four prototyped layouts, mainly on extensibility, and keeps folder state and holds visible on every page; the cost is more chrome than a one-folder lab needs. The sidebar takes over the dark bar of the web app, so both products look alike.

## Considered Options

- Folder-card dashboard: easiest first day, no home for cross-folder features.
- Activity inbox: best problem visibility, but the feed degrades into a log and needs unread state.
- Tray flyout: lightest, but two navigation systems and no room for features.

## Consequences

- New features add a navigation entry, row actions or a series page section; screens are not restructured.
- Problems surface through "Needs attention", derived from state plus one `attention_cleared_at` per series, set when the series is opened or dismissed.
