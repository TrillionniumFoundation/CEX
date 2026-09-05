# CEX development changelog

This is a development record, not release evidence. The exact current PR commit
and repository evidence determine what was implemented and actually verified.

## Unreleased — Sequence 54 integration work, 2026-09-05

Architecture sequence remains 52 under accepted ADR-004. No Sequence 54 candidate
freeze is claimed by this entry. Production authorization: `not_granted`.

The current audit-remediation branch adds repository governance documents, removes
the two legacy source-writing Sequence-53 CI workflows, and changes TRNM build
qualification to require the committed lock. TRNM build evidence now depends on
the static-contract job and explicit prior step outcomes, is bound to committed
source bytes, and is collected into a fresh closed-set directory outside checkout.
The packet verifier detects missing, additional, altered or linked files. This is
not a deployment package or an independently approved final candidate.

Earlier changes on this branch implement stricter Matrix startup profiles,
source-observation replay, bounded sync-gap recovery, poison quarantine evidence,
Matrix send bindings and receipts, response validation, current-schema SQL test
orchestration, and more accurate source-derived route semantics. Their existence
does not mean Rust compilation or real PostgreSQL/homeserver tests have passed.

Remaining work is tracked in `docs/status/audit-remediation-2026-09-05.json`.
Complete-source lock/inventory integration, runtime qualification, adapter result
reconciliation, large-gap and role/SLO work, first-playable integration and all
independent release conditions remain explicitly open where evidence is missing.
