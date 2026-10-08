use crate::domain::models::Result;

/// Stores the last model a user picked for the composer.
pub trait SelectedModelRepo: Send + Sync + 'static {
    /// The stored model id, if this user has picked one.
    fn get(
        &self,
        user_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<String>>> + Send;

    /// Replace the stored model id for this user.
    fn set(
        &self,
        user_id: &str,
        model_id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}
