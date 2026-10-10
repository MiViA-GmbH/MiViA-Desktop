use crate::transition::IllegalTransition;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceFileState {
    Detected,
    Ready,
    CopyInBatch,
    KnownFile,
    KnownContent,
    Uploading,
    Uploaded { image_gone: bool },
    Rejected(RejectReason),
    Vanished,
    Unreadable(UnreadableReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RejectReason {
    TooLarge,
    Server,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnreadableReason {
    InUse,
    Incomplete,
    CloudOnly,
    AccessDenied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Classification {
    CopyInBatch,
    KnownFile,
    KnownContent,
    New,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpenFailure {
    Gone,
    Denied,
    Busy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceFileEvent {
    ReadinessPassed,
    Gone,
    GaveUp(UnreadableReason),
    Classified(Classification),
    TooLarge,
    UploadClaimed,
    UploadSucceeded,
    UploadRejected,
    UploadOpenFailed(OpenFailure),
    UploadInterrupted,
    ImageGone,
    ConfirmReupload,
    Removed,
    ContentChanged,
}

impl SourceFileState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::KnownFile | Self::Rejected(_) | Self::Vanished | Self::Unreadable(_)
        )
    }

    pub const fn apply(self, event: SourceFileEvent) -> Result<Self, IllegalTransition> {
        use SourceFileEvent as E;
        match (self, event) {
            (Self::Detected, E::ReadinessPassed)
            | (Self::Uploading, E::UploadInterrupted)
            | (Self::Uploaded { image_gone: true }, E::ConfirmReupload) => Ok(Self::Ready),
            (Self::Detected, E::GaveUp(reason)) => Ok(Self::Unreadable(reason)),
            (Self::Ready, E::Classified(class)) => Ok(classified(class)),
            (Self::Ready, E::TooLarge) => Ok(Self::Rejected(RejectReason::TooLarge)),
            (Self::Ready, E::UploadClaimed) => Ok(Self::Uploading),
            (Self::Uploading, E::UploadSucceeded) => Ok(Self::Uploaded { image_gone: false }),
            (Self::Uploading, E::UploadRejected) => Ok(Self::Rejected(RejectReason::Server)),
            (Self::Uploading, E::UploadOpenFailed(failure)) => Ok(open_failed(failure)),
            (Self::Uploaded { .. }, E::ImageGone) => Ok(Self::Uploaded { image_gone: true }),
            (Self::Detected, E::Gone) => Ok(Self::Vanished),
            (state, E::Removed) if !state.is_terminal() => Ok(Self::Vanished),
            (state, E::ContentChanged) if !state.is_terminal() => Ok(Self::Detected),
            _ => Err(IllegalTransition("source file")),
        }
    }
}

const fn classified(class: Classification) -> SourceFileState {
    match class {
        Classification::CopyInBatch => SourceFileState::CopyInBatch,
        Classification::KnownFile => SourceFileState::KnownFile,
        Classification::KnownContent => SourceFileState::KnownContent,
        Classification::New => SourceFileState::Ready,
    }
}

const fn open_failed(failure: OpenFailure) -> SourceFileState {
    match failure {
        OpenFailure::Gone => SourceFileState::Vanished,
        OpenFailure::Denied => SourceFileState::Unreadable(UnreadableReason::AccessDenied),
        OpenFailure::Busy => SourceFileState::Ready,
    }
}
