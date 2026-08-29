import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const [app, browser, html, http, library, metrics] = await Promise.all([
  readFile(new URL("src/app.rs", root), "utf8"),
  readFile(new URL("src/browser.js", root), "utf8"),
  readFile(new URL("src/html.rs", root), "utf8"),
  readFile(new URL("src/practice_http.rs", root), "utf8"),
  readFile(new URL("src/lib.rs", root), "utf8"),
  readFile(new URL("src/metrics.rs", root), "utf8"),
]);

assert.ok(app.includes(".merge(crate::practice_http::router())"));
assert.ok(library.includes("pub mod practice_http;"));
for (const route of [
  "/league/practice",
  "/api/practice/session",
  "/api/practice/start",
  "/api/practice/advance",
  "/api/practice/abandon",
]) assert.ok(http.includes(`\"${route}\"`), `missing route ${route}`);
for (const route of [
  "/league/practice",
  "/api/practice/session",
  "/api/practice/start",
  "/api/practice/advance",
  "/api/practice/abandon",
]) assert.ok(metrics.includes(`\"${route}\"`), `missing bounded metric route ${route}`);

const productionHttp = http.split("#[cfg(test)]", 1)[0];
for (const marker of [
  "#[serde(deny_unknown_fields)]",
  "state.session(headers).await",
  "state.sessions.consume_csrf(headers, &session).await",
  "resolve_exact_active_binding",
  "subject_id=$1 AND player_id=$2",
  "FOR UPDATE",
  "AND binding_id=$16 AND version=$17",
  "persist_transition(&mut transaction, &practice, &event).await?",
  "transaction.commit().await?",
  "PracticeEligibilityV1",
]) assert.ok(productionHttp.includes(marker), `missing fail-closed HTTP/store marker: ${marker}`);
for (const forbidden of [
  "forward_command(",
  "create_paper",
  "activate_challenge",
  "finalize_paper",
  "qualification_id",
  "release_candidate_hash",
  "paper_bundle_hash",
]) assert.equal(productionHttp.includes(forbidden), false, `practice HTTP crossed authority boundary: ${forbidden}`);

const practiceHtml = html.slice(
  html.indexOf("pub(crate) fn practice("),
  html.indexOf("pub fn onboarding("),
);
assert.ok(practiceHtml.length > 1000, "practice page renderer is absent");
for (const notice of [
  "PRACTICE_UNRANKED",
  "15–20",
  "No scientific finality or Challenge activation",
  "No qualification, ranking, score, reward, or economic authority",
  "No automatic submission",
  "Completion is not portable",
  "Pair exactly one active Agent Bridge first",
]) assert.ok(practiceHtml.includes(notice), `missing player boundary copy: ${notice}`);
assert.ok(
  practiceHtml.includes('href="/league?return_to=%2Fleague%2Fpractice"'),
  "practice pairing does not preserve its fixed no-secret return path",
);
for (const forbidden of [
  "practice_session_id",
  "subject_id",
  "player_id",
  "binding_id",
  "bridge_task_id",
  "sha256:",
  "<textarea",
]) assert.equal(practiceHtml.includes(forbidden), false, `practice page leaks raw protocol field: ${forbidden}`);

const waiting = practiceHtml.slice(
  practiceHtml.indexOf("PracticeStageV1::ExperimentWaitingBridge"),
  practiceHtml.indexOf("PracticeStageV1::ExperimentInterpretation"),
);
assert.ok(waiting.includes("Check Agent status"));
assert.ok(waiting.includes("browser cannot impersonate an Agent"));
assert.equal(waiting.includes("<form"), false, "browser must not submit an Agent transition");

const practiceBrowser = browser.slice(
  browser.indexOf("const PRACTICE_CHOICES"),
  browser.indexOf('document.addEventListener("DOMContentLoaded"'),
);
for (const marker of [
  '"/api/practice/start"',
  '"/api/practice/advance"',
  '"/api/practice/abandon"',
  "expected_version: practiceVersion(form)",
  "action: { action, choice: selected }",
  "event.preventDefault()",
  "if (response.ok) window.setTimeout(() => window.location.reload(), 250)",
]) assert.ok(practiceBrowser.includes(marker), `missing explicit practice browser marker: ${marker}`);
for (const forbidden of [
  "practice_session_id",
  "subject_id",
  "player_id",
  "binding_id",
  "bridge_task_id",
  "request_hash",
  ".click(",
  ".submit(",
  "requestSubmit(",
  "dispatchEvent(",
  "/api/agent-bridge/",
]) assert.equal(practiceBrowser.includes(forbidden), false, `practice browser escaped boundary: ${forbidden}`);

const pairingBrowser = browser.slice(
  browser.indexOf("const AGENT_PAIRING_RETURN_PATHS"),
  browser.indexOf("function bindAgentBinding()"),
);
for (const marker of [
  'new Set(["/league/practice"])',
  'page.searchParams.getAll("return_to")',
  'grant.state === "consumed"',
  "grant.signed_health_observed_after_pairing === true",
  "requestGeneration !== statusRequestGeneration",
  "status?.grantId === activePairingGrantId",
  "status?.healthConfirmed",
  "window.location.assign(returnTarget)",
]) assert.ok(pairingBrowser.includes(marker), `missing pairing return boundary: ${marker}`);
for (const forbidden of [
  "paper-raid-agent-bridge:",
  "pairing_code=",
  "localStorage",
  "sessionStorage",
]) assert.equal(pairingBrowser.includes(forbidden), false, `pairing return leaks authority: ${forbidden}`);

const readyIndex = browser.indexOf('document.documentElement.dataset.paperRaidBindingsReady = "true"');
const bindIndex = browser.indexOf("bindPractice();");
assert.ok(bindIndex >= 0 && bindIndex < readyIndex, "practice controls are not bound before readiness");

console.log("paper-raid-bff ordinary-player practice UI boundary: ok");
