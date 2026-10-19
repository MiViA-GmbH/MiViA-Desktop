# MiViA Desktop

A desktop companion to the MiViA image-analysis service. It picks up microscope images from folders on the user's computer (or from Explorer's "Send to"), has the MiViA server analyse them, and saves the reports locally. Server words keep the server's meaning; desktop words differ where the meaning differs.

`_Avoid_` lists are developer language. What the user sees is in **UI wording** at the end.

## Language

### Account and connection

**Server**:
A MiViA installation the app talks to, identified by its address (normally app.mivia.ai).

**Server origin**:
The scheme, host and port of the server the app talks to; it comes from a fixed allowlist or from machine policy, never from a user setting.
_Avoid_: server URL field, endpoint

**Account**:
The MiViA user on a server on whose behalf the app works; it owns everything the app records.
_Avoid_: user (ambiguous with the person at the PC), profile, connection

**Access key**:
The secret that lets the app act for an account.
_Avoid_: token, API key, password

**Credential store**:
Where the app keeps the access key: Windows Credential Manager, local to this PC, never a file.
_Avoid_: vault, keyring (in UI), settings

**Authorizer**:
The single part of the app that hands out the current credential for each request and decides what a rejected credential means.
_Avoid_: auth service, login manager

**Connection**:
The app's current access key for one account and whether the server can be reached with it.
_Avoid_: login, session, account

**Dormant**:
The state of watched folders and history of an account the app is not currently signed in to; kept, not watched.

**License**:
The account's entitlement that the server checks before accepting new images and computations.
_Avoid_: quota, subscription, licence

**Hold**:
An account-wide reason why work waits (offline, service unavailable, network blocked, license, access key, app too old); held work keeps its state and resumes when the reason clears.
_Avoid_: failure, error, pause

**Hold (access key)**:
The hold raised when the server confirms the access key is no longer accepted (deleted, replaced, expired, or the account blocked); lifted by a new key of the same account or a successful re-check.

**Hold (license)**:
The hold raised when the server refuses new images (uploads only) or also new computations (everything) because of the license; lifted when one held item goes through again.

### Models

**Model**:
An analysis the server can run on an image, as offered to the account; same meaning as on the server.
_Avoid_: algorithm, network, compute model

**Customization**:
A named, server-stored preset of settings for one model; same meaning as on the server.
_Avoid_: template, config, profile, preset

**Model choice**:
One model, optionally with one of its customizations; the unit of "what to compute".
_Avoid_: analysis, job type

**Model set**:
The model choices a watched folder applies to new images; no model choice appears twice.

**Model catalogue**:
The models and customizations the account may use, as last seen by the app.

**Unavailable**:
A model choice whose model or customization the account can no longer use.

### Folders and intake

**Watched folder**:
A local folder the app watches for new images, with its own model set, output folder and pause state; its identity is the app's record of it, not its path.
_Avoid_: hot folder, input directory, monitored folder

**Output folder**:
Where reports are saved.

**Report folder mode**:
How a watched folder's output folder is chosen: a subfolder of the watched folder (default), next to the images of each series, or a custom folder.

**Probe**:
The write test the app runs on an output folder when it is set up (works, works without rename, or denied).

**Paused**:
A watched folder that takes in nothing new; work already taken in still finishes.
_Avoid_: stopped, disabled, hold

**Missing folder**:
A watched folder whose path no longer exists or cannot be read.

**Waiting for network folder**:
A watched folder on a network path that is not reachable yet; it becomes a Missing folder after 10 minutes.

**Relocate**:
Pointing a watched folder at a new path while keeping its settings, content ledger and history.

**Baseline**:
The images already in a folder when it became a watched folder; not computed unless the user asks.

**Catch-up scan**:
The look through a watched folder at start-up or resume for images that arrived while the app was not watching.

**Reconciliation scan**:
A periodic catch-up scan of an active watched folder that also notices a watcher that stopped and a folder that moved.

**Watch marker**:
A small file each installation writes into a watched folder's output folder, so a second installation watching the same folder is noticed.

**Ignored file**:
A file the app never takes in: not an image type, a temporary copy, or a file the app wrote itself.

**Content ledger**:
A watched folder's record of which content it has computed with which model choice, and where that content was seen.

### Images and batches

**Source file**:
One image file on the user's disk that the app took in, known by its path and fingerprint.
_Avoid_: image (that is the server record), upload

**Fingerprint**:
The content hash of a source file; equal to the server's image hash, so equal fingerprints mean the same image. Only files have fingerprints.
_Avoid_: checksum, ID, error fingerprint (see Error signature)

**Image**:
The server's record of one picture per account per content; several source files can share one image.
_Avoid_: file, picture

**Copy in batch**:
A source file with the same fingerprint as an earlier source file of the same batch; it shares that file's computations.
_Avoid_: duplicate (the server uses that word for re-uploaded content)

**Known file**:
A source file whose content its watched folder already computed at this path or at a path it has since left (rename, move, re-scan); recorded, not computed.
_Avoid_: duplicate, already processed

**Known content**:
A new source file whose content its watched folder already computed from a file that still exists; it joins the batch and reuses the earlier result.

**Rejected file**:
A source file the server refused as an image (for example too small) or that is larger than the upload limit; it gets no computations.

**Batch**:
The source files that arrived together from one origin and are reported together.
_Avoid_: job, project, session, drop

**Batch key**:
A watched folder plus its top-level subfolder (or none); at most one open batch exists per batch key.

**Origin**:
Where a batch came from: one watched folder, or one one-off request.

**Batch label**:
The human name of a batch, taken from its subfolder or first file.

**Series name rule**:
How a watched folder names batches of loose files: automatic (common start of the file names), first to last file name, or the folder name; a subfolder name always wins.

**Quiet period**:
The per-folder time without new or changing images after which an open batch closes (1, 5, 15 or 30 minutes, or only by hand).

**Closure reason**:
Why a batch stopped accepting files: quiet period, size or age cap, manual, restart, account switch, or one-off.

**One-off request**:
Images handed to the app through "Send to", the context menu, drag and drop or "Add images", with model choices picked at that moment; it forms one batch.
_Avoid_: quick job, ad-hoc job

**Pending one-off request**:
The one unconfirmed one-off request of the account; every further hand-over joins it until the user analyzes or cancels.

**Spool file**:
A file in the app's inbox holding paths that a second app process handed over; read once, so no path is lost if the app stops.

**Instance gate**:
The per-user election at start that makes one process the app and lets every other process hand its paths to it and exit.

### Computing

**Computation**:
One source file computed with one model choice inside one batch; the desktop's unit of work.
_Avoid_: job (that is the server record), task, analysis

**Job**:
The server's record of one image computed with one model choice; a computation refers to at most one current job.
_Avoid_: computation, task

**Result**:
The server's output for one image and one model choice (measurements and masks), shareable between jobs.
_Avoid_: output, report

**Reused result**:
A result that already existed, taken over instead of computing again; the computation counts as done.
_Avoid_: cache hit, cached job

**Run again**:
A new computation the user asks for on purpose, forcing the server to compute even when a result exists; the old computation becomes superseded.
_Avoid_: retry (that is automatic), recompute

**Operation**:
A long-running piece of app work started by the user (building a problem report, importing old settings, exporting history) that reports progress and can be cancelled; not stored, gone after a restart.
_Avoid_: job, task, Rust job

### Reports and history

**Report**:
The PDF or CSV output for an ordered selection of Done computations of one model choice: either a batch (batch report) or a selection the user made in history (Custom report).
_Avoid_: result, export

**Custom report**:
A report over Done computations the user selected in history, possibly from several batches (at most 1000 images).

**Report settings**:
Format, language and decimal separator of a report; a watched folder's settings are copied into each batch when it opens.

**Report part**:
One of the numbered files a report is split into when it covers more computations than the server allows in one report (100).

**Summary page**:
The first page of a PDF report, listing every image of its selection with its outcome.

**Not included**:
Images of a report's selection that are not Done (failed, rejected, unavailable, lost, cancelled); they are listed on the summary page.

**Staging copy**:
A downloaded report part kept in app data until it is saved to the output folder.

**Clash counter**:
The ` (n)` added to a report file name only when that name is already taken; existing files are never overwritten.

**Stale report**:
A saved report one of whose computations was later run again.

**Replaced report**:
A stale report the user moved into the `Replaced` subfolder after its update was saved; besides deleting report files the user asked to delete together with their results, the only change the app ever makes to a saved report.

**History**:
The app's own lasting record of batches, their computations and reports, kept on this PC only.
_Avoid_: log, jobs list

**History entry**:
One batch as shown in history.

**Deleted from history**:
An image or image series the user deleted in history: hidden everywhere, its running work cancelled, optionally with its report files and its jobs on the server; the change log keeps a record.
_Avoid_: removed, purged

**History event**:
One insert-only entry of the action log: time, time zone, Windows user, action, trigger, before and after.
_Avoid_: audit log row, change

**Attention cleared**:
The moment a batch left "Needs attention" because the user opened it or dismissed it; never called reviewed or approved.

**Diagnostic log**:
The technical record of what the app did, for troubleshooting.
_Avoid_: history, activity log

**Failure**:
A permanent problem with one item, shown with its reason; temporary problems and holds are not failures.
_Avoid_: error (too broad)

### Diagnostics and telemetry

**Problem report**:
A package of diagnostic data and an optional description that the user previews and sends to MiViA support; the only way names or paths may leave the PC, and only if the user opts in.
_Avoid_: issue report, bug report, crash report

**Report bundle**:
The zip file built for one problem report; what the user previews is byte for byte what is sent. Not a Report.

**Outbox**:
Problem reports waiting to be sent, retried for up to 7 days.

**Error signature**:
The key `<kind>:<code>:<call site>` that identifies a kind of error, so the app offers a problem report once per signature; never contains message text.
_Avoid_: fingerprint (that is the file hash)

**Reportable error**:
An error kind that is unexpected (internal, contract, crash, repeated server error) and therefore offers a problem report; expected ones (offline, access key, license, file locked) never do.

**Telemetry event**:
One typed, pseudonymous record of something the app did (no names, paths or free text), buffered locally and sent in batches while telemetry is on.
_Avoid_: analytics event, log line, metric

**Field list**:
The generated, versioned list of every telemetry event and field with purpose, category and retention; shown to the user as "What is sent".

**Effective telemetry default**:
Machine policy if set, else the account's default from the server, else on; the user's own choice overrides the defaults but never the policy.

**install_pid / account_pid / org_pid**:
Keyed pseudonyms the server derives from the installation, account and organisation ids before storing telemetry; account_pid is cleared after 90 days. Never shown in the UI.

### Preview

**Preview tier**:
A JPEG the app renders from a source file or result image at a fixed long edge (2048 px, and 4096 px for larger pictures); the only form in which pictures reach the window.

**Preview cache**:
The app-data folder of preview tiers, keyed by content, limited to 512 MiB and safe to delete.

**Renderer version**:
The version of the tier renderer, part of the preview cache path; changing it discards old tiers.

### Updates

**Update channel**:
The stream of releases an installation follows: stable (default) or beta.

**Promotion**:
Making a published version the current one on a channel.

**Beta soak**:
The at least 48 hours a version spends on the beta channel before it may be promoted to stable.

**Pending install**:
A downloaded and verified update waiting for a moment to install (user request, quit, or the first idle moment 24 hours later).

**Update notice**:
Telling the user that a newer version can be installed now, once per version; the user decides when, nothing stops working. A Forced update is not an Update notice.

**Forced update**:
An update the app installs as soon as no upload is in flight, because the server requires a newer minimum version and a signed update satisfies it.

### States

**Source file states**:
Detected (seen, possibly still being copied) -> Ready (complete and fingerprinted) -> Uploading -> Uploaded; or Copy in batch, Known file, Known content, Rejected, Changed (bytes changed after fingerprinting, checked again), Vanished (gone before upload, or removed while its batch is open), Unreadable (still locked after waiting, cloud-only, or no permission).

**Computation states**:
Queued -> Submitting -> Computing -> Done; or Failed, Unavailable, Lost (job gone from the server), Cancelled, Superseded.

**Batch states**:
Open or Closed; whether it is computing, reporting, finished or finished with problems follows from its computations and reports.

**Report states**:
Waiting (for its computations) -> Requested -> Rendering -> Saved; or Nothing to report, Failed, Not saved (made but not writable to the output folder), Stale. The server does not yet tell queued from rendering, so Requested and Rendering show as one state.

## Relationships

- An **Account** belongs to one **Server**; the app has one **Connection** at a time, to one account. Watched folders, batches and history belong to the **Account**.
- A **Watched folder** has one **Model set**, one **Output folder**, one **Content ledger** and many **Batches**; watched folders never lie inside each other or inside an output folder.
- A **Batch** has one **Origin**, one or more **Source files**, a copy of the model set and the report settings taken when it opened, and one **Report** per model choice in that copy.
- A **Source file** belongs to exactly one **Batch** and maps to at most one **Image**.
- A **Computation** is one **Source file** x one **Model choice**, belongs to one **Batch**, and refers to at most one current **Job**; a **Job** has at most one **Result**.
- A batch **Report** covers only computations of its own **Batch** and **Model choice**; a **Custom report** covers selected computations of one **Model choice** from any batches of the account. Every report has one or more **Report parts**.
- A **Hold** stops work for the whole **Account**; a **Failure** stops one item.

## Example dialogue

> **Dev:** "The lab copied 40 images into the folder and two of them are byte-identical copies."
> **Domain expert:** "That is one **Batch** with 40 **Source files**; one is a **Copy in batch**, so there are 39 **Computations** per **Model choice**, and each model choice gets one **Report**."
> **Dev:** "Next week they copy the same 40 under new names, and the old ones are still there."
> **Domain expert:** "That is **Known content**: a new batch, 40 computations that take over the earlier **Results** without server work, and a report like any other. If they had only renamed the old files, those would be **Known files** and nothing would be reported."

## Flagged ambiguities

- "Job" meant both the local work item and the server record in the old client; resolved: **Computation** is local, **Job** is the server record.
- "Image" meant both the file on disk and the server record; resolved: **Source file** vs **Image**.
- The server's upload flag "duplicate" means the account already had that content; the desktop does not use the word.
- The server uses "batch" internally for one job request; unrelated to the desktop **Batch**.
- The web app's **Project** groups jobs; it is not a **Batch**.
- The server's job status `NEW` means the job has finished: the computation is Done, not new.
- "Fingerprint" is only the file content hash; the key that groups errors is the **Error signature**.
- "Report" alone means the analysis output; a **Problem report** goes to support and its zip is a **Report bundle**.

## UI wording

Model and Customization use the MiViA web app's words so both products speak alike; German UI uses *Sie*, English UI is en-US.

| Term | en | de |
|---|---|---|
| Watched folder | Input folder | Eingangsordner |
| Output folder | Report folder | Berichtsordner |
| Model | Model | Modell |
| Customization | Customization | Benutzerdefinierte Konfiguration |
| Model set | Models for this folder | Modelle für diesen Ordner |
| Unavailable | Model no longer available | Modell nicht mehr verfügbar |
| Paused | Paused | Pausiert |
| Missing folder | Folder not found | Ordner nicht gefunden |
| Waiting for network folder | Waiting for network folder | Wartet auf Netzwerkordner |
| Relocate | Choose new location | Neuen Ort wählen |
| Baseline | Existing images | Vorhandene Bilder |
| Source file | Image | Bild |
| Copy in batch / Known content / Known file | Same image as <file> (analyzed <date>) | Gleiches Bild wie <Datei> (ausgewertet am <Datum>) |
| Rejected file | Image not usable | Bild nicht verwendbar |
| Batch | Image series | Bildserie |
| One-off request | Single images | Einzelbilder |
| Computation | Evaluation | Auswertung |
| Reused result | Done (result from <date> taken over) | Fertig (Ergebnis vom <Datum> übernommen) |
| Run again | Analyze again | Erneut auswerten |
| Report | Report | Bericht |
| Report part | Report, part <n> of <m> | Bericht, Teil <n> von <m> |
| Custom report | Custom report | Eigener Bericht |
| Not included | Not included | Nicht enthalten |
| Replaced report | Replaced | Ersetzt |
| History | History | Verlauf |
| Diagnostic log | Technical log (for support) | Technisches Protokoll (für Support) |
| Problem report | Report a problem | Problem melden |
| Account | Account | Konto |
| Access key | Access key | Zugangsschlüssel |
| License | License | Lizenz |
| Dormant | Not watched (other account) | Nicht überwacht (anderes Konto) |
| Sign out | Sign out | Abmelden |
| Replace access key | Replace access key | Zugangsschlüssel ersetzen |
| Hold (offline) | Waiting for connection | Wartet auf Verbindung |
| Hold (service unavailable) | MiViA service unavailable, waiting | MiViA-Dienst nicht verfügbar, wartet |
| Hold (network blocked) | Network blocks MiViA | Netzwerk blockiert MiViA |
| Hold (access key) | Waiting for a valid access key | Wartet auf gültigen Zugangsschlüssel |
| Hold (access key, account blocked) | Your MiViA account is blocked. Contact MiViA support. | Ihr MiViA-Konto ist gesperrt. Wenden Sie sich an den MiViA-Support. |
| Hold (license) | Waiting for license | Wartet auf Lizenz |
| Hold recovery action | Check connection now | Verbindung jetzt prüfen |
| License Hold action | Ask the MiViA contact in your lab | MiViA-Ansprechperson im Labor fragen |
| Detected | Being copied | Wird kopiert |
| Ready, Queued | Waiting | Wartet |
| Uploading | Uploading | Wird hochgeladen |
| Submitting, Computing | Analyzing | Wird ausgewertet |
| Done | Done | Fertig |
| Failed | Analysis not possible | Auswertung nicht möglich |
| Cancelled | Cancelled | Abgebrochen |
| Open batch | Collecting images | Bilder werden gesammelt |
| Report Requested, Rendering | Creating report | Bericht wird erstellt |
| Report Saved | Saved | Gespeichert |
| Report Failed | Report could not be created | Bericht konnte nicht erstellt werden |
| Report Not saved | Report ready, but not saved to <folder> | Bericht fertig, aber nicht in <Ordner> gespeichert |
| Stale report | Newer results available | Neuere Ergebnisse vorhanden |
| Batch with no Done computation | No report: no image of this series could be analyzed | Kein Bericht: kein Bild dieser Bildserie auswertbar |
| Report again | Create report again | Bericht neu erstellen |
| Needs attention | Needs attention | Bitte prüfen |
| Overview | Overview | Übersicht |
| Close batch now | Create report now (hint: otherwise the report follows automatically after the quiet period) | Bericht jetzt erstellen |
| Empty Model set | No model available for this folder | Kein Modell für diesen Ordner verfügbar |
| Dismiss | Dismiss | Ausblenden |
| Explorer entry (Send to shortcut, context-menu verbs, one name everywhere) | Analyze with MiViA | Mit MiViA auswerten |
| Upload cost line | Uses up to <n> of your <remaining> remaining image uploads. | Verbraucht bis zu <n> Ihrer <remaining> verbleibenden Bild-Uploads. |
| Reused result cost note | No extra license use | Kein zusätzlicher Lizenzverbrauch |
| The .NET client | old MiViA Desktop | altes MiViA Desktop |
| First run wizard | Setup | Einrichtung |
| Wizard commit button | Finish setup | Einrichtung abschließen |
| Policy-locked setting | Set by your IT department | Von Ihrer IT-Abteilung festgelegt |
| Update channel | Update channel: Stable / Beta | Update-Kanal: Stabil / Beta |
| Pending install action | Restart and update | Neu starten und aktualisieren |
| Forced update | Update required | Update erforderlich |

Never shown: fingerprint, hash, job, batch, computation, model choice, model set, baseline, catch-up scan, origin, hold, dormant, token, server status names, error signature. The upload cost line counts image uploads, never analyses, and the app never says "charge".
