# Internal Service Authentication v1

## 1. Scope

The first workload-identity slice protects:

`POST /v1/executions`

Only the authenticated `gateway-service` caller may create an execution when enforcement is enabled.

This closes the highest-priority anonymous internal write path without changing the public invocation API.

## 2. Configuration

```env
CEX_INTERNAL_SERVICE_AUTH_MODE=enforce
CEX_INTERNAL_SERVICE_TOKENS_JSON={"gateway-service":"<high-entropy-token>"}
```

Supported modes:

- `off` / `disabled`: compatibility mode for local/dev tests;
- `enforce` / `required`: reject missing, unknown, or invalid service identity.

Production-like gateway and execution binaries require enforce mode during startup.

Tokens must:

- be at least 32 bytes;
- avoid known development/placeholder markers;
- be supplied through a secret-managed environment;
- never be logged or returned in an error response.

## 3. Wire contract

Authenticated internal clients send:

- `x-cex-service-id: gateway-service`
- `x-cex-service-token: <token>`

The execution create route validates:

1. service id exists;
2. service id is allowed for `execution:create`;
3. token exists;
4. token matches using a length-aware constant-time comparison.

Failure returns `401` with a stable error code but no secret detail.

## 4. Client behavior

Gateway replaces its default reqwest client with a client carrying the workload identity headers whenever enforcement is enabled. The headers are harmless on services that do not yet enforce the contract and allow incremental rollout.

## 5. Rollout

1. Generate a high-entropy gateway token in the secret manager.
2. Configure the same token map for gateway and execution deployments.
3. Deploy execution with config present but mode off if a compatibility rehearsal is needed.
4. Verify gateway calls carry both headers.
5. Switch both services to enforce.
6. Verify anonymous and incorrect-token requests return 401.
7. Rotate by introducing a versioned token registry in v2; v1 supports one active token per service id.

## 6. Deliberate limitations

- v1 uses shared symmetric service tokens, not mTLS or signed short-lived JWTs.
- Audience is enforced by route allow-list, not encoded in the credential.
- Audit write and identity resolve remain on the next slice because every existing writer must first be upgraded to carry workload identity.
- Token rotation currently requires coordinated replacement.

## 7. Next slice

- versioned dual-token rotation window;
- authenticated `POST /v1/audit/events`;
- authenticated `POST /v1/auth/resolve`;
- writer identity derived from authenticated principal;
- workload-auth metrics and rejection taxonomy;
- mTLS/SPIFFE or signed service JWT ADR.
