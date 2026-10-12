use mivia_application::legacy::LegacyConfig;
use mivia_application::setup::{SetupDraft, read_draft};
use mivia_domain::ids::AccountId;
use mivia_domain::naming::Language;
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::folders::{CatalogueModelView, CatalogueView, CustomizationView};
use mivia_ports::ipc::setup::{LegacyTake, ModelChoiceView};
use mivia_ports::server::{CatalogueModel, ServerApi};
use mivia_ports::store::StoreExt;

use super::super::cx::{Cx, Ready};
use super::super::pipeline;
use super::language;
use super::legacy::{legacy_install, read_config};
use super::wizard::failed;

fn model_view(model: &CatalogueModel, language: Language) -> CatalogueModelView {
    CatalogueModelView {
        model_id: model.model_id.clone(),
        name: if language == Language::De {
            model.name_de.clone()
        } else {
            model.name_en.clone()
        },
        description: if language == Language::De {
            model.description_de.clone()
        } else {
            model.description_en.clone()
        },
        customizations: model
            .customizations
            .iter()
            .map(|custom| CustomizationView {
                id: custom.id.clone(),
                name: if language == Language::De {
                    custom.name_de.clone()
                } else {
                    custom.name_en.clone()
                },
            })
            .collect(),
    }
}

fn legacy_choices(models: &[CatalogueModel], config: &LegacyConfig, view: &mut CatalogueView) {
    for name in &config.models {
        let Some(model) = models
            .iter()
            .find(|model| model.internal_name.eq_ignore_ascii_case(name))
        else {
            view.legacy_gone.push(name.clone());
            continue;
        };
        let custom = config
            .customizations
            .iter()
            .find(|(owner, _)| owner.eq_ignore_ascii_case(name));
        let known =
            custom.filter(|(_, id)| model.customizations.iter().any(|known| known.id == *id));
        if custom.is_some() && known.is_none() {
            view.legacy_custom_gone.push(model.model_id.clone());
        }
        view.preselected.push(ModelChoiceView {
            model_id: model.model_id.clone(),
            customization_id: known.map(|(_, id)| id.clone()),
        });
    }
}

async fn fetch(cx: &Cx, ready: &Ready, draft: &SetupDraft) -> Option<Vec<CatalogueModel>> {
    let services = &ready.services;
    if let Some(authorizer) = &ready.authorizer {
        return pipeline::server_api(services, ready.scope.0, authorizer)
            .catalogue()
            .await
            .ok();
    }
    let connection = draft.connection.as_ref()?;
    let authorizer = pipeline::authorizer(services, AccountId(0), connection.account_id.clone());
    if !authorizer.load().await.unwrap_or(false) {
        authorizer.remember(cx.setup.session_key()?);
    }
    pipeline::server_api(services, AccountId(0), &authorizer)
        .catalogue()
        .await
        .ok()
}

pub(crate) async fn catalogue(cx: &Cx) -> Result<CatalogueView, AppError> {
    let ready = cx.ready().await;
    let draft = ready
        .store
        .write(read_draft)
        .await
        .map_err(|error| failed(&error))?;
    let language = language(&ready).await;
    let Some(models) = fetch(cx, &ready, &draft).await else {
        return Ok(CatalogueView::default());
    };
    let mut view = CatalogueView {
        loaded: true,
        models: models
            .iter()
            .map(|model| model_view(model, language))
            .collect(),
        ..CatalogueView::default()
    };
    let legacy = (draft.legacy_take == Some(LegacyTake::All))
        .then(|| legacy_install(cx, &draft).and_then(|install| read_config(&install)))
        .flatten();
    if let Some(config) = legacy {
        legacy_choices(&models, &config, &mut view);
    }
    if let ([only], true) = (models.as_slice(), view.preselected.is_empty()) {
        view.preselected.push(ModelChoiceView {
            model_id: only.model_id.clone(),
            customization_id: None,
        });
    }
    Ok(view)
}
