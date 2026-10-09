pub mod login_code;
pub mod mobile_welcome_email;
pub mod passwordless;

// Whether the throttles apply is decided by
// `crate::local_shortcuts::rate_limit_enabled`. Turning them off for local
// builds through the additive `no_rate_limit` feature, rather than by dropping
// the default `rate_limit` feature, keeps every local service binary in one
// `cargo` invocation: a package-scoped `--no-default-features` build resolves
// features differently and invalidates the shared dependency artifacts of
// every other binary (a measured ~2 minutes of rebuild on every `run_local`).
