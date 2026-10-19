//! Runtime switch for upstream's signup extras.
//!
//! On account creation upstream seeds things that only make sense on
//! macro.com: the "Macro how to guide" and its demo tasks, a private
//! "Macro Support x <name>" channel with macro.com staff and a scripted
//! welcome from them, the `/onboarding` setup flow, and marketing events
//! (Meta CompleteRegistration, Loops).
//!
//! `SKIP_UPSTREAM_SIGNUP_EXTRAS=true` skips all of them and marks the new
//! account's tutorial complete, so a self-hosted instance's new users land
//! straight in the workspace. Unset keeps upstream behaviour.

use std::sync::OnceLock;

static SKIPPED: OnceLock<bool> = OnceLock::new();

/// Record the configured switch. Called once at startup; later calls are
/// ignored so the answer cannot change while the service runs.
pub(crate) fn init(skipped: bool) {
    let _ = SKIPPED.set(skipped);
}

/// Whether new accounts skip upstream's signup extras.
pub(crate) fn skipped() -> bool {
    SKIPPED.get().copied().unwrap_or(false)
}
