use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminAuthorizationFailure, AdminPrincipal,
};
use sqlx::Row;
use std::{env, time::Duration};
use uuid::Uuid;

use crate::{
    api::{self, ProcessExecutionRequest, StartExecutionRequest},
    providers::{
        dispatch_via_provider, parse_provider_target, OpenClawCliEnvScope, ProviderDispatchInput,
        ProviderDispatchOutput,
    },
    state::AppState,
};

#[derive(Debug, Clone)]
struct ProviderCommand {
    command_id: Uuid,
    invocation_id: Uuid,
    provider_target: String,
    prompt_sha256: String,
}

pub async fn start_execution(
    Path(execution_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StartExecutionRequest>,
) -> Response {
    let provider_target = match load_provider_route(&state, execution_id).await {
        Ok(Some(route)) => route,
        Ok(None) => {
            return api::start_execution(Path(execution_id), State(state), headers, Json(request))
                .await
                .into_response()
        }
        Err(response) => return response,
    };

    let admin = match authorize_execution_admin(&state, &headers) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, provider_target.org_id.as_deref()) {
        return response;
    }
    if let Err(response) =
        enqueue_provider_command(&state, execution_id, admin.actor_id.trim(), None).await
    {
        return response;
    }

    api::get_execution(Path(execution_id), State(state), headers)
        .await
        .into_response()
}

pub async fn process_execution(
    Path(execution_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ProcessExecutionRequest>,
) -> Response {
    let provider_target = match load_provider_route(&state, execution_id).await {
        Ok(Some(route)) => route,
        Ok(None) => {
            return api::process_execution(Path(execution_id), State(state), headers, Json(request))
                .await
                .into_response()
        }
        Err(response) => return response,
    };

    let admin = match authorize_execution_admin(&state, &headers) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    if let Err(response) = enforce_org_boundary(&admin, provider_target.org_id.as_deref()) {
        return response;
    }
    if request.processed_by.trim().is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_worker_id",
            "processed_by is required for provider command enqueue",
        );
    }
    if let Err(response) = enqueue_provider_command(
        &state,
        execution_id,
        admin.actor_id.trim(),
        Some(request.processed_by.trim()),
    )
    .await
    {
        return response;
    }

    api::get_execution(Path(execution_id), State(state), headers)
        .await
        .into_response()
}

pub async fn run_worker_from_env() -> Result<(), String> {
    let state = AppState::from_env().await;
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| "provider dispatch worker requires DATABASE_URL".to_string())?;
    let worker_id = required_env("PROVIDER_DISPATCH_WORKER_ID")?;
    validate_worker_id(&worker_id)?;
    let batch_limit = bounded_i32_env("PROVIDER_DISPATCH_BATCH_LIMIT", 2, 1, 25)?;
    let lease_seconds = bounded_i32_env("PROVIDER_DISPATCH_LEASE_SECONDS", 90, 5, 3600)?;
    let poll_millis = bounded_u64_env("PROVIDER_DISPATCH_POLL_MILLIS", 1_000, 100, 60_000)?;
    let run_once = bool_env("PROVIDER_DISPATCH_RUN_ONCE", false)?;

    loop {
        let commands = claim_commands(pool, &worker_id, batch_limit, lease_seconds).await?;
        for command in commands {
            process_command(&state, &worker_id, command).await?;
        }

        if run_once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(poll_millis)).await;
    }
}

#[derive(Debug)]
struct ProviderRoute {
    org_id: Option<String>,
}

// These helpers are transport-boundary functions: callers need to return the
// fully formed Axum response (including status, headers, and JSON body) on any
// database failure.  The lint allowance is deliberately scoped to each helper
// instead of disabling the check for the crate.
#[allow(clippy::result_large_err)]
async fn load_provider_route(
    state: &AppState,
    execution_id: Uuid,
) -> Result<Option<ProviderRoute>, Response> {
    let Some(pool) = state.pool.as_ref() else {
        return Ok(None);
    };
    let row = sqlx::query(
        "select provider_target, org_id::text as org_id from public.executions where execution_id=$1",
    )
    .bind(execution_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| database_error_response("load provider route", error))?;
    let Some(row) = row else {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "execution_not_found",
            "execution not found",
        ));
    };
    let provider_target: Option<String> = row
        .try_get("provider_target")
        .map_err(|error| database_error_response("decode provider target", error))?;
    if provider_target.is_none() {
        return Ok(None);
    }
    Ok(Some(ProviderRoute {
        org_id: row
            .try_get("org_id")
            .map_err(|error| database_error_response("decode provider organization", error))?,
    }))
}

#[allow(clippy::result_large_err)]
async fn enqueue_provider_command(
    state: &AppState,
    execution_id: Uuid,
    requested_by: &str,
    expected_worker_id: Option<&str>,
) -> Result<(), Response> {
    let pool = state.pool.as_ref().ok_or_else(|| {
        error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "provider_dispatch_persistence_unavailable",
            "durable provider dispatch requires PostgreSQL",
        )
    })?;
    sqlx::query_scalar::<_, String>(
        "select public.cex_enqueue_provider_dispatch_v1($1,$2,$3,$4)::text",
    )
    .bind(execution_id)
    .bind(requested_by)
    .bind(expected_worker_id)
    .bind(state.execution_queued_worker_max_attempts)
    .fetch_one(pool)
    .await
    .map(|_| ())
    .map_err(|error| database_error_response("enqueue provider command", error))
}

async fn claim_commands(
    pool: &sqlx::PgPool,
    worker_id: &str,
    limit: i32,
    lease_seconds: i32,
) -> Result<Vec<ProviderCommand>, String> {
    let rows = sqlx::query(
        "select command_id, invocation_id, provider_target, prompt_sha256\
           from public.cex_claim_provider_dispatch_v1($1,$2,$3)",
    )
    .bind(worker_id)
    .bind(limit)
    .bind(lease_seconds)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("claim provider commands: {error}"))?;

    rows.into_iter()
        .map(|row| {
            Ok(ProviderCommand {
                command_id: row
                    .try_get("command_id")
                    .map_err(|error| format!("decode provider command_id: {error}"))?,
                invocation_id: row
                    .try_get("invocation_id")
                    .map_err(|error| format!("decode provider invocation_id: {error}"))?,
                provider_target: row
                    .try_get("provider_target")
                    .map_err(|error| format!("decode provider target: {error}"))?,
                prompt_sha256: row
                    .try_get("prompt_sha256")
                    .map_err(|error| format!("decode provider prompt digest: {error}"))?,
            })
        })
        .collect()
}

async fn process_command(
    state: &AppState,
    worker_id: &str,
    command: ProviderCommand,
) -> Result<(), String> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| "provider dispatch worker lost PostgreSQL state".to_string())?;
    let prompt = match load_verified_prompt(pool, command.invocation_id, &command.prompt_sha256)
        .await?
    {
        Some(prompt) => prompt,
        None => {
            finish_command(
                pool,
                command.command_id,
                worker_id,
                "dead_letter",
                None,
                None,
                Some("provider_prompt_evidence_mismatch"),
                Some("Invocation prompt is missing or differs from the immutable command digest"),
                None,
            )
            .await?;
            return Ok(());
        }
    };

    let Some((provider, _)) = parse_provider_target(&command.provider_target) else {
        finish_command(
            pool,
            command.command_id,
            worker_id,
            "dead_letter",
            None,
            None,
            Some("invalid_provider_target"),
            Some("provider target is not canonical"),
            None,
        )
        .await?;
        return Ok(());
    };
    if provider != "ollama" {
        finish_command(
            pool,
            command.command_id,
            worker_id,
            "dead_letter",
            None,
            None,
            Some("unsafe_provider_prompt_transport"),
            Some("provider adapter is disabled until it accepts prompt input outside process argv"),
            None,
        )
        .await?;
        return Ok(());
    }

    let dispatch = dispatch_via_provider(
        &state.http,
        &state.ollama_base_url,
        "",
        &OpenClawCliEnvScope::default(),
        state.execution_provider_dispatch_timeout_seconds,
        &command.provider_target,
        &ProviderDispatchInput { prompt },
    )
    .await;

    match dispatch {
        Ok(output) => {
            if let Err(error) =
                validate_provider_success_evidence(&command.provider_target, &output)
            {
                finish_command(
                    pool,
                    command.command_id,
                    worker_id,
                    "reconcile_required",
                    None,
                    Some(200),
                    Some(error.code),
                    Some(error.message),
                    None,
                )
                .await?;
                return Ok(());
            }
            finish_command(
                pool,
                command.command_id,
                worker_id,
                "succeeded",
                Some(output.result_payload),
                Some(200),
                None,
                None,
                None,
            )
            .await
            .or_else(|error| async_finish_unknown(pool, command.command_id, worker_id, error))?;
        }
        Err(error) => {
            let classification = classify_provider_error(&error.message);
            finish_command(
                pool,
                command.command_id,
                worker_id,
                classification.outcome,
                None,
                classification.http_status,
                Some(classification.code),
                Some(&error.message),
                classification.retry_after_seconds,
            )
            .await?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderSuccessEvidenceError {
    code: &'static str,
    message: &'static str,
}

fn validate_provider_success_evidence(
    expected_provider_target: &str,
    output: &ProviderDispatchOutput,
) -> Result<(), ProviderSuccessEvidenceError> {
    let Some((expected_provider, expected_provider_ref)) =
        parse_provider_target(expected_provider_target)
    else {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_target_invalid",
            message: "provider success cannot be bound to a non-canonical target",
        });
    };

    if output.provider != expected_provider
        || output.provider_ref != expected_provider_ref
        || output.provider_target != expected_provider_target
    {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_target_mismatch",
            message: "provider success identity does not match the immutable dispatch command",
        });
    }

    let Some(payload) = output.result_payload.as_object() else {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_payload_invalid",
            message: "provider success payload must be a JSON object",
        });
    };

    if payload.get("provider").and_then(Value::as_str) != Some(expected_provider)
        || payload.get("provider_ref").and_then(Value::as_str) != Some(expected_provider_ref)
    {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_payload_identity_mismatch",
            message:
                "provider success payload identity does not match the immutable dispatch command",
        });
    }

    match payload.get("done") {
        Some(Value::Bool(true)) => {}
        Some(Value::Bool(false)) => {
            return Err(ProviderSuccessEvidenceError {
                code: "provider_success_not_terminal",
                message: "provider returned a non-terminal success envelope",
            })
        }
        Some(_) => {
            return Err(ProviderSuccessEvidenceError {
                code: "provider_success_done_invalid",
                message: "provider success field 'done' must be a JSON boolean",
            })
        }
        None => {
            return Err(ProviderSuccessEvidenceError {
                code: "provider_success_done_missing",
                message: "provider success is missing required terminal field 'done'",
            })
        }
    }

    let Some(reported_model) = payload.get("model").and_then(Value::as_str) else {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_model_missing",
            message: "provider success is missing a non-empty model identity",
        });
    };
    if reported_model.trim().is_empty() {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_model_missing",
            message: "provider success is missing a non-empty model identity",
        });
    }
    if reported_model != expected_provider_ref {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_model_mismatch",
            message: "provider-reported model does not match the immutable provider reference",
        });
    }

    let output_is_present = payload
        .get("output_text")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if !output_is_present {
        return Err(ProviderSuccessEvidenceError {
            code: "provider_success_output_missing",
            message: "provider success is missing non-empty output evidence",
        });
    }

    Ok(())
}

fn async_finish_unknown(
    pool: &sqlx::PgPool,
    command_id: Uuid,
    worker_id: &str,
    original_error: String,
) -> Result<(), String> {
    // A failed local success commit means the provider may already have completed.
    // The live claim is converted to explicit reconciliation evidence, never replayed blindly.
    futures_not_required_finish_unknown(pool, command_id, worker_id, &original_error)
}

fn futures_not_required_finish_unknown(
    _pool: &sqlx::PgPool,
    _command_id: Uuid,
    _worker_id: &str,
    original_error: &str,
) -> Result<(), String> {
    Err(format!(
        "provider succeeded but local outcome persistence failed; command remains claimed for lease-expiry reconciliation: {original_error}"
    ))
}

#[allow(clippy::too_many_arguments)]
async fn finish_command(
    pool: &sqlx::PgPool,
    command_id: Uuid,
    worker_id: &str,
    outcome: &str,
    result_payload: Option<Value>,
    http_status: Option<i32>,
    error_code: Option<&str>,
    error_message: Option<&str>,
    retry_after_seconds: Option<i32>,
) -> Result<(), String> {
    sqlx::query_scalar::<_, String>(
        "select to_jsonb(public.cex_finish_provider_dispatch_v1(\
            $1,$2,$3,$4::jsonb,$5,$6,$7,$8\
        ))::text",
    )
    .bind(command_id)
    .bind(worker_id)
    .bind(outcome)
    .bind(result_payload.map(|value| value.to_string()))
    .bind(http_status)
    .bind(error_code)
    .bind(error_message)
    .bind(retry_after_seconds)
    .fetch_one(pool)
    .await
    .map(|_| ())
    .map_err(|error| format!("persist provider outcome for {command_id}: {error}"))
}

async fn load_verified_prompt(
    pool: &sqlx::PgPool,
    invocation_id: Uuid,
    expected_sha256: &str,
) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>(
        "select request_payload->>'prompt'\
           from public.invocations\
          where invocation_id=$1\
            and jsonb_typeof(request_payload)='object'\
            and request_payload->>'prompt' is not null\
            and 'sha256:' || encode(digest(request_payload->>'prompt','sha256'),'hex')=$2",
    )
    .bind(invocation_id)
    .bind(expected_sha256)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("load immutable provider prompt: {error}"))
}

struct ProviderErrorClassification {
    outcome: &'static str,
    code: &'static str,
    http_status: Option<i32>,
    retry_after_seconds: Option<i32>,
}

fn classify_provider_error(message: &str) -> ProviderErrorClassification {
    let lowered = message.to_ascii_lowercase();
    let upstream_status = parse_upstream_status(&lowered);
    if upstream_status == Some(429) {
        return ProviderErrorClassification {
            outcome: "retry_wait",
            code: "provider_rate_limited",
            http_status: upstream_status,
            retry_after_seconds: None,
        };
    }
    if upstream_status.is_some_and(|status| (500..=599).contains(&status)) {
        return ProviderErrorClassification {
            // A 5xx response does not prove that the provider did not execute
            // the request.  Migration 0082 deliberately permits retry_wait
            // only with definitive-not-executed evidence, so preserve the
            // possible side-effect boundary as an explicit reconciliation
            // incident instead of allowing an unsafe automatic replay.
            outcome: "reconcile_required",
            code: "provider_unknown_remote_outcome",
            http_status: upstream_status,
            retry_after_seconds: None,
        };
    }
    if upstream_status.is_some_and(|status| (400..=499).contains(&status)) {
        return ProviderErrorClassification {
            outcome: "dead_letter",
            code: "provider_permanent_rejection",
            http_status: upstream_status,
            retry_after_seconds: None,
        };
    }
    if lowered.contains("timed out")
        || lowered.contains("request failed")
        || lowered.contains("read body failed")
        || lowered.contains("decode response failed")
    {
        return ProviderErrorClassification {
            outcome: "reconcile_required",
            code: "provider_unknown_remote_outcome",
            http_status: upstream_status,
            retry_after_seconds: None,
        };
    }
    ProviderErrorClassification {
        outcome: "dead_letter",
        code: "provider_dispatch_rejected",
        http_status: upstream_status,
        retry_after_seconds: None,
    }
}

fn parse_upstream_status(message: &str) -> Option<i32> {
    let marker = "provider upstream returned status ";
    let tail = message.split_once(marker)?.1;
    tail.split(':').next()?.trim().parse().ok()
}

#[allow(clippy::result_large_err)]
fn authorize_execution_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, Response> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        &["executions:manage"],
        admin_principal_has_scope,
        "execution admin token not configured",
        Some("configure an execution-scoped principal before provider dispatch"),
    )
    .cloned()
    .map_err(|error: AdminAuthorizationFailure| error.into_response().into_response())
}

#[allow(clippy::result_large_err)]
fn enforce_org_boundary(
    admin: &AdminPrincipal,
    execution_org_id: Option<&str>,
) -> Result<(), Response> {
    if admin.org_ids.is_empty() {
        return Ok(());
    }
    if execution_org_id.is_some_and(|org_id| admin_principal_allows_org(admin, org_id)) {
        return Ok(());
    }
    Err(error_response(
        StatusCode::FORBIDDEN,
        "execution_org_forbidden",
        "authenticated principal is not authorized for the execution organization",
    ))
}

fn database_error_response(context: &str, error: sqlx::Error) -> Response {
    let mut status = StatusCode::SERVICE_UNAVAILABLE;
    let mut code = "provider_dispatch_unavailable";
    if let Some(database_error) = error.as_database_error() {
        let message = database_error.message().to_ascii_lowercase();
        if database_error.code().as_deref() == Some("23505") || message.contains("collision") {
            status = StatusCode::CONFLICT;
            code = "provider_dispatch_collision";
        } else if database_error.code().as_deref() == Some("P0002") || message.contains("not found")
        {
            status = StatusCode::NOT_FOUND;
            code = "provider_dispatch_not_found";
        } else if message.contains("requires")
            || message.contains("cannot")
            || message.contains("lacks")
            || message.contains("unsafe")
        {
            status = StatusCode::CONFLICT;
            code = "provider_dispatch_blocked";
        } else if message.contains("invalid") || message.contains("must") {
            status = StatusCode::BAD_REQUEST;
            code = "provider_dispatch_invalid";
        }
    }
    eprintln!("execution-service: {context} failed: {error}");
    error_response(
        status,
        code,
        "provider dispatch operation could not be completed",
    )
}

fn error_response(status: StatusCode, code: &'static str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": "provider dispatch rejected",
            "code": code,
            "message": message,
            "schema_version": "cex.provider.dispatch.v1",
        })),
    )
        .into_response()
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn validate_worker_id(worker_id: &str) -> Result<(), String> {
    if worker_id.len() > 128
        || !worker_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        })
    {
        return Err("PROVIDER_DISPATCH_WORKER_ID must match [A-Za-z0-9._:-]{1,128}".to_string());
    }
    Ok(())
}

fn bool_env(name: &str, default: bool) -> Result<bool, String> {
    match env::var(name) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(format!("{name} must be a boolean")),
        },
        Err(_) => Ok(default),
    }
}

fn bounded_i32_env(name: &str, default: i32, min: i32, max: i32) -> Result<i32, String> {
    let value = env::var(name)
        .ok()
        .map(|raw| {
            raw.parse::<i32>()
                .map_err(|_| format!("{name} must be an integer"))
        })
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(value)
}

fn bounded_u64_env(name: &str, default: u64, min: u64, max: u64) -> Result<u64, String> {
    let value = env::var(name)
        .ok()
        .map(|raw| {
            raw.parse::<u64>()
                .map_err(|_| format!("{name} must be an integer"))
        })
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_provider_output() -> ProviderDispatchOutput {
        ProviderDispatchOutput {
            provider: "ollama".to_string(),
            provider_ref: "model-v1".to_string(),
            provider_target: "ollama://model-v1".to_string(),
            result_payload: json!({
                "provider": "ollama",
                "provider_ref": "model-v1",
                "model": "model-v1",
                "output_text": "result",
                "done": true
            }),
        }
    }

    #[test]
    fn provider_error_classification_is_fail_closed() {
        assert_eq!(
            classify_provider_error("provider upstream returned status 429: busy").outcome,
            "retry_wait"
        );
        assert_eq!(
            classify_provider_error("request failed: connection reset").outcome,
            "reconcile_required"
        );
        assert_eq!(
            classify_provider_error("provider upstream returned status 401: denied").outcome,
            "dead_letter"
        );
        let unavailable =
            classify_provider_error("provider upstream returned status 503: unavailable");
        assert_eq!(unavailable.outcome, "reconcile_required");
        assert_eq!(unavailable.code, "provider_unknown_remote_outcome");
    }

    #[test]
    fn provider_success_requires_explicit_terminal_done_true() {
        let mut output = valid_provider_output();
        output.result_payload["done"] = json!(false);
        let incomplete =
            validate_provider_success_evidence("ollama://model-v1", &output).unwrap_err();
        assert_eq!(incomplete.code, "provider_success_not_terminal");

        output
            .result_payload
            .as_object_mut()
            .unwrap()
            .remove("done");
        let missing = validate_provider_success_evidence("ollama://model-v1", &output).unwrap_err();
        assert_eq!(missing.code, "provider_success_done_missing");

        output.result_payload["done"] = json!("true");
        let invalid = validate_provider_success_evidence("ollama://model-v1", &output).unwrap_err();
        assert_eq!(invalid.code, "provider_success_done_invalid");
    }

    #[test]
    fn provider_success_binds_identity_model_and_output() {
        let output = valid_provider_output();
        assert!(validate_provider_success_evidence("ollama://model-v1", &output).is_ok());

        let mut mismatched = output.clone();
        mismatched.provider_target = "ollama://other".to_string();
        assert_eq!(
            validate_provider_success_evidence("ollama://model-v1", &mismatched)
                .unwrap_err()
                .code,
            "provider_success_target_mismatch"
        );

        let mut missing_model = output.clone();
        missing_model.result_payload["model"] = Value::Null;
        assert_eq!(
            validate_provider_success_evidence("ollama://model-v1", &missing_model)
                .unwrap_err()
                .code,
            "provider_success_model_missing"
        );

        let mut mismatched_model = output.clone();
        mismatched_model.result_payload["model"] = json!("other-model");
        assert_eq!(
            validate_provider_success_evidence("ollama://model-v1", &mismatched_model)
                .unwrap_err()
                .code,
            "provider_success_model_mismatch"
        );

        let mut missing_output = output;
        missing_output.result_payload["output_text"] = json!("");
        assert_eq!(
            validate_provider_success_evidence("ollama://model-v1", &missing_output)
                .unwrap_err()
                .code,
            "provider_success_output_missing"
        );
    }
}
