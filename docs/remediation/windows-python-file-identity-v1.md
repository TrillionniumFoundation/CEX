# Native Windows file-identity qualification

Status: native timestamp repair requiring exact-head hosted verification.
Production authorization: `not_granted`.

## Observed failures

The Windows job on `fb16531dedbbef30523f79908db438ae028dea47`
(job `103479691404`) passed development documentation, then failed the original
candidate-hygiene scratch-file self-test at path/descriptor identity comparison.
Its image manifest and later interpreter observation identified Python 3.12.10.

Commit `dd8ba373683ae8eb2d1cc1d187b0377bbb291200` selected CPython 3.13.15 and
added an actual scratch-file diagnostic. Native job `103489709268` then proved
that changing Python alone did not resolve the defect. Device, file identifier,
size and last-write values matched, but path `st_ctime_ns` was
`1789183249024729800` while descriptor `st_ctime_ns` was
`1789183249025285900`. Both observations were internally stable. These values
are recorded observations, not fabricated success evidence or production data.

Python documents Windows `st_ctime` as deprecated and recommends the distinct
`st_birthtime` field for creation time. Cross-API comparison must not assume that
creation time and metadata-change time are interchangeable. Neither removing all
timestamp checks nor comparing only size is an acceptable repair.

## Native identity, without a timestamp fallback

The hygiene bootstrap and byte-bound workflow-trust adapter now obtain Windows
stamps through `GetFileInformationByHandleEx`: `FileIdInfo` retains the full
128-bit identifier and volume identity; `FileStandardInfo` provides size, link
count, directory and deletion state; `FileBasicInfo` provides creation,
last-write and metadata-change times as distinct native 100-nanosecond integers.
Each native snapshot reads the record set twice and rejects disagreement.
Path snapshots use `CreateFileW` with read-attributes access, read sharing,
`OPEN_EXISTING` and `FILE_FLAG_OPEN_REPARSE_POINT`. The helper never creates,
truncates, writes, or follows a final reparse point; unsupported native APIs fail
closed. Existing CRT descriptors are borrowed, not closed by the stamp helper.

Before-open, opened-descriptor, after-read-descriptor and final-path snapshots
must agree on the complete native stamp, including metadata-change time. Each
stamp is additionally bound to Python's device, full identifier, size, last-write
and explicit birthtime values. This retains the original byte/read-size bounds
and rejects changes between the Python and native observations. Zero identities,
unavailable times, reparse points, directories, pending deletion and multiple
links fail. The POSIX five-field stat identity and its ctime checks are unchanged.

The native routines intentionally remain inline in both bootstraps: no new
repository module is imported before workflow trust. Their three definitions
are AST-compared with the diagnostic's definitions in the existing Windows
preflight self-test. Pure record tests cover metadata mismatch, every changed
stamp field, full-width identifiers and unsafe records. Local POSIX tests are
not native Windows execution; the unchanged hosted hygiene and later Cargo jobs
must still execute successfully.

## Scope and retained requirements

The original four workflow jobs, existing steps, Rust selectors, database lanes,
permissions, triggers, concurrency, manifest checks and status names remain.
The pinned interpreter is not a waiver. The complete target catalog, actual Cargo
metadata, Windows builds and Linux runtime/CLI/database tests remain required.
No production Matrix CLI portability, source qualification, live Ruleset,
independent review, custody or release approval is granted by this repair.
The existing workflow-trust implementation blob and all its old corrections and
negative tests remain intact. Only its bootstrap file-reading boundary changes.

Primary sources: Python `os.stat_result` documentation; Microsoft Learn
`FILE_BASIC_INFO`, `FILE_STANDARD_INFO`, `FILE_ID_INFO`,
`FILE_INFO_BY_HANDLE_CLASS`, and `CreateFileW`; `actions/runner-images` tag
`win25-vs2026/20260907.229`; immutable `actions/setup-python` v7.0.0 commit
`5fda3b95a4ea91299a34e894583c3862153e4b97`.

Close this subtask only after native conformance and unchanged hygiene pass on
the repaired head. Full Windows qualification additionally requires subsequent
Cargo and service-local steps to succeed, not be skipped. The separate target
catalog repair and all broader plan requirements remain open until accepted.
