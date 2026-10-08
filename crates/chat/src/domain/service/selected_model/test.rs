use std::sync::Mutex;

use super::*;
use crate::domain::models::{FREE_MODEL, UPGRADE_MODEL};

struct Mem(Mutex<Option<String>>);

impl SelectedModelRepo for Mem {
    async fn get(&self, _user_id: &str) -> Result<Option<String>> {
        Ok(self.0.lock().expect("lock").clone())
    }

    async fn set(&self, _user_id: &str, model_id: &str) -> Result<()> {
        *self.0.lock().expect("lock") = Some(model_id.to_owned());
        Ok(())
    }
}

fn service(stored: Option<&str>) -> SelectedModelService<Mem> {
    SelectedModelService::new(Mem(Mutex::new(stored.map(str::to_owned))))
}

#[tokio::test]
async fn paid_user_without_a_pick_lands_on_opus() {
    let model = service(None)
        .composer_model("macro|a@b.c", true)
        .await
        .unwrap();
    assert_eq!(model.model_id, UPGRADE_MODEL);
    assert!(!model.explicit);
}

#[tokio::test]
async fn a_saved_pick_including_gemini_is_kept() {
    let gemini = service(Some(FREE_MODEL))
        .composer_model("macro|a@b.c", true)
        .await
        .unwrap();
    assert_eq!(gemini.model_id, FREE_MODEL);
    assert!(gemini.explicit);

    let kimi = service(Some("fireworks/kimi-k3"))
        .composer_model("macro|a@b.c", true)
        .await
        .unwrap();
    assert_eq!(kimi.model_id, "fireworks/kimi-k3");
    assert!(kimi.explicit);
}

#[tokio::test]
async fn an_unknown_stored_model_falls_back() {
    let model = service(Some("retired/model"))
        .composer_model("macro|a@b.c", true)
        .await
        .unwrap();
    assert_eq!(model.model_id, UPGRADE_MODEL);
    assert!(!model.explicit);
}

#[tokio::test]
async fn a_free_user_gets_gemini_and_cannot_save_a_pick() {
    let service = service(Some(UPGRADE_MODEL));
    let model = service.composer_model("macro|a@b.c", false).await.unwrap();
    assert_eq!(model.model_id, FREE_MODEL);
    assert!(!model.explicit);

    let remembered = service
        .remember("macro|a@b.c", false, "fireworks/kimi-k3")
        .await
        .unwrap();
    assert_eq!(remembered.model_id, FREE_MODEL);
    assert!(!remembered.explicit);
    assert_eq!(
        service.repo.get("macro|a@b.c").await.unwrap().as_deref(),
        Some(UPGRADE_MODEL)
    );
}

#[tokio::test]
async fn remembering_a_paid_model_stores_it() {
    let service = service(None);
    let remembered = service
        .remember("macro|a@b.c", true, "fireworks/kimi-k3")
        .await
        .unwrap();
    assert!(remembered.explicit);
    assert_eq!(remembered.model_id, "fireworks/kimi-k3");
    let loaded = service.composer_model("macro|a@b.c", true).await.unwrap();
    assert_eq!(loaded, remembered);
}

#[tokio::test]
async fn remembering_an_unknown_model_is_rejected() {
    let error = service(None)
        .remember("macro|a@b.c", true, "not-a-model")
        .await
        .unwrap_err();
    assert!(matches!(error, ChatErr::BadRequest(_)));
}
