# Audit Writer Authentication v1

## Scope

`POST /v1/audit/events` is now protected by the internal workload-identity middleware when `CEX_INTERNAL_SERVICE_AUTH_MODE=enforce`.

Registered writers:

- `gateway-service`
- `identity-service`
- `execution-service`

The audit service requires all three writer tokens in production-like profiles. Each writer uses its own token from `CEX_INTERNAL_SERVICE_TOKENS_JSON`.

## Writer identity binding

An authenticated request cannot choose its persisted service identity. The audit handler overwrites `AuditEventCreateRequest.actor_type` with the authenticated `x-cex-service-id` value.

`actor_id` remains the domain actor/operator identity supplied by the authenticated service. This keeps two concepts separate:

- writer/service identity: authenticated workload;
- domain actor identity: user, administrator, worker, or operator represented by that service.

Compatibility mode preserves the legacy `actor_type` so existing local tests and unprotected development flows can continue while migration is in progress.

## Identity authority hardening in the same slice

Identity now uses a library entry wrapper that installs an authenticated internal HTTP client and clears the in-memory/static API-key map in production-like profiles after environment loading.

Consequences:

- the public `local-dev-key` cannot become authoritative after a database query failure;
- the transitional random deny-only sink is also removed from runtime state after construction;
- database lookup failures return backend-unavailable rather than authenticating through static state;
- local/dev tests retain their existing static-key behavior.

The legacy static fallback code still exists in the included source and should be physically removed in a later refactor once the branch has executable CI evidence.

## Configuration

```env
CEX_INTERNAL_SERVICE_AUTH_MODE=enforce
CEX_INTERNAL_SERVICE_TOKENS_JSON={
  "gateway-service": "<gateway-token>",
  "identity-service": "<identity-token>",
  "execution-service": "<execution-token>"
}
```

The JSON must be encoded on one line when supplied as a normal dotenv value.

## Failure semantics

Audit writes return `401` for:

- missing service id;
- missing token;
- unregistered writer;
- token mismatch.

Configuration errors reject audit-service startup before listening in production-like profiles.

## Remaining integrity work

Authentication does not by itself make the audit trail compliance-grade. The next required layer is:

1. transactional audit outbox in each source service;
2. at-least-once delivery with event id dedupe;
3. append-only database role and retention policy;
4. per-tenant sequence/hash-chain or signed receipt;
5. backlog age metrics and alerting;
6. payload schema versioning and redaction policy.
