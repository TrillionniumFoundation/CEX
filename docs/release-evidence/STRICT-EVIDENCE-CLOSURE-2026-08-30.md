# Strict exact-SHA evidence closure

Status: active repository qualification control

The active v12 candidate must not treat a workflow-level `success` as sufficient evidence. The aggregate candidate workflow now requires all five authoritative runs to be bound to the exact branch, commit and run attempt, and independently verifies their complete expected job set.

For every selected hosted job, qualification requires:

- a positive GitHub runner identifier and non-empty runner name;
- the exact candidate commit and exact workflow run attempt;
- a completed successful job;
- a non-empty step list;
- a successful checkout step;
- at least one successful substantive step;
- no failed, cancelled, timed-out or action-required step.

The generated candidate manifest contains the exact thirteen-entry v12 set, ending in first-class `local-evidence-binding` and `hosted-gate-execution` attestations. `repository-governance` and `hosted-run-execution` remain mandatory exact-tree, indexed payload-only attestations. Every manifest entry must be `pass`, and waivers are prohibited.

The strict manifest contract binds each hosted evidence URI to the final collector context (including
the exact run/attempt and workflow path), binds all attestation digests to the context file index,
and re-hashes every indexed payload file immediately before manifest validation. The governance
observer also records and checks the exact Git tree and re-reads the candidate branch after its API
snapshot. The shared candidate-trigger sequence is the sole committed freeze authority; secondary
qualification-freeze markers are forbidden.

The hosted-gate checker performs the only hosted latest-run selection. The strict collector injects
that exact, job-validated snapshot into the evidence core in-process; the exact-attempt verifier
and hosted-gate attestation consume the same frozen context. A freshness verifier revalidates the
run/attempt set after job proof, before manifest generation, and after manifest publication. No
later step may silently select or substitute a different rerun.

The aggregate workflow records nine required governance contexts (the five constituent gates,
the four repository/Rust/Hepta/aggregate contexts) and fails closed when any is absent. Exact-state
backup/restore uses the host PostgreSQL clients when complete, otherwise a credential-safe Docker
client fallback with an explicitly mapped local port; it never puts the database URI in process
arguments.

Any `run/p0-release-support` files produced by the aggregate workflow are transient diagnostics, not
part of the canonical upload namespace. Lifecycle qualification is carried by the retained
`database-lifecycle.json` record and the exact hosted job/step attestation, so a support log cannot
silently become a second evidence source.

This strengthens repository qualification only. Production authorization remains `not_granted`, and external gates X1 through X8 remain independently evidenced upstream blockers.
