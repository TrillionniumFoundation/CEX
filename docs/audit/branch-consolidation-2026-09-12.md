# Branch consolidation record — 2026-09-12

This record describes the repository state observed before consolidation. The selected
product tree is the latest remediation candidate (`remediation/cex-audit-acceptance-20260912`).
Branches containing diagnostics, probes, backups, temporary workflow experiments or superseded
implementations are retained in Git history only through the consolidation merge and are not
used as the product source tree. Their source files are intentionally not copied into `main`:
adding them would reintroduce source-writing workflows, stale gates and destructive experiment
artifacts that the active v12 hygiene contract requires to be absent.

The final commit and tree hashes, the remote branch deletion result, and the post-consolidation
integrity attestation must be generated after the merge. This file is an operational record, not
release qualification or production authorization.

| Classification | Treatment |
|---|---|
| Active remediation candidate | Fast-forwarded into `main`; this is the selected product tree. |
| Already contained by the candidate | Recorded as merged ancestry; no duplicate content commit is created. |
| Older source or workflow experiment | Recorded as a parent of the consolidation merge with its files superseded by the active candidate. |
| `tmp/`, `probe`, `__*`, backup, schema/ruleset probe | Recorded for provenance only; excluded from product content. |

Production authorization remains `not_granted`; branch consolidation does not create hosted or
external evidence and does not close any V12-X1 through V12-X8 gate.

## Observed remote heads

The following inventory was generated with `git fetch --all --prune` and records every remote head except `main`; it is the input to the consolidation operation.

| Branch | SHA | Commits after original main | Classification |
|---|---|---:|---|
| `__do_not_use__` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `__ruleset_schema_probe__` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `__ruleset_tool_discovery_only__` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `__schema_probe__` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `__schema_probe_ruleset_function__` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `automation/seq39-exact-apply` | `0ceefbbfbe2e0c9092c5b56f170791342b39ae52` | 93 | older source/workflow experiment |
| `automation/seq39-validated` | `4e00480376682a9cc4cb3c9b3b2aece579767a95` | 67 | already contained by candidate |
| `automation/seq40-workflow-trust-windows` | `e23fde3a931b128dcf2200dcab9c040de253af37` | 91 | older source/workflow experiment |
| `automation/seq41-matrix-clippy-close` | `710a65580b6fe537affba1364ace0a612aa999a5` | 78 | already contained by candidate |
| `automation/seq42-final-gap-closure` | `04dd42500f32a63ab7ad377487dc2aa993929aae` | 86 | already contained by candidate |
| `automation/seq44-residual-gap-closure` | `95d55d25dc8d23b4b6ea70cd93595436d44fcac1` | 89 | already contained by candidate |
| `backup/cex-v12-sequence54-before-gap-closure-20260909` | `882014451c22026dfe4848248f2b26a9b46ac049` | 454 | diagnostic/probe/backup |
| `fix/cex-all-blocker-closure-20260908` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | already contained by candidate |
| `fix/cex-rust-toolchain-convergence-20260908` | `31fcb8c8d99ab6fdef7c29f22831b9d84818d802` | 144 | older source/workflow experiment |
| `fix/cex-v12-audit-remediation-20260905` | `acc798ad07c1005dd8b94b54c4a71eff6288f4c7` | 276 | already contained by candidate |
| `fix/cex-v12-final-closure-20260908` | `f7de71a2ef1fb7f359ce5bda604a2ebadf2f1e5a` | 164 | older source/workflow experiment |
| `fix/cex-v12-final-closure-v2-20260908` | `e0952feb86b300f554cbc49e1d7e5cedaa930d41` | 139 | older source/workflow experiment |
| `fix/cex-v12-final-closure-v3-20260908` | `ba0719e3d23a3d3533ba8941438bb848322b9044` | 140 | older source/workflow experiment |
| `fix/cex-v12-final-closure-v4-20260908` | `9e1426c09cb19d062504b4a6732bf83867f675ad` | 172 | already contained by candidate |
| `fix/cex-v12-final-closure-v5-20260908` | `ec75ca4377f5b6001d8aded956cacd138c604664` | 123 | older source/workflow experiment |
| `fix/cex-v12-operator-evidence-gate-20260905` | `bb8f78a98a1a3168bd29a16a5475ee44944b5333` | 251 | already contained by candidate |
| `fix/cex-v12-replay-snapshot-integration-20260905` | `42beccc10ea9914eeacd18b277dfe1ca723cd6bb` | 241 | already contained by candidate |
| `fix/cex-v12-ruleset-negative-probe-20260908` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | already contained by candidate |
| `fix/cex-v12-seq45-full-gap-closure-20260902` | `dc0862b8cf88a1f4e6328d519947e19b81122de0` | 96 | already contained by candidate |
| `fix/cex-v12-seq51-architecture-gap-closure-20260902` | `5956f87e06aafeb4413e8db1f56599c7d5d46ec8` | 142 | already contained by candidate |
| `fix/cex-v12-seq52-final-gap-closure-20260903` | `652a0524076206006fa7298ce67a83c78e2a670e` | 171 | already contained by candidate |
| `fix/cex-v12-seq52-freeze3-repository-facts-20260903` | `da13cb7dfb77fb113d9ae170996d1601284664db` | 181 | already contained by candidate |
| `fix/cex-v12-seq52-freeze3-surface-edge-gap-closure-20260903` | `652a0524076206006fa7298ce67a83c78e2a670e` | 171 | already contained by candidate |
| `fix/cex-v12-seq53-close-repository-gaps-20260904` | `04491cff7cb317324f57a10de07872d39f3e56c2` | 221 | older source/workflow experiment |
| `fix/cex-v12-seq53-close-repository-gaps-20260904-copy` | `a2b17fe6212fb0fa5c7b67b53ca054219ca4aa4a` | 184 | already contained by candidate |
| `fix/cex-v12-seq53-close-repository-gaps-20260904-current` | `a2b17fe6212fb0fa5c7b67b53ca054219ca4aa4a` | 184 | already contained by candidate |
| `fix/cex-v12-seq53-close-repository-gaps-20260904-head` | `a2b17fe6212fb0fa5c7b67b53ca054219ca4aa4a` | 184 | already contained by candidate |
| `fix/cex-v12-seq53-close-repository-gaps-20260904-probe` | `a2b17fe6212fb0fa5c7b67b53ca054219ca4aa4a` | 184 | already contained by candidate |
| `fix/cex-v12-seq53-close-repository-gaps-20260904-probe2` | `a2b17fe6212fb0fa5c7b67b53ca054219ca4aa4a` | 184 | already contained by candidate |
| `fix/cex-v12-seq53-close-repository-gaps-20260904-snapshot` | `a2b17fe6212fb0fa5c7b67b53ca054219ca4aa4a` | 184 | already contained by candidate |
| `fix/cex-v12-seq53-matrix-durability-20260903` | `652a0524076206006fa7298ce67a83c78e2a670e` | 171 | already contained by candidate |
| `fix/cex-v12-seq53-pr-target-stable-20260905` | `b0c8e419caad2629371581fa303ba189c1fd2862` | 220 | already contained by candidate |
| `fix/cex-v12-seq53-repository-facts-and-matrix-durability-20260903` | `652a0524076206006fa7298ce67a83c78e2a670e` | 171 | already contained by candidate |
| `fix/cex-v12-seq53-surface-edge-gap-closure-20260903` | `652a0524076206006fa7298ce67a83c78e2a670e` | 171 | already contained by candidate |
| `fix/cex-v12-source-hardening-stage-20260906` | `21525243f7b1ca476b6aee0736e2c1fff25b5604` | 278 | older source/workflow experiment |
| `fix/hepta-p0-blocker-closure-20260906` | `e492f8650729bf4f39409ff4220780b886289057` | 120 | already contained by candidate |
| `fix/hepta-sequence54-blocker-closure-20260911` | `6994d3d72bb0f992d47dbc8fec4ccfe5ab4bf3a0` | 605 | already contained by candidate |
| `fix/hepta-sequence54-blocker-closure-v3-20260911` | `5a56108f0ccfc60a023418460582dc6d59107592` | 613 | older source/workflow experiment |
| `fix/hepta-v12-gap-closure-20260829` | `d7e706968ff3b75f372d617c380799eefc4c4014` | 67 | already contained by candidate |
| `fix/p0-blocker-closure-20260906` | `fa84599a15e9013d148a0407c9693ac9c4d4932c` | 8 | already contained by candidate |
| `fix/repository-semantics-world-map-20260911` | `74418fcb0ec6c179902ee593dafab2b73055a66c` | 619 | already contained by candidate |
| `fix/ruleset-schema-probe-20260908` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | already contained by candidate |
| `fix/sequence54-matrix-v3-runtime-closure-20260911` | `54cfc188fc09bb3c0e36818a2b85a46a89d002b1` | 611 | already contained by candidate |
| `fix/world-map-delta-tick-race-20260911` | `692a783037f178e100e1bcdf6a1d75ba5bcafc4e` | 618 | already contained by candidate |
| `integration/cex-v12-sequence54-20260908` | `74418fcb0ec6c179902ee593dafab2b73055a66c` | 619 | already contained by candidate |
| `ops/cex-blocker-batch-28c9-20260910` | `1f6f2910cb8b6d74863dd038fdb1cdd34ef3059d` | 578 | older source/workflow experiment |
| `ops/cex-evidence-stage-combined-28c9-20260910` | `4ede6f7c783873f275b79f76d55e828c19087f11` | 578 | older source/workflow experiment |
| `ops/object-export-fc8643d-20260910` | `756aee7c0ee9ce841a3fc9e310ccdfa6620bf85f` | 607 | older source/workflow experiment |
| `ops/world-batch-validator-58f76a-20260910` | `2e23e094ecaee1bf9ed88ab35c502ac440b3198e` | 575 | older source/workflow experiment |
| `probe-do-not-create` | `9c788ea93c28003e970364989d92d2d6e27ceb36` | 455 | diagnostic/probe/backup |
| `probe/sequence54-self-hosted-allocation-20260908` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `remediation/cex-audit-acceptance-20260912` | `81bd52e5e0ee19722089b32b2cbe2ce4698b6f76` | 627 | active remediation candidate |
| `tmp/cex-admin-probe-r2` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/cex-advisory-chain-20260907` | `9ff233ee47fdb3387c7c1d2f7e56b9bb30fdf159` | 107 | diagnostic/probe/backup |
| `tmp/cex-advisory-v2-20260907` | `0030409407103ad04df9783cbd08ea217a8afd26` | 105 | diagnostic/probe/backup |
| `tmp/cex-closure-state-20260907-r2` | `ad20ccbe50b8ee11e0c954439b5ac6e39b108f84` | 103 | diagnostic/probe/backup |
| `tmp/cex-sequence17-validation-20260907` | `3e8f52ceda2db16fb0cabe80127e7382ec46f497` | 102 | diagnostic/probe/backup |
| `tmp/cex-world-pin-materialize-20260907` | `d1a98e33eff75d5862449e1b3a8cfbb0a64a6613` | 1 | diagnostic/probe/backup |
| `tmp/cex-world-repin-sequence17-20260907` | `3f3305837a89fc3968d93ce5e733f9a0be74bd9e` | 104 | diagnostic/probe/backup |
| `tmp/cex-world-repin-sequence18b-20260907` | `99308dee8873cc01c42b0933af5562a6dcdf67f3` | 103 | diagnostic/probe/backup |
| `tmp/controller-probe-20260907` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-controller-final2` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-controller-final3` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-convergence-controller-20260907` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-convergence-controller-20260907-final` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-convergence-controller-20260907-r2` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-convergence-controller-20260907-r3` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-convergence-controller-20260907-r4` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-convergence-controller-20260907-r5` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-convergence-controller-20260907-r6` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/p0-finalizer-20260907` | `ca66b08913659df5439c478deec60731ff515e67` | 1 | diagnostic/probe/backup |
| `tmp/ruleset-probe-20260907` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/schema-probe` | `db75c74094748a1139fd64b0360e122d8ec797a0` | 0 | diagnostic/probe/backup |
| `tmp/sequence54-metadata-restamp-20260909` | `882014451c22026dfe4848248f2b26a9b46ac049` | 454 | diagnostic/probe/backup |
| `tmp/sequence54-semantic-regenerate-c529-20260910` | `c36c405a629e45783ba8352e2ca7a33afd9b597c` | 578 | diagnostic/probe/backup |
| `tmp/sequence54-source-export-de90027-20260910` | `530d4414dcf7db28c970a6020bcead78a27258fb` | 14 | diagnostic/probe/backup |
| `tmp/vendor-provenance-materialize-20260907` | `b58a3397712d0a123f88b2e3f87ba226feb408da` | 98 | diagnostic/probe/backup |
| `tmp/world-plan-v4-development-transfer-20260831` | `40732362128b9649554afafb0927ec452b8ccb36` | 4 | diagnostic/probe/backup |
| `tmp/world-single-writer-catalog-closure-20260907` | `d4c005b84972ad3835fa4715062f75de463f0776` | 114 | diagnostic/probe/backup |
| `tooling/data-only-catalog-full-20260912` | `d58f523c807971384c458338c545a9833b9ff7a2` | 630 | older source/workflow experiment |
| `tooling/source-export-world-map-20260911` | `7551e454d69b7a2003b9917df38300bb17e47e6f` | 625 | older source/workflow experiment |
| `verify/cex-current-full-20260910` | `13722af3fc2bcc9736a3b61dbfbc1a7163949bc8` | 587 | older source/workflow experiment |
| `verify/pr65-merge-da620412-20260912` | `da620412a33ab8d604ca6e6e6c612754980a7b7c` | 628 | older source/workflow experiment |
| `work/sequence54-gap-closure-20260909` | `5b9f896ecc39045432c6029396704038e8ff912b` | 458 | older source/workflow experiment |
| `work/sequence54-round2-staging-20260909` | `9f207140635acb99c0e599c3888b1bb06494bb81` | 604 | older source/workflow experiment |

## Local consolidation result

- Provenance merge commit: `ac4f9ce3d757a8abb795a49ba5a83e038cb99b43`
- Resulting tree: `fe367a91829d2aa2ac7fb8eec28af62077d12ce9`
- Remote heads represented as merge parents: 90 (all fetched remote branches except `main` and the symbolic `HEAD`).
- The merge used the `ours` strategy for superseded/diagnostic tips so their history is auditable without reintroducing stale or source-writing artifacts.
