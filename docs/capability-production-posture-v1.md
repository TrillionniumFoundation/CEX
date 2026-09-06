# Capability service production posture v1

Status: active repository control  
Production authorization: `not_granted`

## Authority

`capability-service` is a read-only registry boundary. Production-like authority comes only from an explicit, validated `CAPABILITY_STATIC_REGISTRY_JSON` snapshot. Local OpenClaw discovery and built-in demo records are development conveniences and never production authority.

## Production-like startup

For `beta`, `staging`, or `production`:

- `CEX_RUNTIME_PROFILE` or `APP_ENV` must be explicit and non-conflicting;
- `CAPABILITY_STATIC_REGISTRY_JSON` must be valid, non-empty and within the configured record limit;
- capability IDs must be unique and all identity fields must be non-empty;
- at least one capability must be enabled;
- demo, placeholder, change-me, local-development and `openclaw-local` authority markers are rejected;
- local OpenClaw discovery variables are rejected;
- `CAPABILITY_BIND_ADDR` must parse as a socket address;
- any violation exits before serving with configuration exit code 78.

Development/test profiles may retain local discovery and demo fallback, but those records cannot qualify or authorize a production deployment.

## Recovery and rollout

A production registry update is an immutable replacement of the validated startup snapshot. The deployed record set should be associated with a non-secret digest and release candidate. Until a governed hot-reload protocol exists, rollback means restoring the previous approved registry and replacing the process.

## Verification

```bash
cargo test --locked -p capability-service
cargo clippy --locked -p capability-service --all-targets -- -D warnings
python3 scripts/check-capability-production-posture.py
```

A green repository check proves code and configuration rules only. Provider availability, credential custody and production activation remain independently evidenced gates.
