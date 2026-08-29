#!/usr/bin/env node

import crypto from 'node:crypto';
import fs from 'node:fs';

const fixturePath = process.argv[2] ?? 'docs/sdk-fixtures/hepta-paper-collaboration-v3.json';
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
  u64(value) { const bytes = Buffer.alloc(8); bytes.writeBigUInt64BE(BigInt(value)); this.parts.push(bytes); return this; }
  i64(value) { const bytes = Buffer.alloc(8); bytes.writeBigInt64BE(BigInt(value)); this.parts.push(bytes); return this; }
  finish() { return Buffer.concat(this.parts); }
}

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

function agentProposalFrame(value) {
  if (value.schema !== 'hepta.paper_raid.agent_proposal.v1' || !['proposal', 'delivery'].includes(value.proposal_kind)) fail('invalid Agent proposal');
  return new Frame('hepta_paper_raid_agent_proposal_v1')
    .string(value.schema).string(value.proposal_id).string(value.paper_project_id)
    .string(value.work_item_id).string(text('section_key', value.section_key))
    .string(value.parent_revision_id).string(value.proposal_kind).digest(value.payload_hash)
    .digest(value.artifact_manifest_hash).string(text('agent_id', value.agent_id))
    .string(value.binding_id).string(text('agent_key_id', value.agent_key_id))
    .i64(value.signed_at_unix).finish();
}

function humanDecisionFrame(value) {
  if (value.schema !== 'hepta.paper_raid.human_decision.v1' || !['accept', 'rework', 'reject'].includes(value.decision)) fail('invalid human decision');
  return new Frame('hepta_paper_raid_human_decision_v1')
    .string(value.schema).string(value.decision_id).string(value.paper_project_id)
    .string(value.proposal_id).string(value.player_id).string(value.decision)
    .digest(value.reason_hash).u64(value.expected_proposal_version)
    .string(text('signing_key_id', value.signing_key_id)).digest(value.signing_public_key_hash)
    .i64(value.signed_at_unix).finish();
}

function humanEvidenceFrame(value) {
  if (value.schema !== 'hepta.paper_raid.human_evidence_verification.v1' || !['evidence_card', 'citation'].includes(value.record_kind)) fail('invalid evidence verification');
  return new Frame('hepta_paper_raid_human_evidence_verification_v1')
    .string(value.schema).string(value.verification_id).string(value.paper_project_id)
    .string(value.record_kind).string(value.record_id)
    .string(text('source_identifier', value.source_identifier)).digest(value.source_hash)
    .string(text('locator', value.locator)).string(text('license', value.license))
    .string(value.player_id).string(text('signing_key_id', value.signing_key_id))
    .digest(value.signing_public_key_hash).i64(value.signed_at_unix).finish();
}

function sectionReviewFrame(value) {
  if (value.schema !== 'hepta.paper_raid.section_review.v1' || !['approve', 'rework', 'reject'].includes(value.verdict)) fail('invalid section review');
  return new Frame('hepta_paper_raid_section_review_v1')
    .string(value.schema).string(value.review_id).string(value.paper_project_id)
    .string(value.section_revision_id).string(value.reviewer_player_id).string(value.verdict)
    .digest(value.review_hash).u64(value.expected_revision_version)
    .string(text('signing_key_id', value.signing_key_id)).digest(value.signing_public_key_hash)
    .i64(value.signed_at_unix).finish();
}

function sectionMergeFrame(value) {
  if (value.schema !== 'hepta.paper_raid.section_merge.v1' || value.fencing_token < 1) fail('invalid section merge');
  return new Frame('hepta_paper_raid_section_merge_v1')
    .string(value.schema).string(value.merge_id).string(value.paper_project_id)
    .string(text('section_key', value.section_key)).string(value.section_revision_id)
    .string(value.parent_revision_id).string(value.merged_section_revision_id)
    .string(value.lease_id).u64(value.fencing_token).string(value.merged_by_player_id)
    .string(text('signing_key_id', value.signing_key_id)).digest(value.signing_public_key_hash)
    .i64(value.merged_at_unix).finish();
}

if (fixture.schema !== 'hepta.paper_raid.collaboration.golden_vectors.v3'
    || fixture.protocol !== 'hepta.paper_raid.collaboration.v3'
    || fixture.negative_cases.length !== 5) fail('invalid collaboration fixture header');

for (const key of Object.values(fixture.keys)) {
  if (digest(base64('public_key', key.public_key_base64, 32)) !== key.public_key_hash) fail('public key hash mismatch');
}

const vectors = [
  [fixture.agent_proposal, fixture.keys.agent, agentProposalFrame],
  [fixture.human_decision, fixture.keys.human, humanDecisionFrame],
  [fixture.human_evidence_verification, fixture.keys.human, humanEvidenceFrame],
  [fixture.section_review, fixture.keys.reviewer, sectionReviewFrame],
  [fixture.section_merge, fixture.keys.human, sectionMergeFrame],
];
for (const [vector, key, buildFrame] of vectors) {
  const frame = buildFrame(vector.signing);
  if (frame.toString('hex') !== vector.signing_frame_hex) fail('signing frame mismatch');
  if (!verify(frame, key, vector.signature)) fail('signature verification failed');
}

const tampered = [
  [{...fixture.agent_proposal.signing, payload_hash: digest(Buffer.from('tampered'))}, fixture.agent_proposal, fixture.keys.agent, agentProposalFrame],
  [{...fixture.human_decision.signing, reason_hash: digest(Buffer.from('tampered'))}, fixture.human_decision, fixture.keys.human, humanDecisionFrame],
  [{...fixture.human_evidence_verification.signing, source_identifier: `${fixture.human_evidence_verification.signing.source_identifier}?tampered`}, fixture.human_evidence_verification, fixture.keys.human, humanEvidenceFrame],
  [{...fixture.section_review.signing, verdict: 'reject'}, fixture.section_review, fixture.keys.reviewer, sectionReviewFrame],
  [{...fixture.section_merge.signing, fencing_token: fixture.section_merge.signing.fencing_token + 1}, fixture.section_merge, fixture.keys.human, sectionMergeFrame],
];
for (const [value, vector, key, buildFrame] of tampered) {
  if (verify(buildFrame(value), key, vector.signature)) fail('tampered vector unexpectedly verified');
}

console.log(`verified ${fixture.schema}`);
