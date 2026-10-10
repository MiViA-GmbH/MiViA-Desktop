# Desktop reports opt out of server supersede

The server cancels an account's queued reports on every new report request. Desktop requests send `supersede: false` (a server field persisted per report) so automatic reports neither cancel nor get cancelled by the web app or other installs, with a server cap of 2 such reports in flight per account and a client limit of one per install. Against a server without the field the client sends plain requests and re-requests cancelled parts.
