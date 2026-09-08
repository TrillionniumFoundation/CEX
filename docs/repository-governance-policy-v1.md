# Repository governance policy v1

Status: active policy  
Production authorization: `not_granted`

## Required main-branch ruleset

The GitHub ruleset for `main` must:

- block force pushes and branch deletion;
- require pull requests;
- require at least two approving reviews;
- dismiss stale approvals and require review of the latest push;
- require CODEOWNERS review;
- require resolved conversations;
- require the authoritative exact-SHA migration, Rust, Gateway, Execution, provider and aggregate release-candidate checks;
- prevent bypass except a separately audited emergency role;
- require signed commits or an equivalently verified merge/release identity.

Repository Actions are evidence producers and must use `contents: read`. A workflow with `contents: write` or a workflow shell step that performs `git push` is forbidden by repository checks. Source changes are merged by the reviewed pull-request path, not by convergence automation.

## Ownership

`.github/CODEOWNERS` protects workflows, authority/traceability, migrations, exact-money contracts, project boundaries and critical services.

## Evidence

Source policy is checked by `scripts/check-source-governance.py`. Actual GitHub ruleset state is not inferred from source and must be queried and retained in release evidence. Until the required ruleset is active, production authorization remains `not_granted` and the administration blocker remains open.
