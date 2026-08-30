include!("lib.rs");

/// Rejects a production-like startup if the durable Identity backend could not
/// be acquired after the shared readiness preflight.
///
/// The guard preflight and AppState construction intentionally use separate
/// connections. A transient failure between them must stop startup rather than
/// leave a listening process with no durable identity authority.
pub fn validate_runtime_backend(
    state: &AppState,
    require_database: bool,
) -> Result<(), &'static str> {
    if require_database && state.pool.is_none() {
        Err("production-like identity runtime requires a live PostgreSQL pool")
    } else {
        Ok(())
    }
}

/// Installs the authenticated internal client and removes the legacy static-key
/// authority from production-like runtime state after environment loading.
pub fn harden_runtime_state(
    state: &mut AppState,
    internal_http: reqwest::Client,
    disable_static_api_keys: bool,
) {
    state.http = internal_http;
    if disable_static_api_keys {
        state.api_keys = Arc::new(HashMap::new());
    }
}

#[cfg(test)]
mod runtime_hardening_tests {
    use super::*;

    #[test]
    fn production_backend_requires_a_durable_pool() {
        let state = AppState::new_for_tests(HashMap::new(), None, None);
        assert!(validate_runtime_backend(&state, false).is_ok());
        assert_eq!(
            validate_runtime_backend(&state, true),
            Err("production-like identity runtime requires a live PostgreSQL pool")
        );
    }

    #[test]
    fn production_hardening_clears_static_authority() {
        let mut state = AppState::new_for_tests(
            HashMap::from([(
                "legacy-static-key".to_string(),
                AuthContext {
                    org_id: Uuid::new_v4().to_string(),
                    actor_id: Some("legacy-actor".to_string()),
                    actor_label: None,
                },
            )]),
            None,
            None,
        );

        harden_runtime_state(&mut state, reqwest::Client::new(), true);
        assert!(state.api_keys.is_empty());
    }
}
