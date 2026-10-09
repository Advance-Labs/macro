//! Local-build login shortcuts and the runtime switch that turns them off.
//!
//! The local stack compiles this service with `return_passwordless_code` (the
//! passwordless start response carries the login code, so the web app can
//! sign in without reading mail) and `no_rate_limit` (no login-code
//! throttles). Both are fine on a developer's machine and dangerous anywhere
//! else: with the code in the response, anyone who can reach the service can
//! sign in as any user.
//!
//! `DISABLE_LOCAL_AUTH_SHORTCUTS=true` cancels both at runtime, whatever the
//! build, so a local-stack build can serve a non-throwaway instance (for
//! example one exposed through a tunnel) without a separate compile.

#[cfg(test)]
mod test;

use std::sync::OnceLock;

static DISABLED: OnceLock<bool> = OnceLock::new();

/// Record the configured switch. Called once at startup; later calls are
/// ignored so the answer cannot change while the service runs.
pub(crate) fn init(disabled: bool) {
    let _ = DISABLED.set(disabled);
}

fn disabled() -> bool {
    DISABLED.get().copied().unwrap_or(false)
}

/// Whether the passwordless start response may carry the login code.
pub(crate) fn return_passwordless_code() -> bool {
    resolve_return_passwordless_code(cfg!(feature = "return_passwordless_code"), disabled())
}

/// Whether the login-code throttles apply.
///
/// On by default via the `rate_limit` feature. `no_rate_limit` is the local
/// opt-out: `just run_local` needs it off (a 1-request/minute login code makes
/// local dev painful), and expressing that as an *additive* feature is what
/// lets every local service binary build in one `cargo` invocation.
/// `DISABLE_LOCAL_AUTH_SHORTCUTS` cancels that opt-out.
pub(crate) fn rate_limit_enabled() -> bool {
    resolve_rate_limit_enabled(
        cfg!(feature = "rate_limit"),
        cfg!(feature = "no_rate_limit"),
        disabled(),
    )
}

fn resolve_return_passwordless_code(compiled: bool, disabled: bool) -> bool {
    compiled && !disabled
}

fn resolve_rate_limit_enabled(rate_limit: bool, no_rate_limit: bool, disabled: bool) -> bool {
    rate_limit && (!no_rate_limit || disabled)
}
