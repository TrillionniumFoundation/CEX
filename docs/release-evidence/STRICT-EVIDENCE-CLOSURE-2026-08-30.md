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

The generated candidate manifest contains first-class `repository-governance` and `hosted-run-execution` evidence entries. Candidate evidence must contain the exact thirteen-entry v12 set, every entry must be `pass`, and waivers are prohibited.

The strict manifest contract binds each hosted evidence URI to the final collector context (including
the exact run/attempt and workflow path), binds all attestation digests to the context file index,
and re-hashes every indexed payload file immediately before manifest validation. The governance
observer also records and checks the exact Git tree and re-reads the candidate branch after its API
snapshot. The committed qualification-freeze sequence must equal the shared trigger sequence.

This strengthens repository qualification only. Production authorization remains `not_granted`, and external gates X1 through X8 remain independently evidenced upstream blockers.
