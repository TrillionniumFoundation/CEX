# Audit Writer Authentication v1

## Scope

`POST /v1/audit/events` is protected by the internal workload-identity middleware when `CEX_INTERNAL_SERVICE_AUTH_MODE=enforce`.

Registered writers:

- `gateway-service`
- `identity-service`
- `execution-service`

The audit service requires all three writer tokens in production-like profiles. Each writer uses its own token from `CEX_INTERNAL_SERVICE_TOKENS_JSON`.

## Writer identity binding

The existing audit contract uses `actor_type` for business semantics such as `policy-engine`, `approver`, or a service role. Replacing it with the authenticated workload would lose information, so v1 keeps that field unchanged.

Instead, the audit service writes authenticated workload identity into the server-owned payload key:

```json
{
  "_cex_audit_writer": {
    "service_id": "execution-service",
    "authentication": "workload-token-v1"
  }
}
```

Rules:

- callers cannot select the persisted writer identity;
- any caller-supplied `_cex_audit_writer` value is overwritten;
- scalar/array payloads are wrapped under `event_payload` so writer metadata can be attached;
- compatibility mode does not mutate legacy payloads;
- `actor_type` and `actor_id` remain domain actor information.

This payload representation is transitional. A later migration will add a dedicated `writer_service_id` column and versioned event envelope, then backfill from the reserved metadata key.

## Identity authority hardening in the same slice

Identity uses a library entry wrapper that installs an authenticated internal HTTP client and clears the in-memory/static API-key map in production-like profiles after environment loading.

Consequences:

- the public `local-dev-key` cannot become authoritative after a database query failure;
- the transitional random deny-only sink is also removed from runtime state after construction;
- database lookup failures return backend-unavailable rather than authenticating through static state;
- local/dev tests retain their existing static-key behavior.

The legacy static fallback source still exists and should be physically removed once the branch has executable CI evidence.

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

1. dedicated writer columns and versioned event envelope;
2. transactional audit outbox in each source service;
3. at-least-once delivery with event-id dedupe;
4. append-only database role and retention policy;
5. per-tenant sequence/hash-chain or signed receipt;
6. backlog age metrics and alerting;
7. payload schema versioning and redaction policy.
