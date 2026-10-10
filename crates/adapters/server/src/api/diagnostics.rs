use std::time::Duration;

use mivia_api::{IssueReport, read_capped};
use mivia_ports::server::{ProblemReportForm, ServerError, TelemetryAck};
use reqwest::header::HeaderValue;
use serde::Deserialize;

use super::{Failure, HttpServerApi, status_failure};
use crate::classify::{self, Call};

const RESPONSE_LIMIT: usize = 64 * 1024;
const SEND_TIMEOUT: Duration = Duration::from_secs(300);

fn fields(form: &ProblemReportForm) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("reportId", form.report_id.clone()),
        ("trigger", form.trigger.as_str().to_owned()),
        ("appVersion", form.app_version.clone()),
        ("os", form.os.clone()),
        ("machineId", form.machine_id.clone()),
        ("installId", form.install_id.clone()),
    ];
    fields.extend(form.signature.clone().map(|value| ("signature", value)));
    fields.extend(form.description.clone().map(|value| ("description", value)));
    fields
}

impl HttpServerApi {
    pub(super) async fn post_problem_report(
        &self,
        form: &ProblemReportForm,
        bundle: Vec<u8>,
    ) -> Result<(), ServerError> {
        self.authorized(|authorization| {
            let report = IssueReport {
                session_id: form.session_id.clone(),
                fields: fields(form),
                file_name: format!("MiViA-report-{}.zip", form.report_id),
                bundle: bundle.clone(),
            };
            self.issue(authorization, report)
        })
        .await
    }

    async fn issue(&self, authorization: HeaderValue, report: IssueReport) -> Result<(), Failure> {
        let request =
            mivia_api::issue_report(&self.clients().upload, self.base(), authorization, report)
                .timeout(SEND_TIMEOUT);
        let response = request
            .send()
            .await
            .map_err(|error| classify::transport(&error))?;
        let capped = read_capped(response, RESPONSE_LIMIT)
            .await
            .map_err(|_| ServerError::AfterSend)?;
        self.seen("issueReport", &capped.headers);
        if capped.status.is_success() {
            return Ok(());
        }
        Err(status_failure(
            capped.status,
            &capped.headers,
            &capped.body,
            Call::Other,
        ))
    }
}

#[derive(Deserialize)]
struct Rejected {
    seq: i64,
    reason: String,
}

#[derive(Deserialize)]
struct Accepted {
    accepted: u32,
    #[serde(default)]
    rejected: Vec<Rejected>,
}

fn min_interval(headers: &reqwest::header::HeaderMap) -> Option<u32> {
    headers
        .get("x-telemetry-min-interval-s")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

impl HttpServerApi {
    pub(super) async fn post_telemetry(
        &self,
        session_id: &str,
        body: Vec<u8>,
    ) -> Result<TelemetryAck, ServerError> {
        self.authorized(|authorization| self.batch(authorization, session_id, body.clone()))
            .await
    }

    async fn batch(
        &self,
        authorization: HeaderValue,
        session_id: &str,
        body: Vec<u8>,
    ) -> Result<TelemetryAck, Failure> {
        let request = mivia_api::telemetry_batch(
            &self.clients().api,
            self.base(),
            authorization,
            (session_id, body),
        );
        let response = request
            .send()
            .await
            .map_err(|error| classify::transport(&error))?;
        let capped = read_capped(response, RESPONSE_LIMIT)
            .await
            .map_err(|_| ServerError::AfterSend)?;
        self.seen("telemetry", &capped.headers);
        if !capped.status.is_success() {
            return Err(status_failure(
                capped.status,
                &capped.headers,
                &capped.body,
                Call::Other,
            ));
        }
        let parsed: Accepted =
            serde_json::from_slice(&capped.body).map_err(|_| ServerError::Contract)?;
        Ok(TelemetryAck {
            accepted: parsed.accepted,
            rejected: parsed
                .rejected
                .into_iter()
                .map(|entry| (entry.seq, entry.reason))
                .collect(),
            min_interval_s: min_interval(&capped.headers),
        })
    }
}
