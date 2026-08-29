#!/usr/bin/env python3
"""Apply the deterministic closure fixes identified from executed CI evidence.

This script is intentionally strict: every replacement must match the exact
reviewed source shape, otherwise it aborts without writing a partial patch.
"""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(
            f"{path}: expected {expected} occurrence(s), found {count}: {old[:120]!r}"
        )
    target.write_text(text.replace(old, new), encoding="utf-8")


def main() -> None:
    migration = "migrations/0056_expand_exact_money_minor_units.sql"
    for column in ("balance", "reserved", "amount"):
        replace_exact(
            migration,
            f"public.cex_numeric_to_minor({column}, coalesce(currency_scale, 6))",
            f"public.cex_numeric_to_minor({column}, coalesce(currency_scale, 6::smallint))",
        )

    adapter = "services/consumer-entry-api/src/term_exchange_backend.rs"
    replace_exact(
        adapter,
        'pub(super) const TERM_EXCHANGE_BACKEND_ADAPTER_CONTRACT_VERSION: &str =\n'
        '    "trillionnium_term_exchange_backend_adapter_v2";\n',
        'pub(super) const TERM_EXCHANGE_BACKEND_ADAPTER_CONTRACT_VERSION: &str =\n'
        '    "trillionnium_term_exchange_backend_adapter_v2";\n'
        'const TERM_EXCHANGE_EXACT_CURRENCY_SCALE: u8 = 6;\n',
    )
    replace_exact(
        adapter,
        "async fn execute_cex_ledger_action(\n"
        "    state: &AppState,\n"
        "    request: TermExchangeLedgerActionRequest,\n"
        ") -> TermExchangeBackendReceipt {\n"
        "    if request.amount_credits <= 0 {\n",
        "async fn execute_cex_ledger_action(\n"
        "    state: &AppState,\n"
        "    request: TermExchangeLedgerActionRequest,\n"
        ") -> TermExchangeBackendReceipt {\n"
        "    let amount_minor = match exact_minor_from_compatibility_amount(\n"
        "        request.amount,\n"
        "        TERM_EXCHANGE_EXACT_CURRENCY_SCALE,\n"
        "    ) {\n"
        "        Ok(value) => value,\n"
        "        Err(error) => {\n"
        "            return backend_receipt(\n"
        "                request,\n"
        "                \"failed_ledger\",\n"
        "                None,\n"
        "                None,\n"
        "                None,\n"
        "                Some(error),\n"
        "                None,\n"
        "                json!({\"amount_conversion\": \"rejected\"}),\n"
        "            )\n"
        "        }\n"
        "    };\n"
        "    if amount_minor <= 0 {\n",
    )
    replace_exact(
        adapter,
        '        "currency_scale": 0,\n'
        '        "amount_minor": request.amount_credits.to_string(),\n',
        '        "currency_scale": TERM_EXCHANGE_EXACT_CURRENCY_SCALE,\n'
        '        "amount_minor": amount_minor.to_string(),\n',
    )
    replace_exact(
        adapter,
        "fn deterministic_uuid(namespace: &str) -> Uuid {\n",
        "fn exact_minor_from_compatibility_amount(value: f64, scale: u8) -> Result<i64, String> {\n"
        "    if !value.is_finite() || value < 0.0 {\n"
        "        return Err(\"settlement amount must be a finite non-negative value\".to_string());\n"
        "    }\n"
        "    let factor = 10_i64\n"
        "        .checked_pow(u32::from(scale))\n"
        "        .ok_or_else(|| \"settlement currency scale is unsupported\".to_string())?;\n"
        "    let scaled = value * factor as f64;\n"
        "    let rounded = scaled.round();\n"
        "    let tolerance = f64::EPSILON * scaled.abs().max(1.0) * 16.0;\n"
        "    if (scaled - rounded).abs() > tolerance {\n"
        "        return Err(format!(\n"
        "            \"settlement amount {value} exceeds configured scale {scale}\"\n"
        "        ));\n"
        "    }\n"
        "    if rounded > i64::MAX as f64 {\n"
        "        return Err(\"settlement amount exceeds signed minor-unit range\".to_string());\n"
        "    }\n"
        "    Ok(rounded as i64)\n"
        "}\n\n"
        "fn deterministic_uuid(namespace: &str) -> Uuid {\n",
    )
    replace_exact(
        adapter,
        "    #[test]\n"
        "    fn exact_operation_and_reference_ids_are_stable() {\n",
        "    #[test]\n"
        "    fn compatibility_amount_is_converted_to_exact_minor_units() {\n"
        "        assert_eq!(\n"
        "            exact_minor_from_compatibility_amount(4.24, 6).unwrap(),\n"
        "            4_240_000\n"
        "        );\n"
        "        assert_eq!(\n"
        "            exact_minor_from_compatibility_amount(0.000_001, 6).unwrap(),\n"
        "            1\n"
        "        );\n"
        "        assert!(exact_minor_from_compatibility_amount(0.000_000_1, 6).is_err());\n"
        "        assert!(exact_minor_from_compatibility_amount(f64::NAN, 6).is_err());\n"
        "    }\n\n"
        "    #[test]\n"
        "    fn exact_operation_and_reference_ids_are_stable() {\n",
    )

    api = "services/ledger-service/src/api.rs"
    replace_exact(
        api,
        "    drop(accounts);\n"
        "    state\n"
        "        .idempotency_keys\n",
        "    let account_balance = account.balance;\n"
        "    let account_reserved = account.reserved;\n"
        "    drop(accounts);\n"
        "    sync_exact_memory_account_from_legacy(\n"
        "        state,\n"
        "        account_id,\n"
        "        account_balance,\n"
        "        account_reserved,\n"
        "    )\n"
        "    .await?;\n"
        "    state\n"
        "        .idempotency_keys\n",
    )
    replace_exact(
        api,
        "pub async fn post_trnm_wallet_snapshot(\n",
        "async fn sync_exact_memory_account_from_legacy(\n"
        "    state: &AppState,\n"
        "    account_id: Uuid,\n"
        "    balance: f64,\n"
        "    reserved: f64,\n"
        ") -> Result<(), Response> {\n"
        "    let mut exact = state.exact_memory.write().await;\n"
        "    let Some(opening) = exact.account_openings_by_account.get_mut(&account_id) else {\n"
        "        return Ok(());\n"
        "    };\n"
        "    let Some(account) = opening\n"
        "        .get_mut(\"account_state\")\n"
        "        .and_then(serde_json::Value::as_object_mut)\n"
        "    else {\n"
        "        return Err((\n"
        "            StatusCode::SERVICE_UNAVAILABLE,\n"
        "            Json(ErrorResponse {\n"
        "                error: \"exact_memory_projection_sync_failed\".to_string(),\n"
        "                message: Some(\"exact account state is malformed\".to_string()),\n"
        "            }),\n"
        "        )\n"
        "            .into_response());\n"
        "    };\n"
        "    let scale = account\n"
        "        .get(\"currency_scale\")\n"
        "        .and_then(serde_json::Value::as_u64)\n"
        "        .and_then(|value| u8::try_from(value).ok())\n"
        "        .ok_or_else(|| {\n"
        "            (\n"
        "                StatusCode::SERVICE_UNAVAILABLE,\n"
        "                Json(ErrorResponse {\n"
        "                    error: \"exact_memory_projection_sync_failed\".to_string(),\n"
        "                    message: Some(\"exact account currency scale is missing\".to_string()),\n"
        "                }),\n"
        "            )\n"
        "                .into_response()\n"
        "        })?;\n"
        "    let balance_minor = exact_minor_from_legacy_value(balance, scale).map_err(|message| {\n"
        "        (\n"
        "            StatusCode::SERVICE_UNAVAILABLE,\n"
        "            Json(ErrorResponse {\n"
        "                error: \"exact_memory_projection_sync_failed\".to_string(),\n"
        "                message: Some(message),\n"
        "            }),\n"
        "        )\n"
        "            .into_response()\n"
        "    })?;\n"
        "    let reserved_minor = exact_minor_from_legacy_value(reserved, scale).map_err(|message| {\n"
        "        (\n"
        "            StatusCode::SERVICE_UNAVAILABLE,\n"
        "            Json(ErrorResponse {\n"
        "                error: \"exact_memory_projection_sync_failed\".to_string(),\n"
        "                message: Some(message),\n"
        "            }),\n"
        "        )\n"
        "            .into_response()\n"
        "    })?;\n"
        "    account.insert(\"balance_minor\".to_string(), json!(balance_minor));\n"
        "    account.insert(\"reserved_minor\".to_string(), json!(reserved_minor));\n"
        "    Ok(())\n"
        "}\n\n"
        "fn exact_minor_from_legacy_value(value: f64, scale: u8) -> Result<i64, String> {\n"
        "    if !value.is_finite() || value < 0.0 {\n"
        "        return Err(\"legacy mirror value must be finite and non-negative\".to_string());\n"
        "    }\n"
        "    let factor = 10_i64\n"
        "        .checked_pow(u32::from(scale))\n"
        "        .ok_or_else(|| \"exact account currency scale is unsupported\".to_string())?;\n"
        "    let scaled = value * factor as f64;\n"
        "    let rounded = scaled.round();\n"
        "    let tolerance = f64::EPSILON * scaled.abs().max(1.0) * 16.0;\n"
        "    if (scaled - rounded).abs() > tolerance || rounded > i64::MAX as f64 {\n"
        "        return Err(format!(\n"
        "            \"legacy mirror value {value} cannot be represented at scale {scale}\"\n"
        "        ));\n"
        "    }\n"
        "    Ok(rounded as i64)\n"
        "}\n\n"
        "pub async fn post_trnm_wallet_snapshot(\n",
    )

    tests = "services/consumer-entry-api/src/tests.rs"
    replace_exact(
        tests,
        '        "trillionnium_term_exchange_backend_adapter_v1"\n',
        '        "trillionnium_term_exchange_backend_adapter_v2"\n',
    )
    replace_exact(
        tests,
        "fn exact_test_minor(value: f64) -> i64 {\n"
        "    assert!(value.is_finite() && value >= 0.0 && value.fract() == 0.0);\n"
        "    assert!(value <= i64::MAX as f64);\n"
        "    value as i64\n"
        "}\n",
        "const EXACT_TEST_CURRENCY_SCALE: u8 = 6;\n\n"
        "fn exact_test_minor(value: f64) -> i64 {\n"
        "    assert!(value.is_finite() && value >= 0.0);\n"
        "    let factor = 10_i64.pow(u32::from(EXACT_TEST_CURRENCY_SCALE));\n"
        "    let scaled = value * factor as f64;\n"
        "    let rounded = scaled.round();\n"
        "    let tolerance = f64::EPSILON * scaled.abs().max(1.0) * 16.0;\n"
        "    assert!((scaled - rounded).abs() <= tolerance);\n"
        "    assert!(rounded <= i64::MAX as f64);\n"
        "    rounded as i64\n"
        "}\n",
    )
    replace_exact(
        tests,
        '            "currency_unit": "credits",\n'
        '            "currency_scale": 0,\n'
        '            "opening_minor": opening_minor.to_string(),\n',
        '            "currency_unit": "credits",\n'
        '            "currency_scale": EXACT_TEST_CURRENCY_SCALE,\n'
        '            "opening_minor": opening_minor.to_string(),\n',
    )
    replace_exact(
        tests,
        '            "currency_unit": "credits",\n'
        '            "currency_scale": 0,\n'
        '            "amount_minor": amount_minor.to_string(),\n',
        '            "currency_unit": "credits",\n'
        '            "currency_scale": EXACT_TEST_CURRENCY_SCALE,\n'
        '            "amount_minor": amount_minor.to_string(),\n',
    )

    print("closure fixes applied successfully")


if __name__ == "__main__":
    main()
