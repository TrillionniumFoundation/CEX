#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_parser_accepts_only_terminal_settlements() {
        assert!(matches!(
            parse_action("consume").unwrap(),
            SettlementAction::Consume
        ));
        assert!(matches!(
            parse_action("refund").unwrap(),
            SettlementAction::Refund
        ));
        assert!(parse_action("reserve").is_err());
    }

    #[test]
    fn retry_backoff_is_bounded_and_monotonic() {
        assert_eq!(exponential_retry_seconds(1), 1);
        assert_eq!(exponential_retry_seconds(2), 2);
        assert_eq!(exponential_retry_seconds(3), 4);
        assert_eq!(exponential_retry_seconds(100), 1_024);
    }

    #[test]
    fn serial_batch_requires_a_safe_lease_budget() {
        assert!(validate_serial_lease_budget(2, 20, 90).is_ok());
        assert!(validate_serial_lease_budget(2, 20, 60).is_err());
        assert!(validate_serial_lease_budget(3, 20, 90).is_err());
        assert!(validate_serial_lease_budget(1, 55, 60).is_err());
    }

    #[test]
    fn worker_id_rejects_ambiguous_or_oversized_values() {
        assert!(validate_worker_id("worker-1:local").is_ok());
        assert!(validate_worker_id("worker with spaces").is_err());
        assert!(validate_worker_id(&"x".repeat(129)).is_err());
    }

    #[test]
    fn final_retryable_attempt_becomes_reconciliation_not_assumed_failure() {
        let retry = PersistedOutcome::from_settlement(
            SettlementOutcome::RetryableExactReplay {
                code: "temporary".to_string(),
                http_status: Some(503),
            },
            3,
        );
        let final_outcome = enforce_attempt_boundary(retry, 3, 3);
        assert_eq!(final_outcome.outcome, "reconcile_required");
        assert_eq!(
            final_outcome.error_code.as_deref(),
            Some("retry_budget_exhausted_unknown_outcome")
        );
        assert!(final_outcome.retry_after_seconds.is_none());
    }

    #[test]
    fn settlement_outcomes_preserve_unknown_result_boundary() {
        let retry = PersistedOutcome::from_settlement(
            SettlementOutcome::RetryableExactReplay {
                code: "temporary".to_string(),
                http_status: Some(503),
            },
            2,
        );
        assert_eq!(retry.outcome, "retry_wait");
        assert_eq!(retry.retry_after_seconds, Some(2));

        let unknown = PersistedOutcome::from_settlement(
            SettlementOutcome::ReconcileRequired {
                code: "timeout_unknown".to_string(),
                http_status: None,
            },
            1,
        );
        assert_eq!(unknown.outcome, "reconcile_required");
        assert!(unknown.retry_after_seconds.is_none());
    }
}
