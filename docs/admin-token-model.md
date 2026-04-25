# Admin Token Model

This note is the single source of truth for how CEX resolves management/admin tokens today.

## Why this exists

CEX now has four different management surfaces with different scope needs:

- **key management / identity admin** needs `api_keys:manage`
- **audit trace read** needs `audit:read`
- **execution lifecycle admin/operator access** needs `executions:manage` and sometimes `executions:read`
- **direct ledger/account admin access** needs `ledger:manage` and sometimes `ledger:read`

The repo still supports a single shared dev token for compatibility, but the preferred rehearsal shape is **split admin principals**.

## Supported shapes

### 1. Shared-token compatibility path

Use one token that carries all needed scopes:

- `api_keys:manage`
- `audit:read`
- `executions:manage`
- `executions:read`
- `ledger:manage`
- `ledger:read`

Typical dev fallback:

- `local-dev-admin-token`

This remains supported so local dev and older flows do not break.

### 2. Split-admin preferred rehearsal path

Use separate principals/tokens:

- identity/key-management token, for example `identity-manage-token`
- identity read-only token, for example `identity-read-token`
- audit-read token, for example `audit-read-token`
- execution manage token, for example `execution-manage-token`
- execution read-only token, for example `execution-read-token`
- ledger manage token, for example `ledger-manage-token`
- ledger read-only token, for example `ledger-read-token`

Typical env shape:

- `IDENTITY_ADMIN_TOKENS_JSON` contains one or more records with `api_keys:manage` and optionally separate records with `api_keys:read`
- `AUDIT_ADMIN_TOKENS_JSON` contains one or more records with `audit:read`
- `EXECUTION_ADMIN_TOKENS_JSON` contains one or more records with `executions:manage` and optionally separate records with `executions:read`
- `LEDGER_ADMIN_TOKENS_JSON` contains one or more records with `ledger:manage` and optionally separate records with `ledger:read`

Ready-to-copy example:

- `.env.split-admin.example`

Helpful Windows helpers:

- `scripts/use-split-admin-env.ps1`
- `scripts/validate-split-admin-env.ps1`

## Scope requirements

### Key management / identity admin write endpoints

Required scope:

- `api_keys:manage`

Examples:

- `POST /v1/api-keys`
- `POST /v1/api-keys/:id/revoke`

### API key list / read endpoints

Required scope:

- `api_keys:read`

Also accepted:

- `api_keys:manage`

Examples:

- `GET /v1/api-keys?org_id=...`

When a dedicated read-only identity principal exists, runtime list/read helpers should prefer that `api_keys:read` token before falling back to a broader `api_keys:manage` token.

### Audit trace reads

Required scope:

- `audit:read`

Examples:

- `GET /v1/audit/events/trace/:trace_id`

### Execution lifecycle write endpoints

Required scope:

- `executions:manage`

Examples:

- `POST /v1/executions/:id/approve`
- `POST /v1/executions/:id/reject`
- `POST /v1/executions/:id/dispatch`
- `POST /v1/executions/:id/start`
- `POST /v1/executions/:id/cancel`
- `POST /v1/executions/:id/timeout`
- `POST /v1/executions/:id/succeed`
- `POST /v1/executions/:id/fail`
- `POST /v1/executions/:id/provider-dead-letter/ack`

### Execution read endpoints

Required scope:

- `executions:read`

Also accepted:

- `executions:manage`

Examples:

- `GET /v1/executions/:id`
- `GET /v1/executions/provider-dead-letters`

When a dedicated execution read-only principal exists, runtime helpers should prefer `executions:read` before falling back to `executions:manage`.

### Ledger/account write endpoints

Required scope:

- `ledger:manage`

Examples:

- `POST /v1/accounts`
- `POST /v1/ledger/reserve`
- `POST /v1/ledger/consume`
- `POST /v1/ledger/refund`

### Ledger/account read endpoints

Required scope:

- `ledger:read`

Also accepted:

- `ledger:manage`

Examples:

- `GET /v1/accounts/:id`

When a dedicated ledger read-only principal exists, runtime helpers should prefer `ledger:read` before falling back to `ledger:manage`.

## Resolution precedence

### Identity / key-management resolution

When code needs a token for `api_keys:manage`, precedence is:

1. `IDENTITY_ADMIN_TOKENS_JSON`
2. `IDENTITY_ADMIN_TOKEN`
3. `APP_ENV=dev` fallback to `local-dev-admin-token`

### Audit-read resolution

When code needs a token for `audit:read`, precedence is:

1. `AUDIT_ADMIN_TOKENS_JSON`
2. shared `IDENTITY_ADMIN_TOKENS_JSON`
3. `AUDIT_ADMIN_TOKEN`
4. `IDENTITY_ADMIN_TOKEN`
5. `APP_ENV=dev` fallback to `local-dev-admin-token`

This lets audit use its own dedicated principal when present, while still supporting older shared-token setups.

### Execution admin resolution

When code needs a token for execution lifecycle access, precedence is:

1. `EXECUTION_ADMIN_TOKENS_JSON`
2. shared `IDENTITY_ADMIN_TOKENS_JSON`
3. `EXECUTION_ADMIN_TOKEN`
4. `IDENTITY_ADMIN_TOKEN`
5. `APP_ENV=dev` fallback to `local-dev-admin-token`

This lets execution runtime probes and direct execution-service admin flows use a dedicated principal when present, while still preserving the older shared-token path.

### Ledger admin resolution

When code needs a token for direct ledger/account access, precedence is:

1. `LEDGER_ADMIN_TOKENS_JSON`
2. shared `IDENTITY_ADMIN_TOKENS_JSON`
3. `LEDGER_ADMIN_TOKEN`
4. `IDENTITY_ADMIN_TOKEN`
5. `APP_ENV=dev` fallback to `local-dev-admin-token`

This lets ledger-service direct account and credit mutation surfaces use a dedicated principal when present, while still preserving the older shared-token path.

## Bundle format

Token bundles are JSON arrays of records like:

```json
[
  {
    "token": "identity-manage-token",
    "actor_id": "identity-admin",
    "actor_label": "Identity Admin",
    "scopes": ["api_keys:manage"],
    "org_ids": ["00000000-0000-0000-0000-00000000ce01"]
  }
]
```

Audit-read bundles look the same, but include `audit:read` in `scopes`. Execution bundles include `executions:manage` and optionally `executions:read`. Ledger bundles include `ledger:manage` and optionally `ledger:read`.

### Optional org scoping

Management principals can optionally declare `org_ids`.

- when `org_ids` is omitted or empty, the principal is treated as unrestricted across orgs
- when `org_ids` is present, identity key-management endpoints only allow operations against those org ids
- audit-read principals can also declare `org_ids`; when they do, `GET /v1/audit/events/trace/:trace_id` only succeeds for traces whose audit events carry one matching org boundary
- execution principals can also declare `org_ids`; when they do, direct execution reads and lifecycle transitions only succeed for executions whose persisted `org_id` matches one of those orgs
- ledger principals can also declare `org_ids`; when they do, direct account reads and ledger mutations only succeed for accounts whose persisted `org_id` matches one of those orgs, and account creation only succeeds for requested orgs inside that allowed set

This is the current path toward a more real operator/admin permission model, instead of leaving every valid management token globally powerful.

## Repo behavior today

The following now follow the same scoped-token model instead of assuming anonymous or single-token-only access:

- `identity-service`
- `audit-service`
- `execution-service`
- `ledger-service`
- `gateway-service` runtime blackbox helpers
- `gateway-service/tests/runtime_approval_probe.rs` when it directly hits execution-service or ledger-service admin endpoints
- legacy/manual PowerShell probes under `scripts/legacy/` when they read protected audit traces

So split-admin rehearsal should behave consistently across:

- service-local Rust tests
- ignored runtime blackbox tests
- compatibility/manual PowerShell probes

## Current status

- Shared-token dev mode is still supported for compatibility.
- Split-admin is the preferred rehearsal model.
- On this Linux session, Windows/PowerShell full-gate execution is still **ready for validation**, not already proven here.
