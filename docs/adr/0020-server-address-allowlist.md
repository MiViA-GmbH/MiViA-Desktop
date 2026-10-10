# The server address comes from an allowlist and only machine policy can override it

Release builds talk only to servers on a compiled-in allowlist (`app.mivia.ai`, `test.internal.mivia.ai`). Any other server can be set only through `HKLM\SOFTWARE\Policies\MiViA\Desktop\ServerUrl`; HKCU cannot set it. The old app's `ServerUrl` is ignored unless it is on the allowlist. If users could edit the server field, a planted config file or a scam call could get the access key sent to any host.

## Consequences

- Only https; redirects are never followed and there is no certificate bypass of any kind.
- A non-default server is shown permanently in the window header.
- Developers override the server only in debug builds, through an environment variable.
