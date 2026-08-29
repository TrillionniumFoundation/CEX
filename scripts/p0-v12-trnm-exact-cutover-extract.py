#!/usr/bin/env python3
"""Extract the staged TRNM exact-money patch into a runnable temporary script."""

from pathlib import Path

SOURCE = Path(".github/workflows/p0-v12-trnm-exact-cutover.yml")
OUTPUT = Path("/tmp/p0-v12-trnm-exact-cutover.py")

source = SOURCE.read_text(encoding="utf-8")
start_marker = "          python3 <<'PY'\n"
end_marker = "\n          PY\n\n      - name: Install Rust stable"
start = source.index(start_marker) + len(start_marker)
end = source.index(end_marker, start)

lines: list[str] = []
for line in source[start:end].splitlines():
    lines.append(line[10:] if line.startswith("          ") else line)
patch = "\n".join(lines) + "\n"

product_start = patch.index(
    "replace_once(\n    '''        let account_id = Uuid::new_v4();"
)
product_end = patch.index(
    "\n\nreplace_between(\n    'async fn load_account_for_update('",
    product_start,
)
product_replacement = """replace_between(
    '''        let account_id = Uuid::new_v4();
        sqlx::query(
            "insert into accounts (account_id, org_id, account_type, currency_unit, status)
             values ($1, $2, 'trnm-online-player', 'credit', 'active')",
        )''',
    '        let recovery_key_hash = recovery_key_hash(recovery_key)?;',
    '''        let account_id = Uuid::new_v4();
        let opening_trace_id = deterministic_uuid(&format!(
            "trnm-product-registration:{org_id}:{player_id}:{account_id}"
        ));
        sqlx::query_scalar::<_, Value>(
            "select public.cex_open_account_v2(
                $1, $2, $3, 'trnm-online-player', 'credit', 6::smallint, 0::bigint,
                'trnm.product.registration', $4, 'trnm-product-registration'
             )",
        )
        .bind(account_id)
        .bind(org_id)
        .bind(opening_trace_id)
        .bind(format!("player:{player_id}"))
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| db_error("create exact TRNM product account", error))?;
''',
    'product account opening',
)"""

patch = patch[:product_start] + product_replacement + patch[product_end:]

currency_helper_marker = "fn minor_to_whole_credits("
if patch.count(currency_helper_marker) != 1:
    raise SystemExit("exact currency helper insertion marker missing or duplicated")
currency_helper = """fn intent_currency(intent: &EconomicIntent) -> Result<&str, LedgerActionError> {
    let currency = intent.currency.as_deref().ok_or_else(|| {
        LedgerActionError::IdentityRejected(
            "TRNM value intent currency is required for exact Ledger writes".to_string(),
        )
    })?;
    if currency.trim().is_empty() {
        return Err(LedgerActionError::IdentityRejected(
            "TRNM value intent currency cannot be empty".to_string(),
        ));
    }
    Ok(currency)
}

"""
patch = patch.replace(currency_helper_marker, currency_helper + currency_helper_marker, 1)

legacy_currency_reference = "&intent.currency"
legacy_currency_references = patch.count(legacy_currency_reference)
if legacy_currency_references != 5:
    raise SystemExit(
        "expected five optional intent currency references, "
        f"found {legacy_currency_references}"
    )
patch = patch.replace(legacy_currency_reference, "intent_currency(intent)?")
if patch.count("intent_currency(intent)?") != 5:
    raise SystemExit("exact intent currency rewiring is incomplete")

compile(patch, str(OUTPUT), "exec")
OUTPUT.write_text(patch, encoding="utf-8")
print(f"wrote {OUTPUT} ({len(patch)} bytes)")
