# TRNM economy production credential contract v1

Status: operational security contract

This document defines the launch-time credential and issuer-registry requirements for `scripts/run-trnm-economy-service.sh`. It does not contain credentials and is not approval to activate production traffic. The blank, fail-closed deployment template is `deploy/trnm-economy/trnm-production.env.example`. Repository production authorization remains `not_granted`.

## Required runtime inputs

The secret manager must inject nonempty, high-entropy values for:

- `LEDGER_ADMIN_TOKEN`
- `TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET`
- `TRNM_GAME_AUTHORITY_TOKEN`
- `TRNM_PLAYER_SESSION_SIGNING_SECRET`
- `CONSUMER_ENTRY_INGRESS_TOKEN`
- `CONSUMER_ENTRY_SESSION_AUTH_SECRET`
- `CEX_GATEWAY_API_KEY`
- `CONSUMER_ENTRY_LEAGUE_WEB_SESSION_SECRET`

Every value must be at least 32 characters and pairwise distinct. The launcher rejects missing, blank, short, duplicate, placeholder, and known local-development values before either service starts. No value may be derived from `IDENTITY_ADMIN_TOKEN` or another credential.

## Issuer registry

`TRNM_ENTITLEMENT_ISSUER_REGISTRY_PATH` is mandatory. It must resolve to an absolute, readable regular file mounted by the deployment environment. The launcher does not infer a sibling-repository location and does not create a registry automatically.

The registry contents, issuer-key custody, issuance, rotation, revocation, and break-glass process remain independently reviewed production controls. Repository checks can verify only the fail-closed launch contract.

## Runtime-profile invariants

The lane runs with:

```text
APP_ENV=production
CEX_RUNTIME_PROFILE=trnm-economy
CONSUMER_ENTRY_RUNTIME_PROFILE=production
```

`trnm-economy` and `trnm_economy` are production aliases. They must therefore inherit production startup guards, exact-money enforcement, and the permanent rejection of `CEX_GATEWAY_LEGACY_RESERVE_BREAK_GLASS=true`.

## Deployment path

Systemd units must use a portable deployment root. The repository-supplied units use `%h/.openclaw/workspace/CEX`; operators may generate environment-specific units, but no canonical unit or active runbook may depend on an individual developer's home directory.

## Verification

The following repository checks enforce this contract:

```bash
bash -n scripts/run-trnm-economy-service.sh
python3 scripts/check-p0-wiring.py
python3 scripts/check-p0-release-candidate-hygiene.py
```

Hosted exact-SHA evidence must rerun after any change to the launcher, runtime profiles, systemd units, credential contract, environment template, or candidate trigger. Successful repository checks do not substitute for real secret-custody review or final human go/no-go.
