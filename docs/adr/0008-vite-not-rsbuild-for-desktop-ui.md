# Desktop UI uses Vite + Vitest + TanStack Router, not the web app's Rsbuild + Rstest + React Router

The desktop UI diverges from the MiViA web toolchain on purpose: Vite is Tauri's documented frontend path, Vitest is stable (Rstest 0.12 is pre-1.0) and unlocks Stryker and the Storybook test addon, and TanStack Router gives typed params with automatic route code splitting. A prototype measured identical bundle size and build time for both bundlers, so the cost is only a second build config for the team to know.

## Considered Options

- Rsbuild 2 + Rstest + React Router (web parity): rejected for the pre-1.0 runner, no mutation testing and an undocumented Tauri setup.
- No router (view state machine): rejected, deep links from tray, toasts and Send to, and five or more screens.
