# Native Windows Python file-identity qualification

Status: environment correction requiring exact-head hosted verification.
Production authorization: `not_granted`.

The Windows job on source `fb16531dedbbef30523f79908db438ae028dea47`
(job `103479691404`) passed development documentation, then failed the original
candidate-hygiene scratch-file self-test at path/descriptor identity comparison.
No later Cargo or service-local step executed. This is not evidence that Cargo,
PostgreSQL or production custody passed, and is not fixed by suppressing the
file-identity assertion.

The image identified itself as `windows-2025-vs2026/20260907.229.1`; its upstream
manifest lists Python 3.12.10. The log did not print the mismatching identity
fields, so the precise CPython/host cause is not asserted from that log alone.
The Windows lane now selects CPython 3.13.15 explicitly through the immutable
`actions/setup-python` v7.0.0 commit before any existing Python gate. It records
the preinstalled interpreter version and checks the selected interpreter's actual
scratch-file path and descriptor identities. Exact-version selection alone is
not sufficient: native conformance and every original gate must pass.

`scripts/check-python-file-identity.py` compares device, file identity, size and
nanosecond modification/change values across path-before-open, descriptor-open,
descriptor-after-read and path-after-close, and checks single-link regular-file
status plus exact scratch bytes. It never reads a repository credential, uploads
scratch contents or turns a fixture into release evidence. The output includes
the measured values so another incompatibility is diagnosable. Missing identity,
zero file identity, changed fields, or a wrong requested Python version fails.
The existing hygiene reader, stable-read checks and negative tests are unchanged.
No filesystem field is dropped merely to make Windows pass.

The original four jobs, all existing steps, Rust selectors, database lanes,
workflow permissions, triggers, concurrency, manifest checks and status names
are retained. The complete target catalog, actual Cargo metadata, native Windows
build and original Linux runtime/CLI/database checks remain required separately.
This change does not port the POSIX operator CLI to Windows or waive the remaining
module-target catalog repair. Public deployment, live Rulesets, independent
reviews, custody and final approval remain external requirements.

Primary inputs:

- `actions/runner-images` tag `win25-vs2026/20260907.229`,
  `images/windows/Windows2025-VS2026-Readme.md`.
- `actions/setup-python` immutable release `v7.0.0`, commit
  `5fda3b95a4ea91299a34e894583c3862153e4b97`.
- Python release-team announcement for 3.13.15 dated 2026-08-05.

Local POSIX fixture tests are not native Windows execution. Close this environment
subtask only after the actual Windows step and unchanged hygiene complete on the
new exact subject; close full Windows qualification only when its subsequent
Cargo and service-local steps also succeed without skipped work.
