use uuid::Uuid;

macro_rules! kinds {
    ($($variant:ident $name:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum EventKind {
            $($variant),*
        }

        impl EventKind {
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),*
                }
            }
        }
    };
}

kinds! {
    SessionStart "session.start",
    SessionAlive "session.alive",
    SessionStop "session.stop",
    SessionUnclean "session.unclean",
    Crash "crash",
    SettingsSnapshot "settings.snapshot",
    WizardStepShown "wizard.step_shown",
    WizardStepCompleted "wizard.step_completed",
    WizardBack "wizard.back",
    WizardError "wizard.error",
    WizardLegacyFound "wizard.legacy_found",
    WizardRestarted "wizard.restarted",
    WizardFinished "wizard.finished",
    AccountConnected "account.connected",
    AccountSwitched "account.switched",
    AccountDisconnected "account.disconnected",
    LegacyMigrated "legacy.migrated",
    FolderAdded "folder.added",
    FolderChanged "folder.changed",
    FolderPaused "folder.paused",
    FolderResumed "folder.resumed",
    FolderRemoved "folder.removed",
    FolderCondition "folder.condition",
    WatcherRescan "watcher.rescan",
    ResumeCatchup "resume.catchup",
    BatchClosed "batch.closed",
    BatchManualClose "batch.manual_close",
    BatchRenamed "batch.renamed",
    UploadFailed "upload.failed",
    SubmitDone "submit.done",
    ComputationSettled "computation.settled",
    AttentionRaised "attention.raised",
    AttentionCleared "attention.cleared",
    ReportSettled "report.settled",
    ReportOpened "report.opened",
    ReportCustomCreated "report.custom_created",
    ReportStaleUpdated "report.stale_updated",
    HoldRaised "hold.raised",
    HoldLifted "hold.lifted",
    ProxyAuthRequired "proxy.auth_required",
    TlsUntrusted "tls.untrusted",
    WatchMarkerConflict "watch_marker.conflict",
    AppError "app.error",
    UpdateCheck "update.check",
    UpdateDownloaded "update.downloaded",
    UpdateDeferred "update.deferred",
    UpdateInstallStarted "update.install_started",
    UpdateApplied "update.applied",
    UpdateFailed "update.failed",
    UpdateMinVersion "update.min_version",
    UiView "ui.view",
    WindowShown "window.shown",
    NotificationShown "notification.shown",
    NotificationClicked "notification.clicked",
    HistorySearch "history.search",
    HistoryExport "history.export",
    SendtoRequest "sendto.request",
    OneoffDialog "oneoff.dialog",
    PreviewOpened "preview.opened",
    PreviewZoomMaxTier "preview.zoom_max_tier",
    ProblemReportOffered "problem_report.offered",
    ProblemReportSent "problem_report.sent",
    InstanceHandoff "instance.handoff",
    PrivacyViewed "privacy.viewed",
}

#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Tag(&'static str),
    Id(Uuid),
    Entity(&'static str, i64),
    Version(String),
    Num(f64),
    Flag(bool),
}

#[derive(Clone, Debug, PartialEq)]
#[must_use]
pub struct ProductEvent {
    pub kind: EventKind,
    pub fields: Vec<(&'static str, Field)>,
}

impl ProductEvent {
    pub const fn new(kind: EventKind) -> Self {
        Self {
            kind,
            fields: Vec::new(),
        }
    }

    fn with(mut self, name: &'static str, field: Field) -> Self {
        self.fields.push((name, field));
        self
    }

    pub fn tag(self, name: &'static str, value: &'static str) -> Self {
        self.with(name, Field::Tag(value))
    }

    pub fn id(self, name: &'static str, value: Uuid) -> Self {
        self.with(name, Field::Id(value))
    }

    pub fn entity(self, name: &'static str, kind: &'static str, id: i64) -> Self {
        self.with(name, Field::Entity(kind, id))
    }

    pub fn version(self, name: &'static str, value: &str) -> Self {
        self.with(name, Field::Version(value.to_owned()))
    }

    pub fn num(self, name: &'static str, value: f64) -> Self {
        self.with(name, Field::Num(value))
    }

    pub fn count(self, name: &'static str, value: u64) -> Self {
        let value = u32::try_from(value).map_or_else(|_| f64::from(u32::MAX), f64::from);
        self.with(name, Field::Num(value))
    }

    pub fn flag(self, name: &'static str, value: bool) -> Self {
        self.with(name, Field::Flag(value))
    }
}

pub trait TelemetrySink: Send + Sync {
    fn record(&self, event: ProductEvent);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoTelemetry;

impl TelemetrySink for NoTelemetry {
    fn record(&self, _event: ProductEvent) {}
}
