#!/usr/bin/env node

import crypto from 'node:crypto';
import fs from 'node:fs';

const fixturePath = process.argv[2] ?? 'docs/sdk-fixtures/hepta-paper-raid-v2.json';
const fixture = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));

const fail = (message) => { throw new Error(message); };
const text = (name, value) => {
  if (typeof value !== 'string' || value.length === 0 || value.includes('\0') || [...value].length > 512) {
    fail(`${name} is not canonical text`);
  }
  return value;
};
const b64 = (name, value, length) => {
  text(name, value);
  const bytes = Buffer.from(value, 'base64');
  if (bytes.toString('base64') !== value || (length !== undefined && bytes.length !== length)) {
    fail(`${name} is not canonical padded base64${length ? `(${length})` : ''}`);
  }
  return bytes;
};
const digestBytes = (value) => {
  if (!/^sha256:[0-9a-f]{64}$/.test(value)) fail(`invalid digest ${value}`);
  return Buffer.from(value.slice(7), 'hex');
};
const digest = (bytes) => `sha256:${crypto.createHash('sha256').update(bytes).digest('hex')}`;
const canonicalJson = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
};
const canonicalJsonDigest = (value) => digest(Buffer.from(canonicalJson(value), 'utf8'));

class Frame {
  constructor(domain) { this.parts = [Buffer.from(`${domain}\0`, 'utf8')]; }
  bytes(value) {
    const bytes = Buffer.from(value);
    if (bytes.length > 0xffffffff) fail('field exceeds uint32');
    const size = Buffer.alloc(4); size.writeUInt32BE(bytes.length);
    this.parts.push(size, bytes); return this;
  }
  string(value) { return this.bytes(Buffer.from(value, 'utf8')); }
  u32(value) { const bytes = Buffer.alloc(4); bytes.writeUInt32BE(value); this.parts.push(bytes); return this; }
  u64(value) { const bytes = Buffer.alloc(8); bytes.writeBigUInt64BE(BigInt(value)); this.parts.push(bytes); return this; }
  i64(value) { const bytes = Buffer.alloc(8); bytes.writeBigInt64BE(BigInt(value)); this.parts.push(bytes); return this; }
  digest(value) { this.parts.push(digestBytes(value)); return this; }
  finish() { return Buffer.concat(this.parts); }
}

const publicKey = (raw) => crypto.createPublicKey({
  key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw]),
  format: 'der',
  type: 'spki',
});
const verify = (message, publicKeyBase64, signatureBase64) => crypto.verify(
  null,
  message,
  publicKey(b64('public_key', publicKeyBase64, 32)),
  b64('signature', signatureBase64, 64),
);

function assertionFrame(value) {
  const c = value.claim;
  if (c.schema !== 'hepta.consumer-edge.user-assertion.v2' || c.nonce !== c.idempotency_key) fail('invalid assertion contract');
  return new Frame('hepta_consumer_edge_user_assertion_v2')
    .string(c.schema).string(c.assertion_id).string(text('issuer', c.issuer))
    .string(text('audience', c.audience)).string(text('subject_id', c.subject_id))
    .string(c.nakama_user_id).string(c.player_id).string(text('operation', c.operation))
    .string(text('method', c.http_method)).string(text('path', c.canonical_path))
    .string(text('idempotency_key', c.idempotency_key)).digest(c.body_hash)
    .i64(c.issued_at_unix).i64(c.expires_at_unix).string(text('nonce', c.nonce))
    .string(text('issuer_key_id', value.issuer_key_id)).finish();
}

function releaseFrame(candidate) {
  if (candidate.schema !== 'hepta.paper_raid.release_candidate.v2') fail('invalid release schema');
  const authors = structuredClone(candidate.authors).sort((a, b) => a.author_order - b.author_order);
  if (authors.length < 3 || authors.length > 5) fail('invalid author count');
  const frame = new Frame('hepta_paper_raid_release_candidate_v2')
    .string(candidate.schema).string(candidate.paper_project_id).string(candidate.revision_id)
    .string(candidate.team_id).string(candidate.challenge_id).digest(candidate.ruleset_hash)
    .digest(candidate.challenge_snapshot_hash).u64(candidate.roster_version)
    .string(text('title', candidate.title)).string(text('abstract', candidate.abstract_text))
    .string(text('target_format', candidate.target_format)).digest(candidate.source_manifest_hash)
    .digest(candidate.artifact_manifest_hash).digest(candidate.bibliography_hash)
    .digest(candidate.claim_evidence_graph_hash).digest(candidate.collaboration_compact_hash)
    .digest(candidate.research_protocol_snapshot_hash).digest(candidate.ethics_disclosure_hash)
    .digest(candidate.coi_disclosure_hash).digest(candidate.contribution_ledger_hash)
    .digest(candidate.ai_disclosure_hash).string(text('license', candidate.license)).u32(authors.length);
  const seenSlots = new Set();
  for (let index = 0; index < authors.length; index += 1) {
    const author = authors[index];
    if (author.author_order !== index + 1 || author.participant_slot < 1 || author.participant_slot > 5 || seenSlots.has(author.participant_slot)) fail('invalid author order/slot');
    seenSlots.add(author.participant_slot);
    const roles = [...author.credit_roles].sort();
    if (!roles.length || new Set(roles).size !== roles.length) fail('invalid CRediT roles');
    frame.u32(author.author_order).u32(author.participant_slot).string(author.player_id)
      .string(text('display_name', author.display_name)).u32(roles.length);
    for (const role of roles) frame.string(text('credit_role', role));
  }
  return frame.finish();
}

function consentFrame(consent) {
  if (consent.schema !== 'hepta.paper_raid.authorship_consent.v2') fail('invalid consent schema');
  const publicBytes = b64('signing_public_key', consent.signing_public_key, 32);
  if (digest(publicBytes) !== consent.signing_public_key_hash) fail('consent key hash mismatch');
  return new Frame('hepta_paper_raid_authorship_consent_v2')
    .string(consent.schema).string(consent.consent_id).string(consent.paper_project_id)
    .string(consent.revision_id).string(consent.player_id).string(text('key_id', consent.signing_key_id))
    .bytes(publicBytes).digest(consent.signing_public_key_hash).digest(consent.release_candidate_hash)
    .i64(consent.signed_at_unix).finish();
}

function bundleFrame(bundle) {
  if (bundle.schema !== 'hepta.paper_raid.paper_bundle.v2') fail('invalid bundle schema');
  const release = releaseFrame(bundle.release_candidate);
  if (digest(release) !== bundle.release_candidate_hash) fail('bundle release hash mismatch');
  const authors = structuredClone(bundle.release_candidate.authors).sort((a, b) => a.author_order - b.author_order);
  const consents = structuredClone(bundle.author_consents).sort((a, b) => a.author_order - b.author_order);
  if (authors.length !== consents.length || consents.length < 3 || consents.length > 5) fail('bundle consent count mismatch');
  const frame = new Frame('hepta_paper_raid_paper_bundle_v2').string(bundle.schema)
    .bytes(release).digest(bundle.release_candidate_hash).u32(consents.length);
  for (let index = 0; index < consents.length; index += 1) {
    const author = authors[index]; const consent = consents[index];
    if (consent.author_order !== author.author_order || consent.participant_slot !== author.participant_slot || consent.player_id !== author.player_id) fail('consent author mismatch');
    const signing = {
      schema: 'hepta.paper_raid.authorship_consent.v2', consent_id: consent.consent_id,
      paper_project_id: bundle.release_candidate.paper_project_id,
      revision_id: bundle.release_candidate.revision_id, player_id: consent.player_id,
      signing_key_id: consent.signing_key_id, signing_public_key: consent.signing_public_key,
      signing_public_key_hash: consent.signing_public_key_hash,
      release_candidate_hash: bundle.release_candidate_hash, signed_at_unix: consent.signed_at_unix,
    };
    if (!verify(consentFrame(signing), consent.signing_public_key, consent.signature)) fail('invalid consent signature');
    frame.u32(consent.author_order).u32(consent.participant_slot).string(consent.player_id)
      .string(consent.consent_id).string(consent.signing_key_id)
      .bytes(b64('signing_public_key', consent.signing_public_key, 32))
      .digest(consent.signing_public_key_hash).i64(consent.signed_at_unix)
      .bytes(b64('signature', consent.signature, 64));
  }
  return frame.finish();
}

function consumptionReceiptFrame(receipt) {
  if (receipt.schema !== 'hepta.paper_raid.authorization_set_consumption_receipt.v1'
      || receipt.session_roster_version < 1
      || receipt.authorization_ids.length < 3
      || receipt.authorization_ids.length > 5) fail('invalid consumption receipt');
  if (new Set(receipt.authorization_ids).size !== receipt.authorization_ids.length) fail('duplicate authorization IDs');
  const frame = new Frame('hepta_research_session_authorization_set_consumption_receipt_v1')
    .string(receipt.schema).string(text('session_id', receipt.session_id))
    .string(receipt.team_id).string(receipt.paper_project_id).string(receipt.challenge_id)
    .u64(receipt.session_roster_version).digest(receipt.roster_root)
    .u32(receipt.authorization_ids.length);
  for (const authorizationId of receipt.authorization_ids) frame.string(authorizationId);
  return frame.i64(receipt.consumed_at_unix).string(text('issuer_key_id', receipt.issuer_key_id)).finish();
}

function terminalFactsFrame(facts) {
  return new Frame('trnm_research_session_terminal_facts_v1')
    .string(text('result_code', facts.result_code)).digest(facts.paper_bundle_hash)
    .digest(facts.paper_release_candidate_hash).digest(facts.contribution_ledger_hash).finish();
}

function completionReceiptFrame(receipt) {
  if (receipt.schema !== 'hepta.paper_raid.nakama_completion_receipt.v1'
      || receipt.roster_version < 1 || receipt.event_count < 1) fail('invalid completion receipt');
  return new Frame('hepta_nakama_research_session_completion_receipt_v1')
    .string(receipt.schema).digest(receipt.commitment_id).string(text('session_id', receipt.session_id))
    .string(receipt.team_id).string(receipt.paper_project_id).string(receipt.challenge_id)
    .u64(receipt.roster_version).digest(receipt.roster_root).u64(receipt.event_count)
    .digest(receipt.event_root).digest(receipt.archive_hash).digest(receipt.ruleset_hash)
    .digest(receipt.challenge_snapshot_hash).string(text('nakama_authority_key_id', receipt.nakama_authority_key_id))
    .bytes(terminalFactsFrame(receipt.terminal_facts)).i64(receipt.verified_at_unix)
    .string(text('issuer_key_id', receipt.issuer_key_id)).finish();
}

function optionalDigest(frame, value) {
  if (value === null) return frame.u32(0);
  return frame.u32(1).digest(value);
}

function evidenceEnvelopeFrame(envelope) {
  if (envelope.schema !== 'hepta.paper_raid.evidence_envelope.v1'
      || envelope.session_roster_version < 1) fail('invalid evidence envelope');
  let frame = new Frame('hepta_paper_raid_evidence_envelope_v1')
    .string(envelope.schema).string(envelope.evidence_envelope_id)
    .digest(envelope.paper_bundle_hash).digest(envelope.nakama_completion_receipt_hash)
    .string(text('session_id', envelope.session_id)).u64(envelope.session_roster_version)
    .digest(envelope.roster_root).digest(envelope.event_root).digest(envelope.archive_hash)
    .digest(envelope.ruleset_hash).digest(envelope.challenge_snapshot_hash)
    .digest(envelope.evaluation_report_hash).digest(envelope.reproduction_report_hash);
  frame = optionalDigest(frame, envelope.appeal_resolution_hash);
  frame = optionalDigest(frame, envelope.finality_receipt_hash);
  return frame.i64(envelope.created_at_unix).string(text('issuer_key_id', envelope.issuer_key_id)).finish();
}

function evidenceEnvelopeHash(envelope) {
  const signing = evidenceEnvelopeFrame(envelope);
  return digest(new Frame('hepta_paper_raid_evidence_envelope_record_v1')
    .bytes(signing).bytes(b64('signature', envelope.signature, 64)).finish());
}

function verifyEvidenceAgainst(envelope, bundle, receipt, refs, issuerPublicKey) {
  if (envelope.paper_bundle_hash !== digest(bundleFrame(bundle))) fail('evidence bundle mismatch');
  if (envelope.nakama_completion_receipt_hash !== canonicalJsonDigest(receipt)) fail('evidence receipt hash mismatch');
  for (const field of ['session_id', 'roster_root', 'event_root', 'archive_hash', 'ruleset_hash', 'challenge_snapshot_hash']) {
    if (envelope[field] !== receipt[field]) fail(`evidence receipt ${field} mismatch`);
  }
  if (envelope.session_roster_version !== receipt.roster_version) fail('evidence receipt epoch mismatch');
  if (envelope.evaluation_report_hash !== refs.evaluation_report_hash
      || envelope.reproduction_report_hash !== refs.reproduction_report_hash
      || envelope.appeal_resolution_hash !== refs.appeal_resolution_hash
      || envelope.finality_receipt_hash !== refs.finality_receipt_hash) fail('evidence review/finality reference mismatch');
  if (envelope.issuer_key_id !== receipt.issuer_key_id) fail('evidence issuer mismatch');
  if (!verify(completionReceiptFrame(receipt), issuerPublicKey, receipt.signature)) fail('completion receipt signature');
  if (!verify(evidenceEnvelopeFrame(envelope), issuerPublicKey, envelope.signature)) fail('evidence signature');
}

function publicationAuthorFrame(release, consent) {
  const key = b64('publication_signing_public_key', consent.signing_public_key, 32);
  if (digest(key) !== consent.signing_public_key_hash) fail('publication key hash mismatch');
  return new Frame('hepta_paper_raid_publication_release_author_v1')
    .string(release.schema).string(release.publication_release_id).digest(release.paper_bundle_hash)
    .digest(release.evidence_envelope_hash).string(text('destination', release.destination))
    .digest(release.release_manifest_hash).string(text('license', release.license))
    .i64(release.released_at_unix).u32(consent.author_order).u32(consent.participant_slot)
    .string(consent.player_id).string(text('signing_key_id', consent.signing_key_id))
    .bytes(key).digest(consent.signing_public_key_hash).finish();
}

function publicationFrame(release) {
  if (release.schema !== 'hepta.paper_raid.publication_release.v1') fail('invalid publication schema');
  const consents = structuredClone(release.author_consents).sort((a, b) => a.author_order - b.author_order);
  if (consents.length < 3 || consents.length > 5) fail('invalid publication author count');
  const slots = new Set(); const players = new Set();
  const frame = new Frame('hepta_paper_raid_publication_release_v1')
    .string(release.schema).string(release.publication_release_id).digest(release.paper_bundle_hash)
    .digest(release.evidence_envelope_hash).string(text('destination', release.destination))
    .digest(release.release_manifest_hash).string(text('license', release.license))
    .i64(release.released_at_unix).u32(consents.length);
  for (let index = 0; index < consents.length; index += 1) {
    const consent = consents[index];
    if (consent.author_order !== index + 1 || slots.has(consent.participant_slot) || players.has(consent.player_id)) fail('invalid publication roster');
    slots.add(consent.participant_slot); players.add(consent.player_id);
    const signing = publicationAuthorFrame(release, consent);
    if (!verify(signing, consent.signing_public_key, consent.signature)) fail('publication author signature');
    frame.bytes(signing).bytes(b64('publication_signature', consent.signature, 64));
  }
  return frame.finish();
}

function verifyPublicationAgainst(release, bundle, envelope) {
  if (release.paper_bundle_hash !== digest(bundleFrame(bundle))
      || release.evidence_envelope_hash !== evidenceEnvelopeHash(envelope)) fail('publication artifact mismatch');
  if (release.license !== bundle.release_candidate.license) fail('publication license mismatch');
  const expected = structuredClone(bundle.release_candidate.authors).sort((a, b) => a.author_order - b.author_order);
  const actual = structuredClone(release.author_consents).sort((a, b) => a.author_order - b.author_order);
  if (expected.length !== actual.length || expected.some((author, index) =>
    author.author_order !== actual[index].author_order
      || author.participant_slot !== actual[index].participant_slot
      || author.player_id !== actual[index].player_id)) fail('publication author replacement');
  publicationFrame(release);
}

if (fixture.schema !== 'hepta.paper_raid.golden_vectors.v2') fail('fixture schema');
const assertion = fixture.consumer_assertion;
const assertionBytes = assertionFrame(assertion.value);
if (assertionBytes.toString('hex') !== assertion.signing_frame_hex) fail('assertion frame mismatch');
if (!verify(assertionBytes, fixture.keys.consumer_edge.public_key_base64, assertion.value.signature)) fail('assertion signature');

const releaseBytes = releaseFrame(fixture.release_candidate.value);
if (releaseBytes.toString('hex') !== fixture.release_candidate.frame_hex || digest(releaseBytes) !== fixture.release_candidate.hash) fail('release vector mismatch');
for (const vector of fixture.authorship_consents) {
  const bytes = consentFrame(vector.signing);
  if (bytes.toString('hex') !== vector.signing_frame_hex || !verify(bytes, vector.signing.signing_public_key, vector.signature)) fail('consent vector mismatch');
}
const bundleBytes = bundleFrame(fixture.paper_bundle.value);
if (bundleBytes.toString('hex') !== fixture.paper_bundle.frame_hex || digest(bundleBytes) !== fixture.paper_bundle.value.paper_bundle_hash) fail('bundle vector mismatch');

const receiptIssuerKey = fixture.keys.hepta_receipt_issuer.public_key_base64;
const consumption = fixture.authorization_consumption_receipt;
const consumptionBytes = consumptionReceiptFrame(consumption.value);
if (consumptionBytes.toString('hex') !== consumption.signing_frame_hex
    || !verify(consumptionBytes, receiptIssuerKey, consumption.value.signature)) fail('consumption receipt vector mismatch');
const completion = fixture.nakama_completion_receipt;
const completionBytes = completionReceiptFrame(completion.value);
if (completionBytes.toString('hex') !== completion.signing_frame_hex
    || !verify(completionBytes, receiptIssuerKey, completion.value.signature)) fail('completion receipt vector mismatch');
const evidence = fixture.paper_raid_evidence_envelope;
const evidenceBytes = evidenceEnvelopeFrame(evidence.value);
if (evidenceBytes.toString('hex') !== evidence.signing_frame_hex
    || evidenceEnvelopeHash(evidence.value) !== evidence.hash) fail('evidence vector mismatch');
verifyEvidenceAgainst(evidence.value, fixture.paper_bundle.value, completion.value, {
  evaluation_report_hash: evidence.value.evaluation_report_hash,
  reproduction_report_hash: evidence.value.reproduction_report_hash,
  appeal_resolution_hash: evidence.value.appeal_resolution_hash,
  finality_receipt_hash: evidence.value.finality_receipt_hash,
}, receiptIssuerKey);
const publication = fixture.publication_release;
const publicationBytes = publicationFrame(publication.value);
if (publicationBytes.toString('hex') !== publication.frame_hex
    || digest(publicationBytes) !== publication.value.publication_release_hash) fail('publication vector mismatch');
verifyPublicationAgainst(publication.value, fixture.paper_bundle.value, evidence.value);

const assertionTamper = structuredClone(assertion.value); assertionTamper.claim.body_hash = digest(Buffer.from('tampered'));
if (verify(assertionFrame(assertionTamper), fixture.keys.consumer_edge.public_key_base64, assertionTamper.signature)) fail('assertion tamper accepted');
const releaseTamper = structuredClone(fixture.release_candidate.value); releaseTamper.title += ' tampered';
if (digest(releaseFrame(releaseTamper)) === fixture.release_candidate.hash) fail('release tamper accepted');
const consentTamper = structuredClone(fixture.authorship_consents[0]); consentTamper.signing.release_candidate_hash = digest(Buffer.from('tampered'));
if (verify(consentFrame(consentTamper.signing), consentTamper.signing.signing_public_key, consentTamper.signature)) fail('consent tamper accepted');
const bundleTamper = structuredClone(fixture.paper_bundle.value);
bundleTamper.author_consents[0].signature = Buffer.alloc(64).toString('base64');
try { bundleFrame(bundleTamper); fail('bundle signature tamper accepted'); } catch (error) { if (error.message === 'bundle signature tamper accepted') throw error; }
const consumptionTamper = structuredClone(consumption.value); consumptionTamper.consumed_at_unix += 1;
if (verify(consumptionReceiptFrame(consumptionTamper), receiptIssuerKey, consumptionTamper.signature)) fail('consumption tamper accepted');
const completionTamper = structuredClone(completion.value); completionTamper.ruleset_hash = digest(Buffer.from('tampered ruleset'));
if (verify(completionReceiptFrame(completionTamper), receiptIssuerKey, completionTamper.signature)) fail('completion tamper accepted');
const evidenceTamper = structuredClone(evidence.value); evidenceTamper.event_root = digest(Buffer.from('tampered event root'));
if (verify(evidenceEnvelopeFrame(evidenceTamper), receiptIssuerKey, evidenceTamper.signature)) fail('evidence tamper accepted');
const publicationTamper = structuredClone(publication.value); publicationTamper.destination += '-tampered';
try { publicationFrame(publicationTamper); fail('publication tamper accepted'); } catch (error) { if (error.message === 'publication tamper accepted') throw error; }

const rootMismatch = structuredClone(completion.value); rootMismatch.event_root = digest(Buffer.from('different signed receipt root'));
try { verifyEvidenceAgainst(evidence.value, fixture.paper_bundle.value, rootMismatch, {
  evaluation_report_hash: evidence.value.evaluation_report_hash,
  reproduction_report_hash: evidence.value.reproduction_report_hash,
  appeal_resolution_hash: null,
  finality_receipt_hash: null,
}, receiptIssuerKey); fail('receipt root mismatch accepted'); } catch (error) { if (error.message === 'receipt root mismatch accepted') throw error; }
const receiptHashTamper = structuredClone(evidence.value); receiptHashTamper.nakama_completion_receipt_hash = digest(Buffer.from('wrong receipt'));
try { verifyEvidenceAgainst(receiptHashTamper, fixture.paper_bundle.value, completion.value, {
  evaluation_report_hash: receiptHashTamper.evaluation_report_hash,
  reproduction_report_hash: receiptHashTamper.reproduction_report_hash,
  appeal_resolution_hash: null,
  finality_receipt_hash: null,
}, receiptIssuerKey); fail('receipt hash tamper accepted'); } catch (error) { if (error.message === 'receipt hash tamper accepted') throw error; }
const reviewReferenceTamper = structuredClone(evidence.value); reviewReferenceTamper.evaluation_report_hash = digest(Buffer.from('wrong evaluation'));
try { verifyEvidenceAgainst(reviewReferenceTamper, fixture.paper_bundle.value, completion.value, {
  evaluation_report_hash: evidence.value.evaluation_report_hash,
  reproduction_report_hash: evidence.value.reproduction_report_hash,
  appeal_resolution_hash: null,
  finality_receipt_hash: null,
}, receiptIssuerKey); fail('outer reference tamper accepted'); } catch (error) { if (error.message === 'outer reference tamper accepted') throw error; }
const authorReplacement = structuredClone(publication.value); authorReplacement.author_consents[0].player_id = 'ffffffff-ffff-4fff-8fff-ffffffffffff';
try { verifyPublicationAgainst(authorReplacement, fixture.paper_bundle.value, evidence.value); fail('author replacement accepted'); } catch (error) { if (error.message === 'author replacement accepted') throw error; }

console.log(JSON.stringify({
  schema: 'hepta.paper_raid.fixture_verification.v2',
  fixture: fixturePath,
  release_candidate_hash: fixture.release_candidate.hash,
  paper_bundle_hash: fixture.paper_bundle.value.paper_bundle_hash,
  completion_receipt_hash: canonicalJsonDigest(completion.value),
  evidence_envelope_hash: evidence.hash,
  publication_release_hash: publication.value.publication_release_hash,
  tamper_negatives: 12,
  ok: true,
}));
