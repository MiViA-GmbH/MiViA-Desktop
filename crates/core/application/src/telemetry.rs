use mivia_domain::batch::ClosureReason;
use mivia_domain::hold::{HoldReason, KeyProblem, LicenseScope, NetworkBlock};
use mivia_ports::ipc::setup::{ArrivalPreset, ReportMode, SetupStep};

pub const TELEMETRY_DEFAULT_ON: bool = true;

pub const fn effective(
    policy: Option<bool>,
    user: Option<bool>,
    server_default: Option<bool>,
) -> bool {
    match (policy, user, server_default) {
        (Some(policy), _, _) => policy,
        (None, Some(user), _) => user,
        (None, None, Some(server)) => server,
        (None, None, None) => TELEMETRY_DEFAULT_ON,
    }
}

pub const fn hold_tag(reason: HoldReason) -> &'static str {
    match reason {
        HoldReason::Offline => "offline",
        HoldReason::ServiceUnavailable => "service_unavailable",
        HoldReason::NetworkBlocked(NetworkBlock::Tls) => "tls",
        HoldReason::NetworkBlocked(NetworkBlock::ProxyAuth) => "proxy_auth",
        HoldReason::NetworkBlocked(NetworkBlock::ProxyRefused) => "proxy_refused",
        HoldReason::NetworkBlocked(NetworkBlock::Redirect) => "redirect",
        HoldReason::AccessKeyInvalid(KeyProblem::Invalid) => "key_invalid",
        HoldReason::AccessKeyInvalid(KeyProblem::AccountBlocked) => "account_blocked",
        HoldReason::AccessKeyInvalid(KeyProblem::AdminScope) => "admin_scope",
        HoldReason::LicenseInvalid(LicenseScope::Uploads) => "license_uploads",
        HoldReason::LicenseInvalid(LicenseScope::All) => "license_all",
        HoldReason::ClientTooOld => "client_too_old",
    }
}

pub const fn step_tag(step: SetupStep) -> &'static str {
    match step {
        SetupStep::Welcome => "welcome",
        SetupStep::Legacy => "legacy",
        SetupStep::Key => "key",
        SetupStep::Folder => "folder",
        SetupStep::Series => "series",
        SetupStep::Models => "models",
        SetupStep::Finish => "finish",
    }
}

pub const fn preset_tag(preset: ArrivalPreset) -> &'static str {
    match preset {
        ArrivalPreset::Microscope => "microscope",
        ArrivalPreset::WholeFolder => "whole_folder",
        ArrivalPreset::ByHand => "by_hand",
    }
}

pub const fn report_mode_tag(mode: ReportMode) -> &'static str {
    match mode {
        ReportMode::Subfolder => "subfolder",
        ReportMode::Beside => "beside",
        ReportMode::Custom => "custom",
    }
}

pub const fn closure_tag(reason: ClosureReason) -> &'static str {
    match reason {
        ClosureReason::QuietPeriod => "quiet_period",
        ClosureReason::Cap => "cap",
        ClosureReason::Manual => "manual",
        ClosureReason::Restart => "restart",
        ClosureReason::AccountSwitch => "account_switch",
        ClosureReason::OneOff => "one_off",
    }
}
