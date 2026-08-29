use std::{collections::BTreeMap, env, ffi::OsString, fmt};

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use paper_raid_bff::{config::MAX_ALPHA_IDENTITIES, db};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperatorCommand {
    SchemaMigrate,
    BatchCreate,
    BatchPause,
    BatchResume,
    BatchRevoke,
    InviteIssue,
    InviteReissue,
    InviteRevoke,
    CredentialRotate,
    AccountSuspend,
    AccountReactivate,
    AccountClose,
    AccountExport,
    Prune,
    Unsupported,
}

impl OperatorCommand {
    fn from_raw(value: Option<&str>) -> Self {
        match value {
            Some("schema-migrate") => Self::SchemaMigrate,
            Some("batch-create") => Self::BatchCreate,
            Some("batch-pause") => Self::BatchPause,
            Some("batch-resume") => Self::BatchResume,
            Some("batch-revoke") => Self::BatchRevoke,
            Some("invite-issue") => Self::InviteIssue,
            Some("invite-reissue") => Self::InviteReissue,
            Some("invite-revoke") => Self::InviteRevoke,
            Some("credential-rotate") => Self::CredentialRotate,
            Some("account-suspend") => Self::AccountSuspend,
            Some("account-reactivate") => Self::AccountReactivate,
            Some("account-close") => Self::AccountClose,
            Some("account-export") => Self::AccountExport,
            Some("prune") => Self::Prune,
            _ => Self::Unsupported,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::SchemaMigrate => "schema_migrate",
            Self::BatchCreate => "batch_create",
            Self::BatchPause => "batch_pause",
            Self::BatchResume => "batch_resume",
            Self::BatchRevoke => "batch_revoke",
            Self::InviteIssue => "invite_issue",
            Self::InviteReissue => "invite_reissue",
            Self::InviteRevoke => "invite_revoke",
            Self::CredentialRotate => "credential_rotate",
            Self::AccountSuspend => "account_suspend",
            Self::AccountReactivate => "account_reactivate",
            Self::AccountClose => "account_close",
            Self::AccountExport => "account_export",
            Self::Prune => "prune",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperatorAuditEvent {
    Attempt,
    Result,
}

impl OperatorAuditEvent {
    fn action(self) -> &'static str {
        match self {
            Self::Attempt => "operator_command_attempt",
            Self::Result => "operator_command_result",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperatorAuditOutcome {
    Succeeded,
    Denied,
    Indeterminate,
}

impl OperatorAuditOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Denied => "denied",
            Self::Indeterminate => "indeterminate",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandReason {
    AttemptRecorded,
    CommandCommitted,
    CommandRejected,
    UnsupportedCommand,
    DatabaseOperationFailed,
    IdentityModeRejected,
    DatabaseConfigurationMissing,
    OperatorConfigurationMissing,
    OperatorConfigurationRejected,
    RetentionConfigurationMissing,
    RetentionConfigurationRejected,
    DatabaseConnectUnavailable,
    DatabaseMigrationUnavailable,
    SchemaActivationRequired,
    CommandNotRun,
}

impl CommandReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::AttemptRecorded => "attempt_recorded",
            Self::CommandCommitted => "command_committed",
            Self::CommandRejected => "command_rejected",
            Self::UnsupportedCommand => "unsupported_command",
            Self::DatabaseOperationFailed => "database_operation_failed",
            Self::IdentityModeRejected => "identity_mode_rejected",
            Self::DatabaseConfigurationMissing => "database_configuration_missing",
            Self::OperatorConfigurationMissing => "operator_configuration_missing",
            Self::OperatorConfigurationRejected => "operator_configuration_rejected",
            Self::RetentionConfigurationMissing => "retention_configuration_missing",
            Self::RetentionConfigurationRejected => "retention_configuration_rejected",
            Self::DatabaseConnectUnavailable => "database_connect_unavailable",
            Self::DatabaseMigrationUnavailable => "database_migration_unavailable",
            Self::SchemaActivationRequired => "schema_activation_required",
            Self::CommandNotRun => "command_not_run",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandState {
    NotRun,
    NotCommitted,
    Committed,
    Unknown,
}

impl CommandState {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotRun => "not_run",
            Self::NotCommitted => "not_committed",
            Self::Committed => "committed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuditStatus {
    Unavailable,
    Recorded,
    Failed,
    Unknown,
}

impl AuditStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Recorded => "recorded",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuditFailureReason {
    AttemptWriteFailed,
    ResultWriteFailed,
    AttemptCommitUnknown,
    ResultCommitUnknown,
}

impl AuditFailureReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::AttemptWriteFailed => "attempt_audit_write_failed",
            Self::ResultWriteFailed => "result_audit_write_failed",
            Self::AttemptCommitUnknown => "attempt_audit_commit_unknown",
            Self::ResultCommitUnknown => "result_audit_commit_unknown",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CliFailure {
    command: OperatorCommand,
    command_state: CommandState,
    audit_status: AuditStatus,
    reason: CommandReason,
    audit_failure_reason: Option<AuditFailureReason>,
    attempt_id: Option<Uuid>,
}

#[derive(Clone, Copy, Debug)]
struct CommandContext {
    attempt_id: Uuid,
    command: OperatorCommand,
}

#[derive(Clone, Copy, Debug)]
struct OperatorAuditRecord {
    attempt_id: Uuid,
    command: OperatorCommand,
    event: OperatorAuditEvent,
    outcome: OperatorAuditOutcome,
    command_state: CommandState,
    reason: CommandReason,
}

const MAX_ARGUMENT_TOKENS: usize = 32;
const MAX_OPTION_NAME_BYTES: usize = 64;
const MAX_OPTION_VALUE_BYTES: usize = 4096;

#[derive(Debug)]
struct CommitAckUnknown;

impl fmt::Display for CommitAckUnknown {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("commit acknowledgement unavailable")
    }
}

impl std::error::Error for CommitAckUnknown {}

#[derive(Debug)]
struct DatabaseOperationNotCommitted;

impl fmt::Display for DatabaseOperationNotCommitted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("database operation not committed")
    }
}

impl std::error::Error for DatabaseOperationNotCommitted {}

impl CliFailure {
    fn audit_unavailable(command: OperatorCommand, reason: CommandReason) -> Self {
        Self {
            command,
            command_state: CommandState::NotRun,
            audit_status: AuditStatus::Unavailable,
            reason,
            audit_failure_reason: None,
            attempt_id: None,
        }
    }

    fn as_json(self) -> Value {
        json!({
            "schema":"paper-raid-bff.accessctl.failure.v1",
            "command_code":self.command.as_str(),
            "command_state":self.command_state.as_str(),
            "audit_status":self.audit_status.as_str(),
            "reason_code":self.reason.as_str(),
            "audit_failure_reason_code":self.audit_failure_reason.map(AuditFailureReason::as_str),
            "attempt_id":self.attempt_id
        })
    }
}

fn audit_failure_truth(
    error: &anyhow::Error,
    event: OperatorAuditEvent,
) -> (AuditStatus, AuditFailureReason) {
    if error_chain_contains::<CommitAckUnknown>(error) {
        (
            AuditStatus::Unknown,
            match event {
                OperatorAuditEvent::Attempt => AuditFailureReason::AttemptCommitUnknown,
                OperatorAuditEvent::Result => AuditFailureReason::ResultCommitUnknown,
            },
        )
    } else {
        (
            AuditStatus::Failed,
            match event {
                OperatorAuditEvent::Attempt => AuditFailureReason::AttemptWriteFailed,
                OperatorAuditEvent::Result => AuditFailureReason::ResultWriteFailed,
            },
        )
    }
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(output) => println!("{output}"),
        Err(failure) => {
            eprintln!("{}", failure.as_json());
            std::process::exit(2);
        }
    }
}

async fn run() -> std::result::Result<Value, CliFailure> {
    let (command, parsed_args) = parse_args_os(env::args_os().skip(1));
    if command == OperatorCommand::Unsupported {
        return Err(reject_unsupported_command().await);
    }
    let database_url = required_env("PAPER_RAID_ACCESS_DATABASE_URL").map_err(|_| {
        CliFailure::audit_unavailable(command, CommandReason::DatabaseConfigurationMissing)
    })?;
    let operator = required_env("PAPER_RAID_ACCESS_OPERATOR_SUBJECT").map_err(|_| {
        CliFailure::audit_unavailable(command, CommandReason::OperatorConfigurationMissing)
    })?;
    validate_identifier("operator subject", &operator).map_err(|_| {
        CliFailure::audit_unavailable(command, CommandReason::OperatorConfigurationRejected)
    })?;
    let pool = db::connect(&database_url).await.map_err(|_| {
        CliFailure::audit_unavailable(command, CommandReason::DatabaseConnectUnavailable)
    })?;

    if command == OperatorCommand::SchemaMigrate {
        return schema_migrate(&pool, &operator, parsed_args).await;
    }
    if !db::invite_schema_ready(&pool).await {
        return Err(CliFailure::audit_unavailable(
            command,
            CommandReason::SchemaActivationRequired,
        ));
    }

    let attempt_id = Uuid::new_v4();
    let context = CommandContext {
        attempt_id,
        command,
    };
    if let Err(error) = write_operator_audit(
        &pool,
        &operator,
        OperatorAuditRecord {
            attempt_id,
            command,
            event: OperatorAuditEvent::Attempt,
            outcome: OperatorAuditOutcome::Succeeded,
            command_state: CommandState::NotRun,
            reason: CommandReason::AttemptRecorded,
        },
    )
    .await
    {
        let (audit_status, audit_failure_reason) =
            audit_failure_truth(&error, OperatorAuditEvent::Attempt);
        return Err(CliFailure {
            command,
            command_state: CommandState::NotRun,
            audit_status,
            reason: CommandReason::CommandNotRun,
            audit_failure_reason: Some(audit_failure_reason),
            attempt_id: Some(attempt_id),
        });
    }

    if env::var("PAPER_RAID_BFF_IDENTITY_MODE").as_deref() != Ok("invite_alpha") {
        return Err(audited_command_failure(
            &pool,
            &operator,
            context,
            CommandState::NotRun,
            CommandReason::IdentityModeRejected,
        )
        .await);
    }
    let args = match parsed_args {
        Ok(parsed) => parsed,
        Err(_) => {
            return Err(audited_command_failure(
                &pool,
                &operator,
                context,
                CommandState::NotRun,
                CommandReason::CommandRejected,
            )
            .await)
        }
    };

    let retention_policy_id = if command == OperatorCommand::Prune {
        let value = match required_env("PAPER_RAID_BFF_ACCESS_RETENTION_POLICY_ID") {
            Ok(value) => value,
            Err(_) => {
                return Err(audited_command_failure(
                    &pool,
                    &operator,
                    context,
                    CommandState::NotRun,
                    CommandReason::RetentionConfigurationMissing,
                )
                .await)
            }
        };
        if validate_identifier("retention policy id", &value).is_err() {
            return Err(audited_command_failure(
                &pool,
                &operator,
                context,
                CommandState::NotRun,
                CommandReason::RetentionConfigurationRejected,
            )
            .await);
        }
        Some(value)
    } else {
        None
    };

    let command_result = match command {
        OperatorCommand::BatchCreate => batch_create(&pool, &operator, &args, context).await,
        OperatorCommand::BatchPause => {
            batch_state(&pool, &operator, &args, "paused", context).await
        }
        OperatorCommand::BatchResume => {
            batch_state(&pool, &operator, &args, "active", context).await
        }
        OperatorCommand::BatchRevoke => {
            batch_state(&pool, &operator, &args, "revoked", context).await
        }
        OperatorCommand::InviteIssue => invite_issue(&pool, &operator, &args, context).await,
        OperatorCommand::InviteReissue => invite_reissue(&pool, &operator, &args, context).await,
        OperatorCommand::InviteRevoke => invite_revoke(&pool, &operator, &args, context).await,
        OperatorCommand::CredentialRotate => {
            credential_rotate(&pool, &operator, &args, context).await
        }
        OperatorCommand::AccountSuspend => {
            account_state(&pool, &operator, &args, "suspended", context).await
        }
        OperatorCommand::AccountReactivate => {
            account_state(&pool, &operator, &args, "active", context).await
        }
        OperatorCommand::AccountClose => {
            account_state(&pool, &operator, &args, "closed", context).await
        }
        OperatorCommand::AccountExport => account_export(&pool, &operator, &args, context).await,
        OperatorCommand::Prune => {
            prune(
                &pool,
                &operator,
                retention_policy_id.as_deref().expect("prune policy is set"),
                &args,
                context,
            )
            .await
        }
        OperatorCommand::SchemaMigrate => unreachable!("schema migration handled above"),
        OperatorCommand::Unsupported => unreachable!("unsupported command rejected above"),
    };

    match command_result {
        Ok(output) => Ok(output),
        Err(error) => {
            let (command_state, reason) = if error_chain_contains::<CommitAckUnknown>(&error) {
                (
                    CommandState::Unknown,
                    CommandReason::DatabaseOperationFailed,
                )
            } else if error_chain_contains::<DatabaseOperationNotCommitted>(&error)
                || error_chain_contains_sqlx(&error)
            {
                (
                    CommandState::NotCommitted,
                    CommandReason::DatabaseOperationFailed,
                )
            } else {
                (CommandState::NotCommitted, CommandReason::CommandRejected)
            };
            Err(audited_command_failure(&pool, &operator, context, command_state, reason).await)
        }
    }
}

async fn reject_unsupported_command() -> CliFailure {
    let command = OperatorCommand::Unsupported;
    let unavailable = || CliFailure::audit_unavailable(command, CommandReason::UnsupportedCommand);
    let Ok(database_url) = required_env("PAPER_RAID_ACCESS_DATABASE_URL") else {
        return unavailable();
    };
    let Ok(operator) = required_env("PAPER_RAID_ACCESS_OPERATOR_SUBJECT") else {
        return unavailable();
    };
    if validate_identifier("operator subject", &operator).is_err() {
        return unavailable();
    }
    let Ok(pool) = db::connect(&database_url).await else {
        return unavailable();
    };
    if !db::invite_schema_ready(&pool).await {
        return unavailable();
    }
    let context = CommandContext {
        attempt_id: Uuid::new_v4(),
        command,
    };
    if let Err(error) = write_operator_audit(
        &pool,
        &operator,
        OperatorAuditRecord {
            attempt_id: context.attempt_id,
            command,
            event: OperatorAuditEvent::Attempt,
            outcome: OperatorAuditOutcome::Succeeded,
            command_state: CommandState::NotRun,
            reason: CommandReason::AttemptRecorded,
        },
    )
    .await
    {
        let (audit_status, audit_failure_reason) =
            audit_failure_truth(&error, OperatorAuditEvent::Attempt);
        return CliFailure {
            command,
            command_state: CommandState::NotRun,
            audit_status,
            reason: CommandReason::UnsupportedCommand,
            audit_failure_reason: Some(audit_failure_reason),
            attempt_id: Some(context.attempt_id),
        };
    }
    audited_command_failure(
        &pool,
        &operator,
        context,
        CommandState::NotRun,
        CommandReason::UnsupportedCommand,
    )
    .await
}

async fn schema_migrate(
    pool: &PgPool,
    operator: &str,
    parsed_args: Result<ParsedArgs>,
) -> std::result::Result<Value, CliFailure> {
    let command = OperatorCommand::SchemaMigrate;
    let attempt_id = Uuid::new_v4();
    let context = CommandContext {
        attempt_id,
        command,
    };
    let schema_ready_before = db::invite_schema_ready(pool).await;
    let attempt_audit_status = if schema_ready_before {
        if let Err(error) = write_operator_audit(
            pool,
            operator,
            OperatorAuditRecord {
                attempt_id,
                command,
                event: OperatorAuditEvent::Attempt,
                outcome: OperatorAuditOutcome::Succeeded,
                command_state: CommandState::NotRun,
                reason: CommandReason::AttemptRecorded,
            },
        )
        .await
        {
            let (audit_status, audit_failure_reason) =
                audit_failure_truth(&error, OperatorAuditEvent::Attempt);
            return Err(CliFailure {
                command,
                command_state: CommandState::NotRun,
                audit_status,
                reason: CommandReason::CommandNotRun,
                audit_failure_reason: Some(audit_failure_reason),
                attempt_id: Some(attempt_id),
            });
        }
        AuditStatus::Recorded
    } else {
        AuditStatus::Unavailable
    };

    let unaudited_failure = |reason| CliFailure {
        command,
        command_state: CommandState::NotRun,
        audit_status: AuditStatus::Unavailable,
        reason,
        audit_failure_reason: None,
        attempt_id: Some(attempt_id),
    };
    if env::var("PAPER_RAID_BFF_IDENTITY_MODE").as_deref() != Ok("invite_alpha") {
        return if schema_ready_before {
            Err(audited_command_failure(
                pool,
                operator,
                context,
                CommandState::NotRun,
                CommandReason::IdentityModeRejected,
            )
            .await)
        } else {
            Err(unaudited_failure(CommandReason::IdentityModeRejected))
        };
    }
    let args = match parsed_args {
        Ok(args) if args.only(&[]).is_ok() => args,
        _ => {
            return if schema_ready_before {
                Err(audited_command_failure(
                    pool,
                    operator,
                    context,
                    CommandState::NotRun,
                    CommandReason::CommandRejected,
                )
                .await)
            } else {
                Err(unaudited_failure(CommandReason::CommandRejected))
            }
        }
    };
    let _ = args;
    // The closed readiness read is the activation truth even if the migration
    // call's final acknowledgement was lost. Every migration is idempotent.
    let _migration_call_succeeded = db::migrate(pool).await.is_ok();
    if !db::invite_schema_ready(pool).await {
        return if schema_ready_before {
            Err(audited_command_failure(
                pool,
                operator,
                context,
                CommandState::NotCommitted,
                CommandReason::DatabaseMigrationUnavailable,
            )
            .await)
        } else {
            Err(CliFailure {
                command,
                command_state: CommandState::NotCommitted,
                audit_status: AuditStatus::Unavailable,
                reason: CommandReason::DatabaseMigrationUnavailable,
                audit_failure_reason: None,
                attempt_id: Some(attempt_id),
            })
        };
    }
    let result_record = OperatorAuditRecord {
        attempt_id,
        command,
        event: OperatorAuditEvent::Result,
        outcome: OperatorAuditOutcome::Succeeded,
        command_state: CommandState::Committed,
        reason: CommandReason::CommandCommitted,
    };
    if let Err(error) = write_operator_audit(pool, operator, result_record).await {
        let (audit_status, audit_failure_reason) =
            audit_failure_truth(&error, OperatorAuditEvent::Result);
        return Err(CliFailure {
            command,
            command_state: CommandState::Committed,
            audit_status,
            reason: CommandReason::CommandCommitted,
            audit_failure_reason: Some(audit_failure_reason),
            attempt_id: Some(attempt_id),
        });
    }
    Ok(json!({
        "schema":"paper-raid-bff.schema-migrate-result.v1",
        "command_code":command.as_str(),
        "command_state":CommandState::Committed.as_str(),
        "attempt_id":attempt_id,
        "schema_ready_before":schema_ready_before,
        "schema_ready_after":true,
        "attempt_audit_status":attempt_audit_status.as_str(),
        "result_audit_status":AuditStatus::Recorded.as_str()
    }))
}

async fn batch_create(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["label", "max-issued", "expires-at"])?;
    let label = args.required("label")?;
    validate_label(label)?;
    let max_issued = args
        .required("max-issued")?
        .parse::<i32>()
        .map_err(|_| anyhow!("max-issued must be an integer"))?;
    if !(1..=MAX_ALPHA_IDENTITIES as i32).contains(&max_issued) {
        bail!("max-issued must be between 1 and {MAX_ALPHA_IDENTITIES}");
    }
    let expires_at = parse_expiry(args.required("expires-at")?)?;
    if expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("batch expiry must be in the future");
    }
    let batch_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO paper_raid_bff_invite_batches \
         (batch_id, label, state, max_issued, expires_at) \
         VALUES ($1, $2, 'active', $3, $4)",
    )
    .bind(batch_id)
    .bind(label)
    .bind(max_issued)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        None,
        "batch_create",
        json!({"batch_id": batch_id}),
    )
    .await?;
    let output = json!({"schema":"paper-raid-bff.accessctl.result.v1","batch_id":batch_id,"state":"active","max_issued":max_issued,"expires_at":expires_at});
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn batch_state(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    state: &str,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["batch-id"])?;
    let batch_id = parse_uuid(args.required("batch-id")?, "batch-id")?;
    let mut tx = pool.begin().await?;
    let update = if state == "active" {
        "UPDATE paper_raid_bff_invite_batches SET state = $1, updated_at = now() \
         WHERE batch_id = $2 AND state <> 'revoked' \
           AND (expires_at IS NULL OR expires_at > now())"
    } else {
        "UPDATE paper_raid_bff_invite_batches SET state = $1, updated_at = now() \
         WHERE batch_id = $2 AND state <> 'revoked'"
    };
    let result = sqlx::query(update)
        .bind(state)
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 1 {
        bail!("batch not found or already revoked");
    }
    if state == "revoked" {
        sqlx::query(
            "UPDATE paper_raid_bff_invites SET state = 'revoked', revoked_at = now() \
             WHERE batch_id = $1 AND state = 'issued'",
        )
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_accounts a SET state = 'closed', closed_at = now(), updated_at = now() \
             FROM paper_raid_bff_invites i \
             WHERE i.batch_id = $1 AND i.account_id = a.account_id AND a.state = 'invited'",
        )
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
    }
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        None,
        &format!("batch_{state}"),
        json!({"batch_id": batch_id}),
    )
    .await?;
    let output =
        json!({"schema":"paper-raid-bff.accessctl.result.v1","batch_id":batch_id,"state":state});
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn invite_issue(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    context: CommandContext,
) -> Result<Value> {
    args.only(&[
        "batch-id",
        "subject",
        "display-name",
        "nakama-user-id",
        "player-id",
        "scopes",
        "author-roles",
        "expires-at",
        "credential-expires-at",
    ])?;
    let batch_id = parse_uuid(args.required("batch-id")?, "batch-id")?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let display_name = args.required("display-name")?;
    validate_display_name(display_name)?;
    let nakama_user_id = parse_uuid(args.required("nakama-user-id")?, "nakama-user-id")?;
    let player_id = parse_uuid(args.required("player-id")?, "player-id")?;
    let scopes = parse_set(
        args.required("scopes")?,
        &["author", "evaluator", "reviewer", "reproducer"],
        false,
    )?;
    let roles = parse_set(
        args.required("author-roles")?,
        &["captain", "evidence", "experiment"],
        true,
    )?;
    let has_author_scope = scopes.iter().any(|scope| scope == "author");
    if has_author_scope == roles.is_empty() {
        bail!("author scope requires at least one author role; non-authors require none");
    }
    let expires_at = parse_expiry(args.required("expires-at")?)?;
    if expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("invite expiry must be in the future");
    }
    let credential_expires_at = parse_expiry(args.required("credential-expires-at")?)?;
    if credential_expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("credential expiry must be in the future");
    }
    let secret = random_secret();
    let secret_hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
    let account_id = Uuid::new_v4();
    let invite_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(742381917)")
        .execute(&mut *tx)
        .await?;
    let account_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM paper_raid_bff_accounts WHERE state <> 'closed'")
            .fetch_one(&mut *tx)
            .await?;
    if account_count >= MAX_ALPHA_IDENTITIES as i64 {
        bail!("invite alpha account cap reached");
    }
    let batch = sqlx::query(
        "SELECT state, expires_at, max_issued, issued_count \
         FROM paper_raid_bff_invite_batches \
         WHERE batch_id = $1 FOR UPDATE",
    )
    .bind(batch_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("batch not found"))?;
    let batch_state: String = batch.try_get("state")?;
    let batch_expires_at: Option<DateTime<Utc>> = batch.try_get("expires_at")?;
    let max_issued: i32 = batch.try_get("max_issued")?;
    let issued_count: i32 = batch.try_get("issued_count")?;
    if batch_state != "active" || batch_expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("batch is not active");
    }
    if issued_count >= max_issued {
        bail!("batch issue limit reached");
    }
    if let (Some(invite_expiry), Some(batch_expiry)) = (expires_at, batch_expires_at) {
        if invite_expiry > batch_expiry {
            bail!("invite expiry exceeds batch expiry");
        }
    }
    sqlx::query(
        "INSERT INTO paper_raid_bff_accounts \
         (account_id, subject_id, display_name, nakama_user_id, player_id, state) \
         VALUES ($1, $2, $3, $4, $5, 'invited')",
    )
    .bind(account_id)
    .bind(subject)
    .bind(display_name)
    .bind(nakama_user_id)
    .bind(player_id)
    .execute(&mut *tx)
    .await?;
    for scope in &scopes {
        sqlx::query("INSERT INTO paper_raid_bff_account_scopes(account_id, scope) VALUES ($1, $2)")
            .bind(account_id)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
    }
    for role in &roles {
        sqlx::query(
            "INSERT INTO paper_raid_bff_account_author_roles(account_id, author_role) \
             VALUES ($1, $2)",
        )
        .bind(account_id)
        .bind(role)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO paper_raid_bff_invites \
         (invite_id, batch_id, account_id, secret_hash, state, expires_at, credential_expires_at) \
         VALUES ($1, $2, $3, $4, 'issued', $5, $6)",
    )
    .bind(invite_id)
    .bind(batch_id)
    .bind(account_id)
    .bind(secret_hash.as_slice())
    .bind(expires_at)
    .bind(credential_expires_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE paper_raid_bff_invite_batches \
         SET issued_count = issued_count + 1, updated_at = now() \
         WHERE batch_id = $1 AND issued_count < max_issued",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        Some(account_id),
        "invite_issue",
        json!({"invite_id":invite_id,"batch_id":batch_id}),
    )
    .await?;
    let output = json!({
        "schema":"paper-raid-bff.accessctl.secret-result.v1",
        "invite_id":invite_id,
        "account_id":account_id,
        "subject_id":subject,
        "login_credential":secret,
        "secret_delivery":"displayed_once_not_stored",
        "invite_expires_at":expires_at,
        "credential_expires_at":credential_expires_at
    });
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn invite_reissue(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["subject"])?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let secret = random_secret();
    let secret_hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT a.account_id, i.invite_id, i.batch_id, i.expires_at, \
                i.credential_expires_at \
         FROM paper_raid_bff_accounts a \
         JOIN paper_raid_bff_invites i ON i.account_id = a.account_id \
         WHERE a.subject_id = $1 AND a.state = 'invited' AND i.state = 'issued' \
           AND (i.expires_at IS NULL OR i.expires_at > now()) \
         FOR UPDATE OF a, i",
    )
    .bind(subject)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("issued invitation for invited subject not found"))?;
    let account_id: Uuid = row.try_get("account_id")?;
    let invite_id: Uuid = row.try_get("invite_id")?;
    let batch_id: Uuid = row.try_get("batch_id")?;
    let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at")?;
    let credential_expires_at: Option<DateTime<Utc>> = row.try_get("credential_expires_at")?;
    let update = sqlx::query(
        "UPDATE paper_raid_bff_invites SET secret_hash = $1 \
         WHERE invite_id = $2 AND account_id = $3 AND state = 'issued'",
    )
    .bind(secret_hash.as_slice())
    .bind(invite_id)
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    if update.rows_affected() != 1 {
        bail!("issued invitation changed during reissue");
    }
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        Some(account_id),
        "invite_reissue",
        json!({"invite_id":invite_id,"batch_id":batch_id}),
    )
    .await?;
    let output = json!({
        "schema":"paper-raid-bff.accessctl.secret-result.v1",
        "invite_id":invite_id,
        "account_id":account_id,
        "subject_id":subject,
        "login_credential":secret,
        "secret_delivery":"displayed_once_not_stored",
        "invite_expires_at":expires_at,
        "credential_expires_at":credential_expires_at,
        "batch_capacity_consumed":false
    });
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn invite_revoke(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["invite-id"])?;
    let invite_id = parse_uuid(args.required("invite-id")?, "invite-id")?;
    let mut tx = pool.begin().await?;
    let account_id: Uuid = sqlx::query_scalar(
        "UPDATE paper_raid_bff_invites SET state = 'revoked', revoked_at = now() \
         WHERE invite_id = $1 AND state = 'issued' RETURNING account_id",
    )
    .bind(invite_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("issued invite not found"))?;
    sqlx::query(
        "UPDATE paper_raid_bff_accounts SET state = 'closed', closed_at = now(), updated_at = now() \
         WHERE account_id = $1 AND state = 'invited'",
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        Some(account_id),
        "invite_revoke",
        json!({"invite_id":invite_id}),
    )
    .await?;
    let output = json!({"schema":"paper-raid-bff.accessctl.result.v1","invite_id":invite_id,"state":"revoked"});
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn credential_rotate(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["subject", "expires-at"])?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let expires_at = parse_expiry(args.required("expires-at")?)?;
    if expires_at.is_some_and(|value| value <= Utc::now()) {
        bail!("credential expiry must be in the future");
    }
    let secret = random_secret();
    let secret_hash: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
    let credential_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    let account_id: Uuid = sqlx::query_scalar(
        "SELECT account_id FROM paper_raid_bff_accounts \
         WHERE subject_id = $1 AND state = 'active' FOR UPDATE",
    )
    .bind(subject)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("active account not found"))?;
    sqlx::query(
        "UPDATE paper_raid_bff_login_credentials SET state = 'revoked', revoked_at = now() \
         WHERE account_id = $1 AND state = 'active'",
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO paper_raid_bff_login_credentials \
         (credential_id, account_id, secret_hash, state, expires_at) \
         VALUES ($1, $2, $3, 'active', $4)",
    )
    .bind(credential_id)
    .bind(account_id)
    .bind(secret_hash.as_slice())
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    revoke_sessions(&mut tx, subject).await?;
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        Some(account_id),
        "credential_rotate",
        json!({"credential_id":credential_id}),
    )
    .await?;
    let output = json!({
        "schema":"paper-raid-bff.accessctl.secret-result.v1",
        "subject_id":subject,
        "credential_id":credential_id,
        "login_credential":secret,
        "secret_delivery":"displayed_once_not_stored",
        "expires_at":expires_at
    });
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn account_state(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    target: &str,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["subject"])?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let mut tx = pool.begin().await?;
    let query = match target {
        "suspended" => {
            "UPDATE paper_raid_bff_accounts SET state = 'suspended', suspended_at = now(), updated_at = now() WHERE subject_id = $1 AND state = 'active' RETURNING account_id"
        }
        "active" => {
            "UPDATE paper_raid_bff_accounts SET state = 'active', suspended_at = NULL, updated_at = now() WHERE subject_id = $1 AND state = 'suspended' RETURNING account_id"
        }
        "closed" => {
            "UPDATE paper_raid_bff_accounts SET state = 'closed', closed_at = now(), updated_at = now() WHERE subject_id = $1 AND state <> 'closed' RETURNING account_id"
        }
        _ => bail!("invalid account target state"),
    };
    let account_id: Uuid = sqlx::query_scalar(query)
        .bind(subject)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow!("account is not in the required source state"))?;
    if target != "active" {
        revoke_sessions(&mut tx, subject).await?;
    }
    if target == "closed" {
        sqlx::query(
            "UPDATE paper_raid_bff_login_credentials SET state = 'revoked', revoked_at = now() \
             WHERE account_id = $1 AND state = 'active'",
        )
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE paper_raid_bff_invites SET state = 'revoked', revoked_at = now() \
             WHERE account_id = $1 AND state = 'issued'",
        )
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    }
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        Some(account_id),
        &format!("account_{target}"),
        json!({}),
    )
    .await?;
    let output =
        json!({"schema":"paper-raid-bff.accessctl.result.v1","subject_id":subject,"state":target});
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn account_export(
    pool: &PgPool,
    operator: &str,
    args: &Args,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["subject"])?;
    let subject = args.required("subject")?;
    validate_identifier("subject", subject)?;
    let mut snapshot = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *snapshot)
        .await?;
    let account = sqlx::query(
        "SELECT account_id, subject_id, display_name, nakama_user_id, player_id, state, \
                created_at, activated_at, suspended_at, closed_at, updated_at \
         FROM paper_raid_bff_accounts WHERE subject_id = $1",
    )
    .bind(subject)
    .fetch_optional(&mut *snapshot)
    .await?
    .ok_or_else(|| anyhow!("account not found"))?;
    let account_id: Uuid = account.try_get("account_id")?;
    let scopes: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM paper_raid_bff_account_scopes WHERE account_id = $1 ORDER BY scope",
    )
    .bind(account_id)
    .fetch_all(&mut *snapshot)
    .await?;
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT author_role FROM paper_raid_bff_account_author_roles WHERE account_id = $1 ORDER BY author_role",
    )
    .bind(account_id)
    .fetch_all(&mut *snapshot)
    .await?;
    let credential_rows = sqlx::query(
        "SELECT credential_id, state, expires_at, created_at, last_used_at, revoked_at \
         FROM paper_raid_bff_login_credentials WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(account_id)
    .fetch_all(&mut *snapshot)
    .await?;
    let mut credentials = Vec::with_capacity(credential_rows.len());
    for row in credential_rows {
        credentials.push(json!({
            "credential_id":row.try_get::<Uuid,_>("credential_id")?,
            "state":row.try_get::<String,_>("state")?,
            "expires_at":row.try_get::<Option<DateTime<Utc>>,_>("expires_at")?,
            "created_at":row.try_get::<DateTime<Utc>,_>("created_at")?,
            "last_used_at":row.try_get::<Option<DateTime<Utc>>,_>("last_used_at")?,
            "revoked_at":row.try_get::<Option<DateTime<Utc>>,_>("revoked_at")?
        }));
    }
    let invite_rows = sqlx::query(
        "SELECT invite_id, batch_id, state, expires_at, credential_expires_at, \
                created_at, redeemed_at, revoked_at \
         FROM paper_raid_bff_invites WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(account_id)
    .fetch_all(&mut *snapshot)
    .await?;
    let mut invites = Vec::with_capacity(invite_rows.len());
    for row in invite_rows {
        invites.push(json!({
            "invite_id":row.try_get::<Uuid,_>("invite_id")?,
            "batch_id":row.try_get::<Uuid,_>("batch_id")?,
            "state":row.try_get::<String,_>("state")?,
            "expires_at":row.try_get::<Option<DateTime<Utc>>,_>("expires_at")?,
            "credential_expires_at":row.try_get::<Option<DateTime<Utc>>,_>("credential_expires_at")?,
            "created_at":row.try_get::<DateTime<Utc>,_>("created_at")?,
            "redeemed_at":row.try_get::<Option<DateTime<Utc>>,_>("redeemed_at")?,
            "revoked_at":row.try_get::<Option<DateTime<Utc>>,_>("revoked_at")?
        }));
    }
    let audit_rows = sqlx::query(
        "SELECT audit_id, operator_subject IS NOT NULL AS operator_subject_present, \
                action, outcome, metadata, operator_lineage_status, occurred_at \
         FROM paper_raid_bff_access_audit WHERE account_id = $1 ORDER BY occurred_at",
    )
    .bind(account_id)
    .fetch_all(&mut *snapshot)
    .await?;
    let mut access_audit = Vec::with_capacity(audit_rows.len());
    for row in audit_rows {
        access_audit.push(project_access_audit_row(&row)?);
    }
    if snapshot.commit().await.is_err() {
        return Err(anyhow!(DatabaseOperationNotCommitted));
    }
    let credential_count = credentials.len();
    let export_boundary = account_export_boundary(
        scopes.len(),
        roles.len(),
        credential_count,
        invites.len(),
        access_audit.len(),
    );
    let export = json!({
        "schema":"paper-raid-bff.account-export.v2",
        "export_boundary":export_boundary,
        "account_id":account_id,
        "subject_id":account.try_get::<String,_>("subject_id")?,
        "display_name":account.try_get::<String,_>("display_name")?,
        "nakama_user_id":account.try_get::<Uuid,_>("nakama_user_id")?,
        "player_id":account.try_get::<Uuid,_>("player_id")?,
        "state":account.try_get::<String,_>("state")?,
        "created_at":account.try_get::<DateTime<Utc>,_>("created_at")?,
        "activated_at":account.try_get::<Option<DateTime<Utc>>,_>("activated_at")?,
        "suspended_at":account.try_get::<Option<DateTime<Utc>>,_>("suspended_at")?,
        "closed_at":account.try_get::<Option<DateTime<Utc>>,_>("closed_at")?,
        "updated_at":account.try_get::<DateTime<Utc>,_>("updated_at")?,
        "scopes":scopes,
        "author_roles":roles,
        "credential_count":credential_count,
        "credentials":credentials,
        "invites":invites,
        "access_audit":access_audit,
        "contains_secret_material":false,
        "contains_secret_material_scope":"reconstructed_export_fields_only"
    });
    let mut tx = pool.begin().await?;
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        Some(account_id),
        "account_export",
        json!({
            "export_schema":"paper-raid-bff.account-export.v2",
            "export_scope":"bff_local_access_directory"
        }),
    )
    .await?;
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(export)
}

fn project_access_audit_row(row: &sqlx::postgres::PgRow) -> Result<Value> {
    let audit_id: Uuid = row.try_get("audit_id")?;
    let operator_subject_present: bool = row.try_get("operator_subject_present")?;
    let stored_action: String = row.try_get("action")?;
    let outcome: String = row.try_get("outcome")?;
    let raw_metadata: Value = row.try_get("metadata")?;
    let operator_lineage_status: String = row.try_get("operator_lineage_status")?;
    if !matches!(
        operator_lineage_status.as_str(),
        "linked" | "legacy_unavailable" | "not_applicable"
    ) {
        bail!("access audit operator lineage status is outside the closed projection");
    }
    let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
    let mut projected = serde_json::Map::new();
    projected.insert("audit_id".to_string(), json!(audit_id));
    projected.insert("operator_subject".to_string(), Value::Null);
    projected.insert(
        "operator_subject_provenance".to_string(),
        if operator_subject_present {
            json!("unverified_host_assertion_redacted")
        } else {
            json!("not_recorded")
        },
    );
    let action = project_access_audit_action(&stored_action);
    projected.insert(
        "action".to_string(),
        json!(action.unwrap_or("redacted_unverified")),
    );
    projected.insert(
        "action_status".to_string(),
        if action.is_some() {
            json!("verified_projection")
        } else {
            json!("redacted_unverified")
        },
    );
    projected.insert("outcome".to_string(), json!(outcome.as_str()));
    projected.insert(
        "operator_lineage_status".to_string(),
        json!(operator_lineage_status),
    );
    projected.insert("occurred_at".to_string(), json!(occurred_at));
    if let Some(metadata) =
        action.and_then(|action| project_access_audit_metadata(action, &raw_metadata))
    {
        projected.insert("metadata_status".to_string(), json!("verified_projection"));
        projected.insert("metadata".to_string(), metadata);
    } else {
        projected.insert("metadata_status".to_string(), json!("redacted_unverified"));
    }
    Ok(Value::Object(projected))
}

fn project_access_audit_action(action: &str) -> Option<&'static str> {
    match action {
        "login" => Some("login"),
        "invite_redeem" => Some("invite_redeem"),
        "invite_issue" => Some("invite_issue"),
        "invite_reissue" => Some("invite_reissue"),
        "invite_revoke" => Some("invite_revoke"),
        "credential_rotate" => Some("credential_rotate"),
        "account_suspended" => Some("account_suspended"),
        "account_active" => Some("account_active"),
        "account_closed" => Some("account_closed"),
        "account_export" => Some("account_export"),
        _ => None,
    }
}

fn project_access_audit_metadata(action: &str, metadata: &Value) -> Option<Value> {
    let source = metadata.as_object()?;
    if source.get("schema")?.as_str()? != "paper-raid-bff.accessctl.object-audit.v1" {
        return None;
    }
    let attempt_id = source.get("attempt_id")?.as_str()?;
    Uuid::parse_str(attempt_id).ok()?;
    let mut projected = serde_json::Map::new();
    projected.insert(
        "schema".to_string(),
        json!("paper-raid-bff.accessctl.object-audit.v1"),
    );
    projected.insert("attempt_id".to_string(), json!(attempt_id));
    match action {
        "invite_issue" | "invite_reissue" => {
            project_access_audit_uuid_field(source, &mut projected, "invite_id")?;
            project_access_audit_uuid_field(source, &mut projected, "batch_id")?;
        }
        "invite_revoke" => {
            project_access_audit_uuid_field(source, &mut projected, "invite_id")?;
        }
        "credential_rotate" => {
            project_access_audit_uuid_field(source, &mut projected, "credential_id")?;
        }
        "account_suspended" | "account_active" | "account_closed" => {}
        "account_export" => {
            if source.get("export_schema")?.as_str()? != "paper-raid-bff.account-export.v2"
                || source.get("export_scope")?.as_str()? != "bff_local_access_directory"
            {
                return None;
            }
            projected.insert(
                "export_schema".to_string(),
                json!("paper-raid-bff.account-export.v2"),
            );
            projected.insert(
                "export_scope".to_string(),
                json!("bff_local_access_directory"),
            );
        }
        _ => return None,
    }
    Some(Value::Object(projected))
}

fn project_access_audit_uuid_field(
    source: &serde_json::Map<String, Value>,
    projected: &mut serde_json::Map<String, Value>,
    name: &str,
) -> Option<()> {
    let value = source.get(name)?.as_str()?;
    Uuid::parse_str(value).ok()?;
    projected.insert(name.to_string(), json!(value));
    Some(())
}

fn account_export_boundary(
    scope_count: usize,
    author_role_count: usize,
    credential_count: usize,
    invite_count: usize,
    access_audit_count: usize,
) -> Value {
    json!({
        "authority":"paper_raid_bff",
        "scope":"bff_local_access_directory",
        "global_account_export_complete":false,
        "snapshot_semantics":{
            "consistency":"repeatable_read_read_only",
            "current_account_export_audit_included":false,
            "current_account_export_audit_write":"separate_atomic_audit_transaction_after_export_snapshot"
        },
        "included_bff_records":{
            "paper_raid_bff_accounts":{
                "status":"included",
                "record_count":1,
                "selection":"subject_id"
            },
            "paper_raid_bff_account_scopes":{
                "status":"included",
                "record_count":scope_count,
                "selection":"account_id"
            },
            "paper_raid_bff_account_author_roles":{
                "status":"included",
                "record_count":author_role_count,
                "selection":"account_id"
            },
            "paper_raid_bff_login_credentials_lifecycle":{
                "status":"included",
                "record_count":credential_count,
                "selection":"account_id",
                "field_projection":"lifecycle_fields_without_secret_hash",
                "secret_hashes_included":false
            },
            "paper_raid_bff_invites":{
                "status":"included",
                "record_count":invite_count,
                "selection":"account_id",
                "field_projection":"lifecycle_fields_without_secret_hash",
                "secret_hashes_included":false
            },
            "paper_raid_bff_access_audit":{
                "status":"included",
                "record_count":access_audit_count,
                "selection":"account_id",
                "field_projection":"closed_action_and_whitelisted_metadata_reconstruction",
                "unknown_action_or_metadata":"omitted_and_marked_redacted_unverified",
                "operator_subject_projection":"raw_value_omitted_provenance_unverified"
            }
        },
        "component_statuses":{
            "bff_sessions":{
                "status":"not_queried",
                "reason":"outside_bff_access_directory_export_scope"
            },
            "bff_request_security_state":{
                "status":"not_queried",
                "reason":"outside_bff_access_directory_export_scope"
            },
            "bff_agent_bridge":{
                "status":"not_queried",
                "reason":"outside_bff_access_directory_export_scope"
            },
            "bff_product_telemetry":{
                "status":"not_queried",
                "reason":"outside_bff_access_directory_export_scope"
            },
            "bff_invite_batches":{
                "status":"not_queried",
                "reason":"only_invite_batch_ids_are_projected"
            },
            "bff_quota_and_retention_metadata":{
                "status":"not_queried",
                "reason":"outside_bff_access_directory_export_scope"
            },
            "bff_schema_metadata":{
                "status":"not_queried",
                "reason":"non_account_operational_metadata"
            },
            "bff_operator_command_audit":{
                "status":"not_queried",
                "reason":"global_operator_events_have_no_account_id"
            },
            "hepta":{
                "status":"not_supported",
                "reason":"separate_authority_not_queried_by_accessctl"
            },
            "nakama":{
                "status":"not_supported",
                "reason":"separate_authority_not_queried_by_accessctl"
            },
            "cas_reachability":{
                "status":"not_supported",
                "reason":"requires_cross_authority_manifest_inventory"
            },
            "cas_bytes":{
                "status":"not_supported",
                "reason":"content_store_bytes_not_read_by_accessctl"
            },
            "backups":{
                "status":"not_supported",
                "reason":"backup_inventory_not_available_to_accessctl"
            }
        }
    })
}

async fn prune(
    pool: &PgPool,
    operator: &str,
    configured_policy_id: &str,
    args: &Args,
    context: CommandContext,
) -> Result<Value> {
    args.only(&["before", "policy-id"])?;
    let policy_id = args.required("policy-id")?;
    if policy_id != configured_policy_id {
        bail!("policy-id does not match PAPER_RAID_BFF_ACCESS_RETENTION_POLICY_ID");
    }
    let cutoff = parse_timestamp(args.required("before")?, "before")?;
    if cutoff >= Utc::now() {
        bail!("prune cutoff must be in the past");
    }
    let mut tx = pool.begin().await?;
    let sessions = sqlx::query(
        "DELETE FROM paper_raid_bff_sessions \
         WHERE expires_at < $1 OR (revoked_at IS NOT NULL AND revoked_at < $1)",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let assertions = sqlx::query("DELETE FROM paper_raid_bff_assertions WHERE expires_at < $1")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let idempotency = sqlx::query(
        "DELETE FROM paper_raid_bff_idempotency \
         WHERE state = 'completed' AND completed_at < $1",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let read_cache = sqlx::query("DELETE FROM paper_raid_bff_read_cache WHERE expires_at < $1")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let product_events =
        sqlx::query("DELETE FROM paper_raid_bff_product_events WHERE occurred_at < $1")
            .bind(cutoff)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    let quotas =
        sqlx::query("DELETE FROM paper_raid_bff_quota_windows WHERE window_started_at < $1")
            .bind(cutoff)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    // Signed Agent response bytes are a <=60s replay cache, not a retention-
    // policy data set. Never preserve an expired row until an older cutoff.
    let agent_request_uses =
        sqlx::query("DELETE FROM paper_raid_bff_agent_request_uses WHERE expires_at <= now()")
            .execute(&mut *tx)
            .await?
            .rows_affected();
    let agent_delivery_drafts =
        sqlx::query("DELETE FROM paper_raid_bff_agent_delivery_drafts WHERE expires_at <= now()")
            .execute(&mut *tx)
            .await?
            .rows_affected();
    let revoked_credentials = sqlx::query(
        "DELETE FROM paper_raid_bff_login_credentials \
         WHERE state = 'revoked' AND revoked_at < $1",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let terminal_invites = sqlx::query(
        "DELETE FROM paper_raid_bff_invites \
         WHERE (state = 'revoked' AND revoked_at < $1) \
            OR (state = 'redeemed' AND redeemed_at < $1)",
    )
    .bind(cutoff)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let deleted_counts = json!({
        "sessions":sessions,
        "assertions":assertions,
        "idempotency":idempotency,
        "read_cache":read_cache,
        "product_events":product_events,
        "access_audit":0,
        "quota_windows":quotas,
        "agent_request_uses":agent_request_uses,
        "agent_delivery_drafts":agent_delivery_drafts,
        "revoked_credentials":revoked_credentials,
        "terminal_invites":terminal_invites
    });
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO paper_raid_bff_retention_runs \
         (run_id, policy_id, cutoff, operator_subject, deleted_counts) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(run_id)
    .bind(policy_id)
    .bind(cutoff)
    .bind(operator)
    .bind(deleted_counts.clone())
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        operator,
        context.attempt_id,
        None,
        "retention_prune",
        json!({"run_id":run_id,"policy_id":policy_id,"cutoff":cutoff}),
    )
    .await?;
    let output = json!({"schema":"paper-raid-bff.retention-run.v1","run_id":run_id,"policy_id":policy_id,"cutoff":cutoff,"deleted_counts":deleted_counts});
    finalize_business_transaction(tx, pool, operator, context).await?;
    Ok(output)
}

async fn revoke_sessions(tx: &mut Transaction<'_, Postgres>, subject: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO paper_raid_bff_session_generation(subject_id, generation) VALUES ($1, 1) \
         ON CONFLICT (subject_id) DO UPDATE SET generation = paper_raid_bff_session_generation.generation + 1, updated_at = now()",
    )
    .bind(subject)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE paper_raid_bff_sessions SET revoked_at = now() \
         WHERE subject_id = $1 AND revoked_at IS NULL",
    )
    .bind(subject)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    operator: &str,
    attempt_id: Uuid,
    account_id: Option<Uuid>,
    action: &str,
    mut metadata: Value,
) -> Result<()> {
    let object = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow!("object audit metadata must be an object"))?;
    if object.contains_key("schema") || object.contains_key("attempt_id") {
        bail!("object audit metadata contains reserved fields");
    }
    object.insert(
        "schema".to_string(),
        json!("paper-raid-bff.accessctl.object-audit.v1"),
    );
    object.insert("attempt_id".to_string(), json!(attempt_id));
    sqlx::query(
        "INSERT INTO paper_raid_bff_access_audit \
         (audit_id, operator_subject, account_id, action, outcome, metadata, \
          operator_attempt_id, operator_event, operator_lineage_status) \
         VALUES ($1, $2, $3, $4, 'succeeded', $5, $6, NULL, 'linked')",
    )
    .bind(Uuid::new_v4())
    .bind(operator)
    .bind(account_id)
    .bind(action)
    .bind(metadata)
    .bind(attempt_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperatorAuditReadback {
    Exact,
    Absent,
    Mismatch,
}

fn operator_audit_id(attempt_id: Uuid, event: OperatorAuditEvent) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"paper-raid-bff.operator-command-audit-id.v1\0");
    hasher.update(attempt_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(event.action().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

async fn insert_operator_audit_tx(
    tx: &mut Transaction<'_, Postgres>,
    operator: &str,
    record: OperatorAuditRecord,
) -> Result<()> {
    let audit_id = operator_audit_id(record.attempt_id, record.event);
    let metadata = operator_command_audit_metadata(
        record.attempt_id,
        record.command,
        record.command_state,
        record.reason,
    );
    sqlx::query(
        "INSERT INTO paper_raid_bff_access_audit \
         (audit_id, operator_subject, account_id, action, outcome, metadata, \
          operator_attempt_id, operator_event, operator_lineage_status) \
         VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, 'linked') \
         ON CONFLICT (audit_id) DO NOTHING",
    )
    .bind(audit_id)
    .bind(operator)
    .bind(record.event.action())
    .bind(record.outcome.as_str())
    .bind(metadata.clone())
    .bind(record.attempt_id)
    .bind(record.event.action())
    .execute(&mut **tx)
    .await?;
    let row = sqlx::query(
        "SELECT operator_subject, account_id, action, outcome, metadata, \
                operator_attempt_id, operator_event, operator_lineage_status \
         FROM paper_raid_bff_access_audit WHERE audit_id = $1",
    )
    .bind(audit_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| anyhow!(DatabaseOperationNotCommitted))?;
    let exact = row
        .try_get::<Option<String>, _>("operator_subject")?
        .as_deref()
        == Some(operator)
        && row.try_get::<Option<Uuid>, _>("account_id")?.is_none()
        && row.try_get::<String, _>("action")? == record.event.action()
        && row.try_get::<String, _>("outcome")? == record.outcome.as_str()
        && row.try_get::<Value, _>("metadata")? == metadata
        && row.try_get::<Option<Uuid>, _>("operator_attempt_id")? == Some(record.attempt_id)
        && row
            .try_get::<Option<String>, _>("operator_event")?
            .as_deref()
            == Some(record.event.action())
        && row.try_get::<String, _>("operator_lineage_status")? == "linked";
    if !exact {
        return Err(anyhow!(DatabaseOperationNotCommitted));
    }
    Ok(())
}

async fn operator_audit_readback(
    pool: &PgPool,
    operator: &str,
    record: OperatorAuditRecord,
) -> std::result::Result<OperatorAuditReadback, sqlx::Error> {
    let metadata = operator_command_audit_metadata(
        record.attempt_id,
        record.command,
        record.command_state,
        record.reason,
    );
    let row = sqlx::query(
        "SELECT operator_subject, account_id, action, outcome, metadata, \
                operator_attempt_id, operator_event, operator_lineage_status \
         FROM paper_raid_bff_access_audit WHERE audit_id = $1",
    )
    .bind(operator_audit_id(record.attempt_id, record.event))
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(OperatorAuditReadback::Absent);
    };
    let exact = row
        .try_get::<Option<String>, _>("operator_subject")?
        .as_deref()
        == Some(operator)
        && row.try_get::<Option<Uuid>, _>("account_id")?.is_none()
        && row.try_get::<String, _>("action")? == record.event.action()
        && row.try_get::<String, _>("outcome")? == record.outcome.as_str()
        && row.try_get::<Value, _>("metadata")? == metadata
        && row.try_get::<Option<Uuid>, _>("operator_attempt_id")? == Some(record.attempt_id)
        && row
            .try_get::<Option<String>, _>("operator_event")?
            .as_deref()
            == Some(record.event.action())
        && row.try_get::<String, _>("operator_lineage_status")? == "linked";
    Ok(if exact {
        OperatorAuditReadback::Exact
    } else {
        OperatorAuditReadback::Mismatch
    })
}

async fn commit_with_operator_audit_readback(
    tx: Transaction<'_, Postgres>,
    pool: &PgPool,
    operator: &str,
    record: OperatorAuditRecord,
) -> Result<()> {
    if tx.commit().await.is_ok() {
        return Ok(());
    }
    match operator_audit_readback(pool, operator, record).await {
        Ok(OperatorAuditReadback::Exact) => Ok(()),
        Ok(OperatorAuditReadback::Absent | OperatorAuditReadback::Mismatch) | Err(_) => {
            Err(anyhow!(CommitAckUnknown))
        }
    }
}

async fn write_operator_audit(
    pool: &PgPool,
    operator: &str,
    record: OperatorAuditRecord,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    insert_operator_audit_tx(&mut tx, operator, record).await?;
    commit_with_operator_audit_readback(tx, pool, operator, record).await
}

async fn finalize_business_transaction(
    mut tx: Transaction<'_, Postgres>,
    pool: &PgPool,
    operator: &str,
    context: CommandContext,
) -> Result<()> {
    let result_record = OperatorAuditRecord {
        attempt_id: context.attempt_id,
        command: context.command,
        event: OperatorAuditEvent::Result,
        outcome: OperatorAuditOutcome::Succeeded,
        command_state: CommandState::Committed,
        reason: CommandReason::CommandCommitted,
    };
    insert_operator_audit_tx(&mut tx, operator, result_record).await?;
    commit_with_operator_audit_readback(tx, pool, operator, result_record).await
}

fn operator_command_audit_metadata(
    attempt_id: Uuid,
    command: OperatorCommand,
    command_state: CommandState,
    reason: CommandReason,
) -> Value {
    json!({
        "schema":"paper-raid-bff.operator-command-audit.v1",
        "attempt_id":attempt_id,
        "command_code":command.as_str(),
        "command_state":command_state.as_str(),
        "reason_code":reason.as_str()
    })
}

async fn audited_command_failure(
    pool: &PgPool,
    operator: &str,
    context: CommandContext,
    command_state: CommandState,
    reason: CommandReason,
) -> CliFailure {
    let audit_result = write_operator_audit(
        pool,
        operator,
        OperatorAuditRecord {
            attempt_id: context.attempt_id,
            command: context.command,
            event: OperatorAuditEvent::Result,
            outcome: if command_state == CommandState::Unknown {
                OperatorAuditOutcome::Indeterminate
            } else {
                OperatorAuditOutcome::Denied
            },
            command_state,
            reason,
        },
    )
    .await;
    let (audit_status, audit_failure_reason) = match &audit_result {
        Ok(()) => (AuditStatus::Recorded, None),
        Err(error) => {
            let (status, failure_reason) = audit_failure_truth(error, OperatorAuditEvent::Result);
            (status, Some(failure_reason))
        }
    };
    CliFailure {
        command: context.command,
        command_state,
        audit_status,
        reason,
        audit_failure_reason,
        attempt_id: Some(context.attempt_id),
    }
}

fn error_chain_contains<T>(error: &anyhow::Error) -> bool
where
    T: std::error::Error + Send + Sync + 'static,
{
    error
        .chain()
        .any(|cause| cause.downcast_ref::<T>().is_some())
}

fn error_chain_contains_sqlx(error: &anyhow::Error) -> bool {
    error_chain_contains::<sqlx::Error>(error)
}

fn random_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn parse_expiry(value: &str) -> Result<Option<DateTime<Utc>>> {
    if value == "never" {
        return Ok(None);
    }
    Ok(Some(parse_timestamp(value, "expires-at")?))
}

fn parse_timestamp(value: &str, name: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| anyhow!("{name} must be RFC3339"))
}

fn parse_uuid(value: &str, name: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|_| anyhow!("{name} must be a UUID"))
}

fn parse_set(value: &str, allowed: &[&str], empty_allowed: bool) -> Result<Vec<String>> {
    if value == "none" && empty_allowed {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for item in value.split(',') {
        if !allowed.contains(&item) || values.iter().any(|existing| existing == item) {
            bail!("invalid or duplicate set value: {item}");
        }
        values.push(item.to_string());
    }
    if values.is_empty() {
        bail!("set must not be empty");
    }
    Ok(values)
}

fn validate_identifier(name: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        bail!("{name} must be an opaque ASCII identifier");
    }
    Ok(())
}

fn validate_display_name(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 80 || value.as_bytes().contains(&0) {
        bail!("display-name must contain 1..80 characters");
    }
    Ok(())
}

fn validate_label(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 80 || value.as_bytes().contains(&0) {
        bail!("label must contain 1..80 characters");
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("{name} is required"))
}

type Args = ParsedArgs;

struct ParsedArgs(BTreeMap<String, String>);

impl ParsedArgs {
    fn required(&self, name: &str) -> Result<&str> {
        self.0
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| anyhow!("--{name} is required"))
    }

    fn only(&self, allowed: &[&str]) -> Result<()> {
        if let Some(name) = self.0.keys().find(|name| !allowed.contains(&name.as_str())) {
            bail!("unknown option --{name}");
        }
        Ok(())
    }
}

fn parse_args_os<I>(mut raw: I) -> (OperatorCommand, Result<ParsedArgs>)
where
    I: Iterator<Item = OsString>,
{
    let command = raw
        .next()
        .and_then(|value| value.into_string().ok())
        .map(|value| OperatorCommand::from_raw(Some(&value)))
        .unwrap_or(OperatorCommand::Unsupported);
    if command == OperatorCommand::Unsupported {
        return (command, Ok(ParsedArgs(BTreeMap::new())));
    }
    let mut parsed = BTreeMap::new();
    let mut token_count = 0_usize;
    while let Some(name) = raw.next() {
        token_count += 1;
        if token_count > MAX_ARGUMENT_TOKENS {
            return (command, Err(anyhow!("argument token limit exceeded")));
        }
        let Ok(name) = name.into_string() else {
            return (command, Err(anyhow!("option name must be Unicode")));
        };
        if name.len() > MAX_OPTION_NAME_BYTES {
            return (command, Err(anyhow!("option name exceeds bound")));
        }
        let Some(name) = name.strip_prefix("--") else {
            return (
                command,
                Err(anyhow!("arguments must be --name value pairs")),
            );
        };
        if name.is_empty()
            || name.len() > MAX_OPTION_NAME_BYTES - 2
            || name
                .bytes()
                .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
        {
            return (command, Err(anyhow!("option name is invalid")));
        }
        let Some(value) = raw.next() else {
            return (command, Err(anyhow!("option requires a value")));
        };
        token_count += 1;
        if token_count > MAX_ARGUMENT_TOKENS {
            return (command, Err(anyhow!("argument token limit exceeded")));
        }
        let Ok(value) = value.into_string() else {
            return (command, Err(anyhow!("option value must be Unicode")));
        };
        if value.len() > MAX_OPTION_VALUE_BYTES
            || value.starts_with("--")
            || parsed.insert(name.to_string(), value).is_some()
        {
            return (
                command,
                Err(anyhow!("option value or duplication is invalid")),
            );
        }
    }
    (command, Ok(ParsedArgs(parsed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_export_boundary_is_explicitly_bff_local() {
        let boundary = account_export_boundary(2, 1, 3, 4, 5);

        assert_eq!(boundary["authority"], json!("paper_raid_bff"));
        assert_eq!(boundary["scope"], json!("bff_local_access_directory"));
        assert_eq!(boundary["global_account_export_complete"], json!(false));
        assert_eq!(
            boundary["snapshot_semantics"]["consistency"],
            json!("repeatable_read_read_only")
        );
        assert_eq!(
            boundary["snapshot_semantics"]["current_account_export_audit_included"],
            json!(false)
        );
        assert_eq!(
            boundary["included_bff_records"]["paper_raid_bff_accounts"]["record_count"],
            json!(1)
        );
        assert_eq!(
            boundary["included_bff_records"]["paper_raid_bff_account_scopes"]["record_count"],
            json!(2)
        );
        assert_eq!(
            boundary["included_bff_records"]["paper_raid_bff_account_author_roles"]["record_count"],
            json!(1)
        );
        assert_eq!(
            boundary["included_bff_records"]["paper_raid_bff_login_credentials_lifecycle"]
                ["record_count"],
            json!(3)
        );
        assert_eq!(
            boundary["included_bff_records"]["paper_raid_bff_invites"]["record_count"],
            json!(4)
        );
        assert_eq!(
            boundary["included_bff_records"]["paper_raid_bff_access_audit"]["record_count"],
            json!(5)
        );
    }

    #[test]
    fn account_export_boundary_closes_every_omitted_component() {
        let boundary = account_export_boundary(0, 0, 0, 0, 0);
        let statuses = boundary["component_statuses"]
            .as_object()
            .expect("component statuses are an object");

        assert_eq!(statuses.len(), 13);
        for component in [
            "bff_sessions",
            "bff_request_security_state",
            "bff_agent_bridge",
            "bff_product_telemetry",
            "bff_invite_batches",
            "bff_quota_and_retention_metadata",
            "bff_schema_metadata",
            "bff_operator_command_audit",
        ] {
            assert_eq!(statuses[component]["status"], json!("not_queried"));
        }
        for component in [
            "hepta",
            "nakama",
            "cas_reachability",
            "cas_bytes",
            "backups",
        ] {
            assert_eq!(statuses[component]["status"], json!("not_supported"));
        }
        assert!(statuses.values().all(|component| component["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())));
    }

    #[test]
    fn operator_command_codes_are_closed_and_unknown_input_is_not_echoed() {
        for (raw, expected) in [
            ("schema-migrate", "schema_migrate"),
            ("batch-create", "batch_create"),
            ("batch-pause", "batch_pause"),
            ("batch-resume", "batch_resume"),
            ("batch-revoke", "batch_revoke"),
            ("invite-issue", "invite_issue"),
            ("invite-reissue", "invite_reissue"),
            ("invite-revoke", "invite_revoke"),
            ("credential-rotate", "credential_rotate"),
            ("account-suspend", "account_suspend"),
            ("account-reactivate", "account_reactivate"),
            ("account-close", "account_close"),
            ("account-export", "account_export"),
            ("prune", "prune"),
        ] {
            assert_eq!(OperatorCommand::from_raw(Some(raw)).as_str(), expected);
        }
        assert_eq!(
            OperatorCommand::from_raw(Some("raw-secret-command")).as_str(),
            "unsupported"
        );
        assert_eq!(OperatorCommand::from_raw(None).as_str(), "unsupported");
    }

    #[test]
    fn operator_command_audit_metadata_contains_only_bounded_fields() {
        let attempt_id = Uuid::nil();
        let metadata = operator_command_audit_metadata(
            attempt_id,
            OperatorCommand::InviteIssue,
            CommandState::Unknown,
            CommandReason::DatabaseOperationFailed,
        );
        let object = metadata.as_object().expect("audit metadata is an object");

        assert_eq!(object.len(), 5);
        assert_eq!(
            metadata["schema"],
            json!("paper-raid-bff.operator-command-audit.v1")
        );
        assert_eq!(metadata["attempt_id"], json!(attempt_id));
        assert_eq!(metadata["command_code"], json!("invite_issue"));
        assert_eq!(metadata["command_state"], json!("unknown"));
        assert_eq!(metadata["reason_code"], json!("database_operation_failed"));
        assert_eq!(OperatorAuditOutcome::Succeeded.as_str(), "succeeded");
        assert_eq!(OperatorAuditOutcome::Denied.as_str(), "denied");
        assert_eq!(
            OperatorAuditOutcome::Indeterminate.as_str(),
            "indeterminate"
        );
    }

    #[test]
    fn operator_audit_ids_are_stable_and_event_scoped() {
        let attempt_id = Uuid::parse_str("018f0678-7dd0-7cc4-b56f-c83f5f772400").unwrap();
        assert_eq!(
            operator_audit_id(attempt_id, OperatorAuditEvent::Attempt),
            operator_audit_id(attempt_id, OperatorAuditEvent::Attempt)
        );
        assert_ne!(
            operator_audit_id(attempt_id, OperatorAuditEvent::Attempt),
            operator_audit_id(attempt_id, OperatorAuditEvent::Result)
        );
    }

    #[test]
    fn commit_ack_loss_preserves_unknown_audit_truth() {
        let error = anyhow!(CommitAckUnknown);
        assert_eq!(
            audit_failure_truth(&error, OperatorAuditEvent::Attempt),
            (
                AuditStatus::Unknown,
                AuditFailureReason::AttemptCommitUnknown
            )
        );
        assert_eq!(AuditStatus::Unknown.as_str(), "unknown");
        assert_eq!(
            AuditFailureReason::ResultCommitUnknown.as_str(),
            "result_audit_commit_unknown"
        );
    }

    #[test]
    fn bounded_os_parser_prioritizes_unsupported_without_reading_parameters() {
        let (command, args) = parse_args_os(
            [
                OsString::from("not-a-command"),
                OsString::from("secret-bearing-free-form-token"),
            ]
            .into_iter(),
        );
        assert_eq!(command, OperatorCommand::Unsupported);
        assert!(args.unwrap().0.is_empty());

        let (command, args) = parse_args_os(
            [
                OsString::from("invite-reissue"),
                OsString::from("--subject"),
                OsString::from("player-1"),
            ]
            .into_iter(),
        );
        assert_eq!(command, OperatorCommand::InviteReissue);
        assert_eq!(args.unwrap().required("subject").unwrap(), "player-1");
    }

    #[test]
    fn bounded_os_parser_rejects_oversized_known_command_inputs() {
        let mut too_many = vec![OsString::from("batch-create")];
        for index in 0..17 {
            too_many.push(OsString::from(format!("--x{index}")));
            too_many.push(OsString::from("v"));
        }
        let (command, args) = parse_args_os(too_many.into_iter());
        assert_eq!(command, OperatorCommand::BatchCreate);
        assert!(args.is_err());

        let (command, args) = parse_args_os(
            [
                OsString::from("invite-reissue"),
                OsString::from("--subject"),
                OsString::from("x".repeat(MAX_OPTION_VALUE_BYTES + 1)),
            ]
            .into_iter(),
        );
        assert_eq!(command, OperatorCommand::InviteReissue);
        assert!(args.is_err());
    }

    #[test]
    fn access_audit_metadata_is_reconstructed_only_for_known_schema_and_action() {
        let attempt_id = Uuid::nil();
        let invite_id = Uuid::new_v4();
        let batch_id = Uuid::new_v4();
        let raw = json!({
            "schema":"paper-raid-bff.accessctl.object-audit.v1",
            "attempt_id":attempt_id,
            "invite_id":invite_id,
            "batch_id":batch_id,
            "untrusted_extra":"must-not-survive"
        });
        let projected = project_access_audit_metadata("invite_reissue", &raw).unwrap();
        assert_eq!(projected["attempt_id"], json!(attempt_id));
        assert_eq!(projected["invite_id"], json!(invite_id));
        assert_eq!(projected["batch_id"], json!(batch_id));
        assert!(projected.get("untrusted_extra").is_none());
        assert!(project_access_audit_metadata("unknown_action", &raw).is_none());
        assert!(project_access_audit_metadata(
            "invite_reissue",
            &json!({"schema":"legacy","attempt_id":attempt_id})
        )
        .is_none());
        assert_eq!(
            project_access_audit_action("invite_reissue"),
            Some("invite_reissue")
        );
        assert_eq!(
            project_access_audit_action("self_declared_secret_action"),
            None
        );
    }

    #[test]
    fn audit_failure_disclosure_never_claims_command_success() {
        let failure = CliFailure {
            command: OperatorCommand::CredentialRotate,
            command_state: CommandState::Committed,
            audit_status: AuditStatus::Failed,
            reason: CommandReason::CommandCommitted,
            audit_failure_reason: Some(AuditFailureReason::ResultWriteFailed),
            attempt_id: Some(Uuid::nil()),
        }
        .as_json();

        assert_eq!(failure["command_code"], json!("credential_rotate"));
        assert_eq!(failure["command_state"], json!("committed"));
        assert_eq!(failure["audit_status"], json!("failed"));
        assert_eq!(
            failure["audit_failure_reason_code"],
            json!("result_audit_write_failed")
        );
        assert!(failure.get("error").is_none());
        assert!(failure.get("details").is_none());
    }
}
