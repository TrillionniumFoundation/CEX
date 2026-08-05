# Hepta Research League failure recovery

This runbook covers the Hepta module only. Nakama remains the sole
authoritative realtime match writer; TRNM remains the sole finality writer.
Recovery must never manufacture either module's facts.

## Invariants

- Keep all results `pending_finality` until an authenticated TRNM receipt says
  they are final.
- Never skip a Nakama event sequence or accept a mismatched completed root.
- Never export or log `HEPTA_NAKAMA_AUTHORIZATION_ED25519_SEED_BASE64`.
  Signed authorization documents are public handoff artifacts; replaying one
  cannot bypass Nakama's durable one-time `authorization_id` consumption.
- Never put raw papers, datasets, prompts, outputs, or private research on
  TRNM. Only commitments and protocol metadata cross that boundary.
- Do not introduce an Agent runner, model credential, inference route, or a
  fourth top-level module while recovering the service.

## Hepta process or instance loss

1. Confirm PostgreSQL is reachable and migration 0031 is applied.
2. Start a replacement instance with the same database and three separately
   scoped service secrets.
3. Check `/ready`; it must report `storage=postgresql`,
   `agent_execution_mode=external_only`, and exactly `hepta/nakama/trnm`.
4. Check `/metrics` for the pending-finality and Nakama-match gauges.
5. Resume outbox workers. Expired leases are reclaimable; delivered rows are
   not selected again.
6. Query affected Nakama reconciliation endpoints before publishing results.

## Outbox worker crash

Do not manually clear rows. Allow its lease to expire, then claim with another
worker using the normal `FOR UPDATE SKIP LOCKED` path. Consumers must write
their inbox record in the same transaction as their projection. An
acknowledgement is accepted only from the worker that owns the live lease.

## TRNM outage or reorganization

Continue challenge, match, evaluation, and reproduction work. Keep commands
and displayed results `pending_finality` or `provisional`. Re-deliver the same
idempotency key after connectivity returns. Only a separately authenticated
receipt advances the projection to `finalized`.

## Nakama outage or event gap

Stop new match admission at the edge. Existing Nakama reconnect policy owns
the realtime outcome. Hepta rejects non-contiguous sequences. Recover missing
events from Nakama's archive/outbox, replay in order, and verify the completed
event root. Hepta must not synthesize a missing round or completion.

## PostgreSQL restore

Restore through the repository's PostgreSQL backup/PITR procedure. Before
reopening writes, compare:

1. `hepta_league_state.revision`;
2. undelivered and leased `hepta_outbox` rows;
3. `hepta_inbox` deduplication keys;
4. finalized receipt projections against a TRNM read node;
5. completed match roots against Nakama archives.

Re-delivery is safe and expected; duplicated business effects are not.
