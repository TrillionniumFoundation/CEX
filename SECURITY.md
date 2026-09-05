# Security policy

## Supported source

Security review is bound to an exact commit and tree. The supported development
line is the current repository candidate derived from `main`. Historical plans,
draft branches, local worktrees and artifacts from another SHA are not
security-qualified releases.

Production authorization is not granted by this policy, by a source commit, or
by a green repository-owned workflow.

## Reporting a vulnerability

Do not disclose credentials, private research material, exploit details or
personally identifying data in a public issue.

Use the repository's private security-advisory channel when available. When
private reporting is unavailable, contact the TrillionniumFoundation repository
administrators through an organization-controlled private channel and request a
private advisory before sending sensitive details.

Include the affected repository, commit SHA and tree SHA; component or protocol;
impact; attacker capability; a minimal synthetic-data reproduction; affected
authority boundary; and recovery constraints. Redact tokens, database URLs,
private keys, prompts, research payloads, session material and real identifiers.

## Response priorities

Critical reports include credential compromise, signature bypass, cross-tenant
authority, Ledger double effect, fabricated provider success, immutable-evidence
mutation, finality forgery, remote code execution, or fail-open production
startup.

Repository maintainers may reject, contain and repair a candidate. Independent
security review, credential custody, deployment rollback and final human
authorization remain separately evidenced external controls.

## Secret handling

Committed examples contain placeholders only. Secrets come from approved
custody, remain separated where active contracts require it, and never enter
logs, traces, metrics, issues, fixtures, candidate manifests or repository-owned
external-evidence templates. Suspected compromise requires revocation or
rotation before reuse.

## Disclosure

Coordinate public disclosure after a fixed exact candidate has been reviewed
and affected operators have had a reasonable opportunity to contain and
recover. This policy does not promise that an unqualified candidate is safe for
production.
