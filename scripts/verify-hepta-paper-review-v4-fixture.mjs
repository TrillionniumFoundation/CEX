#!/usr/bin/env node

import crypto from 'node:crypto';
import fs from 'node:fs';

const fixturePath = process.argv[2] ?? 'docs/sdk-fixtures/hepta-paper-review-v4.json';
const fixture = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));
const fail = (message) => { throw new Error(message); };

const text = (name, value) => {
  if (typeof value !== 'string' || value.length === 0 || value.includes('\0') || Buffer.byteLength(value) > 512) {
    fail(`${name} is not canonical text`);
  }
  return value;
};
const base64 = (name, value, length) => {
  text(name, value);
  const bytes = Buffer.from(value, 'base64');
  if (bytes.toString('base64') !== value || bytes.length !== length) fail(`${name} is not canonical base64(${length})`);
  return bytes;
};
const digestBytes = (value) => {
  if (!/^sha256:[0-9a-f]{64}$/.test(value)) fail(`invalid digest ${value}`);
  return Buffer.from(value.slice(7), 'hex');
};
const digest = (bytes) => `sha256:${crypto.createHash('sha256').update(bytes).digest('hex')}`;

class Frame {
  constructor(domain) { this.parts = [Buffer.from(`${domain}\0`, 'utf8')]; }
  bytes(value) {
    const bytes = Buffer.from(value);
    const size = Buffer.alloc(4);
    size.writeUInt32BE(bytes.length);
    this.parts.push(size, bytes);
    return this;
  }
  string(value) { return this.bytes(Buffer.from(value, 'utf8')); }
  digest(value) { this.parts.push(digestBytes(value)); return this; }
  u32(value) { const bytes = Buffer.alloc(4); bytes.writeUInt32BE(value); this.parts.push(bytes); return this; }
  i64(value) { const bytes = Buffer.alloc(8); bytes.writeBigInt64BE(BigInt(value)); this.parts.push(bytes); return this; }
  finish() { return Buffer.concat(this.parts); }
}

const optionalUuid = (frame, value) => value === null
  ? frame.u32(0)
  : frame.u32(1).string(value);
const publicKey = (raw) => crypto.createPublicKey({
  key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw]),
  format: 'der',
  type: 'spki',
});
const verify = (frame, key, signature) => crypto.verify(
  null,
  frame,
  publicKey(base64('public_key', key.public_key_base64, 32)),
  base64('signature', signature, 64),
);

function evaluationFrame(value) {
  if (value.schema !== 'hepta.paper_raid.evaluation.v1' || value.signed_at_unix < 0) fail('invalid paper evaluation');
  let frame = new Frame('hepta_paper_raid_evaluation_v1')
    .string(value.schema).string(value.evaluation_id).string(value.paper_project_id)
    .string(value.submission_id).digest(value.release_candidate_hash)
    .digest(value.paper_bundle_hash);
  frame = optionalUuid(frame, value.supersedes_evaluation_id);
  return frame.digest(value.tolerance_policy_hash).digest(value.paper_score_hash)
    .digest(value.reference_metrics_hash).digest(value.hard_gates_hash)
    .string(value.evaluator_player_id).string(text('signing_key_id', value.signing_key_id))
    .digest(value.signing_public_key_hash).digest(value.coi_attestation_hash)
    .i64(value.signed_at_unix).finish();
}

function reviewAttestationFrame(value) {
  if (value.schema !== 'hepta.paper_raid.review_attestation.v1'
      || !['approve', 'reject'].includes(value.verdict) || value.signed_at_unix < 0) {
    fail('invalid paper review attestation');
  }
  return new Frame('hepta_paper_raid_review_attestation_v1')
    .string(value.schema).string(value.attestation_id).string(value.evaluation_id)
    .digest(value.evaluation_signing_hash).string(value.reviewer_player_id)
    .string(value.verdict).string(text('signing_key_id', value.signing_key_id))
    .digest(value.signing_public_key_hash).digest(value.coi_attestation_hash)
    .i64(value.signed_at_unix).finish();
}

function reproductionFrame(value) {
  if (value.schema !== 'hepta.paper_raid.reproduction.v1' || value.signed_at_unix < 0) fail('invalid paper reproduction');
  let frame = new Frame('hepta_paper_raid_reproduction_v1')
    .string(value.schema).string(value.reproduction_id).string(value.evaluation_id)
    .string(value.paper_project_id).digest(value.release_candidate_hash)
    .digest(value.paper_bundle_hash).digest(value.tolerance_policy_hash)
    .digest(value.observed_metrics_hash).digest(value.statistical_evidence_hash)
    .digest(value.seed_set_hash).digest(value.environment_hash).digest(value.run_manifest_hash);
  frame = optionalUuid(frame, value.supersedes_reproduction_id);
  return frame.string(value.reproducer_player_id)
    .string(text('signing_key_id', value.signing_key_id))
    .digest(value.signing_public_key_hash).digest(value.coi_attestation_hash)
    .i64(value.signed_at_unix).finish();
}

function appealFrame(value) {
  if (value.schema !== 'hepta.paper_raid.appeal.v1' || value.signed_at_unix < 0) fail('invalid paper appeal');
  return new Frame('hepta_paper_raid_appeal_v1')
    .string(value.schema).string(value.appeal_id).string(value.evaluation_id)
    .string(value.paper_project_id).digest(value.release_candidate_hash)
    .string(value.appellant_player_id).digest(value.grounds_hash)
    .digest(value.evidence_manifest_hash).string(text('signing_key_id', value.signing_key_id))
    .digest(value.signing_public_key_hash).i64(value.signed_at_unix).finish();
}

function appealResolutionFrame(value) {
  if (value.schema !== 'hepta.paper_raid.appeal_resolution.v1'
      || !['upheld', 'denied'].includes(value.outcome) || value.signed_at_unix < 0) {
    fail('invalid paper appeal resolution');
  }
  let frame = new Frame('hepta_paper_raid_appeal_resolution_v1')
    .string(value.schema).string(value.resolution_id).string(value.appeal_id)
    .string(value.evaluation_id).string(value.paper_project_id)
    .digest(value.release_candidate_hash).string(value.outcome);
  frame = optionalUuid(frame, value.superseding_evaluation_id);
  return frame.digest(value.decision_hash).string(value.resolver_player_id)
    .string(text('signing_key_id', value.signing_key_id))
    .digest(value.signing_public_key_hash).i64(value.signed_at_unix).finish();
}

if (fixture.schema !== 'hepta.paper_raid.review.golden_vectors.v4'
    || fixture.protocol !== 'hepta.paper_raid.review.v4'
    || fixture.negative_cases.length !== 5) fail('invalid review fixture header');

for (const key of Object.values(fixture.keys)) {
  if (digest(base64('public_key', key.public_key_base64, 32)) !== key.public_key_hash) fail('public key hash mismatch');
}

const vectors = [
  [fixture.evaluation, fixture.keys.evaluator, evaluationFrame],
  [fixture.review_attestation, fixture.keys.reviewer, reviewAttestationFrame],
  [fixture.reproduction, fixture.keys.reproducer, reproductionFrame],
  [fixture.appeal, fixture.keys.author, appealFrame],
  [fixture.appeal_resolution, fixture.keys.resolver, appealResolutionFrame],
];
for (const [vector, key, buildFrame] of vectors) {
  const frame = buildFrame(vector.signing);
  if (frame.toString('hex') !== vector.signing_frame_hex) fail('signing frame mismatch');
  if (!verify(frame, key, vector.signature)) fail('signature verification failed');
}

const tampered = [
  [{...fixture.evaluation.signing, paper_score_hash: digest(Buffer.from('tampered'))}, fixture.evaluation, fixture.keys.evaluator, evaluationFrame],
  [{...fixture.review_attestation.signing, verdict: 'reject'}, fixture.review_attestation, fixture.keys.reviewer, reviewAttestationFrame],
  [{...fixture.reproduction.signing, seed_set_hash: digest(Buffer.from('tampered'))}, fixture.reproduction, fixture.keys.reproducer, reproductionFrame],
  [{...fixture.appeal.signing, evidence_manifest_hash: digest(Buffer.from('tampered'))}, fixture.appeal, fixture.keys.author, appealFrame],
  [{...fixture.appeal_resolution.signing, outcome: 'upheld'}, fixture.appeal_resolution, fixture.keys.resolver, appealResolutionFrame],
];
for (const [value, vector, key, buildFrame] of tampered) {
  if (verify(buildFrame(value), key, vector.signature)) fail('tampered vector unexpectedly verified');
}

console.log(`verified ${fixture.schema}`);
