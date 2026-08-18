#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const repoRoot = new URL("../../../", import.meta.url);
const bridgeRoot = new URL("tools/paper-raid-agent-bridge/", repoRoot);
const [
  agent,
  app,
  practiceHttp,
  browser,
  html,
  metrics,
  contracts,
  canonical,
  client,
  operations,
  practice,
  daemon,
  release,
  cli,
] = await Promise.all([
  readFile(new URL("src/agent_bridge.rs", root), "utf8"),
  readFile(new URL("src/app.rs", root), "utf8"),
  readFile(new URL("src/practice_http.rs", root), "utf8"),
  readFile(new URL("src/browser.js", root), "utf8"),
  readFile(new URL("src/html.rs", root), "utf8"),
  readFile(new URL("src/metrics.rs", root), "utf8"),
  readFile(new URL("crates/hepta-paper-raid-contracts/src/lib.rs", repoRoot), "utf8"),
  readFile(new URL("src/canonical.mjs", bridgeRoot), "utf8"),
  readFile(new URL("src/client.mjs", bridgeRoot), "utf8"),
  readFile(new URL("src/operations.mjs", bridgeRoot), "utf8"),
  readFile(new URL("src/practice.mjs", bridgeRoot), "utf8"),
  readFile(new URL("src/daemon.mjs", bridgeRoot), "utf8"),
  readFile(new URL("src/release.mjs", bridgeRoot), "utf8"),
  readFile(new URL("src/cli.mjs", bridgeRoot), "utf8"),
]);

const verifier = agent.slice(
  agent.indexOf("async fn verify_agent_request("),
  agent.indexOf("fn reject_unknown_agent_headers("),
);
for (const marker of [
  "load_bridge_mapping(state, claim.binding_id)",
  "identity_for_agent_bridge(&mapping.subject_id)",
  "authoritative_bridge_binding(state, &identity, &mapping)",
  "verify_agent_bridge_request_proof(",
  "read_agent_request_use(state, &claim, &request_hash)",
]) assert.ok(verifier.includes(marker), `signed active-binding verifier lost ${marker}`);

const routeHandlers = agent.slice(
  agent.indexOf("pub async fn agent_practice_tasks("),
  agent.indexOf("pub async fn agent_delivery_draft("),
);
assert.ok(routeHandlers.length > 4_000, "practice Agent handlers are absent");
for (const handler of [
  "agent_practice_tasks",
  "agent_practice_claim",
  "agent_practice_result",
]) {
  assert.ok(routeHandlers.includes(`pub async fn ${handler}(`), `missing ${handler}`);
}
assert.equal(
  (routeHandlers.match(/verify_agent_request\(/g) ?? []).length,
  3,
  "every practice Agent route must use signed Agent-PoP verification",
);
assert.equal(
  (routeHandlers.match(/replay_response\(&verified\)/g) ?? []).length,
  3,
  "every practice Agent route must replay before work",
);
assert.equal(
  (routeHandlers.match(/complete_agent_request\(/g) ?? []).length,
  3,
  "every practice Agent route must persist exact response bytes",
);
for (const marker of [
  "PracticeTaskQueryV1",
  "PracticeClaimRequestV1",
  "PracticeResultRequestV1",
  "apply_agent_practice_transition(",
  "load_current_agent_practice(",
  "ensure_practice_agent_admission(&verified)",
  "task_token",
  '"concern_confirmed"',
  '"concern_not_detected"',
  '"inconclusive"',
  '"hepta.paper_raid.agent_bridge.practice_materials.v1"',
]) {
  assert.ok(routeHandlers.includes(marker) || agent.includes(marker), `missing ${marker}`);
}
for (const forbidden of [
  "BrowserCommand",
  "forward_hepta_command",
  "create_paper",
  "activate",
  "qualification",
  "finality_receipt",
  "PaperScore",
  "ranked",
  "reward",
  "economy",
]) {
  assert.equal(
    routeHandlers.includes(forbidden),
    false,
    `practice Agent route crossed authority boundary: ${forbidden}`,
  );
}

const helper = practiceHttp.slice(
  practiceHttp.indexOf("pub(crate) async fn apply_agent_practice_transition("),
  practiceHttp.indexOf("async fn resolve_exact_active_binding("),
);
assert.ok(helper.length > 6_000, "server-bound practice transition helper is absent");
assert.ok(helper.includes("claim_bridge_task(&request, now)"));
assert.ok(helper.includes("apply_bridge_result(&request, now)"));
assert.ok(helper.includes("event.actor_kind='agent'"));
assert.ok(helper.includes("event.request_hash=$5"));
assert.ok(helper.includes("practice_agent_task_token(&practice)"));
assert.ok(helper.includes("task_token.as_bytes().ct_eq(expected_task_token.as_bytes())"));
assert.ok(
  helper.indexOf("recover_agent_practice_transition(") <
    helper.indexOf("load_live_agent_session_for_update("),
  "recovery must precede new practice execution",
);
assert.ok(
  helper.indexOf("recover_agent_practice_transition(") <
    helper.indexOf("practice_agent_task_token(&practice)"),
  "exact response recovery must precede current task-token verification",
);
assert.ok(
  practiceHttp.includes("ORDER BY created_at DESC, practice_session_id DESC"),
  "Agent recovery discovery must select the latest exact owner/binding practice",
);
for (const forbidden of [
  "paper_project_id",
  "activation_id",
  "qualification_id",
  "finality_receipt_hash",
  "ranking_eligible",
  "reward_eligible",
  "economic_eligible",
]) {
  assert.equal(helper.includes(forbidden), false, `practice helper wrote ${forbidden}`);
}

for (const [route, handler] of [
  ["/api/agent-bridge/practice-tasks", "agent_practice_tasks"],
  ["/api/agent-bridge/practice-claims", "agent_practice_claim"],
  ["/api/agent-bridge/practice-results", "agent_practice_result"],
]) {
  assert.ok(app.includes(`"${route}"`), `app route absent: ${route}`);
  assert.ok(app.includes(`post(agent_bridge::${handler})`), `route is not POST-only: ${route}`);
  assert.ok(metrics.includes(`"${route}" => "${route}"`), `unbounded metric route: ${route}`);
  assert.equal(browser.includes(route), false, `browser may not drive Agent route: ${route}`);
  assert.equal(html.includes(route), false, `HTML may not expose Agent route: ${route}`);
}

const projection = agent.slice(
  agent.indexOf("fn practice_task_projection("),
  agent.indexOf("pub async fn agent_delivery_draft("),
);
assert.ok(projection.includes('"task_token": practice_agent_task_token(practice)?'));
assert.ok(projection.includes("let completed = practice.bridge_task_state"));
for (const forbidden of [
  "practice_session_id",
  "subject_id",
  "player_id",
  "binding_id",
  "bridge_task_id",
  "request_hash",
  "result_hash",
]) {
  assert.equal(projection.includes(forbidden), false, `Agent task projection leaks ${forbidden}`);
}

for (const route of [
  "/api/agent-bridge/practice-tasks",
  "/api/agent-bridge/practice-claims",
  "/api/agent-bridge/practice-results",
]) {
  assert.ok(
    contracts.includes(`(\"POST\", \"${route}\")`),
    `shared Rust signing contract rejects ${route}`,
  );
  assert.ok(
    canonical.includes(`\"POST ${route}\"`),
    `installed JS signing contract rejects ${route}`,
  );
  assert.ok(client.includes(`\"${route}\"`), `installed client endpoint absent: ${route}`);
}
const practiceAuto = operations.slice(
  operations.indexOf("export async function executePracticeAuto("),
  operations.indexOf("export async function downloadChallengeMaterialBundle("),
);
assert.ok(practiceAuto.length > 3_000, "installed practice auto executor is absent");
assert.ok(practiceAuto.includes("task_token: tasks.task.task_token"));
assert.equal(
  (practiceAuto.match(/task_token: tasks\.task\.task_token/g) ?? []).length,
  2,
  "claim and result must echo their separately discovered opaque task tokens",
);
assert.ok(
  practiceAuto.indexOf("getPracticeWithClient(") <
    practiceAuto.indexOf("AGENT_BRIDGE_ENDPOINTS.practice_claims"),
  "practice claim preceded recovery discovery",
);
assert.ok(practiceAuto.includes("nowUnix: freshNowUnix()"));
assert.ok(practice.includes('"task_token"'));
assert.ok(practice.includes("/^sha256:[0-9a-f]{64}$/"));
assert.ok(
  daemon.indexOf("executePracticeAuto(config, identity)") <
    daemon.indexOf("await bridgeHealth(config, identity"),
  "installed daemon does not process bounded practice before formal inbox polling",
);
assert.ok(
  release.includes('Object.freeze({ path: "src/practice.mjs", mode: 0o400 })'),
  "installed release closure omits practice runtime",
);

const pairingCompletion = operations.slice(
  operations.indexOf("export async function pairAgentAndCheckHealth("),
  operations.indexOf("async function signedClientState("),
);
assert.ok(pairingCompletion.length > 500, "installed pair-to-health completion is absent");
assert.ok(
  pairingCompletion.indexOf("await pairAgent(config, identity, options)") <
    pairingCompletion.indexOf("await bridgeHealth(config, identity"),
  "health may not precede persisted pairing",
);
for (const marker of [
  'let signedHealth = "pending"',
  'signedHealth = "observed"',
  "fetchImplementation: options.fetchImplementation",
  "schema: PAIRING_COMPLETION_SCHEMA",
]) assert.ok(pairingCompletion.includes(marker), `pair completion lost ${marker}`);
for (const forbidden of ["pairing_code", "subject_id", "return_to", "location.assign"]) {
  assert.equal(
    pairingCompletion.includes(forbidden),
    false,
    `pair completion leaked browser or secret field ${forbidden}`,
  );
}
assert.ok(
  cli.includes("await pairAgentAndCheckHealth(config, identity"),
  "installed Pair command bypasses its immediate signed health attempt",
);

const pairingStatus = agent.slice(
  agent.indexOf("pub async fn pairing_grant_status("),
  agent.indexOf("pub async fn revoke_pairing_grant("),
);
function assertPairingHealthQuery(value) {
  for (const marker of [
    "b.binding_id=g.pinned_binding_id",
    "b.last_pairing_grant_id=g.grant_id",
    "b.subject_id=g.subject_id",
    "b.player_id=g.player_id",
    "LEFT JOIN paper_raid_bff_agent_health h ON h.binding_id=b.binding_id",
    "g.state='consumed'",
    "g.consumed_at IS NOT NULL",
    "h.assurance='self_declared_unverified'",
    "h.status='healthy'",
    "h.last_seen_at >= g.consumed_at",
    ") AS signed_health_observed_after_pairing",
    'row.get::<bool,_>("signed_health_observed_after_pairing")',
  ]) assert.ok(value.includes(marker), `pairing health query lost ${marker}`);
  assert.equal(
    value.includes("b.grant_id=g.grant_id OR b.last_pairing_grant_id=g.grant_id"),
    false,
    "pairing status retained its ambiguous old/current binding join",
  );
}
assertPairingHealthQuery(pairingStatus);
for (const [name, marker, replacement] of [
  ["unpinned binding", "b.binding_id=g.pinned_binding_id", "b.binding_id=b.binding_id"],
  ["old pairing grant", "b.last_pairing_grant_id=g.grant_id", "b.grant_id=g.grant_id"],
  ["cross subject", "b.subject_id=g.subject_id", "b.subject_id=b.subject_id"],
  ["cross player", "b.player_id=g.player_id", "b.player_id=b.player_id"],
  ["foreign health", "h.binding_id=b.binding_id", "h.binding_id=h.binding_id"],
  ["pre-consumption health", "h.last_seen_at >= g.consumed_at", "h.last_seen_at >= g.created_at"],
  ["degraded health", "h.status='healthy'", "h.status='degraded'"],
]) {
  const mutant = pairingStatus.replace(marker, replacement);
  assert.notEqual(mutant, pairingStatus, `pairing query mutant was not constructed: ${name}`);
  assert.throws(
    () => assertPairingHealthQuery(mutant),
    undefined,
    `pairing health query accepted hostile mutant: ${name}`,
  );
}

console.log("paper-raid-bff practice Agent Bridge boundary: ok");
