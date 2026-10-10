# Proxies resolved by WinHTTP, no proxy authentication in release 1

The client asks the Windows WinHTTP resolver (`WinHttpGetProxyForUrlEx`: PAC, WPAD, static and machine settings) for the proxy of each destination and swaps in a freshly built pair of reqwest clients when the answer changes, instead of using reqwest's system-proxy feature, which ignores PAC/WPAD and per-protocol settings. Proxy authentication (Basic, NTLM, Kerberos) is not supported in release 1: a 407 puts the account on a NetworkBlocked hold with text and details for IT, and telemetry of the offered schemes decides when to build an SSPI tunnel.

## Considered Options

- reqwest system proxy only: no PAC/WPAD, breaks on `http=..;https=..` settings.
- WinHTTP as the whole transport: NTLM/Kerberos for free, but drops reqwest/progenitor, streaming multipart and cross-platform parity.
- Own SSPI Negotiate/NTLM CONNECT tunnel now: a security-sensitive multi-leg handshake with an unclear fit to reqwest; deferred to a spike.

## Consequences

Customers behind authenticating proxies need an IT allowlist rule for app.mivia.ai, the storage host and the update hosts until the spike ships.
