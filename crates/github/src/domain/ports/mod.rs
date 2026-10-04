//! Port definitions for the github domain.
//!
//! These traits define the contracts that adapters must implement.

#[cfg(feature = "link")]
mod link;
#[cfg(feature = "sync")]
mod pull_request;
#[cfg(feature = "sync")]
pub use pull_request::GithubPullRequestClient;
#[cfg(feature = "sync")]
mod resync;
#[cfg(feature = "sync")]
mod sync;
#[cfg(feature = "sync")]
pub use resync::GithubPullRequestResync;

#[cfg(feature = "link")]
pub use link::{Auth, GithubLinkService, GithubOauth, GithubRepo};
#[cfg(feature = "sync")]
pub use sync::{
    GithubInstallationLister, GithubPullRequestIndex, GithubRepositoryClient, GithubSyncClient,
    GithubSyncRealtime, GithubSyncRepo, GithubSyncService,
};
