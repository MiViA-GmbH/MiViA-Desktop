# Browser sign-in ends in an access key, with no stored tokens

The app signs in on the MiViA website, in a browser with PKCE and a loopback redirect or with a device code, exchanges the short-lived token for an access key through `POST /api/api-keys/issue`, and drops the token. Everything after that is the existing access key path, so `StoredCredential` and the `Authorizer` don't change. The app finds its client id by asking the server for the fixed app id `mivia-desktop`, because each server generates its own id. This corrects ADR 0018, which expected a new credential variant and a refresh branch.

## Considered Options

- Stored tokens with refresh: the server caps a refresh chain at 30 days, so users would sign in every month even when working daily, while an issued key lasts as long as it is used.
- Client id compiled in, or set in machine policy for other servers: it needs an app update whenever a client is recreated and a policy entry per server.
- Dynamic client registration: every install would become its own client, while clients are registered by an admin.

## Consequences

- An issued key stops working after a password change or reset, turning on two-factor, disconnecting the app, the client being disabled, or 90 days unused; the hold banner offers signing in again.
- The app can't revoke its key without a browser session, so signing out leaves it live on the server until it idles out or is removed on the website.
- Pasted keys keep working and never expire by idling.
