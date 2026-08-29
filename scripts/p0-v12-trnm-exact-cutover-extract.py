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
compile(patch, str(OUTPUT), "exec")
OUTPUT.write_text(patch, encoding="utf-8")
print(f"wrote {OUTPUT} ({len(patch)} bytes)")
