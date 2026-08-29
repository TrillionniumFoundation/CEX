import assert from "node:assert/strict";
import test from "node:test";
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readdir,
  rm,
  symlink,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";

import {
  AUTHOR_WORK_START_SCHEMA,
  actionableDeliveryCandidates,
  authorWorkStartKey,
  authorWorkStarts,
  challengeMaterialBundles,
  challengeMaterialObjectQuery,
  deliveryCandidateKey,
  deliveryCandidateForAuthorOutput,
  deliveryCandidates,
  downloadAssignedChallengeMaterials,
  materializeAssignedChallengeMaterials,
  proposalInput,
  validateMaterialPublisherExecutable,
  workResult,
} from "../src/work.mjs";
import { canonicalJsonBytes, sha256Digest } from "../src/canonical.mjs";

const PAPER_ID = "11111111-1111-4111-8111-111111111111";
const WORK_ID = "22222222-2222-4222-8222-222222222222";
const REVISION_ID = "33333333-3333-4333-8333-333333333333";
const MANIFEST_ID = "44444444-4444-4444-8444-444444444444";
const BINDING_ID = "66666666-6666-4666-8666-666666666666";
const DRAFT_ID = "77777777-7777-4777-8777-777777777777";
const LEASE_ID = "88888888-8888-4888-8888-888888888888";
const MANIFEST_HASH = `sha256:${"a".repeat(64)}`;
const PAYLOAD_HASH = `sha256:${"b".repeat(64)}`;
const PLAYER_ID = "99999999-9999-4999-8999-999999999999";
const CHALLENGE_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const ACTIVATION_ID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

const materialBytes = Object.freeze({
  brief: Buffer.from("# Frozen brief\n", "utf8"),
  dataset: Buffer.from('{"rows":[1]}', "utf8"),
  baseline: Buffer.from("print('baseline')\n", "utf8"),
  evaluator: Buffer.from("print('evaluate')\n", "utf8"),
});

function withCanonicalHash(value, hashField) {
  const frame = { ...value };
  delete frame[hashField];
  return {
    ...value,
    [hashField]: sha256Digest(canonicalJsonBytes(frame)),
  };
}

function materialAuthority(overrides = {}) {
  return withCanonicalHash({
    schema: "hepta.paper_raid.frozen_challenge_material_authority.v1",
    authority_hash: `sha256:${"0".repeat(64)}`,
    activation_id: ACTIVATION_ID,
    activation_request_sha256: `sha256:${"1".repeat(64)}`,
    challenge_id: CHALLENGE_ID,
    challenge_snapshot_hash: `sha256:${"2".repeat(64)}`,
    template: "evidence-audit",
    pack_id: "paper-raid-evidence-audit-seeded-v1",
    pack_manifest_hash: `sha256:${"3".repeat(64)}`,
    ruleset_version: "paper-raid-evidence-audit-v1",
    ruleset_hash: `sha256:${"4".repeat(64)}`,
    dataset_manifest_hash: `sha256:${"5".repeat(64)}`,
    evaluator_manifest_hash: `sha256:${"6".repeat(64)}`,
    ...overrides,
  }, "authority_hash");
}

function materialObjects() {
  return [
    ["brief", "playable_brief", "challenge/brief.md", "text/markdown; charset=utf-8"],
    ["dataset", "dataset", "challenge/dataset.json", "application/json"],
    ["baseline", "baseline_code", "challenge/baseline.py", "text/x-python; charset=utf-8"],
    ["evaluator", "frozen_evaluator", "challenge/evaluator.py", "text/x-python; charset=utf-8"],
  ].map(([objectKey, role, logicalPath, mediaType]) => ({
    object_key: objectKey,
    source_path: `source/${objectKey}`,
    logical_path: logicalPath,
    role,
    digest: sha256Digest(materialBytes[objectKey]),
    size_bytes: materialBytes[objectKey].length,
    media_type: mediaType,
    download_path: "/api/agent-bridge/challenge-objects",
  }));
}

function legacyGoldenAuthority(overrides = {}) {
  const objects = [
    [
      "brief",
      "qualification/legacy-golden/brief.md",
      "challenge/brief.md",
      "playable_brief",
      "sha256:b926b4c868af652b2fed4671efc07f9bbc021c65ac188c971c2a2b67c365a9a3",
      913,
      "text/markdown; charset=utf-8",
    ],
    [
      "dataset",
      "data/synthetic-observations.csv",
      "challenge/dataset.csv",
      "dataset",
      "sha256:b002e6297f6fd781742866533b89bf781c7f21d5cfa7b42b5c97a9ecd5821314",
      230,
      "text/csv; charset=utf-8",
    ],
    [
      "baseline",
      "code/baseline.py",
      "challenge/baseline.py",
      "baseline_code",
      "sha256:059717cc82d10cee0504ed6af3fa81121d7a7645c53d8e8a788a35f63ae644ba",
      1071,
      "text/x-python; charset=utf-8",
    ],
    [
      "evaluator",
      "evaluator/legacy-golden-evaluator.py",
      "challenge/evaluator.py",
      "frozen_evaluator",
      "sha256:63971194ab97e1d14752795ff1ff8c39a44d1a7782ffbb9459ec2210fe8a8e3d",
      3483,
      "text/x-python; charset=utf-8",
    ],
  ].map(([objectKey, sourcePath, logicalPath, role, digest, sizeBytes, mediaType]) => ({
    object_key: objectKey,
    source_path: sourcePath,
    logical_path: logicalPath,
    role,
    digest,
    size_bytes: sizeBytes,
    media_type: mediaType,
    download_path: "/api/agent-bridge/challenge-objects",
  }));
  return withCanonicalHash({
    schema: "hepta.paper_raid.legacy_golden_qualification_material_authority.v1",
    authority_hash: `sha256:${"0".repeat(64)}`,
    qualification_id: "paper-raid-golden-v2-strict-review-v1",
    challenge_id: CHALLENGE_ID,
    challenge_snapshot_hash: `sha256:${"2".repeat(64)}`,
    challenge_title: "Paper Raid: Reproducible Synthetic Ablation",
    challenge_description: "Reproduce a public deterministic baseline, retain the failed run, and deliver one evidence-bound ablation as a short paper. paper-raid-alpha-evaluator-sha256:805ee4ad69fa1e56cebc0721711419d211cf0fe2090e294db25bf712f10c74f8",
    challenge_status: "open",
    ruleset_version: "paper-raid-golden-v2",
    ruleset_hash: "sha256:39cfc6a5c883e49b78bf315b53079336539c932ca48ff5a040415d7e5dd9b2c0",
    ruleset_absent: true,
    dataset_manifest_hash: "sha256:6c0494ec10383018b4a938528d179d16fcb6dd8961b8294ff6f4093dda42aeb4",
    evaluator_manifest_hash: "sha256:805ee4ad69fa1e56cebc0721711419d211cf0fe2090e294db25bf712f10c74f8",
    objects,
    ...overrides,
  }, "authority_hash");
}

function legacyGoldenBundle(authority = legacyGoldenAuthority()) {
  return materialBundle({
    authority,
    authority_hash: authority.authority_hash,
    objects: authority.objects.map(object => ({ ...object })),
  });
}

function materialBundle(overrides = {}) {
  const authority = overrides.authority || materialAuthority();
  return withCanonicalHash({
    schema: "hepta.paper_raid.assigned_challenge_material_bundle.v1",
    bundle_hash: `sha256:${"7".repeat(64)}`,
    authority,
    authority_hash: authority.authority_hash,
    paper_project_id: PAPER_ID,
    challenge_ruleset_snapshot_hash: `sha256:${"8".repeat(64)}`,
    binding_id: BINDING_ID,
    player_id: PLAYER_ID,
    work_item_id: WORK_ID,
    work_item_version: 3,
    objects: materialObjects(),
    ...overrides,
  }, "bundle_hash");
}

function challengeInbox(bundle = materialBundle(), taskOverrides = {}) {
  return {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: BINDING_ID,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      phase: "drafting",
      tasks: [{
        work_item_id: WORK_ID,
        paper_project_id: PAPER_ID,
        kind: "evidence_analysis",
        assigned_binding_id: BINDING_ID,
        assigned_player_id: PLAYER_ID,
        status: "in_progress",
        version: 3,
        ...taskOverrides,
      }],
      challenge_materials: {
        schema: "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
        status: "available",
        reason_code: null,
        items: [bundle],
      },
      delivery_candidates: {
        schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
        status: "available",
        reason_code: null,
        items: [candidate()],
      },
    }],
  };
}

function authorStartInbox(taskOverrides = {}) {
  const value = challengeInbox(materialBundle(), taskOverrides);
  value.papers[0].proposals = [];
  value.papers[0].delivery_candidates = {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
    status: "unavailable",
    reason_code: "no_recoverable_author_delivery",
    items: [],
  };
  return value;
}

function candidate(overrides = {}) {
  return {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidate.v1",
    delivery_draft_id: DRAFT_ID,
    binding_id: BINDING_ID,
    paper_id: PAPER_ID,
    work_item_id: WORK_ID,
    section_key: "methods",
    lease_id: LEASE_ID,
    lease_fencing_token: 7,
    expected_work_version: 3,
    parent_revision_id: REVISION_ID,
    proposal_kind: "delivery",
    payload_hash: PAYLOAD_HASH,
    artifact_manifest_id: MANIFEST_ID,
    artifact_manifest_hash: MANIFEST_HASH,
    delivery_state: "pending",
    declared_at_unix: 1_999_999_100,
    expires_at_unix: 2_000_000_000,
    ...overrides,
  };
}

function inbox({ status = "available", items = [candidate()], projection = {} } = {}) {
  return {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: BINDING_ID,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      phase: "drafting",
      tasks: [],
      proposals: [],
      delivery_candidates: {
        schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
        status,
        reason_code: status === "unavailable"
          ? "authoritative_task_section_manifest_binding_not_modeled"
          : null,
        items,
        ...projection,
      },
    }],
  };
}

test("work consumes one explicit server-bound delivery without local joins", () => {
  const [selected] = deliveryCandidates(inbox());
  assert.equal(deliveryCandidateKey(selected),
    `${DRAFT_ID}:${PAPER_ID}:${WORK_ID}:methods:${REVISION_ID}:${MANIFEST_ID}`);
  assert.deepEqual(proposalInput(selected), {
    delivery_draft_id: DRAFT_ID,
    paper_id: PAPER_ID,
    work_item_id: WORK_ID,
    section_key: "methods",
    parent_revision_id: REVISION_ID,
    lease_id: LEASE_ID,
    lease_fencing_token: 7,
    expected_work_version: 3,
    proposal_kind: "delivery",
    payload_hash: PAYLOAD_HASH,
    artifact_manifest_id: MANIFEST_ID,
    artifact_manifest_hash: MANIFEST_HASH,
    declared_at_unix: 1_999_999_100,
  });
});

test("work stays idle when the authority cannot prove a delivery binding", () => {
  assert.deepEqual(deliveryCandidates(inbox({
    status: "unavailable",
    items: [],
  })), []);
});

test("review-only papers safely have no Author projection but malformed Author fields fail", () => {
  const reviewOnly = {
    schema: "hepta.paper_raid.agent_bridge.inbox.v2",
    binding_id: BINDING_ID,
    assurance: "self_declared_unverified",
    papers: [{
      paper_id: PAPER_ID,
      review_tasks: {
        schema: "hepta.paper_raid.agent_bridge.review_tasks.v1",
        status: "unavailable",
        reason_code: "no_executable_review_assignment",
        items: [],
      },
    }],
  };
  assert.deepEqual(deliveryCandidates(reviewOnly), []);
  assert.deepEqual(authorWorkStarts(reviewOnly), []);

  const missingAuthorProjection = structuredClone(reviewOnly);
  missingAuthorProjection.papers[0].tasks = [];
  assert.throws(
    () => deliveryCandidates(missingAuthorProjection),
    /delivery candidate projection is unsupported/,
  );
  const malformedAuthorProjection = structuredClone(reviewOnly);
  malformedAuthorProjection.papers[0].delivery_candidates = {
    schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
    status: "available",
    reason_code: null,
    items: [],
  };
  assert.throws(
    () => deliveryCandidates(malformedAuthorProjection),
    /available delivery projection is invalid/,
  );
});

test("persisted submitting and consumed candidates remain restart-recoverable", () => {
  for (const deliveryState of ["submitting", "consumed"]) {
    const recovery = inbox({
      items: [candidate({ delivery_state: deliveryState })],
    });
    recovery.papers[0].phase = "submission_ready";
    assert.equal(deliveryCandidates(recovery)[0].delivery_state, deliveryState);
  }
  const stalePending = inbox();
  stalePending.papers[0].phase = "submission_ready";
  assert.throws(
    () => deliveryCandidates(stalePending),
    /outside an author phase/,
  );
});

test("watch suppresses only a locally acknowledged consumed replay", () => {
  const consumed = inbox({
    items: [candidate({ delivery_state: "consumed" })],
  });
  consumed.papers[0].phase = "submission_ready";
  const key = deliveryCandidateKey(candidate());
  assert.equal(actionableDeliveryCandidates(consumed, new Set()).length, 1);
  assert.equal(actionableDeliveryCandidates(consumed, new Set([key])).length, 0);

  const restartedProcess = new Set();
  assert.equal(actionableDeliveryCandidates(consumed, restartedProcess).length, 1);
  assert.throws(
    () => actionableDeliveryCandidates(consumed, []),
    /must be a Set/,
  );
});

test("legacy task x lease x manifest collections are never guessed", () => {
  const legacy = inbox();
  legacy.papers[0].delivery_candidates = {
    leases: [{ section_key: "methods", status: "active" }],
    section_heads: [{
      section_key: "methods",
      current_head_revision_id: REVISION_ID,
    }],
    artifact_manifests: [{
      manifest_id: MANIFEST_ID,
      manifest_hash: MANIFEST_HASH,
    }],
  };
  assert.throws(() => deliveryCandidates(legacy), /projection is unsupported/);
});

test("unavailable or empty available projections fail closed", () => {
  assert.throws(
    () => deliveryCandidates(inbox({ status: "unavailable", items: [candidate()] })),
    /must contain no candidates/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ status: "available", items: [] })),
    /available delivery projection is invalid/,
  );
});

test("cross-paper, cross-binding, malformed, and duplicate candidates fail closed", () => {
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ paper_id: WORK_ID })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ binding_id: WORK_ID })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ payload_hash: "sha256:no" })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ expected_work_version: 0 })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ lease_fencing_token: 0 })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate({ delivery_state: "invalidated" })] })),
    /candidate is invalid/,
  );
  assert.throws(
    () => proposalInput(candidate({ binding_id: undefined })),
    /candidate is invalid/,
  );
  assert.throws(
    () => deliveryCandidates(inbox({ items: [candidate(), candidate()] })),
    /duplicate binding/,
  );
});

test("work rejects non-authoritative inbox schemas", () => {
  assert.throws(
    () => deliveryCandidates({ schema: "hepta.paper_raid.agent_bridge.inbox.v1" }),
    /schema is unsupported/,
  );
});

test("one-shot and watch can share the exact work result wrapper", () => {
  const selected = candidate();
  assert.deepEqual(workResult("idle"), {
    schema: "hepta.paper_raid.agent_bridge.work_result.v1",
    status: "idle",
    candidate_count: 0,
  });
  assert.deepEqual(workResult("awaiting_local_selection", { candidateCount: 2 }), {
    schema: "hepta.paper_raid.agent_bridge.work_result.v1",
    status: "awaiting_local_selection",
    candidate_count: 2,
  });
  assert.deepEqual(workResult("submitted", {
    candidateCount: 1,
    candidate: selected,
    result: { schema: "hepta.paper_raid.agent_bridge.proposal_result.v2" },
  }), {
    schema: "hepta.paper_raid.agent_bridge.work_result.v1",
    status: "submitted",
    candidate_count: 1,
    candidate_key: `${DRAFT_ID}:${PAPER_ID}:${WORK_ID}:methods:${REVISION_ID}:${MANIFEST_ID}`,
    result: { schema: "hepta.paper_raid.agent_bridge.proposal_result.v2" },
  });
});

test("Author work starts only from one exact planned or in-progress task and bundle", () => {
  for (const status of ["planned", "in_progress"]) {
    const [start] = authorWorkStarts(authorStartInbox({ status }));
    assert.equal(start.schema, AUTHOR_WORK_START_SCHEMA);
    assert.equal(start.binding_id, BINDING_ID);
    assert.equal(start.player_id, PLAYER_ID);
    assert.equal(start.paper_id, PAPER_ID);
    assert.equal(start.work_item_id, WORK_ID);
    assert.equal(start.work_item_version, 3);
    assert.equal(start.task_kind, "evidence_analysis");
    assert.equal(
      authorWorkStartKey(start),
      `${PAPER_ID}:${WORK_ID}:3:${start.bundle.bundle_hash}`,
    );
    assert.deepEqual(
      authorWorkStarts(authorStartInbox({ status }), new Set([authorWorkStartKey(start)])),
      [],
    );
  }
  for (const status of ["review", "accepted", "rejected", "cancelled"]) {
    assert.deepEqual(authorWorkStarts(authorStartInbox({ status })), []);
  }
  const missingKind = authorStartInbox();
  delete missingKind.papers[0].tasks[0].kind;
  assert.throws(
    () => authorWorkStarts(missingKind),
    /requires one exact task kind/,
  );
});

test("delivery recovery globally wins and a same-version proposal hides a new Author start", () => {
  const mixed = authorStartInbox();
  mixed.papers.unshift({
    paper_id: REVISION_ID,
    phase: "drafting",
    tasks: [],
    proposals: [],
    delivery_candidates: {
      schema: "hepta.paper_raid.agent_bridge.delivery_candidates.v1",
      status: "available",
      reason_code: null,
      items: [candidate({ paper_id: REVISION_ID, work_item_id: CHALLENGE_ID })],
    },
  });
  assert.deepEqual(authorWorkStarts(mixed), []);

  const submitted = authorStartInbox();
  submitted.papers[0].proposals.push({
    work_item_id: WORK_ID,
    expected_work_version: 3,
  });
  assert.deepEqual(authorWorkStarts(submitted), []);
  submitted.papers[0].proposals[0].expected_work_version = 2;
  assert.equal(authorWorkStarts(submitted).length, 1);
});

test("fresh inbox must project the one exact candidate produced from an Author work start", () => {
  const [start] = authorWorkStarts(authorStartInbox());
  const output = {
    paper_id: PAPER_ID,
    work_item_id: WORK_ID,
    section_key: "methods",
    artifact_manifest_id: MANIFEST_ID,
    payload_hash: PAYLOAD_HASH,
  };
  const prepared = {
    schema: "hepta.paper_raid.agent_bridge.delivery_draft_result.v1",
    candidate: candidate(),
  };
  assert.deepEqual(
    deliveryCandidateForAuthorOutput(challengeInbox(), start, output, prepared),
    candidate(),
  );
  const crossed = challengeInbox();
  crossed.papers[0].delivery_candidates.items[0] = candidate({
    payload_hash: `sha256:${"f".repeat(64)}`,
  });
  assert.throws(
    () => deliveryCandidateForAuthorOutput(crossed, start, output, prepared),
    /fresh inbox delivery candidate differs/,
  );
});

test("frozen challenge materials are projected only for the exact Author work item", () => {
  const [bundle] = challengeMaterialBundles(challengeInbox());
  assert.equal(bundle.paper_project_id, PAPER_ID);
  assert.equal(bundle.binding_id, BINDING_ID);
  assert.equal(bundle.player_id, PLAYER_ID);
  assert.equal(bundle.work_item_id, WORK_ID);
  assert.equal(bundle.work_item_version, 3);
  assert.equal(
    challengeMaterialObjectQuery(bundle, bundle.objects[0]),
    `bundle_hash=${encodeURIComponent(bundle.bundle_hash)}` +
      `&digest=${encodeURIComponent(bundle.objects[0].digest)}` +
      "&object_key=brief" +
      `&paper_id=${PAPER_ID}` +
      `&work_item_id=${WORK_ID}`,
  );
});

test("challenge material projection rejects cross-assignment and ignores terminal history", () => {
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(
      materialBundle({ binding_id: MANIFEST_ID }),
    )),
    /crosses its Author assignment/,
  );
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(
      materialBundle({ player_id: MANIFEST_ID }),
    )),
    /crosses its Author assignment/,
  );
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(
      materialBundle({ work_item_id: MANIFEST_ID }),
    )),
    /no matching inbox work item/,
  );
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(
      materialBundle({ work_item_version: 4 }),
    )),
    /crosses its Author assignment/,
  );
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(
      materialBundle(),
      { paper_project_id: MANIFEST_ID },
    )),
    /crosses its Author assignment/,
  );
  for (const status of ["accepted", "rejected", "cancelled"]) {
    assert.deepEqual(
      challengeMaterialBundles(challengeInbox(materialBundle(), { status })),
      [],
    );
  }
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(
      materialBundle({ binding_id: CHALLENGE_ID.toUpperCase() }),
    )),
    /bundle is invalid/,
  );
});

test("one exact active bundle is selected while terminal history cannot be reused", () => {
  const active = materialBundle();
  const terminal = materialBundle({
    work_item_id: MANIFEST_ID,
    work_item_version: 4,
  });
  const value = challengeInbox(active);
  value.papers[0].tasks.push({
    work_item_id: MANIFEST_ID,
    assigned_binding_id: BINDING_ID,
    assigned_player_id: PLAYER_ID,
    status: "accepted",
    version: 4,
  });
  value.papers[0].challenge_materials.items.push(terminal);
  assert.deepEqual(challengeMaterialBundles(value).map(item => item.work_item_id), [WORK_ID]);
  assert.equal(
    challengeMaterialBundles(value)[0].bundle_hash,
    active.bundle_hash,
  );

  const noBundle = challengeInbox();
  noBundle.papers[0].challenge_materials = {
    schema: "hepta.paper_raid.agent_bridge.assigned_challenge_materials.v1",
    status: "unavailable",
    reason_code: "frozen_challenge_material_authority_unavailable",
    items: [],
  };
  assert.deepEqual(challengeMaterialBundles(noBundle), []);

  const duplicate = challengeInbox();
  duplicate.papers[0].challenge_materials.items.push(materialBundle());
  assert.throws(
    () => challengeMaterialBundles(duplicate),
    /duplicates a work-item assignment/,
  );
});

test("challenge material authority and bundle hashes reject tampering", () => {
  const authorityTamper = materialBundle();
  authorityTamper.authority.pack_id = "substituted-pack";
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(authorityTamper)),
    /authority hash mismatch/,
  );

  const bundleTamper = materialBundle();
  bundleTamper.objects[0].digest = `sha256:${"f".repeat(64)}`;
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(bundleTamper)),
    /bundle hash mismatch/,
  );
});

test("legacy golden qualification accepts only its exact non-activation authority", () => {
  const exact = legacyGoldenBundle();
  const [bundle] = challengeMaterialBundles(challengeInbox(exact));
  assert.equal(
    bundle.authority.schema,
    "hepta.paper_raid.legacy_golden_qualification_material_authority.v1",
  );
  assert.equal(Object.hasOwn(bundle.authority, "activation_id"), false);

  for (const extra of [
    { activation_id: ACTIVATION_ID },
    { unknown_authority: true },
  ]) {
    const mixed = legacyGoldenAuthority(extra);
    assert.throws(
      () => challengeMaterialBundles(challengeInbox(legacyGoldenBundle(mixed))),
      /legacy golden qualification material authority is invalid/,
    );
  }
  for (const extra of [
    { qualification_id: "cross-variant" },
    { unknown_authority: true },
  ]) {
    const mixed = materialAuthority(extra);
    assert.throws(
      () => challengeMaterialBundles(challengeInbox(materialBundle({ authority: mixed }))),
      /frozen challenge material authority is invalid/,
    );
  }

  for (const [field, value] of [
    ["qualification_id", "other-qualification"],
    ["challenge_title", "Substituted title"],
    ["challenge_description", "Substituted description"],
    ["challenge_status", "closed"],
    ["ruleset_version", "paper-raid-other-v1"],
    ["ruleset_hash", `sha256:${"a".repeat(64)}`],
    ["ruleset_absent", false],
    ["dataset_manifest_hash", `sha256:${"b".repeat(64)}`],
    ["evaluator_manifest_hash", `sha256:${"c".repeat(64)}`],
  ]) {
    const mutantAuthority = legacyGoldenAuthority({ [field]: value });
    assert.throws(
      () => challengeMaterialBundles(challengeInbox(legacyGoldenBundle(mutantAuthority))),
      /legacy golden qualification material authority is invalid/,
    );
  }

  const objectReplacement = legacyGoldenAuthority();
  objectReplacement.objects[0].digest = `sha256:${"d".repeat(64)}`;
  objectReplacement.authority_hash = withCanonicalHash(
    objectReplacement,
    "authority_hash",
  ).authority_hash;
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(legacyGoldenBundle(objectReplacement))),
    /material objects are not exact/,
  );

  const crossedBundle = legacyGoldenBundle();
  crossedBundle.objects[0].digest = `sha256:${"e".repeat(64)}`;
  crossedBundle.bundle_hash = withCanonicalHash(crossedBundle, "bundle_hash").bundle_hash;
  assert.throws(
    () => challengeMaterialBundles(challengeInbox(crossedBundle)),
    /bundle differs from frozen authority/,
  );
});

test("challenge material downloader verifies every frozen byte sequence", async () => {
  const [bundle] = challengeMaterialBundles(challengeInbox());
  const downloaded = await downloadAssignedChallengeMaterials(
    bundle,
    async (_exactBundle, object) => materialBytes[object.object_key],
  );
  assert.equal(downloaded.length, 4);
  assert.deepEqual(downloaded.map(value => value.descriptor.object_key), [
    "brief",
    "dataset",
    "baseline",
    "evaluator",
  ]);

  await assert.rejects(
    () => downloadAssignedChallengeMaterials(
      bundle,
      async (_exactBundle, object) => object.object_key === "brief"
        ? Buffer.from("wrong", "utf8")
        : materialBytes[object.object_key],
    ),
    /differs from frozen authority/,
  );
});

test("challenge materials publish owner-only and never clobber an existing final", async t => {
  const parent = await mkdtemp(join(tmpdir(), "paper-raid-materials-"));
  await chmod(parent, 0o700);
  t.after(() => rm(parent, { recursive: true, force: true }));
  const [bundle] = challengeMaterialBundles(challengeInbox());
  const downloaded = bundle.objects.map(descriptor => Object.freeze({
    descriptor,
    bytes: materialBytes[descriptor.object_key],
  }));
  const root = join(parent, "materials");
  const created = await materializeAssignedChallengeMaterials(bundle, downloaded, root);
  assert.equal(created.status, "created");
  assert.equal((await lstat(created.directory)).mode & 0o777, 0o700);
  assert.deepEqual(await readdir(created.directory), ["challenge"]);
  for (const object of bundle.objects) {
    const stats = await lstat(join(created.directory, object.logical_path));
    assert.equal(stats.mode & 0o777, 0o400);
    assert.equal(stats.nlink, 1);
  }
  const reused = await materializeAssignedChallengeMaterials(bundle, downloaded, root);
  assert.equal(reused.status, "reused");
  assert.equal(reused.directory, created.directory);

  const concurrentRoot = join(parent, "concurrent-materials");
  const concurrent = await Promise.all([
    materializeAssignedChallengeMaterials(bundle, downloaded, concurrentRoot),
    materializeAssignedChallengeMaterials(bundle, downloaded, concurrentRoot),
  ]);
  assert.deepEqual(
    concurrent.map(value => value.status).sort(),
    ["created", "reused"],
  );
  assert.deepEqual(
    (await readdir(concurrentRoot)).filter(name => name.startsWith(".")),
    [],
  );

  const hostileRoot = join(parent, "hostile-materials");
  await mkdir(hostileRoot, { mode: 0o700 });
  const hostileFinal = join(hostileRoot, basename(created.directory));
  await mkdir(hostileFinal, { mode: 0o700 });
  await assert.rejects(
    () => materializeAssignedChallengeMaterials(bundle, downloaded, hostileRoot),
    /unexpected material entry/,
  );
  assert.deepEqual(await readdir(hostileFinal), []);

  const symlinkRoot = join(parent, "symlink-materials");
  await mkdir(symlinkRoot, { mode: 0o700 });
  await symlink(created.directory, join(symlinkRoot, basename(created.directory)));
  await assert.rejects(
    () => materializeAssignedChallengeMaterials(bundle, downloaded, symlinkRoot),
    /non-symlink directory/,
  );

  const realAncestor = join(parent, "real-ancestor");
  await mkdir(realAncestor, { mode: 0o700 });
  await mkdir(join(realAncestor, "nested"), { mode: 0o700 });
  const linkedAncestor = join(parent, "linked-ancestor");
  await symlink(realAncestor, linkedAncestor);
  await assert.rejects(
    () => materializeAssignedChallengeMaterials(
      bundle,
      downloaded,
      join(linkedAncestor, "nested", "materials"),
    ),
    /non-symlink directory|must not traverse a symbolic link/,
  );
});

test("material publication pins a safe root-owned /usr/bin/mv", async t => {
  assert.equal(await validateMaterialPublisherExecutable(), "/usr/bin/mv");
  const parent = await mkdtemp(join(tmpdir(), "paper-raid-material-mv-"));
  await chmod(parent, 0o700);
  t.after(() => rm(parent, { recursive: true, force: true }));
  await assert.rejects(
    validateMaterialPublisherExecutable(join(parent, "missing-mv")),
    /publisher is unavailable/,
  );
  const linked = join(parent, "linked-mv");
  await symlink("/usr/bin/mv", linked);
  await assert.rejects(
    validateMaterialPublisherExecutable(linked),
    /publisher is unsafe/,
  );
  const replacement = join(parent, "replacement-mv");
  await copyFile("/usr/bin/mv", replacement);
  await chmod(replacement, 0o775);
  await assert.rejects(
    validateMaterialPublisherExecutable(replacement),
    /publisher is unsafe/,
  );
});

test("an incomplete download never creates a consumable material directory", async t => {
  const parent = await mkdtemp(join(tmpdir(), "paper-raid-material-partial-"));
  await chmod(parent, 0o700);
  t.after(() => rm(parent, { recursive: true, force: true }));
  const [bundle] = challengeMaterialBundles(challengeInbox());
  const partial = bundle.objects.slice(0, 2).map(descriptor => ({
    descriptor,
    bytes: materialBytes[descriptor.object_key],
  }));
  const root = join(parent, "materials");
  await assert.rejects(
    () => materializeAssignedChallengeMaterials(bundle, partial, root),
    /download set is incomplete/,
  );
  await assert.rejects(lstat(root), error => error?.code === "ENOENT");
});
