# Release Baseline Manifest v1

## Purpose

A CEX release claim must bind a single source revision to reproducible build, migration, dependency, deployment and approval evidence. A Markdown table saying PASS is not sufficient because later commits can silently inherit stale evidence.

The machine-readable contract is:

- schema: `docs/schemas/cex-release-baseline-manifest-v1.schema.json`
- template: `docs/templates/cex-release-baseline-manifest-v1.json`
- generator: `scripts/generate-release-baseline.py`
- validator: `scripts/check-release-baseline-manifest.py`

## Lifecycle

### Draft

Generated from a clean worktree. It binds:

- repository, branch, commit and tree SHA;
- Cargo.lock SHA-256;
- migration head and migration SHA-256.

Build and runtime evidence remain pending.

### Candidate

Requires:

- real workflow run id;
- digested build artifacts;
- SBOM;
- provenance statement;
- all required evidence pass or explicitly waived;
- at least one approval.

### Released

A candidate deployed through the approved release process. Deployment evidence should be added as a required evidence record before promotion.

### Revoked

Retains the original evidence and adds revocation actor, timestamp and reason. Manifests are not deleted or rewritten into a new release.

## Generate

```bash
./scripts/generate-release-baseline.py \
  --release-id cex-2026.08.27-rc1 \
  --output run/release/cex-2026.08.27-rc1.json
```

Generation requires a clean worktree and discovers the highest numbered SQL migration.

## Validate

```bash
./scripts/check-release-baseline-manifest.py \
  run/release/cex-2026.08.27-rc1.json
```

The checked-in template uses explicit all-zero placeholders and is validated only with:

```bash
./scripts/check-release-baseline-manifest.py \
  docs/templates/cex-release-baseline-manifest-v1.json \
  --allow-template
```

Candidate/released manifests can never use template mode.

## Evidence rules

Every evidence item has:

- stable name;
- status: pending/pass/fail/waived;
- URI to immutable evidence;
- SHA-256 of the evidence bytes;
- waiver rationale when waived.

Recommended required evidence:

- hosted CI;
- full integration gate;
- fresh migration;
- supported-version upgrade migration;
- backup/restore;
- soak;
- security scan;
- monitoring deploy verification;
- incident/rollback drill.

## Storage

Drafts may live under ignored `run/release/`. Candidate/released manifests should be attached to the GitHub Release and copied to immutable release storage. Their digest should be included in deployment metadata.

## Security

- Do not embed secrets, database URLs or tokens.
- Artifact URIs must point to immutable IDs, not `latest`.
- Image references require digest, not tag alone.
- Git commits/tags used for release should be signed.
- Revocation never removes the original manifest.
