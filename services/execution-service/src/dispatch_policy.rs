use shared_types::{ExecutionDispatchMode, ExecutionStatus};

use crate::{providers::parse_provider_target, state::ExecutionRecord};

pub fn dispatch_mode_for_execution(record: &ExecutionRecord) -> ExecutionDispatchMode {
    if !matches!(
        record.status,
        ExecutionStatus::Queued | ExecutionStatus::Dispatching
    ) {
        return ExecutionDispatchMode::Manual;
    }

    match record
        .provider_target
        .as_deref()
        .and_then(parse_provider_target)
        .map(|(provider, provider_ref)| (provider.trim(), provider_ref.trim()))
    {
        Some(("ollama", provider_ref)) if !provider_ref.is_empty() => {
            ExecutionDispatchMode::Immediate
        }
        Some((provider, provider_ref)) if !provider.is_empty() && !provider_ref.is_empty() => {
            ExecutionDispatchMode::QueuedWorker
        }
        _ => ExecutionDispatchMode::Manual,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::Value;
    use shared_types::ExecutionStatus;
    use uuid::Uuid;

    use super::dispatch_mode_for_execution;
    use crate::state::ExecutionRecord;

    fn sample_record(status: ExecutionStatus, provider_target: Option<&str>) -> ExecutionRecord {
        let now = Utc::now();
        ExecutionRecord {
            execution_id: Uuid::nil(),
            invocation_id: Uuid::nil(),
            trace_id: Uuid::nil(),
            org_id: None,
            status,
            provider_target: provider_target.map(str::to_string),
            dispatch_mode: Default::default(),
            attempt_count: 0,
            max_attempts: 1,
            worker_id: None,
            lease_expires_at: None,
            started_at: None,
            ended_at: None,
            result_payload: Option::<Value>::None,
            approval_required: false,
            policy_reason: None,
            approved_by: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn queued_ollama_execution_is_immediate() {
        let record = sample_record(ExecutionStatus::Queued, Some("ollama://qwen2.5:3b"));
        assert_eq!(
            dispatch_mode_for_execution(&record),
            shared_types::ExecutionDispatchMode::Immediate
        );
    }

    #[test]
    fn queued_non_ollama_provider_is_queued_worker() {
        let record = sample_record(ExecutionStatus::Queued, Some("openai://gpt-4.1-mini"));
        assert_eq!(
            dispatch_mode_for_execution(&record),
            shared_types::ExecutionDispatchMode::QueuedWorker
        );
    }

    #[test]
    fn dispatching_non_ollama_execution_stays_queued_worker() {
        let record = sample_record(ExecutionStatus::Dispatching, Some("openai://gpt-4.1-mini"));
        assert_eq!(
            dispatch_mode_for_execution(&record),
            shared_types::ExecutionDispatchMode::QueuedWorker
        );
    }

    #[test]
    fn non_queued_execution_stays_manual() {
        let record = sample_record(
            ExecutionStatus::AwaitingApproval,
            Some("ollama://qwen2.5:3b"),
        );
        assert_eq!(
            dispatch_mode_for_execution(&record),
            shared_types::ExecutionDispatchMode::Manual
        );
    }
}
