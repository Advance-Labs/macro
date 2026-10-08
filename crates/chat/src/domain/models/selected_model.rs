/// The model a composer should open on, and whether the user picked it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerModel {
    /// Provider-qualified model id.
    pub model_id: String,
    /// `true` when `model_id` is a model the user chose.
    pub explicit: bool,
}
