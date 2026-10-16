use mivia_api::Error as ApiError;
use mivia_api::types::{
    CreateJobsDto, CreateJobsDtoSource, CreateJobsDtoSourceVariant0, JobV2ListResponseDataItem,
    JobV2ListResponseDataItemStatus, JobV2StatusListItem, JobV2StatusListItemStatus,
    JobsV2ControllerGetJobsLightSortBy, JobsV2ControllerGetJobsLightSortByVariant0,
    JobsV2ControllerGetJobsLightSortOrder, JobsV2ControllerGetJobsLightSortOrderVariant0,
    JobsV2ControllerGetJobsLightSource, JobsV2ControllerGetJobsLightSourceVariant0, UserJobDto,
    UserJobDtoStatus,
};
use mivia_domain::ids::Timestamp;
use mivia_ports::server::{
    FIND_PAGE_SIZE, FindJobs, ServerError, ServerJob, ServerJobStatus, SubmitRequest,
};
use std::num::NonZeroU64;

use reqwest::StatusCode;
use uuid::Uuid;

use super::{Failure, HttpServerApi};
use crate::classify::{self, Call};

fn status(raw: &str) -> ServerJobStatus {
    match raw {
        "PENDING" => ServerJobStatus::Pending,
        "NEW" => ServerJobStatus::New,
        "CACHED" => ServerJobStatus::Cached,
        "FAILED" => ServerJobStatus::Failed,
        other => {
            tracing::warn!(value = other, "api.unknownValue");
            ServerJobStatus::Unknown(other.to_owned())
        }
    }
}

pub(super) fn uuid(text: &str) -> Result<Uuid, ServerError> {
    Uuid::parse_str(text).map_err(|_| ServerError::BadRequest {
        code: "INVALID_ID".to_owned(),
    })
}

fn created(dto: &UserJobDto) -> ServerJob {
    let raw = match &dto.status {
        UserJobDtoStatus::Variant0(known) => known.to_string(),
        UserJobDtoStatus::Variant1(other) => other.clone(),
    };
    ServerJob {
        job_id: dto.id.to_string(),
        image_id: Some(dto.image_id.to_string()),
        customization_id: None,
        status: status(&raw),
        result_id: dto.result_id.map(|id| id.to_string()),
        has_result: dto.result_id.is_some() || !dto.result.is_empty(),
        created_at: Some(Timestamp(dto.created_at.timestamp_millis())),
    }
}

fn polled(dto: &JobV2StatusListItem) -> ServerJob {
    let raw = match &dto.status {
        JobV2StatusListItemStatus::Variant0(known) => known.to_string(),
        JobV2StatusListItemStatus::Variant1(other) => other.clone(),
    };
    ServerJob {
        job_id: dto.id.to_string(),
        image_id: None,
        customization_id: None,
        status: status(&raw),
        result_id: dto.result_id.map(|id| id.to_string()),
        has_result: dto.result_id.is_some(),
        created_at: None,
    }
}

fn listed(dto: &JobV2ListResponseDataItem) -> ServerJob {
    let raw = match &dto.status {
        JobV2ListResponseDataItemStatus::Variant0(known) => known.to_string(),
        JobV2ListResponseDataItemStatus::Variant1(other) => other.clone(),
    };
    ServerJob {
        job_id: dto.id.to_string(),
        image_id: dto.image_id.map(|id| id.to_string()),
        customization_id: dto.customization_id.map(|id| id.to_string()),
        status: status(&raw),
        result_id: dto.result_id.map(|id| id.to_string()),
        has_result: dto.result_id.is_some(),
        created_at: Some(Timestamp(dto.created_at.timestamp_millis())),
    }
}

fn newest_api_jobs() -> (
    JobsV2ControllerGetJobsLightSortBy,
    JobsV2ControllerGetJobsLightSortOrder,
    JobsV2ControllerGetJobsLightSource,
) {
    (
        JobsV2ControllerGetJobsLightSortBy::Variant0(
            JobsV2ControllerGetJobsLightSortByVariant0::CreatedAt,
        ),
        JobsV2ControllerGetJobsLightSortOrder::Variant0(
            JobsV2ControllerGetJobsLightSortOrderVariant0::Desc,
        ),
        JobsV2ControllerGetJobsLightSource::Variant0(
            JobsV2ControllerGetJobsLightSourceVariant0::Api,
        ),
    )
}

fn body(request: &SubmitRequest) -> Result<CreateJobsDto, ServerError> {
    let image_ids = request
        .image_ids
        .iter()
        .map(|id| uuid(id))
        .collect::<Result<Vec<_>, _>>()?;
    let customization_id = request
        .choice
        .customization_id
        .as_deref()
        .map(uuid)
        .transpose()?;
    Ok(CreateJobsDto {
        customization_config_override: serde_json::Map::new(),
        customization_id,
        force_compute: Some(request.force),
        image_ids,
        model_id: uuid(&request.choice.model_id)?,
        share_results: None,
        source: Some(CreateJobsDtoSource::Variant0(
            CreateJobsDtoSourceVariant0::Api,
        )),
    })
}

impl HttpServerApi {
    pub(super) async fn create_jobs(
        &self,
        request: SubmitRequest,
    ) -> Result<Vec<ServerJob>, ServerError> {
        let dto = body(&request)?;
        let result = self
            .authorized(|auth| {
                let (dto, key, requested) = (&dto, &request.key, &request.image_ids);
                async move {
                    let client = self.client(auth);
                    let result = client.jobs_controller_create_jobs(Some(key), dto).await;
                    self.submit_result(result, requested).await
                }
            })
            .await?;
        Ok(result.iter().map(created).collect())
    }

    async fn submit_result(
        &self,
        result: Result<mivia_api::ResponseValue<Vec<UserJobDto>>, ApiError<()>>,
        requested: &[String],
    ) -> Result<Vec<UserJobDto>, Failure> {
        match result {
            Err(ApiError::UnexpectedResponse(response))
                if response.status() == StatusCode::NOT_FOUND =>
            {
                let body = mivia_api::read_capped(response, 64 * 1024)
                    .await
                    .map(|c| c.body)
                    .unwrap_or_default();
                Err(Failure::Error(ServerError::NotFound {
                    image_id: classify::named_image(&body, requested),
                }))
            }
            other => self.generated("createJobs", other, Call::Submit).await,
        }
    }

    pub(super) async fn statuses(
        &self,
        job_ids: Vec<String>,
    ) -> Result<Vec<ServerJob>, ServerError> {
        let ids = job_ids.join(",");
        let items = self
            .authorized(|auth| {
                let ids = &ids;
                async move {
                    let client = self.client(auth);
                    let result = client.jobs_v2_controller_get_jobs_status(ids).await;
                    self.generated("jobsStatus", result, Call::Other).await
                }
            })
            .await?;
        Ok(items.iter().map(polled).collect())
    }

    pub(super) async fn list_jobs(&self, query: FindJobs) -> Result<Vec<ServerJob>, ServerError> {
        let model = uuid(&query.choice.model_id)?;
        let customization = query
            .choice
            .customization_id
            .as_deref()
            .map(uuid)
            .transpose()?;
        let (sort, order, source) = newest_api_jobs();
        let page = i64::from(query.page);
        let size = NonZeroU64::new(u64::from(FIND_PAGE_SIZE)).ok_or(ServerError::Contract)?;
        let listed_page = self
            .authorized(|auth| {
                let (sort, order, source) = (&sort, &order, &source);
                async move {
                    let client = self.client(auth);
                    let result = client
                        .jobs_v2_controller_get_jobs_light(
                            customization.as_ref(),
                            Some(&model),
                            Some(page),
                            Some(size),
                            Some(sort),
                            Some(order),
                            Some(source),
                        )
                        .await;
                    self.generated("jobsList", result, Call::Other).await
                }
            })
            .await?;
        Ok(listed_page.data.iter().map(listed).collect())
    }

    pub(super) async fn remove_job(&self, job_id: &str) -> Result<(), ServerError> {
        let id = uuid(job_id)?;
        self.authorized(|auth| async move {
            let client = self.client(auth);
            let result = client.jobs_v2_controller_delete_job(&id).await;
            match self.generated("jobDelete", result, Call::Other).await {
                Ok(()) | Err(Failure::Error(ServerError::Gone)) => Ok(()),
                Err(other) => Err(other),
            }
        })
        .await
    }

    pub(super) async fn exists(&self, job_id: &str) -> Result<bool, ServerError> {
        let id = uuid(job_id)?;
        let found = self
            .authorized(|auth| async move {
                let client = self.client(auth);
                let result = client.jobs_v2_controller_get_job_results(&id).await;
                match self.generated("jobDetail", result, Call::Other).await {
                    Ok(_) | Err(Failure::Error(ServerError::Contract)) => Ok(true),
                    Err(Failure::Error(ServerError::Gone)) => Ok(false),
                    Err(other) => Err(other),
                }
            })
            .await?;
        Ok(found)
    }
}
