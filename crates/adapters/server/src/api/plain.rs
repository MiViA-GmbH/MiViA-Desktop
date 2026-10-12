use mivia_api::types::{LicenseDto, ModelDto};
use mivia_api::{Capped, JSON_LIMIT, Version, read_capped};
use mivia_domain::ids::Timestamp;
use mivia_ports::server::{
    CatalogueCustomization, CatalogueModel, LabelsFetch, License, ServerError, ServerVersion,
};
use reqwest::StatusCode;
use reqwest::header::ETAG;
use serde_json::Value;
use uuid::Uuid;

use super::HttpServerApi;
use crate::classify::{self, Call};

const LABELS_LIMIT: usize = 4 * 1024 * 1024;

fn count(value: Option<f64>) -> Option<u64> {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| format!("{value:.0}").parse().unwrap_or_default())
}

fn license(dto: &LicenseDto) -> License {
    License {
        kind: dto.type_.to_string(),
        ends_at: dto.limits.finish.map(|at| Timestamp(at.timestamp_millis())),
        images_used: count(dto.limits.used),
        images_limit: count(dto.limits.images),
    }
}

fn label(labels: Option<&Value>, name: &str, fallback: &str) -> String {
    labels
        .and_then(|labels| labels.get("model")?.get(name)?.as_str())
        .unwrap_or(fallback)
        .to_owned()
}

fn model(dto: &ModelDto, en: Option<&Value>, de: Option<&Value>) -> CatalogueModel {
    CatalogueModel {
        model_id: dto.id.to_string(),
        internal_name: dto.name.clone(),
        name_en: label(en, &dto.name, &dto.display_name),
        name_de: label(de, &dto.name, &dto.display_name),
        description_en: dto.description_en.clone().unwrap_or_default(),
        description_de: dto
            .description_de
            .clone()
            .or_else(|| dto.description_en.clone())
            .unwrap_or_default(),
        customizations: Vec::new(),
    }
}

impl HttpServerApi {
    async fn get_public(
        &self,
        request: reqwest::RequestBuilder,
        limit: usize,
    ) -> Result<Capped, ServerError> {
        let response = request.send().await.map_err(|e| self.transport(&e))?;
        let capped = read_capped(response, limit)
            .await
            .map_err(|_| ServerError::AfterSend)?;
        self.seen("public", &capped.headers);
        Ok(capped)
    }

    pub(super) async fn fetch_version(&self) -> Result<ServerVersion, ServerError> {
        let request = mivia_api::version(&self.clients().api, self.base());
        let capped = self.get_public(request, JSON_LIMIT).await?;
        if !capped.status.is_success() {
            return Err(classify::status(
                capped.status,
                &capped.headers,
                &capped.body,
                Call::Other,
            ));
        }
        let version: Version =
            serde_json::from_slice(&capped.body).map_err(|_| ServerError::Redirect)?;
        Ok(ServerVersion {
            version: version.version,
            min_desktop_version: version.min_desktop_version,
            max_upload_size_mb: version.max_upload_size_mb,
            blocked_versions: version.blocked_desktop_versions.unwrap_or_default(),
        })
    }

    pub(super) async fn fetch_labels(
        &self,
        lang: &str,
        etag: Option<String>,
    ) -> Result<LabelsFetch, ServerError> {
        let request = mivia_api::languages(&self.clients().api, self.base(), lang, etag.as_deref());
        let capped = self.get_public(request, LABELS_LIMIT).await?;
        if capped.status == StatusCode::NOT_MODIFIED {
            return Ok(LabelsFetch::NotModified);
        }
        if !capped.status.is_success() || capped.overflow {
            return Err(classify::status(
                capped.status,
                &capped.headers,
                &capped.body,
                Call::Other,
            ));
        }
        let etag = capped
            .headers
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        Ok(LabelsFetch::Fresh {
            body: capped.body,
            etag,
        })
    }

    async fn served_labels(&self) -> (Option<Value>, Option<Value>) {
        let Some(labels) = &self.labels else {
            return (None, None);
        };
        for lang in ["en", "de"] {
            if let Err(error) = labels.refresh(self, lang).await {
                tracing::warn!(?error, lang, "labels.refreshFailed");
            }
        }
        (Some(labels.served("en")), Some(labels.served("de")))
    }

    pub(super) async fn fetch_license(&self) -> Result<License, ServerError> {
        let dto = self
            .authorized(|auth| async move {
                let client = self.client(auth);
                let result = client.user_controller_get_my_license().await;
                self.generated("license", result, Call::Other).await
            })
            .await?;
        dto.as_ref().map(license).ok_or(ServerError::LicenseInvalid)
    }

    pub(super) async fn fetch_catalogue(&self) -> Result<Vec<CatalogueModel>, ServerError> {
        let dtos = self
            .authorized(|auth| async move {
                let client = self.client(auth);
                let result = client
                    .settings_controller_get_public_and_private_models()
                    .await;
                self.generated("models", result, Call::Other).await
            })
            .await?;
        let (en, de) = self.served_labels().await;
        let mut models = Vec::new();
        for dto in &dtos {
            let mut entry = model(dto, en.as_ref(), de.as_ref());
            entry.customizations = self.customizations(dto.id).await?;
            models.push(entry);
        }
        Ok(models)
    }

    async fn customizations(
        &self,
        model: Uuid,
    ) -> Result<Vec<CatalogueCustomization>, ServerError> {
        let dtos = self
            .authorized(|auth| async move {
                let client = self.client(auth);
                let result = client
                    .models_controller_get_model_customizations(&model)
                    .await;
                self.generated("customizations", result, Call::Other).await
            })
            .await?;
        Ok(dtos
            .into_iter()
            .map(|dto| CatalogueCustomization {
                id: dto.id.to_string(),
                name_en: dto.name.en,
                name_de: dto.name.de,
            })
            .collect())
    }
}
