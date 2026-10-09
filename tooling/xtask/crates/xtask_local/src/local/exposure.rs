//! Public exposure: hardening for a local stack that is served beyond this
//! machine (for example a long-lived team instance behind a tunnel).
//!
//! A normal local stack is built for one developer on a trusted machine:
//! every port is published on all interfaces, Mailpit (which holds every login
//! code) is routed on the app origin, the proxy reflects any `Origin` with
//! credentials, and LocalStack's S3 endpoint answers anyone. None of that is
//! safe once the proxy is reachable from the internet. With exposure on:
//!
//! - every published container port binds to `127.0.0.1` only, Mailpit
//!   publishes nothing, and the agent harness loses the host Docker socket
//!   (see [`super::gen_compose`]);
//! - the proxy drops the wildcard CORS overlay and the Mailpit route, rejects
//!   requests whose `Origin` is not its own, and only serves LocalStack-backed
//!   storage to signed-in sessions (see [`super::proxy`]).
//!
//! Turned on by `MACRO_LOCAL_PUBLIC=1` in the process environment. The choice
//! is then recorded in the instance's artifact directory and stays on for that
//! instance, so a later `stack update` or `gen-compose` run without the
//! variable cannot silently re-open the stack. Delete the marker file to turn
//! it off again.

use anyhow::{Context, Result};

use super::instance::Instance;

/// The process-environment switch.
pub const ENV_VAR: &str = "MACRO_LOCAL_PUBLIC";

/// The per-instance record that keeps exposure on once requested.
const MARKER: &str = "public-exposure";

/// Whether `instance` runs with public-exposure hardening. Records the choice
/// the first time [`ENV_VAR`] asks for it.
pub fn enabled(instance: &Instance) -> Result<bool> {
    let marker = instance.artifact_dir().join(MARKER);
    let requested = macro_env_var::maybe_read_env(ENV_VAR).is_some_and(|v| is_truthy(&v));
    if requested && !marker.exists() {
        let dir = instance.ensure_artifact_dir()?;
        std::fs::write(
            dir.join(MARKER),
            "Public exposure hardening is on for this instance. Delete this file to turn it off.\n",
        )
        .with_context(|| format!("writing {}", marker.display()))?;
    }
    Ok(requested || marker.exists())
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Bind a Compose short-syntax port mapping to loopback. Mappings that
/// already name a host IP are left alone; a bare container port (which Docker
/// would publish on a random port on every interface) gets a loopback binding.
pub fn loopback_port(mapping: &str) -> String {
    match mapping.split(':').count() {
        1 => format!("127.0.0.1::{mapping}"),
        2 => format!("127.0.0.1:{mapping}"),
        _ => mapping.to_owned(),
    }
}

#[cfg(test)]
mod test;
