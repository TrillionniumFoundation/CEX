pub async fn run_from_env() -> Result<(), WorkerError> {
    let config = SettlementWorkerConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .acquire_timeout(Duration::from_secs(DATABASE_OPERATION_TIMEOUT_SECONDS))
        .connect(&config.database_url)
        .await
        .map_err(|error| WorkerError::Database(format!("connect postgres: {error}")))?;
    let http = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(config.request_timeout_seconds))
        .build()
        .map_err(|error| WorkerError::Http(error.to_string()))?;
    let adapter = ExecutionLedgerSettlementAdapter::new(
        pool.clone(),
        http,
        config.ledger_base_url.clone(),
        config.ledger_manage_token.clone(),
        config.mode,
    )
    .map_err(WorkerError::Config)?;

    eprintln!(
        "execution settlement worker started worker_id={} batch={} lease={}s timeout={}s mode={:?}",
        config.worker_id,
        config.batch_size,
        config.lease_seconds,
        config.request_timeout_seconds,
        config.mode
    );

    loop {
        let commands = claim_commands(&pool, &config).await?;
        let claimed_count = commands.len();
        let mut first_error: Option<WorkerError> = None;
        for command in commands {
            if let Err(error) = process_command(&pool, &adapter, &config, command).await {
                eprintln!("execution settlement command processing failed: {error}");
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }

        if config.run_once {
            if let Some(error) = first_error {
                return Err(error);
            }
            return Ok(());
        }
        if claimed_count == 0 {
            sleep(Duration::from_secs(config.poll_seconds)).await;
        }
    }
}

async fn claim_commands(
    pool: &PgPool,
    config: &SettlementWorkerConfig,
) -> Result<Vec<ClaimedSettlementCommand>, WorkerError> {
    let claim = sqlx::query(
        "select command_id, invocation_id, action, attempt_count, max_attempts, lease_expires_at \
         from public.cex_claim_execution_ledger_settlements_v1($1, $2, $3)",
    )
    .bind(&config.worker_id)
    .bind(i32::try_from(config.batch_size).map_err(|_| {
        WorkerError::Config("settlement batch size cannot fit i32".to_string())
    })?)
    .bind(i32::try_from(config.lease_seconds).map_err(|_| {
        WorkerError::Config("settlement lease seconds cannot fit i32".to_string())
    })?)
    .fetch_all(pool);
    let rows = timeout(
        Duration::from_secs(DATABASE_OPERATION_TIMEOUT_SECONDS),
        claim,
    )
    .await
    .map_err(|_| WorkerError::Database("claim settlement commands timed out".to_string()))?
    .map_err(|error| WorkerError::Database(format!("claim settlement commands: {error}")))?;

    rows.into_iter()
        .map(|row| {
            let action_raw: String = row
                .try_get("action")
                .map_err(|error| WorkerError::Database(format!("decode action: {error}")))?;
            Ok(ClaimedSettlementCommand {
                command_id: row.try_get("command_id").map_err(|error| {
                    WorkerError::Database(format!("decode command_id: {error}"))
                })?,
                invocation_id: row.try_get("invocation_id").map_err(|error| {
                    WorkerError::Database(format!("decode invocation_id: {error}"))
                })?,
                action: parse_action(&action_raw)?,
                attempt_count: row.try_get("attempt_count").map_err(|error| {
                    WorkerError::Database(format!("decode attempt_count: {error}"))
                })?,
                max_attempts: row.try_get("max_attempts").map_err(|error| {
                    WorkerError::Database(format!("decode max_attempts: {error}"))
                })?,
                lease_expires_at: row.try_get("lease_expires_at").map_err(|error| {
                    WorkerError::Database(format!("decode lease_expires_at: {error}"))
                })?,
            })
        })
        .collect()
}

async fn process_command(
    pool: &PgPool,
    adapter: &ExecutionLedgerSettlementAdapter,
    config: &SettlementWorkerConfig,
    command: ClaimedSettlementCommand,
) -> Result<(), WorkerError> {
    if command.lease_expires_at <= Utc::now() {
        return Err(WorkerError::Database(format!(
            "command {} was returned with an expired claim lease",
            command.command_id
        )));
    }

    let settlement_timeout = config
        .request_timeout_seconds
        .saturating_add(DATABASE_OPERATION_TIMEOUT_SECONDS);
    let settlement = match timeout(
        Duration::from_secs(settlement_timeout),
        adapter.settle_invocation(command.invocation_id, command.action),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => SettlementOutcome::ReconcileRequired {
            code: "settlement_operation_timeout_unknown_outcome".to_string(),
            http_status: None,
        },
    };
    let persisted = enforce_attempt_boundary(
        PersistedOutcome::from_settlement(settlement, command.attempt_count),
        command.attempt_count,
        command.max_attempts,
    );

    persist_outcome(pool, config, command.command_id, persisted).await
}

fn enforce_attempt_boundary(
    mut outcome: PersistedOutcome,
    attempt_count: i32,
    max_attempts: i32,
) -> PersistedOutcome {
    if outcome.outcome == "retry_wait" && attempt_count >= max_attempts {
        outcome.outcome = "reconcile_required";
        outcome.error_code = Some("retry_budget_exhausted_unknown_outcome".to_string());
        outcome.error_message = Some(
            "automatic attempts are exhausted; reconcile the durable contract before replay"
                .to_string(),
        );
        outcome.retry_after_seconds = None;
    }
    outcome
}

async fn persist_outcome(
    pool: &PgPool,
    config: &SettlementWorkerConfig,
    command_id: Uuid,
    outcome: PersistedOutcome,
) -> Result<(), WorkerError> {
    let receipt_json = outcome
        .receipt
        .map(|receipt| serde_json::to_string(&receipt))
        .transpose()
        .map_err(|error| {
            WorkerError::Database(format!(
                "serialize settlement command {command_id} receipt: {error}"
            ))
        })?;
    let persist = sqlx::query(
        "select command_id from public.cex_finish_execution_ledger_settlement_v1(\
         $1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9)",
    )
    .bind(command_id)
    .bind(&config.worker_id)
    .bind(outcome.outcome)
    .bind(outcome.error_code)
    .bind(outcome.error_message)
    .bind(outcome.http_status.map(i32::from))
    .bind(receipt_json)
    .bind(outcome.replayed)
    .bind(outcome.retry_after_seconds)
    .fetch_one(pool);
    timeout(Duration::from_secs(DATABASE_OPERATION_TIMEOUT_SECONDS), persist)
        .await
        .map_err(|_| {
            WorkerError::Database(format!(
                "persist settlement command {command_id} outcome timed out; claim is intentionally left for lease recovery"
            ))
        })?
        .map_err(|error| {
            WorkerError::Database(format!(
                "persist settlement command {command_id} outcome; claim is intentionally left for lease recovery: {error}"
            ))
        })?;
    Ok(())
}
