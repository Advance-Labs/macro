use crate::domain::models::model_access::{FREE_MODEL, UPGRADE_MODEL, is_composer_model};
use crate::domain::models::{ChatErr, ComposerModel, Result};
use crate::domain::ports::ModelAccessService;
use crate::domain::ports::SelectedModelRepo;
use crate::domain::service::ModelAccessServiceImpl;

/// Reads and records the composer's saved model.
///
/// A free plan has nothing to record: Gemini is the only model. A paid plan
/// with no saved pick opens on [`UPGRADE_MODEL`].
#[derive(Debug, Clone)]
pub struct SelectedModelService<R> {
    repo: R,
}

impl<R> SelectedModelService<R> {
    /// Create a service backed by `repo`.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

impl<R: SelectedModelRepo> SelectedModelService<R> {
    /// The model the composer should open on for `user_id`.
    #[tracing::instrument(skip(self), err)]
    pub async fn composer_model(&self, user_id: &str, professional: bool) -> Result<ComposerModel> {
        let stored = self.repo.get(user_id).await?;
        if professional
            && let Some(model_id) = stored
            && is_composer_model(&model_id)
            && ModelAccessServiceImpl.has_access(true, &model_id)
        {
            return Ok(ComposerModel {
                model_id,
                explicit: true,
            });
        }
        Ok(fallback(professional))
    }

    /// Record `model_id` when the user could choose it.
    ///
    /// A free plan is a no-op: the only model is not a choice. An unknown id
    /// on a paid plan is rejected.
    #[tracing::instrument(skip(self), err)]
    pub async fn remember(
        &self,
        user_id: &str,
        professional: bool,
        model_id: &str,
    ) -> Result<ComposerModel> {
        if !professional {
            return Ok(fallback(false));
        }
        if !is_composer_model(model_id) || !ModelAccessServiceImpl.has_access(true, model_id) {
            return Err(ChatErr::BadRequest(format!("unknown model {model_id}")));
        }
        self.repo.set(user_id, model_id).await?;
        Ok(ComposerModel {
            model_id: model_id.to_owned(),
            explicit: true,
        })
    }
}

fn fallback(professional: bool) -> ComposerModel {
    ComposerModel {
        model_id: if professional {
            UPGRADE_MODEL
        } else {
            FREE_MODEL
        }
        .to_owned(),
        explicit: false,
    }
}

#[cfg(test)]
mod test;
