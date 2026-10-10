use std::time::Duration;

use mivia_api::types::{
    ReportingControllerDownloadReportResponse as Link,
    ReportingControllerDownloadReportResponseVariant0 as LinkBody,
    ReportingControllerGetReportResponse as StatusDto,
    ReportingControllerGetReportResponseErrorCode as ErrorCode,
    ReportingControllerGetReportResponseErrorCodeVariant0 as KnownCode,
    ReportingControllerGetReportResponseStatus as Status,
    ReportingControllerGetReportResponseStatusVariant0 as KnownStatus,
};
use mivia_api::{JSON_LIMIT, ReportCreated, read_capped};
use mivia_domain::output::Format;
use mivia_ports::fs::ByteSink;
use mivia_ports::server::{
    Download, MAX_REPORT_BYTES, ReportExtras, ReportProgress, ReportRequest, ServerError,
};
use reqwest::{Response, StatusCode};
use serde_json::{Map, Value, json};

use super::jobs::uuid;
use super::{HttpServerApi, status_failure};
use crate::classify::{self, Call};

const STALL: Duration = Duration::from_secs(60);

fn extras(format: Format, extras: &ReportExtras) -> Map<String, Value> {
    let mut body = Map::new();
    body.insert("supersede".to_owned(), json!(false));
    body.insert("labels".to_owned(), json!(extras.labels));
    match format {
        Format::Pdf => {
            body.insert("summary".to_owned(), json!(true));
            if let Some((number, of)) = extras.part {
                body.insert("part".to_owned(), json!({ "number": number, "of": of }));
            }
            if !extras.omitted.is_empty() {
                let omitted: Vec<Value> = extras
                    .omitted
                    .iter()
                    .map(|(label, reason)| json!({ "label": label, "reason": reason.code() }))
                    .collect();
                body.insert("omitted".to_owned(), Value::Array(omitted));
            }
            if let Some(replaces) = &extras.replaces {
                body.insert("replaces".to_owned(), json!(replaces));
            }
        }
        Format::Csv => {
            let separator = if extras.decimal_comma { "comma" } else { "dot" };
            body.insert("decimalSeparator".to_owned(), json!(separator));
        }
    }
    body
}

pub(super) fn report_body(request: &ReportRequest) -> Value {
    let mut body = request
        .extras
        .as_ref()
        .map(|more| extras(request.format, more))
        .unwrap_or_default();
    body.insert("jobsIds".to_owned(), json!(request.job_ids));
    body.insert("timezone".to_owned(), json!(request.timezone));
    body.insert("includeImages".to_owned(), json!(request.include_images));
    Value::Object(body)
}

fn progress(dto: &StatusDto) -> Result<ReportProgress, ServerError> {
    let too_large = matches!(
        dto.error_code,
        Some(ErrorCode::Variant0(KnownCode::ReportTooLarge))
    );
    match &dto.status {
        Some(Status::Variant0(KnownStatus::Pending)) => Ok(ReportProgress::Pending),
        Some(Status::Variant0(KnownStatus::Done)) => Ok(ReportProgress::Done),
        Some(Status::Variant0(KnownStatus::Cancelled)) => Ok(ReportProgress::Cancelled),
        Some(Status::Variant0(KnownStatus::Failed)) => Ok(ReportProgress::Failed { too_large }),
        Some(Status::Variant1(other)) => {
            tracing::warn!(value = other.as_str(), "api.unknownValue");
            Ok(ReportProgress::Pending)
        }
        None => Err(ServerError::Contract),
    }
}

fn link(dto: Link) -> Option<String> {
    match dto {
        Link::Variant0(LinkBody::Variant0 { url }) => url,
        Link::Variant0(LinkBody::Variant1 { .. }) | Link::Variant1(_) => None,
    }
}

async fn stream(
    mut response: Response,
    mut sink: Box<dyn ByteSink>,
) -> Result<Download, ServerError> {
    let declared = response.content_length();
    let limit = declared.unwrap_or(MAX_REPORT_BYTES).min(MAX_REPORT_BYTES);
    if declared.is_some_and(|len| len > MAX_REPORT_BYTES) {
        return Err(ServerError::Contract);
    }
    let mut total: u64 = 0;
    loop {
        let next = tokio::time::timeout(STALL, response.chunk())
            .await
            .map_err(|_| ServerError::StallTimeout)?
            .map_err(|_| ServerError::AfterSend)?;
        let Some(chunk) = next else { break };
        total += u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        if total > limit {
            return Err(ServerError::Contract);
        }
        sink.write(chunk.to_vec())
            .await
            .map_err(ServerError::Source)?;
    }
    if declared.is_some_and(|len| len != total) {
        return Err(ServerError::AfterSend);
    }
    let staged = sink.finish().await.map_err(ServerError::Source)?;
    Ok(Download::Staged(staged))
}

impl HttpServerApi {
    pub(super) async fn create_report(
        &self,
        request: &ReportRequest,
    ) -> Result<String, ServerError> {
        let body = report_body(request);
        let target = (request.format.code(), request.language.code(), &body);
        self.authorized(|auth| async move {
            let send = mivia_api::create_report(&self.clients().api, self.base(), auth, target);
            let response = send.send().await.map_err(|e| classify::transport(&e))?;
            let capped = read_capped(response, JSON_LIMIT)
                .await
                .map_err(|_| ServerError::AfterSend)?;
            self.seen("createReport", &capped.headers);
            if !capped.status.is_success() || capped.overflow {
                let status = capped.status;
                return Err(status_failure(
                    status,
                    &capped.headers,
                    &capped.body,
                    Call::Report,
                ));
            }
            let created: ReportCreated =
                serde_json::from_slice(&capped.body).map_err(|_| ServerError::Contract)?;
            Ok(created.report_id)
        })
        .await
    }

    pub(super) async fn status_of_report(&self, id: &str) -> Result<ReportProgress, ServerError> {
        let id = uuid(id)?;
        let dto = self
            .authorized(|auth| async move {
                let client = self.client(auth, None);
                let result = client.reporting_controller_get_report(&id).await;
                self.generated("getReport", result, Call::Other).await
            })
            .await?;
        progress(&dto)
    }

    pub(super) async fn fetch_report(
        &self,
        id: &str,
        sink: Box<dyn ByteSink>,
    ) -> Result<Download, ServerError> {
        let id = uuid(id)?;
        let dto = self
            .authorized(|auth| async move {
                let client = self.client(auth, None);
                let result = client.reporting_controller_download_report(&id).await;
                self.generated("downloadReport", result, Call::Other).await
            })
            .await?;
        let Some(url) = link(dto).filter(|url| self.allowed_link(url)) else {
            return Ok(Download::Expired);
        };
        let response = self
            .clients()
            .upload
            .get(url)
            .send()
            .await
            .map_err(|e| classify::transport(&e))?;
        match response.status() {
            status if status.is_success() => stream(response, sink).await,
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => Ok(Download::Expired),
            status => Err(classify::status(
                status,
                response.headers(),
                &[],
                Call::Other,
            )),
        }
    }

    fn allowed_link(&self, url: &str) -> bool {
        url.starts_with("https://")
            || (self.base().starts_with("http://") && url.starts_with("http://"))
    }
}

#[cfg(test)]
mod tests {
    use mivia_domain::naming::Language;
    use mivia_domain::report::OmittedReason;

    use super::*;

    fn request(format: Format, extras: Option<ReportExtras>) -> ReportRequest {
        ReportRequest {
            format,
            job_ids: vec!["j1".to_owned()],
            timezone: "Europe/Berlin".to_owned(),
            include_images: false,
            language: Language::De,
            extras,
        }
    }

    fn more() -> ReportExtras {
        ReportExtras {
            labels: vec!["a/b.tif".to_owned()],
            part: Some((1, 2)),
            omitted: vec![("c.tif".to_owned(), OmittedReason::Lost)],
            replaces: None,
            decimal_comma: true,
        }
    }

    #[test]
    fn version_one_bodies_carry_only_the_old_fields() {
        let body = report_body(&request(Format::Pdf, None));
        assert_eq!(
            body,
            json!({ "jobsIds": ["j1"], "timezone": "Europe/Berlin", "includeImages": false })
        );
    }

    #[test]
    fn version_two_bodies_add_labels_and_the_summary() {
        let pdf = report_body(&request(Format::Pdf, Some(more())));
        assert_eq!(pdf["supersede"], json!(false));
        assert_eq!(pdf["summary"], json!(true));
        assert_eq!(pdf["labels"], json!(["a/b.tif"]));
        assert_eq!(pdf["part"], json!({ "number": 1, "of": 2 }));
        assert_eq!(
            pdf["omitted"],
            json!([{ "label": "c.tif", "reason": "LOST" }])
        );
        assert!(pdf.get("decimalSeparator").is_none());
        let csv = report_body(&request(Format::Csv, Some(more())));
        assert_eq!(csv["decimalSeparator"], json!("comma"));
        assert!(csv.get("summary").is_none() && csv.get("omitted").is_none());
    }
}
