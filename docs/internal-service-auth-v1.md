# Internal Service Authentication v1

## Protected operations

The first workload-identity baseline protects:

- `POST /v1/executions`: only authenticated `gateway-service` may create an execution;
- `POST /v1/auth/resolve`: only authenticated `gateway-service` may resolve product API keys;
- `POST /v1/audit/events`: only registered gateway/identity/execution writers may append audit events.

Public invocation APIs remain authenticated by product API key and are unchanged.

## Configuration

```env
CEX_INTERNAL_SERVICE_AUTH_MODE=enforce
CEX_INTERNAL_SERVICE_TOKENS_JSON={
  "gateway-service": "<high-entropy-token>",
  "identity-service": "<high-entropy-token>",
  "execution-service": "<high-entropy-token>"
}
```

Supported modes:

- `off` / `disabled`: compatibility mode for local/dev tests;
- `enforce` / `required`: reject missing, unknown, or invalid service identity.

Production-like gateway, identity, execution and audit startup requires enforce mode.

Tokens must:

- be at least 32 bytes;
- avoid known development/placeholder markers;
- be supplied through a secret-managed environment;
- never be logged or returned in an error response.

## Wire contract

Authenticated internal clients send:

- `x-cex-service-id: <service-id>`
- `x-cex-service-token: <token>`

Validation checks:

1. service id exists;
2. service id is allowed for the requested operation;
3. token exists;
4. token matches using a length-aware constant-time comparison.

Failure returns `401` with a stable error code but no secret detail.

## Identity resolve middleware

Identity applies workload authentication only to the exact `POST /v1/auth/resolve` route. Health, metrics and API-key management endpoints retain their existing authentication models.

Gateway uses one authenticated reqwest client for its internal calls, so resolve, execution create and audit append all carry the same gateway workload identity.

## Audit writer behavior

Audit writer identity is server-owned metadata under `_cex_audit_writer`. Domain `actor_type` and `actor_id` are preserved.

## Rollout

1. Generate distinct high-entropy tokens per service in the secret manager.
2. Configure the token map on all participating services.
3. Rehearse with mode off only in a non-production profile.
4. Verify the expected service headers are present.
5. Enable enforce on gateway/identity/execution/audit together.
6. Verify anonymous and incorrect-token requests return 401.
7. Observe rejection counters once workload-auth metrics land.

## Deliberate limitations

- v1 uses shared symmetric service tokens, not mTLS or signed short-lived JWTs;
- audience is enforced by route allow-list, not encoded in the credential;
- token rotation currently requires coordinated replacement;
- capability internal reads and worker-specific credentials remain to be split;
- rejected-auth metrics and a dual-token rotation window remain next work.

## Next version

- versioned dual-token rotation window;
- authenticated capability registry access;
- worker-specific identities for claim/process/renew;
- workload-auth metrics and rejection taxonomy;
- mTLS/SPIFFE or signed service JWT ADR.
