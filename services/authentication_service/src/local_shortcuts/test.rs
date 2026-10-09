use super::*;

#[test]
fn passwordless_code_is_returned_only_when_compiled_in_and_not_disabled() {
    assert!(resolve_return_passwordless_code(true, false));
    assert!(!resolve_return_passwordless_code(true, true));
    assert!(!resolve_return_passwordless_code(false, false));
    assert!(!resolve_return_passwordless_code(false, true));
}

#[test]
fn disabling_shortcuts_restores_the_rate_limit_of_a_local_build() {
    // Deployed build: `rate_limit` only.
    assert!(resolve_rate_limit_enabled(true, false, false));
    assert!(resolve_rate_limit_enabled(true, false, true));
    // Local build: `rate_limit` + `no_rate_limit`.
    assert!(!resolve_rate_limit_enabled(true, true, false));
    assert!(resolve_rate_limit_enabled(true, true, true));
    // No `rate_limit` feature: nothing to restore.
    assert!(!resolve_rate_limit_enabled(false, true, true));
}
