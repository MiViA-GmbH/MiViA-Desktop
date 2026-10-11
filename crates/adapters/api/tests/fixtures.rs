use std::path::{Path, PathBuf};

use mivia_api::types::{
    ImageDto, JobV2ListResponse, JobV2StatusListItem, JobV2StatusListItemStatus, LicenseDto,
    LicenseDtoType, ModelCustomizationDto, ModelDto, UploadRejectedBody,
    UploadRejectedBodyErrorsItem, UserJobDto,
};
use mivia_api::{AccessKeyCurrent, ApiErrorBody, Version};
use serde::de::DeserializeOwned;

type Decode = fn(&[u8]) -> Result<(), String>;

fn parses<T: DeserializeOwned>(bytes: &[u8]) -> Result<(), String> {
    serde_json::from_slice::<T>(bytes)
        .map(drop)
        .map_err(|e| e.to_string())
}

fn decoder(operation: &str, status: &str) -> Option<Decode> {
    let decode: Decode = match (operation, status) {
        ("JobsV2Controller_getJobsStatus", "200") => parses::<Vec<JobV2StatusListItem>>,
        ("JobsController_createJobs", "201") => parses::<Vec<UserJobDto>>,
        ("JobsV2Controller_getJobsLight", "200") => parses::<JobV2ListResponse>,
        ("UserController_getMyLicense", "200") => parses::<LicenseDto>,
        ("SettingsController_getPublicAndPrivateModels", "200") => parses::<Vec<ModelDto>>,
        ("ModelsController_getModelCustomizations", "200") => parses::<Vec<ModelCustomizationDto>>,
        ("ImageController_uploadImages", "200") => parses::<Vec<ImageDto>>,
        ("ImageController_uploadImages", "400") => parses::<UploadRejectedBody>,
        ("version", "200") => parses::<Version>,
        ("accessKeysInfo", "200") => parses::<AccessKeyCurrent>,
        ("errors", _) => parses::<ApiErrorBody>,
        _ => return None,
    };
    Some(decode)
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixtures() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root())
        .unwrap()
        .flat_map(|dir| std::fs::read_dir(dir.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    files
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(root().join(name)).unwrap()
}

#[test]
fn every_fixture_parses_into_the_type_of_its_operation_and_status() {
    let files = fixtures();
    assert!(files.len() >= 20, "only {} fixtures", files.len());
    for file in files {
        let operation = file
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let stem = file.file_stem().unwrap().to_str().unwrap();
        let status = stem.split('-').next().unwrap();
        let decode = decoder(operation, status).unwrap_or_else(|| panic!("no type for {file:?}"));
        decode(&std::fs::read(&file).unwrap()).unwrap_or_else(|e| panic!("{file:?}: {e}"));
    }
}

#[test]
fn unknown_job_status_is_kept_as_text() {
    let items: Vec<JobV2StatusListItem> = serde_json::from_slice(&fixture(
        "JobsV2Controller_getJobsStatus/200-unknown-status.json",
    ))
    .unwrap();
    assert!(matches!(
        &items[0].status,
        JobV2StatusListItemStatus::Variant1(raw) if raw == "RENDERING"
    ));
}

#[test]
fn unknown_licence_type_and_extra_fields_are_tolerated() {
    let license: LicenseDto = serde_json::from_slice(&fixture(
        "UserController_getMyLicense/200-unknown-type.json",
    ))
    .unwrap();
    assert!(matches!(license.type_, LicenseDtoType::Variant1(raw) if raw == "SEAT_BASED"));
}

#[test]
fn unknown_rejection_code_falls_into_the_catch_all() {
    let unknown: UploadRejectedBody = serde_json::from_slice(&fixture(
        "ImageController_uploadImages/400-unknown-code.json",
    ))
    .unwrap();
    assert!(matches!(
        unknown.errors[0],
        UploadRejectedBodyErrorsItem::Variant1(_)
    ));
    let known: UploadRejectedBody =
        serde_json::from_slice(&fixture("ImageController_uploadImages/400-rejected.json")).unwrap();
    assert!(matches!(
        known.errors[0],
        UploadRejectedBodyErrorsItem::Variant0(_)
    ));
}
