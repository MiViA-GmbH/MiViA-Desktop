use std::path::Path;
use std::sync::Arc;

use mivia_domain::intake::upload_cap_bytes;
use mivia_domain::report::supports_v2;
use mivia_domain::update::too_old;
use mivia_ports::auth::{IdentityApi, ServerOrigin};
use mivia_ports::server::{CatalogueModel, ServerApi};
use mivia_server::{HttpIdentityApi, ModelLabels};

use super::{Outcome, Smoke, check, say};

pub(crate) async fn identity(smoke: &Smoke) -> Outcome<()> {
    let identity = HttpIdentityApi::new(Arc::clone(&smoke.clients))
        .identify(&ServerOrigin(smoke.base.clone()), &smoke.secret)
        .await
        .map_err(|e| format!("identify: {e:?}"))?;
    check(
        !identity.admin_scope,
        "access key info, key without admin rights",
    )?;
    smoke.get("accessKeysInfo", "/api/access-keys/info").await?;
    Ok(())
}

pub(crate) async fn version(smoke: &Smoke) -> Outcome<String> {
    let version = smoke
        .api
        .version()
        .await
        .map_err(|e| format!("version: {e:?}"))?;
    say(&format!(
        "      server {} min {:?} max {:?} MiB blocked {:?}",
        version.version,
        version.min_desktop_version,
        version.max_upload_size_mb,
        version.blocked_versions
    ));
    let cap = upload_cap_bytes(version.max_upload_size_mb);
    let expected = version
        .max_upload_size_mb
        .map_or(300, u64::from)
        .saturating_mul(1024 * 1024);
    check(cap == expected, "upload cap follows maxUploadSizeMb")?;
    let held = too_old(
        "2.0.0",
        version.min_desktop_version.as_deref(),
        &version.blocked_versions,
    );
    say(&format!("      version 2.0.0 held as too old: {held}"));
    check(
        supports_v2(&version.version, Some(&version.version)),
        "reports v2 gate opens at the server's own version",
    )?;
    let client = smoke.clients.load().api.clone();
    let request = mivia_api::version(&client, &smoke.base);
    smoke.raw("version", request).await?;
    Ok(version.version)
}

pub(crate) async fn account(smoke: &Smoke) -> Outcome<Vec<CatalogueModel>> {
    let license = smoke
        .api
        .license()
        .await
        .map_err(|e| format!("license: {e:?}"))?;
    check(!license.kind.is_empty(), "licence")?;
    smoke
        .get("UserController_getMyLicense", "/api/users/license")
        .await?;
    let catalogue = smoke
        .api
        .catalogue()
        .await
        .map_err(|e| format!("catalogue: {e:?}"))?;
    check(!catalogue.is_empty(), "catalogue with customizations")?;
    say(&format!("      {} models", catalogue.len()));
    smoke
        .get(
            "SettingsController_getPublicAndPrivateModels",
            "/api/settings/available-models",
        )
        .await?;
    let with_customizations = catalogue
        .iter()
        .find(|model| !model.customizations.is_empty())
        .or_else(|| catalogue.first())
        .ok_or("no model")?;
    smoke
        .get(
            "ModelsController_getModelCustomizations",
            &format!(
                "/api/models/{}/customizations",
                with_customizations.model_id
            ),
        )
        .await?;
    Ok(catalogue)
}

pub(crate) async fn labels(smoke: &Smoke, data: &Path) -> Outcome<()> {
    let labels = ModelLabels::new(data, "smoke");
    for lang in ["en", "de"] {
        labels
            .refresh(&smoke.api, lang)
            .await
            .map_err(|e| format!("labels {lang}: {e:?}"))?;
        let again = labels
            .refresh(&smoke.api, lang)
            .await
            .map_err(|e| format!("labels {lang}: {e:?}"))?;
        check(
            !again,
            &format!("model labels {lang}, second fetch unchanged"),
        )?;
    }
    Ok(())
}
