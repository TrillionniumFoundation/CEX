# ADR-008: Authoritative Paper Raid ChallengeRuleset V1

Status: accepted for the post-b5 feature candidate; live validation pending.

## Context

Challenge cards previously encoded difficulty, duration, victory language, and
modifiers only in display text. The Paper state machine enforced one universal
set of existence checks, so the three cards did not create different games and
an API-aware caller could not prove which rules were applied to a Paper.

## Decision

New gameplay Challenges carry `hepta.challenge.ruleset.v1`. It is a closed,
typed object containing one of three template identities, duration and grace,
all six forward phase gates, and terminal victory minimums. Hepta validates
template-specific floors at their required checkpoints, rejects attempts to
move a hard gate into a later/no-op stage, rejects duplicate or missing
transitions, recomputes the canonical JSON SHA-256, and rejects a caller-supplied
hash mismatch.

At Paper creation Hepta snapshots the exact Challenge snapshot hash, template
version, ruleset hash, typed ruleset, enforcement mode, deadline, and grace
deadline. The snapshot hash and deadlines are immutable in PostgreSQL and every
downstream release candidate and research-session authorization binds the
Paper snapshot, not a later Challenge read.

Forward phase transitions evaluate the snapshotted per-template count minima.
Finalization separately evaluates the snapshotted victory minima. Research
session authorization expiry is capped at the grace deadline. Once grace has
elapsed, gameplay mutations fail closed. Before grace elapses the Captain may
record `failed` or `abandoned`. At or after the boundary, the first authorized
raid-state, Paper, room, or events read lazily persists canonical `expired`;
there is no background timer during a zero-request period and a manual reason
cannot win the expiry race. The immutable `terminal_at` is the grace boundary,
while `updated_at` and the event time record later materialization. Terminal
outcomes are one-way, remain in Raid history, and block further gameplay
mutation; `failed`, `expired`, and `abandoned` are never selected as
`current_raid`. `submission_ready` is set only by the existing unanimous signed
finalization path. These gameplay outcomes do not manufacture or overwrite
scientific or Chain finality.

The optional typed `gameplay.role_resources` block freezes bounded Captain,
Evidence, and Experiment focus plus a shared run budget into the same ruleset
hash. Its Paper state is an idempotent action ledger whose balances are always
recomputed by replay. Evidence focus is spent only against a distinct existing
EvidenceCard. Experiment focus and run budget are spent in the same aggregate
write or PostgreSQL transaction that creates the RunRecord. A retained failed
run receives the configured non-economic focus refund; a cancelled run does
not, and run budget is never refunded. A Captain checkpoint requires one new
Evidence assessment and one new run, but is coordination feedback rather than
a scientific, phase, victory, finality, ranking, reward, or economic gate.

Historical Challenges without a typed ruleset and Papers created before this
ADR remain readable. They retain the conservative pre-V1 phase gates, have no
invented deadline, and are explicitly `legacy_unranked`; they cannot acquire
ranked, reward, or economic eligibility from this compatibility path.

## Consequences

- Human descriptions remain presentation only; server rules are authoritative.
- Benchmark/Ablation requires a successful run and a retained failed run;
  Replication requires a successful run; Evidence Audit requires higher
  evidence, citation, and claim minima without inventing an experiment.
- Every template requires terminal work-item closure and at least one accepted
  work item before its victory gate can pass.
- The next authorized raid-state, Paper, room, or events read materializes
  `expired` after the grace boundary; the server rejects late mutations even
  before that lazy projection write.
- The feature branch requires fresh PostgreSQL, browser, and vertical E2E
  validation after the immutable b5 soak. It does not alter b5 evidence.
