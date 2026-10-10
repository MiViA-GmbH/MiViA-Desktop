use mivia_diagnostics::{Known, Redactor, machine_id};
use mivia_ports::auth::Authorizer;
use mivia_ports::ipc::support::ProblemReportInput;
use mivia_ports::server::ProblemReportForm;
use secrecy::ExposeSecret;

use crate::shell::cx::Ready;

const DESCRIPTION_CHARS: usize = 4000;
const SIGNATURE_CHARS: usize = 128;

pub(super) async fn known(ready: &Ready) -> Known {
    let mut literals = Vec::new();
    if let Some(authorizer) = &ready.authorizer
        && let Ok(auth) = authorizer.authorize().await
    {
        literals.push((auth.key.expose_secret().to_owned(), "<access-key>"));
    }
    literals.extend(mivia_os::windows_user().ok().map(|user| (user, "<user>")));
    let env = |name: &str| std::env::var(name).ok();
    for (name, token) in [
        ("USERNAME", "<user>"),
        ("USERDOMAIN", "<domain>"),
        ("COMPUTERNAME", "<host>"),
    ] {
        literals.extend(env(name).map(|value| (value, token)));
    }
    let roots = [
        ("USERPROFILE", "%USERPROFILE%"),
        ("LOCALAPPDATA", "%LOCALAPPDATA%"),
        ("TEMP", "%TEMP%"),
    ]
    .into_iter()
    .filter_map(|(name, token)| env(name).map(|value| (value, token)))
    .collect();
    Known { literals, roots }
}

fn signature(raw: Option<String>) -> Option<String> {
    raw.filter(|text| {
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-');
        !text.is_empty() && text.len() <= SIGNATURE_CHARS && text.chars().all(allowed)
    })
}

pub(super) fn form(
    ready: &Ready,
    input: ProblemReportInput,
    redactor: &Redactor,
    id: &str,
) -> ProblemReportForm {
    let services = &ready.services;
    let description: String = redactor
        .description(input.description.trim())
        .chars()
        .take(DESCRIPTION_CHARS)
        .collect();
    ProblemReportForm {
        report_id: id.to_owned(),
        trigger: input.trigger,
        signature: signature(input.signature),
        description: (!description.is_empty()).then_some(description),
        app_version: services.version.clone(),
        os: "Windows 10.0".to_owned(),
        machine_id: mivia_os::machine_guid()
            .map_or_else(|| "unknown".to_owned(), |guid| machine_id(&guid)),
        install_id: services.diagnostics.install_id.clone(),
        session_id: services.diagnostics.session_id.clone(),
    }
}
