# The access key is kept only in Windows Credential Manager, with Local persistence

The access key is stored as a generic credential with target `MiViA Desktop/v1/<origin>` and Local persistence. The store's default is Enterprise, which would copy the key to every PC a roaming profile signs in to, while the app data stays on one machine. There is no plaintext fallback: if Credential Manager fails, the key is held in memory for the current session only.

## Consequences

- Malware running as the same Windows user can still read the key. This risk is accepted.
- Users with roaming profiles enter the key once on each PC.
