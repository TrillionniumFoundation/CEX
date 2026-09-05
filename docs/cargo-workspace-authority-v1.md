# Cargo workspace and target authority v1

Status: unqualified source implementation
Owner: platform-foundations / trnm-integration
Parent: v12 implementation addendum, Blocks H and K
Production authorization: `not_granted`

## Corrected inventory boundary

The earlier documentation checker read only the literal `workspace.members` array
and compared that list with the catalog. Cargo can also include path dependencies
inside the workspace. The CEX root manifest already pointed at five local vendor
packages, and several of those packages inherit workspace metadata. A literal
18-entry comparison therefore could not by itself prove actual Cargo membership.

This revision explicitly names the existing five vendor packages in addition to
the 18 existing first-party packages, and gives each its own catalog entry and
module contract. The declared total is 23; real Cargo must still confirm it.
Nothing is excluded from Linux workspace checks to manufacture equality. No
vendor source, package version, dependency declaration or Cargo.lock byte is
changed. `term-exchange-protocol` remains the dependency alias for the package
`trnm-economy-protocol`; the alias is not another package.

Local vendored libraries are not external running authorities. Chain consensus,
Nakama, World/Game runtimes and participating Agents remain external. The
verifier package contains a Unix command-line target, but no resident CEX service
is created by this inventory correction. Its existing Windows library-only
qualification exception remains unchanged; Linux retains all targets.

## Executed authority checker

```text
python3 scripts/check-cargo-workspace-authority.py
```

The CLI requires a real Cargo executable and complete source inputs. It invokes:

```text
cargo metadata --locked --no-deps --format-version 1 --manifest-path Cargo.toml
```

The implementation supplies an absolute manifest path for the current checkout.
Normal Cargo registry/cache prerequisites may still apply; this is not an
independent offline dependency resolver. No saved-metadata argument, success
fixture mode or skip-on-missing-tool option is exposed by the CLI.

Before invoking Cargo, the checker validates the explicit member list and catalog
bijection, actual manifest package names, module documents and catalog source
files, retaining their bytes plus Cargo.lock. Paths must remain repository-local
and cannot traverse symlinks. The current repository profile requires explicit
paths rather than globs. Cargo's JSON must have the supported format, exact
workspace root, unique package/member identities and local member manifests.

Actual Cargo members must equal the catalog, including automatically included
packages. Every discovered target must have an existing local source path listed
in the correct module entry. This covers libraries, binaries, integration tests,
examples, benches, proc macros and custom build scripts; an automatic target is
not exempt because it has no explicit `[[bin]]` or `[[test]]` stanza. Unknown
kinds, duplicate identities and target paths outside their module are rejected.

Input bytes are compared again after the command. A nonzero Cargo exit, timeout,
oversized metadata, malformed/duplicate JSON or changed inputs yields a failure.
The report includes input SHA-256 values only for the inputs actually acquired.
A trusted Cargo/Python toolchain and private stable checkout remain prerequisites;
this byte comparison is not a hostile-filesystem snapshot or an independent host
attestation. Cargo stderr is not copied into the report. Captured output is bounded
on readback, not by an operating-system quota or hostile-process sandbox.

## Evidence and failure interpretation

The report schema is `cex.cargo-workspace-authority.v1`. `status=ok` requires actual
successful metadata execution and all comparisons. `cargo_metadata_succeeded`
remains false for missing tooling, command errors or failed validation, and
`compilation_proven` is always false: metadata does not compile tests or execute
business workflows. Production authorization always remains `not_granted`.

Useful failure categories include `cargo_member_not_catalogued`,
`catalog_member_not_in_cargo`, `cargo_target_not_documented`, package/root
mismatch, missing source, source drift and `cargo_required_not_executed`.
Discovery of an additional target is a real documentation gap to resolve; do not
remove it from the build, substitute old metadata or relabel it external to clear
the gate. The CLI deliberately has no report-writing option; existing CI log
custody and exact-tree integrity provide the enclosing execution evidence.

Both existing Linux and Windows service-local Rust authority jobs run this check
after installing the selected toolchain and restoring the existing Cargo cache.
The existing source-only document checks, semantic inventory check, formatting,
compilation, package tests, Clippy and final aggregate are all preserved.
Repository-integrity and Matrix source jobs execute the pure fixture regressions,
not a fake invocation presented as Cargo acceptance.

## Vendored source and provenance follow-through

The four Chain crates have a separate recorded origin in
`vendor/trnm-chain-vendor-manifest.json`; the economy crate is not in that record.
At inspected parent `f1b09501221ce9885c63447b23b684ff6de8b9ba`, the verifier's current
subtree was `37427509e5913b1c71e35082d5609c69a27a978d`, while the manifest recorded
`3db7fe2b40faa15d9597ea2824b1079ca93c754c`. This remains a vendor-policy reconciliation item. The subsequent round-14
diagnostic below establishes its introducing CEX change without waiving it. This round
changes neither vendor bytes nor that record. Review the actual differences and
approved upstream policy before making a provenance claim; replacing recorded
hashes with current values is not by itself a valid repair.

The new contracts distinguish entitlement shape from signature verification,
canonical encodings from command acceptance, explicit authority sets from
caller-supplied keys, and receipt verification from finality/custody/financial
authority. These are developer boundaries and required integration checks, not
measured performance or completed security qualification.

## Verification and completion

```text
python3 scripts/test-cargo-workspace-authority.py
python3 scripts/check-module-documentation.py
python3 scripts/check-cargo-workspace-authority.py
python3 scripts/generate-repository-semantics.py --check
```

The new test suite uses synthetic Cargo JSON, mock command responses and actual
local filesystem operations. It checks detection of implicit member omissions,
alias confusion, missing automatic targets, duplicate/invalid metadata, source
changes, command failures and refusal to consume a saved metadata replacement.
Those tests do not run Cargo or establish the membership of a complete CEX build.

In the current authoring environment, a real CLI attempt failed with
`cargo_required_not_executed`. Only declared/catalog equality and the five new
contracts' source structure were checked locally. Complete source/target coverage,
updated full semantic inventory, Cargo resolution/compilation and exact-head
hosted qualification remain open. All existing independent release conditions
and the sole final-candidate trigger remain unchanged.

Primary semantic references:

```text
https://doc.rust-lang.org/cargo/reference/workspaces.html
https://doc.rust-lang.org/cargo/commands/cargo-metadata.html
```

## Round 14 integration and target-layout stability

The formerly unpublished round-13 patch is integrated with an additional
before/after layout check. A regression demonstrated that a new automatic
`src/bin/*.rs` target created after the metadata response was omitted from the
old source snapshot, because that snapshot covered only catalogued paths.

The checker now records directory existence and child names/types for each
member root, `src`, `src/bin`, `tests`, `examples`, `benches`, and one level of
nested target directories. This covers new default build.rs, conventional
lib/main, and nested main.rs discovery locations. Missing and empty directories
are distinct. A 100,000-entry total budget bounds this scan; linked and special
entries fail. Creation, removal or renaming during metadata execution invalidates
the result even when the new target was never in the catalog. Directory-only
changes may conservatively fail too. Cargo, not this directory inventory, still
decides which files are actual targets.

The report adds `target_layout_sha256` separately from actual input-file hashes.
This scan is not a semantic Rust parser or an atomic adversarial-filesystem
snapshot. A trusted stable checkout is required. It does not detect intermediate
changes restored before both observations, or certify unrelated descendant
source modules. The authoritative full-tree checks remain separate.

Eight new filesystem/metadata-fixture tests cover late target/build creation,
removal, nested main.rs, symlink rejection, bounds, unchanged-layout output and
empty-directory creation. They are not Cargo or Rust execution. Real Cargo
metadata, compilation, SQL, complete-source and hosted qualification remain open.

`status/vendor-provenance-reconciliation-round14.md` traces the verifier mismatch
to a single test-block replacement in CEX commit `b0105fa6788784df677e4cb124654420dfd63a3d`.
The source manifest's byte-for-byte policy is unchanged and the discrepancy is
not relabelled compliant. No independent approval is inferred from that commit.
