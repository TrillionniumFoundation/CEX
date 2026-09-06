# Authoritative money isolation v1

Status: active repository control  
Production authorization: `not_granted`

## Boundary

New value-bearing authority uses versioned Ledger v2 contracts with currency unit, scale and signed integer minor units. Binary floating-point fields remain only in explicitly enumerated compatibility, display or non-production test surfaces. They do not authorize a new account opening, reserve, consume, refund, provider settlement or TRNM finality transition.

The machine-readable boundary is `docs/compatibility/authoritative-money-isolation-v1.json`.

## Required invariants

- files listed as `authoritative_exact_files` contain no `f32` or `f64`;
- exact code never derives minor units through rounding, truncation or an `as i64` cast;
- Gateway reserve and Execution consume/refund load the immutable exact Invocation/Ledger contract;
- production-like Ledger startup requires PostgreSQL and `LEDGER_FAIL_FAST=true`;
- a lost response is recovered through exact operation/receipt lookup;
- legacy v1 value-writing routes remain retired and compatibility reads cannot become write authority.

## Legacy containment

Every known value-related floating-point source is listed with a narrow classification. Adding another such path requires an explicit manifest update, module-contract review and exact-tree requalification. Removal is preferred; the allowlist is not approval for new behavior.

## Verification

```bash
python3 scripts/check-authoritative-money-isolation.py
python3 scripts/check-ledger-caller-cutover.py
python3 scripts/check-p0-wiring.py
```

The repository check establishes source isolation. Independent financial-control review and representative production reconciliation remain external gates.
