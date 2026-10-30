use mivia_domain::batch::ClosureReason;
use mivia_domain::hold::{HoldReason, KeyProblem, LicenseScope, NetworkBlock};
use mivia_domain::ids::{BatchId, ComputationId, FolderId, ReportId};
use uuid::Uuid;

use super::{Field, FieldKind, Value, Version};
use crate::ipc::privacy::PrivacyPanel;
use crate::ipc::screens::LanguageSetting;
use crate::ipc::setup::{ArrivalPreset, ReportMode, SetupStep};

tag_values! {
    SetupStep {
        SetupStep::Welcome => "welcome",
        SetupStep::Legacy => "legacy",
        SetupStep::Key => "key",
        SetupStep::Folder => "folder",
        SetupStep::Series => "series",
        SetupStep::Models => "models",
        SetupStep::Finish => "finish",
    }
    ArrivalPreset {
        ArrivalPreset::Microscope => "microscope",
        ArrivalPreset::WholeFolder => "whole_folder",
        ArrivalPreset::ByHand => "by_hand",
    }
    ReportMode {
        ReportMode::Subfolder => "subfolder",
        ReportMode::Beside => "beside",
        ReportMode::Custom => "custom",
    }
    LanguageSetting {
        LanguageSetting::Auto => "auto",
        LanguageSetting::En => "en",
        LanguageSetting::De => "de",
    }
    PrivacyPanel {
        PrivacyPanel::WhatIsSent => "what_is_sent",
        PrivacyPanel::ViewSentData => "view_sent_data",
    }
    ClosureReason {
        ClosureReason::QuietPeriod => "quiet_period",
        ClosureReason::Cap => "cap",
        ClosureReason::Manual => "manual",
        ClosureReason::Restart => "restart",
        ClosureReason::AccountSwitch => "account_switch",
        ClosureReason::OneOff => "one_off",
    }
    HoldReason {
        HoldReason::Offline => "offline",
        HoldReason::ServiceUnavailable => "service_unavailable",
        HoldReason::NetworkBlocked(NetworkBlock::Tls) => "tls",
        HoldReason::NetworkBlocked(NetworkBlock::ProxyAuth) => "proxy_auth",
        HoldReason::NetworkBlocked(NetworkBlock::ProxyRefused) => "proxy_refused",
        HoldReason::NetworkBlocked(NetworkBlock::Redirect) => "redirect",
        HoldReason::AccessKeyInvalid(KeyProblem::Invalid) => "key_invalid",
        HoldReason::AccessKeyInvalid(KeyProblem::AccountBlocked) => "account_blocked",
        HoldReason::AccessKeyInvalid(KeyProblem::AdminScope) => "admin_scope",
        HoldReason::AccessKeyInvalid(KeyProblem::MissingScopes) => "missing_scopes",
        HoldReason::LicenseInvalid(LicenseScope::Uploads) => "license_uploads",
        HoldReason::LicenseInvalid(LicenseScope::All) => "license_all",
        HoldReason::ClientTooOld => "client_too_old",
    }
}

tags! {
    StartState { Autostart "autostart", User "user", Sendto "sendto", UpdateRelaunch "update_relaunch", Wizard "wizard" }
    ProxyKind { Direct "direct", Static "static", Pac "pac", Wpad "wpad", Env "env", Policy "policy" }
    StopState { Quit "quit", Update "update", OsShutdown "os_shutdown", AccountSwitch "account_switch" }
    CrashKind { Panic "panic", Webview "webview", TaskPanic "task_panic" }
    WizardErrorKind { Dns "dns", Connect "connect", Timeout "timeout", Tls "tls", ProxyAuth "proxy_auth", Redirect "redirect", Http4xx "http_4xx", Http5xx "http_5xx", KeyInvalid "key_invalid", License "license", Folder "folder" }
    FinishPreset { Microscope "microscope", WholeFolder "whole_folder", ByHand "by_hand", None "none" }
    FinishReportMode { Subfolder "subfolder", Beside "beside", Custom "custom", None "none" }
    ExistingImages { OnlyNew "only_new", AlsoExisting "also_existing", None "none" }
    AccountKind { AccessKey "access_key", Browser "browser" }
    AccountState { User "user", KeyInvalid "key_invalid", KeyRemoved "key_removed" }
    MigrationState { Done "done", Pending "pending", Failed "failed" }
    FolderVia { Wizard "wizard", Settings "settings", EmptyState "empty_state" }
    FolderState { Ok "ok", Missing "missing", WaitingForNetwork "waiting_for_network" }
    DriveType { Local "local", Removable "removable", Network "network", Cloud "cloud" }
    RescanReason { Overflow "overflow", Error "error", Resume "resume", ChannelFull "channel_full" }
    CatchupState { Analyze "analyze", Skip "skip" }
    Origin { Folder "folder", OneOff "one_off" }
    UploadState { Rejected "rejected", Changed "changed", Vanished "vanished", Unreadable "unreadable", KeepsBreaking "keeps_breaking" }
    ImageFormat { Tif "tif", Png "png", Jpg "jpg", Bmp "bmp", Other "other" }
    SubmitState { Ok "ok", Ambiguous "ambiguous", Error "error" }
    SettledState { Done "done", Failed "failed", Lost "lost", Cancelled "cancelled", Unavailable "unavailable", Superseded "superseded" }
    AttentionKind { FailedImages "failed_images", ReportProblem "report_problem", ReportNotSaved "report_not_saved", Other "other" }
    ClearedHow { Resolved "resolved", Dismissed "dismissed", Opened "opened" }
    ReportKind { Auto "auto", Custom "custom" }
    ReportState { Saved "saved", Failed "failed", NotSaved "not_saved", NothingToReport "nothing_to_report" }
    ReportReason { Unreachable "unreachable", Tls "tls", Proxy "proxy", Timeout "timeout", Server "server", TooLarge "too_large", WriteDenied "write_denied", DiskFull "disk_full", PathTooLong "path_too_long", Other "other" }
    ReportFormat { Pdf "pdf", Csv "csv", Xlsx "xlsx", Other "other" }
    OpenedFrom { Toast "toast", History "history", Tray "tray", Series "series" }
    LiftedBy { Probe "probe", Canary "canary", User "user", Update "update" }
    ErrorKind { Validation "validation", NotFound "not_found", Conflict "conflict", Hold "hold", DiskFull "disk_full", StorageUnavailable "storage_unavailable", Contract "contract", Internal "internal", Panic "panic" }
    CheckState { None "none", Found "found", Error "error" }
    CheckTrigger { Schedule "schedule", Manual "manual", Start "start" }
    DeferReason { Upload "upload", OpenBatch "open_batch", Probe "probe", Report "report", UserActive "user_active", NotificationsOff "notifications_off" }
    InstallTrigger { User "user", Quit "quit", IdleAuto "idle_auto", Forced "forced" }
    UpdateStage { Manifest "manifest", Download "download", Signature "signature", Authenticode "authenticode", Install "install", Relaunch "relaunch" }
    UpdateFailure { Network "network", Verify "verify", Disk "disk", Other "other" }
    MinVersionState { Blocked "blocked", Warned "warned" }
    View { Welcome "welcome", Overview "overview", History "history", SingleImages "single_images", NewFolder "new_folder", Folder "folder", FolderSettings "folder_settings", Series "series", Settings "settings" }
    WindowSource { Tray "tray", Notification "notification", Sendto "sendto", Launch "launch", Instance "instance" }
    NotificationKind { ReportSaved "report_saved", ReportProblem "report_problem", UploadProblem "upload_problem", Update "update", Other "other" }
    NotificationChannel { Os "os", InApp "in_app" }
    CountBucket { Zero "0", Few "1-9", Tens "10-99", Hundreds "100-999", Thousands "1000+" }
    EdgeBucket { Small "up_to_2048", Medium "2049-4096", Large "4097-8192", Huge "8193+" }
    ExportFormat { Csv "csv", Xlsx "xlsx" }
    RequestOrigin { Sendto "sendto", Verb "verb", FolderVerb "folder_verb", Drop "drop", AddImages "add_images" }
    DialogState { Analyze "analyze", Cancel "cancel", NoUsableFiles "no_usable_files" }
    ReportTrigger { Manual "manual", Error "error", Crash "crash" }
    ReportOutcome { Sent "sent", Saved "saved", Queued "queued", Failed "failed" }
    HandoffKind { Wake "wake", Spool "spool" }
}

events! {
    SessionStart "session.start" { state: StartState, duration_ms: u64, proxy_kind: ProxyKind, first_run: bool, prev_unclean: bool, holds_active: u64 }
    SessionAlive "session.alive" { duration_ms: u64, open_batches: u64, folders_active: u64, folders_paused: u64, folders_missing: u64, holds_active: u64, attention_open: u64, errors_suppressed: u64, tlm_buffer: u64, tlm_dropped_core: u64, tlm_dropped_interaction: u64, tlm_send_failures: u64, tlm_last_success_age_s: u64, window_open: bool }
    SessionStop "session.stop" { state: StopState, duration_ms: u64 }
    SessionUnclean "session.unclean" { prev_version: Version, last_line_age_s: u64 }
    Crash "crash" { kind: CrashKind, prev_session: bool }
    SettingsSnapshot "settings.snapshot" { language: LanguageSetting, autostart: bool, explorer_entries: bool, folders: u64 }
    WizardStepShown "wizard.step_shown" { state: SetupStep }
    WizardStepCompleted "wizard.step_completed" { state: SetupStep, duration_ms: u64 }
    WizardBack "wizard.back" { from: SetupStep, to: SetupStep }
    WizardError "wizard.error" { state: SetupStep, kind: WizardErrorKind, count: u64 }
    WizardLegacyFound "wizard.legacy_found" { source: u64 }
    WizardRestarted "wizard.restarted" { state: SetupStep, previous_duration_ms: u64 }
    WizardFinished "wizard.finished" { duration_ms: u64, preset: FinishPreset, report_mode: FinishReportMode, autostart: bool, explorer: bool, existing_images: ExistingImages, skipped_steps: u64, model_choices: u64 }
    AccountConnected "account.connected" { kind: AccountKind, imported_from_v1: bool }
    AccountSwitched "account.switched" { state: AccountState }
    AccountDisconnected "account.disconnected" { state: AccountState }
    LegacyMigrated "legacy.migrated" { count: u64, state: MigrationState }
    FolderAdded "folder.added" { folder_id: FolderId, via: FolderVia, preset: ArrivalPreset, report_mode: ReportMode, is_network: bool, is_cloud: bool, existing_images: ExistingImages, count: u64, model_choices: u64 }
    FolderChanged "folder.changed" { folder_id: FolderId, preset_from: ArrivalPreset, preset_to: ArrivalPreset, report_mode_from: ReportMode, report_mode_to: ReportMode, models_added: u64, models_removed: u64 }
    FolderPaused "folder.paused" { folder_id: FolderId }
    FolderResumed "folder.resumed" { folder_id: FolderId, duration_ms: u64 }
    FolderRemoved "folder.removed" { folder_id: FolderId, age_days: u64 }
    FolderCondition "folder.condition" { folder_id: FolderId, state: FolderState, duration_ms: u64, drive_type: DriveType }
    WatcherRescan "watcher.rescan" { folder_id: FolderId, reason: RescanReason }
    ResumeCatchup "resume.catchup" { folder_id: FolderId, count: u64, state: CatchupState }
    BatchClosed "batch.closed" { batch_id: BatchId, folder_id: FolderId, origin: Origin, reason: ClosureReason, count: u64, duration_ms: u64 }
    BatchManualClose "batch.manual_close" { batch_id: BatchId }
    BatchRenamed "batch.renamed" { batch_id: BatchId }
    UploadFailed "upload.failed" { batch_id: BatchId, state: UploadState, bytes: u64, width: u64, height: u64, format: ImageFormat }
    SubmitDone "submit.done" { count: u64, duration_ms: u64, state: SubmitState, model_id: Uuid, recovery_adopted: u64, recovery_resubmitted: u64 }
    ComputationSettled "computation.settled" { computation_id: ComputationId, batch_id: BatchId, folder_id: FolderId, origin: Origin, model_id: Uuid, state: SettledState, duration_ms: u64, e2e_ms: u64, customization_id: Uuid, reused: bool, resubmitted: bool, queue_ms: u64, upload_ms: u64, polls: u64 }
    AttentionRaised "attention.raised" { kind: AttentionKind }
    AttentionCleared "attention.cleared" { kind: AttentionKind, how: ClearedHow, duration_ms: u64 }
    ReportSettled "report.settled" { report_id: ReportId, batch_id: BatchId, origin: Origin, kind: ReportKind, state: ReportState, reason: ReportReason, code: u64, count: u64, duration_ms: u64, e2e_ms: u64, format: ReportFormat, part: u64, parts: u64, bytes: u64 }
    ReportOpened "report.opened" { from: OpenedFrom }
    ReportCustomCreated "report.custom_created" { count: u64, source_batches: u64 }
    ReportStaleUpdated "report.stale_updated" { report_id: ReportId }
    HoldRaised "hold.raised" { kind: HoldReason }
    HoldLifted "hold.lifted" { kind: HoldReason, duration_ms: u64, count: u64, by: LiftedBy }
    ProxyAuthRequired "proxy.auth_required" { basic: bool, ntlm: bool, negotiate: bool, digest: bool, unknown: bool }
    TlsUntrusted "tls.untrusted" { issuer_is_enterprise: bool }
    WatchMarkerConflict "watch_marker.conflict" { folder_id: FolderId }
    AppError "app.error" { kind: ErrorKind, reportable: bool, http_status: u64, attempt: u64 }
    UpdateCheck "update.check" { state: CheckState, to: Version, trigger: CheckTrigger }
    UpdateDownloaded "update.downloaded" { to: Version, bytes: u64, duration_ms: u64 }
    UpdateDeferred "update.deferred" { reason: DeferReason }
    UpdateInstallStarted "update.install_started" { from: Version, to: Version, trigger: InstallTrigger }
    UpdateApplied "update.applied" { from: Version, to: Version, duration_ms: u64 }
    UpdateFailed "update.failed" { stage: UpdateStage, from: Version, to: Version, kind: UpdateFailure }
    UpdateMinVersion "update.min_version" { min: Version, state: MinVersionState }
    UiView "ui.view" { state: View }
    WindowShown "window.shown" { source: WindowSource, cold: bool, duration_ms: u64 }
    NotificationShown "notification.shown" { kind: NotificationKind, channel: NotificationChannel }
    NotificationClicked "notification.clicked" { kind: NotificationKind, channel: NotificationChannel }
    HistorySearch "history.search" { count: CountBucket, duration_ms: u64 }
    HistoryExport "history.export" { count: u64, format: ExportFormat }
    SendtoRequest "sendto.request" { origin: RequestOrigin, count: u64, not_included: u64, joined_pending: bool }
    OneoffDialog "oneoff.dialog" { state: DialogState, duration_ms: u64, model_choices: u64, preselection_kept: bool, remembered: bool, hold_shown: bool }
    PreviewOpened "preview.opened" { model_id: Uuid, duration_ms: u64 }
    PreviewZoomMaxTier "preview.zoom_max_tier" { tier: u64, source_edge: EdgeBucket, model_id: Uuid }
    ProblemReportOffered "problem_report.offered" { trigger: ReportTrigger }
    ProblemReportSent "problem_report.sent" { state: ReportOutcome, names_included: bool }
    InstanceHandoff "instance.handoff" { kind: HandoffKind }
    PrivacyViewed "privacy.viewed" { what: PrivacyPanel }
}
