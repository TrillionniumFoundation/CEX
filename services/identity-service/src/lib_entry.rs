include!("lib.rs");

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
