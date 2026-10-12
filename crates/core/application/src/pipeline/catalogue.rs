use std::collections::BTreeSet;

use mivia_domain::computation::ComputationEvent;
use mivia_domain::hold::{Access, Gate};
use mivia_domain::ids::ModelChoice;
use mivia_ports::server::CatalogueModel;
use mivia_ports::store::pipeline::CatalogueRow;
use serde_json::json;

use super::effects::{Call, Item};
use super::{Pipeline, PipelineError};

fn rows(models: &[CatalogueModel]) -> Vec<CatalogueRow> {
    let mut rows = Vec::new();
    for model in models {
        rows.push(CatalogueRow {
            model_id: model.model_id.clone(),
            customization_id: None,
            names_json: json!({ "en": model.name_en, "de": model.name_de }).to_string(),
        });
        rows.extend(model.customizations.iter().map(|custom| CatalogueRow {
            model_id: model.model_id.clone(),
            customization_id: Some(custom.id.clone()),
            names_json: json!({ "en": custom.name_en, "de": custom.name_de }).to_string(),
        }));
    }
    rows
}

fn key(row: &CatalogueRow) -> String {
    ModelChoice {
        model_id: row.model_id.clone(),
        customization_id: row.customization_id.clone(),
    }
    .key()
}

impl Pipeline {
    pub async fn refresh_catalogue(&self) -> Result<Option<BTreeSet<String>>, PipelineError> {
        if self.access(Gate::Catalogue).await? == Access::Blocked {
            return Ok(None);
        }
        let models = match self.ports.server.catalogue().await {
            Ok(models) => models,
            Err(error) => {
                self.infrastructure(&error, Call::Other, Item::new("catalogue", 0))
                    .await?;
                return Ok(None);
            }
        };
        let rows = rows(&models);
        let available: BTreeSet<String> = rows.iter().map(key).collect();
        let keys = available.clone();
        let now = self.now();
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            repo.replace_catalogue(&rows, now)?;
            let gone = repo
                .queued_choices()?
                .into_iter()
                .filter(|(_, choice)| !keys.contains(choice));
            for (id, _) in gone {
                repo.computation_event(id, ComputationEvent::ChoiceUnavailable, now)?;
            }
            Ok(())
        })
        .await?;
        Ok(Some(available))
    }
}
