# Matrix response decoding and malformed-event quarantine v1

Status: source implementation; real Rust/PostgreSQL/homeserver acceptance pending
Owner: matrix-integration
Parent: v12 implementation addendum, Blocks H and K; Matrix recovery contract v2
Production authorization: `not_granted`

## Corrected failure paths

A `/messages` response ends pagination by omitting its `end` property. The previous
`Option<String>` field also mapped an explicitly present JSON null to None. Thus
`{"start":"new","end":null,"chunk":[]}` could complete local pagination even
though no valid completion boundary was supplied. An empty chunk with a real end
token must still continue. This revision distinguishes those cases at decoding.

The previous sync/page deserializers also allowed duplicate keys inside dynamic
maps and event Values to replace earlier values, and ignored an `errcode` beside
otherwise success-shaped fields. Those are ambiguous/error envelopes, not a
sound basis for a cursor decision. A missing/non-string/empty event type was
separately skipped as though it were a recognized unsupported event type.

The fixes affect the poller only. They do not change the adapter's public legacy
constructors, implement large-gap staging or grant a new homeserver trust model.

## Wire-byte boundary

`apps/matrix-bot-poller/src/wire_response.rs` owns the bounded `decode<T>` helper.
Both `/sync` and each `/messages` page use it before any bootstrap/admission cursor
transition or pagination acceptance. `/sync` now requires HTTP 200, as `/messages`
already did. A non-200 response fails without interpreting its body as success.

The first pass uses Serde's JSON decoder with a recursive map visitor. It rejects
duplicate decoded property names at every object depth, including room maps,
event identities, content and extension data. Escaped spellings of the same name
are duplicates. Equal names in different objects are independent and allowed.
No duplicate value is selected according to first/last-wins interpretation.

The top-level input must be an object. A top-level `errcode` always rejects the
envelope, including null or a non-string value; nested message data named errcode
is not an error envelope. Unique unknown extension fields remain available to the
existing typed decoder. The scan ends explicitly at EOF/trailing whitespace;
concatenated JSON values, invalid UTF-8 and malformed JSON fail.

The second pass decodes the same original bytes directly into the existing
response type, without JSON reserialization or a Value-to-struct conversion.
This preserves existing number handling and field semantics. Object key sets
are held only during the structural scan; the complete parsed response is not
retained twice. This adds a second bounded parsing pass and has not been
performance-qualified. It is not a cryptographic canonicalization scheme.

The helper has a 4 MiB ceiling, matching the maximum allowed poller response size.
Existing smaller configured body limits, request timeouts, recovery page/event/
byte budgets and Serde recursion protection remain enabled. Diagnostics use
closed error codes and never echo a remote key, event, cursor or parse exception.

## Missing versus null pagination tokens

`MessagePage.end` and `TimelineState.prev_batch` use a default only when the field
is absent. A present value must deserialize as a String; null, booleans, numbers,
arrays and objects fail. Existing token emptiness, size and control-character
checks still apply when a token is used. No token is converted into an unfiltered
request, a new partition or a manually advanced cursor.

An omitted end retains the existing meaning: the homeserver-visible range is
exhausted. It does not prove visibility of deleted, inaccessible or previously
filtered events. An empty page with a new end token continues; cycle, wrong-start
and budget rejection are unchanged. A missing limited-timeline boundary still
holds. Initial explicit start_now still establishes a new boundary without
executing historical messages, but its wire envelope is checked first.

## Quarantine instead of silent loss

A nonempty string naming an unsupported event type retains the old ignore policy.
An absent, empty or non-string type, including a scalar or null event, now creates
an `Admission::Poison` with `invalid_event_type`, restricted source payload and a
stable event identity or synthetic content-derived identity. It is not a command.

The existing poison-observation transaction runs before the admission transaction.
The existing unacknowledged-poison guard then prevents cursor advance and business
admission until the established operator quarantine procedure permits it. No new
acknowledgement authority, SQL reset, schema or bypass is added. Raw private bytes
stay in the existing restricted database path, not logs or public artifacts.

Ambiguous/error JSON is rejected before typed admission, so that rejection does
not itself create a poison row. Its cursor remains unchanged. This distinction
is intentional: the decoder does not invent a canonical event from ambiguous
bytes. Quarantine is not proof that a malformed message was repaired or executed.

## Compatibility and verification

Healthy event normalization, source hashes and delivery IDs are unchanged. A
previously tolerated null boundary, duplicate key, error-shaped response or
malformed event type now rejects or holds. This is a deliberate compatibility
change. Do not disable the checks, auto-acknowledge poison or change the cursor
just to restore throughput; correct/review the upstream data and requalify.

No Cargo dependency, lockfile, SQL migration, original SQL assertion or workflow
job is changed. The existing complete poller Cargo test/Clippy lanes own the new
Rust cases; source mutation tests remain a separate, limited check:

```text
cargo test --locked -p matrix-bot-poller --all-targets
cargo clippy --locked -p matrix-bot-poller --all-targets -- -D warnings
python3 scripts/test-matrix-recovery-contract.py
python3 scripts/check-matrix-recovery-contract.py
bash scripts/check-matrix-source-observation-postgres.sh
```

Twenty new Rust test definitions cover wire ambiguity, token absence/null, error
objects, bounds, event quarantine and stable healthy delivery identity. They were
not executed in the authoring environment. Python source checks do not execute
the Rust decoder, a homeserver or PostgreSQL transactions. Real package builds,
wire/lease/restart/poison integration and the final exact-tree authority gates
remain required. The separate embedded-adapter configuration gap remains open.

Primary interface references:

```text
https://spec.matrix.org/v1.19/client-server-api/#get_matrixclientv3roomsroomidmessages
https://serde.rs/deserialize-map.html
https://docs.rs/serde_json/1.0.151/serde_json/struct.Deserializer.html
```
