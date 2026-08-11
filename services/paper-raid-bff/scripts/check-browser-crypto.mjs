import assert from "node:assert/strict";
import { createHash, webcrypto } from "node:crypto";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const browserUrl = new URL("../src/browser.js", import.meta.url);
const source = await readFile(browserUrl, "utf8");
const reviewFixtureBytes = await readFile(
  new URL("../../../docs/sdk-fixtures/hepta-paper-review-v4.json", import.meta.url)
);
const reviewFixture = JSON.parse(reviewFixtureBytes.toString("utf8"));
assert.equal(
  `sha256:${createHash("sha256").update(reviewFixtureBytes).digest("hex")}`,
  "sha256:b25dcbfcf3f9d5830ab8d2b32bdd36b2c073bca8a0bff1da6ba05fb85f6f17b4"
);
assert.equal(reviewFixture.schema, "hepta.paper_raid.review.golden_vectors.v4");
for (const vector of ["evaluation", "review_attestation", "reproduction"]) {
  assert.match(reviewFixture[vector].signing_frame_hex, /^[0-9a-f]+$/);
  assert.match(reviewFixture[vector].signature, /^[A-Za-z0-9+/]+={0,2}$/);
}

assert.equal(source.includes("localStorage"), false);
assert.equal(source.includes("indexedDB"), false);
assert.equal(source.includes(".style"), false);
assert.ok(source.includes("hepta.paper-raid.live-cursor.v1:"));
assert.equal((source.match(/sessionStorage/g) || []).length, 2);
assert.ok(source.includes("bindGuidedPaperActions()"));
assert.ok(source.includes('"transition_paper_project"'));
assert.ok(source.includes('"create_paper_work_item"'));
assert.ok(source.includes('"create_paper_revision"'));
assert.ok(source.includes('"promote_paper_release_candidate"'));
assert.ok(source.includes('"issue_research_session_authorization_set"'));
assert.ok(source.includes('"replace_research_session_authorization_set"'));
assert.ok(source.includes('"cancel_matchmaking_ticket"'));
assert.ok(source.includes('"claim_review_assignment"'));
for (const command of [
  "create_paper_evaluation_draft",
  "submit_evaluation_draft_attestation",
  "finalize_paper_evaluation_draft",
  "submit_reproduction",
  "submit_appeal",
  "resolve_appeal",
]) assert.ok(source.includes(`"${command}"`));
assert.ok(source.includes("review_claim_session_identity_mismatch"));
assert.ok(source.includes("review_claim_slot_not_authorized_for_identity"));
assert.ok(source.includes("/api/agent-bindings"));
assert.ok(source.includes("frozen_team_agent_binding_is_not_currently_active"));
for (const command of [
  "create_experiment_plan",
  "create_evidence_card",
  "create_citation_record",
  "create_claim_record",
  "create_run_record",
]) assert.ok(source.includes(`"${command}"`));
assert.ok(source.includes('input[name="roles"]:checked'));
assert.ok(source.includes("selected_role_is_not_authorized_for_this_identity"));
assert.ok(source.includes("const PARTY_CODE_V1_PATTERN = /^PR1-"));
assert.ok(source.includes("input.value = `PR1-${uuid()}`"));
assert.ok(source.includes("await sha256Label(new TextEncoder().encode(partyCode))"));
assert.ok(source.includes("if (partyCodeInput) partyCodeInput.value = \"\""));
assert.ok(source.includes("payload.party_code_hash = partyCodeHash"));
assert.equal(source.includes("payload.party_code ="), false);
assert.ok(source.includes("input-manifest-wizard-form"));
assert.ok(source.includes("draft-manifest-wizard-form"));
assert.ok(source.includes("run-artifact-wizard-form"));
assert.ok(source.includes("figure-lineage-wizard-form"));
assert.ok(source.includes("role-resource-action-form"));
assert.ok(source.includes('"create_role_resource_action"'));
assert.ok(source.includes("createAuthoritativeRunFromArtifacts"));
assert.ok(source.includes("createAuthoritativeFigureLineage"));
assert.ok(source.includes("authoritativeArtifactRegistration"));
assert.ok(source.includes('"register_artifact"'));
assert.ok(source.includes("async function preserveDisclosure("));
assert.ok(source.includes("new Blob([plainText]"));
assert.ok(source.includes("function invalidateStalePlayerForms("));
assert.ok(source.includes('connection.dataset.state = "stale-authority"'));
assert.ok(source.includes("newHeptaEvents > 0 || currentPhase !== synchronizedPhase"));
const htmlSource = await readFile(new URL("../src/html.rs", import.meta.url), "utf8");
const heptaSource = await readFile(new URL("../src/hepta.rs", import.meta.url), "utf8");
const appSource = await readFile(new URL("../src/app.rs", import.meta.url), "utf8");
const metricsSource = await readFile(new URL("../src/metrics.rs", import.meta.url), "utf8");
assert.ok(htmlSource.includes('name="party_code" type="text"'));
assert.ok(htmlSource.includes('class="generate-party-code"'));
assert.ok(htmlSource.includes('.get("private_party")'));
assert.equal(htmlSource.includes('.get("party_code_hash")'), false);
assert.ok(heptaSource.includes("matchmaking_party_payload_is_safe"));
assert.ok(appSource.includes("matchmaking_party_payload_is_safe(&command.payload)"));
assert.equal(metricsSource.includes("party_code"), false);
assert.ok(htmlSource.includes("at least 7 identities across the full flow"));
assert.ok(htmlSource.includes("Provisional contribution telemetry / 暂定贡献遥测"));
assert.ok(htmlSource.includes('data-after-action-report="v1"'));
assert.ok(htmlSource.includes("Role mastery, challenge unlocks, immutable replay and automatic rematch are not authoritative yet"));
assert.ok(htmlSource.includes('data-eligible="false">locked'));
assert.equal(source.includes('replay_started'), false);
assert.ok(htmlSource.includes("Template version / 模式版本"));
assert.ok(htmlSource.includes("Risk / 主要风险"));
assert.ok(htmlSource.includes("Modifiers / 规则修饰"));
assert.ok(htmlSource.includes("RELEASE-BOUND MATERIALIZATION"));
assert.ok(htmlSource.includes("section_materialization_root"));
assert.ok(htmlSource.includes("PLAYER-SIGNED QUORUM"));
assert.ok(htmlSource.includes("review-receipt-confirm-form"));
assert.ok(htmlSource.includes("review-attestation-form"));
assert.ok(htmlSource.includes("review-finalize-form"));
assert.ok(source.includes("confirmReviewReceipt"));
assert.ok(htmlSource.includes("author-appeal-form"));
assert.ok(htmlSource.includes("review-appeal-resolution-form"));
assert.ok(htmlSource.includes("value=\"upheld\"{}"));
assert.ok(htmlSource.includes("Claim a precise role, never an Author Room"));
const reviewPageStart = htmlSource.indexOf("pub fn review_queue(");
const loginPageStart = htmlSource.indexOf("pub fn login_page()", reviewPageStart);
assert.ok(reviewPageStart >= 0 && loginPageStart > reviewPageStart);
const reviewPageSource = htmlSource.slice(reviewPageStart, loginPageStart);
const htmlProductionSource = htmlSource.slice(0, htmlSource.indexOf("#[cfg(test)]"));
assert.equal(reviewPageSource.includes("/league/papers/"), false);
assert.ok(reviewPageSource.includes("/league/review/"));
assert.equal(htmlSource.includes("Use one of the three externally provisioned alpha login keys"), false);
assert.equal(htmlProductionSource.includes("paper-raid-agent-bridge/src/cli.mjs pair"), false);
assert.equal(htmlProductionSource.includes("prepare-delivery --input"), false);
assert.equal(htmlProductionSource.includes("work --config"), false);
assert.ok(htmlSource.includes("Open the installed Paper Raid Agent Bridge"));
assert.equal(htmlProductionSource.includes("--submit"), false);
assert.ok(htmlSource.includes("agent-pairing-grant-form"));
assert.ok(htmlSource.includes("Shown once / 仅显示一次"));
assert.ok(htmlSource.includes("hepta.paper_raid.agent_binding_proof.v3"));
assert.ok(source.includes("/api/agent-bridge/pairing-grants"));
assert.ok(source.includes("navigator.clipboard.writeText"));
assert.ok(source.includes("/league/review/${encodeURIComponent(claim.paperId)}"));
const promoteHtmlStart = htmlSource.indexOf("fn promote_release_form(");
const approvalHtmlStart = htmlSource.indexOf("fn release_approval_controls(", promoteHtmlStart);
assert.ok(promoteHtmlStart >= 0 && approvalHtmlStart > promoteHtmlStart);
const promoteHtmlSource = htmlSource.slice(promoteHtmlStart, approvalHtmlStart);
assert.equal(promoteHtmlSource.includes('name="contribution_ledger_hash"'), false);
assert.ok(promoteHtmlSource.includes("derives accepted-artifact and approving-review milestones"));
assert.ok(promoteHtmlSource.includes("Splitting records cannot mint extra milestone points"));
assert.ok(promoteHtmlSource.includes("authoritative_contribution_refs(room, paper_id, &player_id)"));
for (const semanticClosure of [
  "fn canonical_uuid_array(",
  "fn frozen_release_credit_roster(",
  "fn validated_joint_submission(",
  "fn validated_run_status",
  "fn validated_paper_score(",
  "fn validated_evaluation_quality(",
  "fn effective_review_evaluation(",
  "fn effective_reproduction_authority(",
  "activation_by_evaluation",
  "created_at < *activation_by_evaluation.get(&record_evaluation_id)?",
  "paper_evaluation_signing_bytes",
  'room.get("paper_revisions")',
  "action_id != subject_id",
  '"challenge_grace_deadline_elapsed"',
  '.get("deadline_at")',
  '.get("grace_expires_at")',
  "deadline_at >= grace_expires_at",
  "terminal_at < grace",
  "verified_at >= terminal_at",
  "struct ValidatedAar",
  "fn aar_unavailable()",
]) assert.ok(htmlSource.includes(semanticClosure));
assert.equal(htmlSource.includes("grace_window_elapsed"), false);
assert.equal(appSource.includes("grace_window_elapsed"), false);
assert.equal(heptaSource.includes("grace_window_elapsed"), false);
for (const sealedBoundary of [
  "pub(crate) struct AuthenticatedPaperRoom",
  "pub(crate) struct AuthenticatedPaperReviewState",
  "Result<AuthenticatedPaperRoom, AppError>",
  "Result<AuthenticatedPaperReviewState, AppError>",
  "struct PaperRoomEnvelopeV3",
  "struct PaperReviewStateEnvelopeV1",
  "struct SealedReviewAssignmentV1",
  "struct SealedPaperReproductionV1",
  "fn sealed_review_assignments(",
  "fn sealed_reproductions(",
  "#[serde(deny_unknown_fields)]",
]) assert.ok(heptaSource.includes(sealedBoundary));
assert.match(heptaSource, /#\[cfg\(test\)\]\s+pub\(crate\) fn test_only_seal/g);
assert.match(htmlSource, /#\[cfg\(test\)\]\s+fn after_action_report\(/);
const rawAarHelperStart = htmlSource.indexOf("#[cfg(test)]\nfn after_action_report(");
const productionAarStart = htmlSource.indexOf("fn authenticated_after_action_report(");
assert.ok(productionAarStart >= 0 && rawAarHelperStart > productionAarStart);
const productionAarSource = htmlSource.slice(productionAarStart, rawAarHelperStart);
assert.ok(productionAarSource.includes("AuthenticatedPaperRoom"));
assert.ok(productionAarSource.includes("AuthenticatedPaperReviewState"));
assert.equal(productionAarSource.includes("test_only_seal"), false);
assert.equal(productionAarSource.includes("VerifyingKey"), false);
assert.ok(htmlSource.includes("fn assert_whole_aar_unavailable("));
for (const negativeFixture of [
  'remove("deadline_at")',
  'remove("grace_expires_at")',
  'serde_json::json!("invented")',
  "appeal_before_evaluation",
  "resolution_before_appeal",
  "premature_verified_finality",
  "child_before_parent_resolution",
  "premature_reproduction_review",
  "later_reproduction",
  "duplicate_assignment",
  "unlinked_reproduction",
  "noncanonical_player",
]) assert.ok(htmlSource.includes(negativeFixture));
assert.equal(htmlSource.includes("artifacts.len() > 256"), false);
assert.equal(htmlSource.includes("accepted_reviews.len() > 256"), false);
assert.ok(promoteHtmlSource.includes('name="coi_disclosure_text"'));
assert.equal(promoteHtmlSource.includes('name="coi_disclosure_hash"'), false);
assert.ok(promoteHtmlSource.includes("No player enters a ledger UUID, reference, JSON, or digest"));
assert.ok(source.includes("contribution_ledger_id: budget.contributionLedgerId"));
const evidenceHtmlStart = htmlSource.indexOf('class="create-evidence-card-form"');
const evidenceHtmlEnd = htmlSource.indexOf('class="create-citation-record-form"', evidenceHtmlStart);
assert.ok(evidenceHtmlStart >= 0 && evidenceHtmlEnd > evidenceHtmlStart);
const evidenceHtmlSource = htmlSource.slice(evidenceHtmlStart, evidenceHtmlEnd);
assert.ok(evidenceHtmlSource.includes('name="source_file" type="file" required'));
assert.ok(evidenceHtmlSource.includes('name="source_media_type"'));
assert.equal(evidenceHtmlSource.includes('name="source_hash"'), false);
const runWizardStart = htmlSource.indexOf('class="run-artifact-wizard-form primary-action"');
const legacyRunStart = htmlSource.indexOf('class="create-run-record-form"', runWizardStart);
const figureWizardStart = htmlSource.indexOf('class="figure-lineage-wizard-form"', legacyRunStart);
assert.ok(runWizardStart >= 0 && legacyRunStart > runWizardStart && figureWizardStart > legacyRunStart);
const runWizardSource = htmlSource.slice(runWizardStart, legacyRunStart);
for (const field of ["stdout_file", "stderr_file", "output_file"]) {
  assert.ok(runWizardSource.includes(`name="${field}" type="file" required`));
}
assert.ok(runWizardSource.includes('name="metrics_file" type="file" accept='));
for (const forbiddenField of ["manifest_id", "manifest_hash", "run_record_id", "metrics_hash"]) {
  assert.equal(runWizardSource.includes(`name="${forbiddenField}"`), false);
}
assert.equal(runWizardSource.includes("sha256"), false);
const figureWizardSource = htmlSource.slice(figureWizardStart, htmlSource.indexOf("if matches!(phase", figureWizardStart));
assert.ok(figureWizardSource.includes('name="figure_file" type="file"'));
assert.ok(figureWizardSource.includes('name="lineage_file" type="file" accept='));
assert.ok(figureWizardSource.includes('name="run_record_ids" multiple'));
for (const forbiddenField of ["figure_manifest_id", "figure_lineage_id", "transform_hash"]) {
  assert.equal(figureWizardSource.includes(`name="${forbiddenField}"`), false);
}
assert.equal(figureWizardSource.includes("sha256"), false);
const roleResourceStart = htmlSource.indexOf("fn role_resource_panel(");
const guidedRoomStart = htmlSource.indexOf("fn guided_paper_room(", roleResourceStart);
assert.ok(roleResourceStart >= 0 && guidedRoomStart > roleResourceStart);
const roleResourceSource = htmlSource.slice(roleResourceStart, guidedRoomStart);
assert.ok(roleResourceSource.includes("NON-ECONOMIC GAMEPLAY / 非经济玩法"));
assert.ok(htmlSource.includes('value.get("player_phase")'));
assert.ok(htmlSource.includes('data-player-phase="{}"'));
assert.ok(htmlSource.includes('"reproduction_readiness"'));
assert.ok(htmlSource.includes("Finality unknown / 终局状态未知"));
assert.ok(htmlSource.includes("Finality temporarily unavailable / 终局暂不可用"));
assert.ok(htmlSource.includes("Finality verification error / 终局验证错误"));
assert.ok(htmlSource.includes("finality-reason"));
assert.ok(roleResourceSource.includes('class="role-resource-action-form"'));
assert.ok(roleResourceSource.includes('data-resource-version="{}"'));
assert.ok(roleResourceSource.includes('name="evidence_card_id" required'));
for (const forbiddenField of ["action_id", "idempotency_key", "expected_resource_version", "payload"]) {
  assert.equal(roleResourceSource.includes(`name="${forbiddenField}"`), false);
}
const sciencePanelStart = htmlSource.indexOf("fn science_action_panel_for_role(");
const workPanelStart = htmlSource.indexOf("fn work_item_panel(", sciencePanelStart);
assert.ok(sciencePanelStart >= 0 && workPanelStart > sciencePanelStart);
const sciencePanelSource = htmlSource.slice(sciencePanelStart, workPanelStart);
assert.ok(sciencePanelSource.includes('role_resource_action(resources, "create_run_record")'));
assert.ok(sciencePanelSource.includes("if run_creation_allowed"));
assert.ok(sciencePanelSource.includes("run-resource-blocked"));
assert.ok(sciencePanelSource.includes("disabled before any CAS mutation"));
assert.ok(heptaSource.includes("CommandName::CreateRoleResourceAction"));
assert.ok(heptaSource.includes('"create_role_resource_action_v1"'));
const revisionHtmlStart = htmlSource.indexOf('class="{}" data-paper-id="{}" data-paper-version="{}" data-parent-revision-id="{}"');
const revisionHtmlEnd = htmlSource.indexOf('fn latest_field', revisionHtmlStart);
assert.ok(revisionHtmlStart >= 0 && revisionHtmlEnd > revisionHtmlStart);
const revisionHtmlSource = htmlSource.slice(revisionHtmlStart, revisionHtmlEnd);
for (const field of [
  "source_manifest_hash",
  "artifact_manifest_hash",
  "bibliography_hash",
  "claim_evidence_graph_hash",
]) assert.ok(revisionHtmlSource.includes(`name="${field}" type="hidden"`));
assert.equal(revisionHtmlSource.includes('placeholder="sha256:…"'), false);
assert.ok(htmlSource.includes("freeze-contribution-ledger-form"));
assert.ok(htmlSource.includes('ledger.get("ledger_hash")'));
assert.ok(source.includes("idempotency_key: budget.contributionLedgerId"));
for (const className of [
  "acquire-section-lease-form",
  "bridge-inbox-task",
  "human-proposal-decision-form",
  "create-section-revision-form",
  "section-review-form",
  "merge-section-form",
]) assert.ok(htmlSource.includes(className));
assert.equal(htmlSource.includes("copy-bridge-proposal"), false);
assert.ok(htmlSource.includes("never asks you to copy a terminal command, JSON, UUID, or digest"));
assert.ok(htmlSource.includes("In confirmation mode it shows one local approval prompt; in auto mode"));
assert.equal(source.includes("bridgeProposalCommand"), false);
assert.equal(source.includes("clipboard_unavailable_copy_the_rendered_command_manually"), false);
assert.ok(htmlSource.includes("data-artifact-manifest-hash"));
assert.ok(htmlSource.includes("formation_renders_authoritative_deadline_and_matched_ticket_withdrawal"));
assert.ok(htmlSource.includes("data-proposal-expires-at"));
assert.ok(htmlSource.includes("formation-withdraw-form"));
assert.ok(source.includes("function bindProposalCountdowns()"));
assert.ok(source.includes("function bindChallengeCountdowns()"));
assert.ok(source.includes("function bindChallengeOutcomeForms()"));
assert.ok(source.includes("/api/papers/${encodeURIComponent(paperId)}/outcome"));
assert.ok(htmlSource.includes("AUTHORITATIVE RULESET"));
assert.ok(htmlSource.includes("Victory conditions / 胜利条件"));
assert.ok(htmlSource.includes("challenge-outcome-form"));
assert.ok(htmlSource.includes("materializes automatically on the next authorized Room/state/event read"));
for (const boundary of [
  "post(transition_paper_challenge_outcome)",
  "guided_outcome_reason_allowed",
  "expired outcome is unavailable until the authoritative grace window elapses",
  "x-paper-raid-terminal-recovered",
  "CommandName::TransitionPaperChallengeOutcome => false",
  '"expected_version": expected_version',
]) assert.ok(appSource.includes(boundary));
const terminalControlsStart = htmlSource.indexOf("fn challenge_terminal_controls(");
const requirementsStart = htmlSource.indexOf("fn challenge_requirements(", terminalControlsStart);
assert.ok(terminalControlsStart >= 0 && requirementsStart > terminalControlsStart);
const terminalControlsSource = htmlSource.slice(terminalControlsStart, requirementsStart);
assert.equal(terminalControlsSource.includes("expired-outcome-control"), false);
assert.equal(terminalControlsSource.includes('name="outcome" value="expired"'), false);
for (const forbiddenField of ['name="paper_id"', 'name="expected_version"', 'name="payload"']) {
  assert.equal(terminalControlsSource.includes(forbiddenField), false);
}
assert.ok(source.includes("cancel_matchmaking_ticket"));
for (const command of [
  "acquire_section_lease",
  "record_human_decision",
  "create_section_revision",
  "submit_review",
  "merge_section",
]) assert.ok(source.includes(`"${command}"`));
for (const frozenFrame of [
  "HumanDecisionSigningV1",
  "human_decision_signing_bytes",
  "SectionReviewSigningV1",
  "section_review_signing_bytes",
  "SectionMergeSigningV1",
  "section_merge_signing_bytes",
  "PaperEvaluationSigningV1",
  "paper_evaluation_signing_bytes",
  "PaperReviewAttestationSigningV1",
  "paper_review_attestation_signing_bytes",
  "PaperReproductionSigningV1",
  "paper_reproduction_signing_bytes",
  "PaperAppealResolutionSigningV1",
  "paper_appeal_resolution_signing_bytes",
]) assert.ok(heptaSource.includes(frozenFrame));
assert.ok(heptaSource.includes("human decision must bind the current submitted proposal"));
assert.ok(heptaSource.includes("section review must bind the current proposed revision"));
assert.ok(heptaSource.includes("section merge does not match the authoritative head"));
assert.ok(heptaSource.includes("review action must bind the current submission-ready PaperBundle"));
assert.ok(heptaSource.includes("review attestation must bind the current open immutable draft"));
assert.ok(heptaSource.includes("reproduction must bind the current finalized evaluation"));
assert.ok(heptaSource.includes("guided Appeal requires one evidence_manifest_id"));
assert.ok(heptaSource.includes("upheld Appeal requires one exact eligible superseding evaluation"));

const createStart = source.indexOf("function bindHumanKeyCreate()");
const registrationStart = source.indexOf("function bindHumanKeyRegistration()");
const pairingStart = source.indexOf("function bindAgentPairing()");
const agentStart = source.indexOf("function bindAgentBinding()");
assert.ok(createStart >= 0 && registrationStart > createStart && pairingStart > registrationStart && agentStart > pairingStart);
const createFlow = source.slice(createStart, registrationStart);
const registrationFlow = source.slice(registrationStart, pairingStart);
const pairingFlow = source.slice(pairingStart, agentStart);
assert.equal(createFlow.includes("/api/onboarding/"), false);
assert.equal(registrationFlow.includes("createEncryptedBundle"), false);
assert.ok(registrationFlow.includes("/api/onboarding/human/challenge"));
assert.ok(registrationFlow.includes("/api/onboarding/human/register"));
assert.ok(registrationFlow.includes("import the original bundle"));
assert.ok(pairingFlow.includes("/api/agent-bridge/pairing-grants"));
assert.equal(pairingFlow.includes("login_key"), false);
assert.equal(pairingFlow.includes("localStorage"), false);
assert.equal(pairingFlow.includes("sessionStorage"), false);

const sessionValues = new Map();
const context = vm.createContext({
  Uint8Array,
  ArrayBuffer,
  Blob,
  TextEncoder,
  TextDecoder,
  URL,
  atob,
  btoa,
  console,
  crypto: webcrypto,
  document: {
    addEventListener() {},
    querySelector() { return null; },
    querySelectorAll() { return []; },
  },
  setTimeout,
  clearTimeout,
  sessionStorage: {
    getItem(key) { return sessionValues.get(key) ?? null; },
    setItem(key, value) { sessionValues.set(key, value); },
  },
  window: {
    location: { assign() {} },
    setTimeout,
  },
});
vm.runInContext(source, context, { filename: browserUrl.pathname });

const appealPayload = await context.appealPayload({
  elements: {
    grounds: { value: "Retained failed-run evidence was omitted from the evaluation." },
    evidence_manifest_id: { value: "11111111-1111-4111-8111-111111111111" },
  },
});
assert.match(appealPayload.appeal_id, /^[0-9a-f-]{36}$/);
assert.match(appealPayload.grounds_hash, /^sha256:[0-9a-f]{64}$/);
assert.equal(appealPayload.evidence_manifest_id, "11111111-1111-4111-8111-111111111111");
assert.deepEqual(Object.keys(appealPayload).sort(), [
  "appeal_id", "evidence_manifest_id", "grounds_hash",
]);
const resolutionPayload = await context.appealResolutionPayload(
  { elements: { decision: { value: "The frozen evaluation already considered the evidence." } } },
  "denied",
);
assert.match(resolutionPayload.resolution_id, /^[0-9a-f-]{36}$/);
assert.match(resolutionPayload.decision_hash, /^sha256:[0-9a-f]{64}$/);
assert.deepEqual(Object.keys(resolutionPayload).sort(), [
  "decision_hash", "outcome", "resolution_id",
]);
await assert.rejects(
  context.appealResolutionPayload(
    { elements: { decision: { value: "Invalid outcome." } } },
    "invented",
  ),
  /invalid_appeal_resolution_outcome/,
);

assert.equal(
  context.semanticPhaseLabel("submission_ready"),
  "Author Raid complete / 作者远征完成",
);
const activeChallengeClock = context.challengeClockState(
  "2026-08-10T10:10:00Z",
  "2026-08-10T10:20:00Z",
  Date.parse("2026-08-10T10:00:00Z"),
);
assert.equal(activeChallengeClock.state, "active");
assert.match(activeChallengeClock.text, /10:00 remaining/);
const overtimeChallengeClock = context.challengeClockState(
  "2026-08-10T10:10:00Z",
  "2026-08-10T10:20:00Z",
  Date.parse("2026-08-10T10:15:00Z"),
);
assert.equal(overtimeChallengeClock.state, "overtime");
assert.match(overtimeChallengeClock.text, /5:00 grace remaining/);
const expiredChallengeClock = context.challengeClockState(
  "2026-08-10T10:10:00Z",
  "2026-08-10T10:20:00Z",
  Date.parse("2026-08-10T10:20:00Z"),
);
assert.equal(expiredChallengeClock.state, "expired");
assert.equal(
  context.challengeClockState("invalid", "also-invalid", 0).state,
  "unavailable",
);
assert.equal(context.semanticRoleLabel("evidence"), "Evidence / 证据");
assert.equal(
  context.semanticEventLabel({
    event_type: "hepta.paper_raid.paper_project.phase_changed.v2",
    payload: { phase: "researching" },
  }),
  "Checkpoint advanced / 阶段已推进 → Research / 研究",
);
assert.equal(
  context.semanticEventLabel({
    action_type: "agent_analysis_ready",
    payload: { status: "accepted" },
  }),
  "Agent analysis ready · Accepted",
);
assert.equal(
  context.semanticEventLabel({
    event_type: "hepta.paper_raid.agent_proposal.submitted.v1",
    payload: { section_key: "methods" },
  }),
  "Agent proposal received / Agent 建议已收到 · methods",
);

assert.equal(context.nextHeptaCursor([{ cursor: 2 }, { cursor: 7 }, { cursor: 5 }], 3), 7);
assert.equal(context.nextHeptaCursor([{ cursor: -1 }, { cursor: "8" }], 4), 4);
assert.equal(context.paperRoomPhase({ paper_room: { paper: { phase: "submission_ready" } } }), "submission_ready");
assert.equal(context.paperRoomPhase({ paper_room: { phase: "untrusted_shape" } }), "waiting_for_authority");
let liveCursor = context.readLiveCursor("paper-alpha");
assert.equal(liveCursor.hepta, 0);
assert.deepEqual(Object.keys(liveCursor), ["hepta"]);
sessionValues.set("hepta.paper-raid.live-cursor.v1:paper-alpha", JSON.stringify({
  hepta: 11,
  nakama: { "paper.raid:one": 19, "paper.raid:two": 7 },
}));
liveCursor = context.readLiveCursor("paper-alpha");
assert.equal(liveCursor.hepta, 11);
assert.deepEqual(Object.keys(liveCursor), ["hepta"]);
sessionValues.set("hepta.paper-raid.live-cursor.v1:paper-alpha", "not-json");
liveCursor = context.readLiveCursor("paper-alpha");
assert.equal(liveCursor.hepta, 0);
assert.deepEqual(Object.keys(liveCursor), ["hepta"]);

const heads = [
  { logical_session_id: "paper.raid:one", roster_version: 1, nakama_completion_received: false },
];
let nakamaSessions = context.reconcileNakamaSessions(heads, new Map());
assert.equal(nakamaSessions.size, 1);
assert.equal(nakamaSessions.get("paper.raid:one").sequence, 0);
const accepted = context.acceptNakamaArchive({
  logical_session_id: "paper.raid:one",
  roster_version: 1,
  requested_after_sequence: 0,
  archive: {
    schema: "trnm.nakama.research-session.archive.v1",
    logical_session_id: "paper.raid:one",
    roster_version: 1,
    after_sequence: 0,
    next_after_sequence: 3,
    has_more: true,
  },
}, nakamaSessions.get("paper.raid:one"));
nakamaSessions.set("paper.raid:one", accepted);
nakamaSessions = context.reconcileNakamaSessions(heads, nakamaSessions);
assert.equal(nakamaSessions.get("paper.raid:one").sequence, 3);
nakamaSessions = context.reconcileNakamaSessions([
  { logical_session_id: "paper.raid:one", roster_version: 2, nakama_completion_received: false },
], nakamaSessions);
assert.equal(nakamaSessions.size, 1);
assert.equal(nakamaSessions.get("paper.raid:one").sequence, 0);
assert.equal(nakamaSessions.get("paper.raid:one").rosterVersion, 2);
assert.throws(() => context.acceptNakamaArchive({
  logical_session_id: "paper.raid:one",
  roster_version: 2,
  requested_after_sequence: 0,
  archive: {
    schema: "trnm.nakama.research-session.archive.v1",
    logical_session_id: "paper.raid:one",
    roster_version: 2,
    after_sequence: 0,
    next_after_sequence: 0,
    has_more: true,
  },
}, nakamaSessions.get("paper.raid:one")), /invalid_nakama_archive_cursor/);

assert.deepEqual(
  JSON.parse(JSON.stringify(context.phaseTransitionPayload("3", "experimenting"))),
  { expected_version: 3, next_phase: "experimenting" },
);
assert.throws(
  () => context.phaseTransitionPayload(3, "submission_ready"),
  /invalid_next_paper_phase/,
);
const workItem = JSON.parse(JSON.stringify(context.workItemPayload(
  "4",
  "paper_section",
  "Verify the primary claim",
  "00000000-0000-4000-8000-000000000001",
  "00000000-0000-4000-8000-000000000002",
)));
assert.equal(workItem.expected_paper_version, 4);
assert.equal(workItem.assigned_player_id, "00000000-0000-4000-8000-000000000001");
assert.match(workItem.work_item_id, /^[0-9a-f-]{36}$/i);
const digest = `sha256:${"a".repeat(64)}`;
const manifestReceipt = context.authoritativeArtifactRegistration({
  manifest_id: "00000000-0000-4000-8000-000000000201",
  paper_project_id: "00000000-0000-4000-8000-000000000202",
  source_bundle_id: "browser-run-logs",
  source_manifest_sha256: "b".repeat(64),
  manifest_hash: `sha256:${"b".repeat(64)}`,
  object_count: 1,
  required_run_ids: ["run-1"],
  version: 1,
}, {
  manifestId: "00000000-0000-4000-8000-000000000201",
  paperId: "00000000-0000-4000-8000-000000000202",
  sourceBundleId: "browser-run-logs",
  expectedSourceManifestSha256: "b".repeat(64),
  requiredRunIds: ["run-1"],
  uploaded: [{
    logicalPath: "runs/run-1/logs/stdout.txt",
    role: "run_stdout",
    stored: {
      digest: `sha256:${"c".repeat(64)}`,
      uri: `cas://sha256/${"c".repeat(64)}`,
      size: 12,
    },
  }],
});
assert.equal(manifestReceipt.manifestId, "00000000-0000-4000-8000-000000000201");
assert.equal(manifestReceipt.manifestHash, `sha256:${"b".repeat(64)}`);
assert.equal(manifestReceipt.objects[0].role, "run_stdout");
assert.throws(() => context.authoritativeArtifactRegistration({
  manifest_id: "00000000-0000-4000-8000-000000000201",
  paper_project_id: "00000000-0000-4000-8000-000000000202",
  source_bundle_id: "browser-run-logs",
  source_manifest_sha256: "b".repeat(64),
  manifest_hash: `sha256:${"d".repeat(64)}`,
  object_count: 1,
  required_run_ids: ["run-1"],
  version: 1,
}, {
  manifestId: "00000000-0000-4000-8000-000000000201",
  paperId: "00000000-0000-4000-8000-000000000202",
  sourceBundleId: "browser-run-logs",
  expectedSourceManifestSha256: "b".repeat(64),
  requiredRunIds: ["run-1"],
  uploaded: [{
    logicalPath: "runs/run-1/logs/stdout.txt",
    role: "run_stdout",
    stored: { digest: `sha256:${"c".repeat(64)}`, uri: `cas://sha256/${"c".repeat(64)}`, size: 12 },
  }],
}), /artifact_manifest_registration_receipt_is_not_authoritative_or_exact/);
const figureLineage = JSON.parse(JSON.stringify(context.figureLineagePayload({
  figureKey: "primary-effect",
  figureManifestId: "00000000-0000-4000-8000-000000000211",
  runRecordIds: [
    "00000000-0000-4000-8000-000000000212",
    "00000000-0000-4000-8000-000000000213",
  ],
  transformHash: digest,
})));
assert.equal(figureLineage.figure_key, "primary-effect");
assert.equal(figureLineage.figure_manifest_id, "00000000-0000-4000-8000-000000000211");
assert.deepEqual(figureLineage.run_record_ids, [
  "00000000-0000-4000-8000-000000000212",
  "00000000-0000-4000-8000-000000000213",
]);
assert.match(figureLineage.figure_lineage_id, /^[0-9a-f-]{36}$/i);
assert.throws(() => context.figureLineagePayload({
  figureKey: "primary-effect",
  figureManifestId: "00000000-0000-4000-8000-000000000211",
  runRecordIds: [
    "00000000-0000-4000-8000-000000000212",
    "00000000-0000-4000-8000-000000000212",
  ],
  transformHash: digest,
}), /figure_lineage_requires_distinct_authoritative_runs/);
assert.deepEqual(
  JSON.parse(JSON.stringify(context.workItemTransitionPayload(2, "accepted", digest))),
  { expected_version: 2, next_status: "accepted", artifact_manifest_hash: digest },
);
assert.throws(
  () => context.workItemTransitionPayload(2, "accepted", "sha256:not-a-digest"),
  /artifact_manifest_hash_must_be_a_sha256_digest/,
);
const revision = JSON.parse(JSON.stringify(context.paperRevisionPayload(5, null, {
  sourceManifestHash: digest,
  artifactManifestHash: digest,
  bibliographyHash: digest,
  claimEvidenceGraphHash: digest,
})));
assert.equal(revision.expected_paper_version, 5);
assert.equal(revision.parent_revision_id, null);
assert.equal(revision.claim_evidence_graph_hash, digest);

const leasePayload = JSON.parse(JSON.stringify(context.acquireSectionLeasePayload({
  sectionKey: "methods",
  holderBindingId: "00000000-0000-4000-8000-000000000031",
  currentRevisionId: "00000000-0000-4000-8000-000000000032",
  previousFencingToken: "0",
  ttlSeconds: "900",
})));
assert.equal(leasePayload.section_key, "methods");
assert.equal(leasePayload.expected_previous_fencing_token, 0);
assert.equal(leasePayload.ttl_seconds, 900);
assert.match(leasePayload.lease_id, /^[0-9a-f-]{36}$/i);
assert.throws(() => context.acquireSectionLeasePayload({
  sectionKey: "methods/../../admin",
  holderBindingId: "00000000-0000-4000-8000-000000000031",
  currentRevisionId: "00000000-0000-4000-8000-000000000032",
  previousFencingToken: 0,
  ttlSeconds: 900,
}), /section_key_must_be_a_safe_logical_identifier/);

const decisionPayload = JSON.parse(JSON.stringify(await context.humanDecisionPayload({
  proposalId: "00000000-0000-4000-8000-000000000033",
  proposalVersion: "1",
  decision: "accept",
  reason: "The artifact matches the assigned task.",
})));
assert.equal(decisionPayload.decision, "accept");
assert.match(decisionPayload.reason_hash, /^sha256:[0-9a-f]{64}$/);
const sectionRevision = JSON.parse(JSON.stringify(context.sectionRevisionPayload({
  sectionKey: "methods",
  parentRevisionId: "00000000-0000-4000-8000-000000000032",
  proposalId: "00000000-0000-4000-8000-000000000033",
  leaseId: leasePayload.lease_id,
  fencingToken: "1",
  patchManifestId: "00000000-0000-4000-8000-000000000034",
  patchHash: digest,
})));
assert.equal(sectionRevision.fencing_token, 1);
assert.equal(sectionRevision.patch_hash, digest);
const sectionReview = JSON.parse(JSON.stringify(await context.sectionReviewPayload({
  revisionVersion: "1",
  verdict: "approve",
  review: "Lineage and claims are internally consistent.",
})));
assert.equal(sectionReview.verdict, "approve");
assert.match(sectionReview.review_hash, /^sha256:[0-9a-f]{64}$/);
const mergePayload = JSON.parse(JSON.stringify(context.sectionMergePayload({
  sectionRevisionId: sectionRevision.section_revision_id,
  revisionVersion: "2",
  parentRevisionId: sectionRevision.parent_revision_id,
  leaseId: sectionRevision.lease_id,
  fencingToken: "1",
})));
assert.equal(mergePayload.merged_section_revision_id, sectionRevision.section_revision_id);
assert.equal(mergePayload.expected_revision_version, 2);
const ledgerPaperId = "00000000-0000-4000-8000-000000000101";
const ledgerRevisionId = "00000000-0000-4000-8000-000000000102";
const ledgerAuthors = [
  {
    player_id: "00000000-0000-4000-8000-000000000105",
    credit_roles: ["validation", "methodology"],
    accepted_artifact_manifest_ids: [
      "00000000-0000-4000-8000-000000000202",
      "00000000-0000-4000-8000-000000000201",
    ],
    accepted_section_review_ids: [],
  },
  {
    player_id: "00000000-0000-4000-8000-000000000103",
    credit_roles: ["conceptualization", "writing_review_editing"],
    accepted_artifact_manifest_ids: [],
    accepted_section_review_ids: ["00000000-0000-4000-8000-000000000301"],
  },
  {
    player_id: "00000000-0000-4000-8000-000000000104",
    credit_roles: ["data_curation"],
    accepted_artifact_manifest_ids: [],
    accepted_section_review_ids: [],
  },
];
const promoteBudget = await context.contributionLedgerBudget(
  ledgerPaperId,
  ledgerRevisionId,
  ledgerAuthors,
);
assert.equal(promoteBudget.contributionLedgerId, "52411704-c8f0-53f3-b981-2c859f99cdcc");
assert.equal(promoteBudget.ledgerHash, "sha256:153fd3c7913bafb523dce23a1ae4d749c6363effda6ccb6e5541ba67213edb42");
assert.deepEqual(
  JSON.parse(JSON.stringify(promoteBudget.requestEntries.map(entry => entry.player_id))),
  [
    "00000000-0000-4000-8000-000000000103",
    "00000000-0000-4000-8000-000000000104",
    "00000000-0000-4000-8000-000000000105",
  ],
);
await assert.rejects(
  context.contributionLedgerBudget(
    ledgerPaperId,
    ledgerRevisionId,
    ledgerAuthors.map((author, index) => index === 0 ? {
      ...author,
      accepted_artifact_manifest_ids: [
        author.accepted_artifact_manifest_ids[0],
        author.accepted_artifact_manifest_ids[0],
      ],
    } : author),
  ),
  /accepted_artifact_manifest_id_must_not_contain_duplicates/,
);
assert.deepEqual(
  JSON.parse(JSON.stringify(promoteBudget.frozenEntries.map(entry => ({
    player_id: entry.player_id,
    contribution_points: entry.contribution_points,
  })) )),
  [
    { player_id: "00000000-0000-4000-8000-000000000103", contribution_points: 150 },
    { player_id: "00000000-0000-4000-8000-000000000104", contribution_points: 0 },
    { player_id: "00000000-0000-4000-8000-000000000105", contribution_points: 100 },
  ],
);
// Reload repair reconstructs from frozen release_candidate authors, whose
// order may differ, and must produce the same UUID/hash byte-for-byte.
const frozenAuthorNodes = [ledgerAuthors[1], ledgerAuthors[2], ledgerAuthors[0]].map(author => ({
  dataset: { playerId: author.player_id },
  querySelectorAll(selector) {
    if (selector === ".ledger-credit-role") {
      return author.credit_roles.map(role => ({ dataset: { role } }));
    }
    if (selector === ".contribution-artifact-ref") {
      return author.accepted_artifact_manifest_ids.map(manifestId => ({ dataset: { manifestId } }));
    }
    if (selector === ".contribution-review-ref") {
      return author.accepted_section_review_ids.map(reviewId => ({ dataset: { reviewId } }));
    }
    assert.fail(`unexpected selector ${selector}`);
  },
}));
const reconstructedAuthors = context.frozenLedgerAuthors({
  querySelectorAll(selector) {
    assert.equal(selector, ".ledger-author");
    return frozenAuthorNodes;
  },
});
const repairBudget = await context.contributionLedgerBudget(
  ledgerPaperId,
  ledgerRevisionId,
  reconstructedAuthors,
);
assert.equal(repairBudget.contributionLedgerId, promoteBudget.contributionLedgerId);
assert.equal(repairBudget.ledgerHash, promoteBudget.ledgerHash);

assert.equal(await context.semanticDigest("  reproducible protocol  ", "protocol"),
  await context.sha256Label(new TextEncoder().encode("reproducible protocol")));
assert.equal(await context.semanticDigest(digest.toUpperCase(), "protocol"),
  await context.sha256Label(new TextEncoder().encode(digest.toUpperCase())));
assert.notEqual(await context.semanticDigest(digest, "protocol"), digest);
const experimentPlan = JSON.parse(JSON.stringify(await context.experimentPlanPayload({
  protocol: "frozen protocol",
  codeManifestId: "00000000-0000-4000-8000-000000000011",
  datasetManifestId: "00000000-0000-4000-8000-000000000012",
  environmentManifestId: "00000000-0000-4000-8000-000000000013",
  seedPolicy: "seeds 1 through 10",
  stoppingRule: "stop after ten runs",
})));
assert.match(experimentPlan.experiment_plan_id, /^[0-9a-f-]{36}$/i);
assert.match(experimentPlan.protocol_snapshot_hash, /^sha256:[0-9a-f]{64}$/);
await assert.rejects(context.experimentPlanPayload({
  protocol: "protocol",
  codeManifestId: "same",
  datasetManifestId: "same",
  environmentManifestId: "different",
  seedPolicy: "seeds",
  stoppingRule: "stop",
}), /three_distinct_manifests/);
const evidencePayload = JSON.parse(JSON.stringify(context.evidenceVerificationPayload({
  sourceUri: "https://example.org/source",
  sourceHash: digest,
  locator: "p. 3, Table 1",
  license: "CC-BY-4.0",
})));
assert.deepEqual(Object.keys(evidencePayload).sort(), [
  "evidence_card_id", "license", "locator", "source_hash", "source_uri",
]);
assert.throws(() => context.evidenceVerificationPayload({
  sourceUri: "http://example.org/source",
  sourceHash: digest,
  locator: "p. 3",
  license: "CC-BY-4.0",
}), /credential_free_https/);
const citationPayload = JSON.parse(JSON.stringify(context.citationVerificationPayload({
  evidenceCardId: evidencePayload.evidence_card_id,
  doi: "10.1234/example",
  canonicalUrl: "",
})));
assert.equal(citationPayload.doi, "10.1234/example");
assert.equal(citationPayload.canonical_url, null);
const failedRun = JSON.parse(JSON.stringify(await context.runRecordPayload({
  experimentPlanId: experimentPlan.experiment_plan_id,
  status: "failed",
  seed: "0",
  parameters: "batch=32",
  logsManifestId: "00000000-0000-4000-8000-000000000021",
  outputsManifestId: "",
  metrics: "",
  failure: "out of memory",
})));
assert.equal(failedRun.outputs_manifest_id, null);
assert.equal(failedRun.metrics_hash, null);
assert.match(failedRun.failure_hash, /^sha256:[0-9a-f]{64}$/);
const claim = JSON.parse(JSON.stringify(await context.claimRecordPayload({
  claimKey: "primary-effect",
  claimKind: "main",
  statement: "The primary effect is positive.",
  evidenceCardIds: [evidencePayload.evidence_card_id],
  runRecordIds: [],
  figureLineageIds: [],
})));
assert.match(claim.statement_hash, /^sha256:[0-9a-f]{64}$/);
await assert.rejects(context.claimRecordPayload({
  claimKey: "unsupported",
  claimKind: "main",
  statement: "Unsupported claim",
  evidenceCardIds: [],
  runRecordIds: [],
  figureLineageIds: [],
}), /requires_evidence_run_or_figure_lineage/);
assert.equal(
  context.canonicalJson({ z: 1, a: { y: false, b: [2, { d: 4, c: 3 }] } }),
  '{"a":{"b":[2,{"c":3,"d":4}],"y":false},"z":1}',
);
const bundleFixture = {
  schema: "paper-raid.artifact-bundle.v1",
  required_run_ids: ["planned-run-1"],
  objects: [],
  object_count: 0,
  human_authority_materialized: false,
  hepta_binding_status: "unbound",
  created_at: "2026-08-10T00:00:00.000Z",
  challenge_id: "00000000-0000-4000-8000-000000000001",
  bundle_id: "browser-fixture",
  artifact_root: {
    digest_file: "artifact-bundle.v1.sha256",
    algorithm: "sha256-canonical-manifest-v1",
  },
};
assert.equal(
  await context.neutralBundleRawSha256(bundleFixture),
  "9b52b73d6dc2be5f766fb2515180883bee70fa196b513e234b3b4ca8a7b8c887",
);
assert.equal(context.safeLogicalFilename("../My data (final).CSV", "data.bin"), "My-data-final-.CSV");

const reviewerClaim = JSON.parse(JSON.stringify(context.reviewClaimPayload(
  {
    player_id: "00000000-0000-4000-8000-000000000041",
    scopes: ["reviewer"],
  },
  {
    paperId: "00000000-0000-4000-8000-000000000042",
    playerId: "00000000-0000-4000-8000-000000000041",
    reviewRound: "1",
    slot: "reviewer_2",
  },
)));
assert.equal(reviewerClaim.paperId, "00000000-0000-4000-8000-000000000042");
assert.equal(reviewerClaim.payload.player_id, "00000000-0000-4000-8000-000000000041");
assert.equal(reviewerClaim.payload.review_round, 1);
assert.equal(reviewerClaim.payload.slot, "reviewer_2");
assert.match(reviewerClaim.payload.assignment_id, /^[0-9a-f-]{36}$/i);
assert.throws(() => context.reviewClaimPayload(
  { player_id: "00000000-0000-4000-8000-000000000041", scopes: ["reviewer"] },
  {
    paperId: "00000000-0000-4000-8000-000000000042",
    playerId: "00000000-0000-4000-8000-000000000041",
    reviewRound: "1",
    slot: "evaluator",
  },
), /review_claim_slot_not_authorized/);

const passphrase = "paper-raid-browser-gate-passphrase";
const created = await context.createEncryptedBundle(passphrase);
assert.equal(created.bundle.schema, "hepta.paper_raid.human_key_bundle.v1");
assert.equal(created.bundle.kdf.iterations, 310000);
assert.equal(created.bundle.cipher.name, "AES-GCM");
assert.equal(created.signer.privateKey.extractable, false);

const exactServerBytes = webcrypto.getRandomValues(new Uint8Array(127));
const signedEvidenceFrame = await context.signServerFrame(
  "create_evidence_card",
  {
    signing_public_key: created.signer.publicKeyBase64,
    signing_bytes: Buffer.from(exactServerBytes).toString("base64"),
    payload: evidencePayload,
  },
  created.signer,
);
assert.equal("signature" in signedEvidenceFrame.payload, false);
assert.equal(typeof signedEvidenceFrame.payload.verification_signature, "string");
const exactFramePublicKey = await webcrypto.subtle.importKey(
  "raw",
  Uint8Array.from(atob(created.signer.publicKeyBase64), character => character.charCodeAt(0)),
  { name: "Ed25519" },
  false,
  ["verify"],
);
assert.equal(await webcrypto.subtle.verify(
  "Ed25519",
  exactFramePublicKey,
  Uint8Array.from(atob(signedEvidenceFrame.payload.verification_signature), character => character.charCodeAt(0)),
  exactServerBytes,
), true);
const tamperedFrameBytes = exactServerBytes.slice();
tamperedFrameBytes[0] ^= 1;
assert.equal(await webcrypto.subtle.verify(
  "Ed25519",
  exactFramePublicKey,
  Uint8Array.from(atob(signedEvidenceFrame.payload.verification_signature), character => character.charCodeAt(0)),
  tamperedFrameBytes,
), false);

const input = value => ({ value });
const reviewDigest = `sha256:${"a".repeat(64)}`;
const reviewPublicKeyHash = `sha256:${createHash("sha256")
  .update(Buffer.from(created.signer.publicKeyBase64, "base64"))
  .digest("hex")}`;
const reviewSigningKeyId = `human-ed25519:${reviewPublicKeyHash.slice("sha256:".length)}`;
const scoreComponents = {
  method_rigor_bps: 2000,
  experiment_statistics_bps: 1200,
  reproducibility_bps: 1200,
  evidence_citations_bps: 1200,
  value_originality_bps: 1200,
  argument_expression_bps: 800,
  ethics_transparency_bps: 400,
};
const observableHardGates = {
  citations_and_data_authentic: false,
  failed_runs_disclosed: false,
  core_claims_have_evidence: false,
  license_ethics_coi_complete: false,
};
const evaluationRequest = {
  coi_attestation_hash: reviewDigest,
  score_components: scoreComponents,
  observable_hard_gates: observableHardGates,
};
const reviewContext = {
  schema: "hepta.paper_raid.review_receipt_confirmation_context.v1",
  receipt_id: "00000000-0000-4000-8000-000000000043",
  receipt_hash: reviewDigest,
  task_id: "00000000-0000-4000-8000-000000000044",
  assignment_id: "00000000-0000-4000-8000-000000000045",
  assignment_version: 1,
  paper_project_id: "00000000-0000-4000-8000-000000000042",
  submission_id: "00000000-0000-4000-8000-000000000046",
  evaluation_id: "00000000-0000-4000-8000-000000000047",
  kind: "evaluate",
  bundle_hash: reviewDigest,
  release_candidate_hash: reviewDigest,
  paper_bundle_hash: reviewDigest,
  artifact_manifest_hash: reviewDigest,
  evaluator_version: reviewDigest,
  agent_id: "agent-evaluator",
  agent_key_id: "agent-key-evaluator",
  input_root: reviewDigest,
  output_root: reviewDigest,
  metrics_hash: reviewDigest,
  candidate_passed: true,
  seed_set_hash: reviewDigest,
  environment_hash: reviewDigest,
  run_manifest_hash: reviewDigest,
  logs_hash: reviewDigest,
  completed_at_unix: 1786439700,
};
const evaluationFrame = {
  schema: "hepta.paper_raid.review_receipt_confirmation_frame.v1",
  receipt_context: reviewContext,
  command: "create_paper_evaluation_draft",
  resource_id: reviewContext.paper_project_id,
  child_id: null,
  payload: {
    score_components: scoreComponents,
    hard_gates: {
      ...observableHardGates,
      all_authors_consented: true,
      artifact_lineage_complete: true,
    },
    evaluator_coi_attestation_hash: reviewDigest,
  },
  signing_bytes: Buffer.from(exactServerBytes).toString("base64"),
  signing_key_id: reviewSigningKeyId,
  signing_public_key: created.signer.publicKeyBase64,
  signing_public_key_hash: reviewPublicKeyHash,
};
evaluationFrame.receipt_context_signing_bytes = Buffer.from(context.canonicalJson({
  schema: "hepta.paper_raid.review_receipt_confirmation_context_signing.v1",
  receipt_context: evaluationFrame.receipt_context,
  command: evaluationFrame.command,
  resource_id: evaluationFrame.resource_id,
  child_id: evaluationFrame.child_id,
  upstream_signing_bytes: evaluationFrame.signing_bytes,
  signing_key_id: evaluationFrame.signing_key_id,
  signing_public_key_hash: evaluationFrame.signing_public_key_hash,
})).toString("base64");
context.validateReviewConfirmationFrame(
  evaluationFrame,
  evaluationFrame.command,
  evaluationFrame.resource_id,
  reviewContext.receipt_id,
  evaluationRequest,
);
const signedReviewFrame = await context.signReviewConfirmationFrame(
  evaluationFrame.command,
  evaluationFrame,
  created.signer,
);
assert.equal(typeof signedReviewFrame.signedFrame.payload.evaluator_signature, "string");
assert.equal(typeof signedReviewFrame.receiptContextSignature, "string");
assert.equal(await webcrypto.subtle.verify(
  "Ed25519",
  exactFramePublicKey,
  Uint8Array.from(atob(signedReviewFrame.receiptContextSignature), character => character.charCodeAt(0)),
  Uint8Array.from(atob(evaluationFrame.receipt_context_signing_bytes), character => character.charCodeAt(0)),
), true);
const tamperedReviewFrame = JSON.parse(JSON.stringify(evaluationFrame));
tamperedReviewFrame.receipt_context.output_root = `sha256:${"b".repeat(64)}`;
assert.throws(() => context.validateReviewConfirmationFrame(
  tamperedReviewFrame,
  tamperedReviewFrame.command,
  tamperedReviewFrame.resource_id,
  reviewContext.receipt_id,
  evaluationRequest,
), /review_receipt_confirmation_context_mismatch/);

const attestation = JSON.parse(JSON.stringify(await context.evaluationAttestationPayload(
  { elements: { coi_statement: input("No reviewer conflict.") } },
  "approve",
)));
assert.equal(attestation.verdict, "approve");
assert.match(attestation.coi_attestation_hash, /^sha256:[0-9a-f]{64}$/);

const reproductionFrame = JSON.parse(JSON.stringify(evaluationFrame));
reproductionFrame.receipt_context.kind = "reproduce";
reproductionFrame.receipt_context.candidate_passed = null;
reproductionFrame.command = "submit_reproduction";
reproductionFrame.child_id = reproductionFrame.receipt_context.evaluation_id;
reproductionFrame.payload = { coi_attestation_hash: reviewDigest };
reproductionFrame.receipt_context_signing_bytes = Buffer.from(context.canonicalJson({
  schema: "hepta.paper_raid.review_receipt_confirmation_context_signing.v1",
  receipt_context: reproductionFrame.receipt_context,
  command: reproductionFrame.command,
  resource_id: reproductionFrame.resource_id,
  child_id: reproductionFrame.child_id,
  upstream_signing_bytes: reproductionFrame.signing_bytes,
  signing_key_id: reproductionFrame.signing_key_id,
  signing_public_key_hash: reproductionFrame.signing_public_key_hash,
})).toString("base64");
context.validateReviewConfirmationFrame(
  reproductionFrame,
  reproductionFrame.command,
  reproductionFrame.resource_id,
  reviewContext.receipt_id,
  { coi_attestation_hash: reviewDigest },
);

const restored = await context.decryptBundle(
  JSON.parse(JSON.stringify(created.bundle)),
  passphrase,
);
assert.equal(restored.privateKey.extractable, false);
assert.equal(restored.publicKeyBase64, created.signer.publicKeyBase64);

const challenge = webcrypto.getRandomValues(new Uint8Array(96));
const publicKey = await webcrypto.subtle.importKey(
  "raw",
  Uint8Array.from(atob(restored.publicKeyBase64), character => character.charCodeAt(0)),
  { name: "Ed25519" },
  false,
  ["verify"],
);
for (const signer of [created.signer, restored]) {
  const signature = await webcrypto.subtle.sign("Ed25519", signer.privateKey, challenge);
  assert.equal(
    await webcrypto.subtle.verify("Ed25519", publicKey, signature, challenge),
    true,
  );
}

await assert.rejects(
  context.decryptBundle(created.bundle, "wrong-paper-raid-passphrase"),
);

console.log("paper-raid-bff browser crypto and same-key recovery gate: ok");
