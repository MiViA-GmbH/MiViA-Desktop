# Authentication goes through an Authorizer port

Every API request gets its credential from an `Authorizer` port (`authorize()`, `on_unauthorized(&used)`), backed by one adapter that owns the stored credential and is its only writer. Sign-in methods are separate command handlers. Release 1 supports only the access key. Browser sign-in (Better Auth `oauthProvider` with PKCE and a loopback redirect) comes later as a new credential variant plus a refresh branch, and callers do not change.

## Considered Options

- The HTTP adapter reads the header from settings. Rejected: adding another sign-in method later would touch every caller.
- One `AuthProvider` trait per method, including sign-in. Rejected: sign-in has a different shape for each method, so the trait would leak.
- An auth layer in reqwest-middleware. Rejected: it adds a dependency and covers neither storage nor identity.
