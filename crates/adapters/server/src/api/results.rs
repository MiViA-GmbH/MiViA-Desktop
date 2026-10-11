use mivia_api::{JSON_LIMIT, read_capped};
use mivia_ports::fs::ByteSink;
use mivia_ports::server::{Download, MAX_RESULT_IMAGE_BYTES, ServerError};
use reqwest::StatusCode;
use reqwest::header::LOCATION;
use serde_json::Value;

use super::jobs::uuid;
use super::reports::stream;
use super::{HttpServerApi, status_failure};
use crate::classify::Call;

use mivia_ports::server::plain_result_name as safe_name;

fn results_of(body: &[u8]) -> Result<Option<String>, ServerError> {
    let detail: Value = serde_json::from_slice(body).map_err(|_| ServerError::Contract)?;
    detail
        .get("result")
        .and_then(|result| result.get("results"))
        .filter(|results| results.is_array())
        .map(|results| serde_json::to_string(results).map_err(|_| ServerError::Contract))
        .transpose()
}

impl HttpServerApi {
    pub(super) async fn fetch_results(&self, job_id: &str) -> Result<Option<String>, ServerError> {
        let id = uuid(job_id)?.to_string();
        let id = id.as_str();
        self.authorized(|auth| async move {
            let send = mivia_api::job_detail(&self.clients().api, self.base(), auth, id);
            let response = send.send().await.map_err(|e| self.transport(&e))?;
            let capped = read_capped(response, JSON_LIMIT)
                .await
                .map_err(|_| ServerError::AfterSend)?;
            self.seen("getJobResults", &capped.headers);
            match capped.status {
                _ if capped.overflow => Err(ServerError::Contract.into()),
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => Ok(None),
                status if status.is_success() => Ok(results_of(&capped.body)?),
                status => Err(status_failure(
                    status,
                    &capped.headers,
                    &capped.body,
                    Call::Other,
                )),
            }
        })
        .await
    }

    async fn image_location(&self, id: &str, name: &str) -> Result<Option<String>, ServerError> {
        self.authorized(|auth| async move {
            let send = mivia_api::result_image(&self.clients().api, self.base(), auth, (id, name));
            let response = send.send().await.map_err(|e| self.transport(&e))?;
            self.seen("getResultImage", response.headers());
            let (status, headers) = (response.status(), response.headers().clone());
            match status {
                status if status.is_redirection() => Ok(headers
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)),
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => Ok(None),
                status => Err(status_failure(status, &headers, &[], Call::Other)),
            }
        })
        .await
    }

    pub(super) async fn fetch_result_image(
        &self,
        job_id: &str,
        filename: &str,
        sink: Box<dyn ByteSink>,
    ) -> Result<Download, ServerError> {
        let id = uuid(job_id)?.to_string();
        if !safe_name(filename) {
            return Err(ServerError::BadRequest {
                code: "INVALID_FILENAME".to_owned(),
            });
        }
        let location = self.image_location(&id, filename).await?;
        let Some(url) = location.filter(|url| self.allowed_link(url)) else {
            return Ok(Download::Expired);
        };
        let response = self
            .clients()
            .upload
            .get(url)
            .send()
            .await
            .map_err(|e| self.transport(&e))?;
        match response.status() {
            status if status.is_success() => stream(response, sink, MAX_RESULT_IMAGE_BYTES).await,
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => Ok(Download::Expired),
            status => Err(crate::classify::status(
                status,
                response.headers(),
                &[],
                Call::Other,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_file_names_pass() {
        assert!(safe_name("01a12b41-61d1-74b2-9e9e-fa7640849486.png"));
        for bad in [
            "", "../x.png", "a/b.png", r"a\b.png", ".hidden", "a b.png", "a?b", "a%2f.png",
        ] {
            assert!(!safe_name(bad), "{bad}");
        }
    }

    #[test]
    fn results_are_taken_from_the_detail() {
        let body = br#"{"result":{"results":[{"type":"message","label":"x"}]}}"#;
        assert_eq!(
            results_of(body).unwrap().as_deref(),
            Some(r#"[{"label":"x","type":"message"}]"#)
        );
        assert_eq!(results_of(br#"{"result":null}"#).unwrap(), None);
        assert_eq!(results_of(b"nope"), Err(ServerError::Contract));
    }
}
