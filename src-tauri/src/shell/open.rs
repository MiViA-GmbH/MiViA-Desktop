use std::io;

use mivia_application::errors::{pipeline_error, report_file_error};
use mivia_application::pipeline::SavedCheck;
use mivia_domain::ids::ReportPartId;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::screens::OpenTarget;
use mivia_ports::ipc::support::KnownLink;

use super::cx::{Ready, parse_id};
use super::preview;

const SITE: &str = "open_target";
const RELEASES: &str = "https://github.com/MiViA-GmbH/MiViA-Desktop/releases";
const INFO_MAIL: &str = "mailto:info@mivia.ai";
const NOTICES: &str = "THIRD-PARTY-NOTICES.txt";

fn shell_error(error: &io::Error) -> AppError {
    AppError::new(
        AppErrorKind::Internal,
        Some("shell"),
        error.to_string(),
        SITE,
    )
}

fn opened(result: Result<io::Result<()>, tokio::task::JoinError>) -> Result<(), AppError> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(shell_error(&error)),
        Err(error) => Err(AppError::new(
            AppErrorKind::Panic,
            None,
            error.to_string(),
            SITE,
        )),
    }
}

fn notices() -> Result<String, AppError> {
    let exe = std::env::current_exe().map_err(|error| shell_error(&error))?;
    Ok(exe.with_file_name(NOTICES).to_string_lossy().into_owned())
}

async fn link(ready: &Ready, link: KnownLink) -> Result<(), AppError> {
    let target = match link {
        KnownLink::Support => super::support::support_link(&ready.services),
        KnownLink::Releases => RELEASES.to_owned(),
        KnownLink::License => format!("{}/dashboard", ready.services.origin.0),
        KnownLink::AccessKeys => format!("{}/access-keys", ready.services.origin.0),
        KnownLink::InfoMail => INFO_MAIL.to_owned(),
        KnownLink::Notices => notices()?,
    };
    opened(tokio::task::spawn_blocking(move || mivia_os::open_file(&target)).await)
}

pub(crate) async fn target(ready: &Ready, target: OpenTarget) -> Result<(), AppError> {
    let part_id = match &target {
        OpenTarget::Report { part_id } | OpenTarget::RevealReport { part_id } => part_id,
        OpenTarget::Link { link: known } => return link(ready, *known).await,
        OpenTarget::Original { .. } | OpenTarget::ResultImage { .. } => {
            return preview::open_target(ready, &target).await;
        }
    };
    let part = ReportPartId(parse_id(part_id, SITE)?);
    let check = ready
        .pipeline()?
        .check_saved_report(part)
        .await
        .map_err(|error| pipeline_error(&error, SITE))?;
    let reveal = matches!(target, OpenTarget::RevealReport { .. });
    let path = match check {
        SavedCheck::Intact(path) => path,
        SavedCheck::Changed(path) if reveal => path,
        SavedCheck::Changed(path) => {
            return Err(report_file_error("reportChanged", Some(&path), SITE));
        }
        SavedCheck::Missing(path) => {
            return Err(report_file_error("reportMissing", path.as_ref(), SITE));
        }
    };
    let path = path.to_string();
    let action = if reveal {
        mivia_os::reveal_file
    } else {
        mivia_os::open_file
    };
    opened(tokio::task::spawn_blocking(move || action(&path)).await)
}
