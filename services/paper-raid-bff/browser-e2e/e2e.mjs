import assert from "node:assert/strict";
import { readFile, stat } from "node:fs/promises";
import { chromium } from "playwright";

const credentialsPath = process.env.PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE;
const baseUrl = new URL(
  process.env.PAPER_RAID_BFF_BROWSER_BASE_URL || "http://127.0.0.1:17020",
);
const requireLobby = process.env.PAPER_RAID_BFF_BROWSER_REQUIRE_LOBBY !== "0";
const expectedCsp = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'";
const phaseLabels = Object.freeze({
  forming: "Form team / 组队成形",
  preregistering: "Preregister / 预注册",
  researching: "Research / 研究",
  experimenting: "Experiment / 实验",
  drafting: "Draft / 起草",
  integrity_review: "Integrity review / 完整性审查",
  reproducing: "Reproduce / 复现",
  author_approval: "Author approval / 作者批准",
  integrity_hold: "Integrity hold / 完整性挂起",
  submission_ready: "Author Raid complete / 作者远征完成",
  waiting_for_authority: "Waiting for authority / 等待权威状态",
});

assert.ok(credentialsPath, "PAPER_RAID_BFF_BROWSER_CREDENTIALS_FILE is required");
assert.ok(["http:", "https:"].includes(baseUrl.protocol));
assert.equal(baseUrl.username, "");
assert.equal(baseUrl.password, "");
assert.equal(baseUrl.pathname, "/");
assert.equal(baseUrl.search, "");
assert.equal(baseUrl.hash, "");

const credentialStat = await stat(credentialsPath);
assert.equal(credentialStat.isFile(), true);
assert.equal(credentialStat.mode & 0o077, 0, "credentials file must be mode 0600 or stricter");
assert.ok(credentialStat.size > 0 && credentialStat.size <= 256 * 1024);
const credentials = JSON.parse(await readFile(credentialsPath, "utf8"));
assert.deepEqual(
  Object.keys(credentials).sort(),
  ["agent_bindings", "login_keys", "paper_id", "schema"],
);
assert.equal(credentials.schema, "hepta.paper_raid.browser_e2e.credentials.v1");
assert.equal(Array.isArray(credentials.login_keys), true);
assert.equal(credentials.login_keys.length, 3);
assert.equal(new Set(credentials.login_keys).size, 3);
for (const key of credentials.login_keys) {
  assert.equal(typeof key, "string");
  assert.ok(key.length >= 32 && key.length <= 4096);
}
if (credentials.agent_bindings !== null) {
  assert.equal(Array.isArray(credentials.agent_bindings), true);
  assert.equal(credentials.agent_bindings.length, 3);
}
if (requireLobby) assert.equal(credentials.agent_bindings?.length, 3);
if (credentials.paper_id !== null) {
  assert.match(credentials.paper_id, /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
}

function url(path) {
  return new URL(path, baseUrl).href;
}

function isExpectedRedirectAbort(failure) {
  return failure.resourceType === "document" && /ERR_ABORTED/.test(failure.errorText || "");
}

async function storageIsEmpty(page) {
  return page.evaluate(async () => ({
    local: Object.keys(localStorage),
    session: Object.keys(sessionStorage),
    indexed: typeof indexedDB.databases === "function"
      ? (await indexedDB.databases()).map(database => database.name).filter(Boolean)
      : [],
  }));
}

async function signIn(page, loginKey) {
  const response = await page.goto(url("/login"), { waitUntil: "domcontentloaded" });
  assert.ok(response);
  assert.equal(response.status(), 200);
  assert.equal(response.headers()["content-security-policy"], expectedCsp);
  await page.getByLabel("Closed-Alpha access key / 封闭 Alpha 访问密钥").fill(loginKey);
  await page.getByRole("button", { name: "Enter Paper Raid / 进入论文远征" }).click();
  await page.waitForURL(current => {
    const path = new URL(current).pathname;
    return path === "/league/onboarding" || path === "/league";
  });
}

async function registerHuman(page, index, allowLostResponse) {
  const createForm = page.locator("#human-key-create-form");
  if (await createForm.count() === 0) return { generated: false, lostResponse: false };

  const passphrase = `paper-raid-browser-context-${index + 1}-same-key-recovery`;
  const bundlePath = `/tmp/paper-raid-browser-context-${index + 1}.json`;
  await page.getByLabel("Encryption passphrase / 密钥包加密口令").fill(passphrase);
  await page.getByLabel("Confirm passphrase / 确认口令").fill(passphrase);
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Generate, encrypt, and download / 生成、加密并下载" }).click();
  const download = await downloadPromise;
  await download.saveAs(bundlePath);

  // Reload discards the in-memory CryptoKey. Re-importing the downloaded
  // bundle proves registration uses the same key rather than generating a
  // silent replacement.
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.getByText("Local signing key / 本地签名密钥", { exact: true }).click();
  await page.getByLabel("Encrypted key bundle / 加密密钥包").setInputFiles(bundlePath);
  await page.getByLabel("Passphrase / 口令").fill(passphrase);
  await page.getByRole("button", { name: "Decrypt into this tab / 仅解密到当前标签页" }).click();
  await page.locator(".human-key-status").filter({ hasText: "In-memory key loaded" }).waitFor();

  let lostResponse = false;
  if (allowLostResponse) {
    await page.route("**/api/onboarding/human/register", async route => {
      if (lostResponse) {
        await route.continue();
        return;
      }
      const committed = await route.fetch();
      assert.ok(committed.ok(), "the response-loss fixture must abort only after commit");
      lostResponse = true;
      await route.abort("failed");
    });
  }

  await page.getByRole("button", { name: "Register current in-memory key / 注册当前内存密钥" }).click();
  await page.locator("#human-key-create-form").waitFor({ state: "detached" });
  await page.waitForLoadState("domcontentloaded");
  if (allowLostResponse) {
    assert.equal(lostResponse, true, "response-loss fixture did not intercept registration");
    await page.unroute("**/api/onboarding/human/register");
  }
  return { generated: true, lostResponse };
}

async function bindAgent(page, binding, expectedPlayerId) {
  const form = page.locator(".agent-binding-form");
  if (await form.count() === 0) return false;
  await page.locator("details.agent-binding-recovery").evaluate(details => { details.open = true; });
  await page.locator(".bridge-onboarding-primary").waitFor();
  assert.match(
    await page.locator(".bridge-onboarding-primary").textContent() || "",
    /pair[\s\S]*--submit/,
  );
  if (binding === null || binding === undefined) {
    assert.equal(requireLobby, false, "external Agent proof is required for the Lobby gate");
    return false;
  }
  assert.equal(binding.player_id, expectedPlayerId);
  await form.getByLabel("Exact externally signed JSON / 外部 Agent 已签名 JSON").fill(
    JSON.stringify(binding, null, 2),
  );
  await form.getByRole("button", { name: "Verify and bind public proof / 验证并绑定公开证明" }).click();
  await page.waitForURL(current => new URL(current).pathname === "/league");
  return true;
}

async function runContext(browser, index) {
  const context = await browser.newContext({
    acceptDownloads: true,
    locale: "en-US",
    serviceWorkers: "block",
  });
  const page = await context.newPage();
  const consoleErrors = [];
  const pageErrors = [];
  const requestFailures = [];
  page.on("console", message => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("pageerror", error => pageErrors.push(error.message));
  page.on("requestfailed", request => {
    requestFailures.push({
      url: new URL(request.url()).pathname,
      resourceType: request.resourceType(),
      errorText: request.failure()?.errorText || "",
    });
  });

  try {
    await signIn(page, credentials.login_keys[index]);
    const sessionResponse = await context.request.get(url("/api/session"), {
      headers: { accept: "application/json" },
    });
    assert.equal(sessionResponse.status(), 200);
    const session = await sessionResponse.json();
    assert.match(session.player_id, /^[0-9a-f-]{36}$/i);

    const registration = await registerHuman(page, index, index === 0);
    await bindAgent(page, credentials.agent_bindings?.[index] ?? null, session.player_id);

    const path = new URL(page.url()).pathname;
    if (requireLobby) {
      assert.equal(path, "/league");
      await page.getByRole("heading", { name: "Research Lobby" }).waitFor();
      const reviewBoundary = await context.request.get(url("/league/review"), {
        headers: { accept: "text/html" },
      });
      assert.equal(reviewBoundary.status(), 403, "Author identities must not enter Reviewer Raid");
      for (const queueForm of await page.locator("form.queue-form").all()) {
        const authorized = (await queueForm.getAttribute("data-authorized-roles") || "")
          .split(",")
          .filter(Boolean);
        assert.deepEqual(authorized, session.author_roles);
        const roleChoices = await queueForm.locator('input[name="roles"]').evaluateAll(inputs =>
          inputs.map(input => ({ value: input.value, checked: input.checked })),
        );
        assert.deepEqual(roleChoices.map(choice => choice.value), authorized);
        assert.ok(roleChoices.some(choice => choice.checked));
      }
      const rotation = page.locator(".agent-rotation-form");
      await rotation.waitFor();
      if (credentials.agent_bindings?.[index]) {
        assert.equal(await rotation.getAttribute("data-agent-id"), credentials.agent_bindings[index].agent_id);
        assert.equal(await rotation.getAttribute("data-old-key-id"), credentials.agent_bindings[index].agent_key_id);
      }
    } else {
      assert.ok(path === "/league" || path === "/league/onboarding");
    }

    const storage = await storageIsEmpty(page);
    assert.deepEqual(storage, { local: [], session: [], indexed: [] });
    const cookies = await context.cookies(baseUrl.href);
    const sessionCookie = cookies.find(cookie => cookie.name === "paper_raid_session");
    assert.ok(sessionCookie);
    assert.equal(sessionCookie.httpOnly, true);
    assert.equal(sessionCookie.sameSite, "Strict");

    if (credentials.paper_id !== null && path === "/league") {
      const roomResponse = await page.goto(url(`/league/papers/${credentials.paper_id}`), {
        waitUntil: "domcontentloaded",
      });
      assert.ok(roomResponse);
      assert.equal(roomResponse.status(), 200);
      for (const heading of [
        "Contribution / 贡献",
        "Evaluations / 评估",
        "Reproductions / 复现",
        "Appeals / 申诉",
        "Resolve Appeal / 裁决申诉",
      ]) {
        await page.getByRole("heading", { name: heading }).waitFor();
      }
      await page.locator('form[data-command="submit_appeal"] .local-sign').waitFor();
      const challengeRules = page.locator(".challenge-ruleset-panel");
      await challengeRules.waitFor();
      assert.equal(await challengeRules.getAttribute("data-ruleset-enforcement"), "authoritative_v1");
      await challengeRules.getByText("Victory conditions / 胜利条件").waitFor();
      await challengeRules.locator(".challenge-countdown").waitFor();
      await challengeRules.locator("[data-challenge-outcome]").waitFor();
      for (const outcomeForm of await challengeRules.locator(".challenge-outcome-form").all()) {
        assert.equal(await outcomeForm.locator('[name="paper_id"]').count(), 0);
        assert.equal(await outcomeForm.locator('[name="expected_version"]').count(), 0);
        assert.equal(await outcomeForm.locator('[name="payload"]').count(), 0);
      }
      await page.locator(".raid-command-center").waitFor();
      const developerTools = page.locator("details.developer-tools");
      await developerTools.waitFor();
      assert.equal(await developerTools.getAttribute("open"), null);
      const commandForms = page.locator("form.command-form");
      assert.ok(await commandForms.count() > 0);
      for (let commandIndex = 0; commandIndex < await commandForms.count(); commandIndex += 1) {
        assert.equal(await commandForms.nth(commandIndex).evaluate(form => Boolean(form.closest(".developer-tools"))), true);
      }
      for (const selector of [
        ".create-experiment-plan-form",
        ".create-evidence-card-form",
        ".create-citation-record-form",
        ".create-claim-record-form",
        ".create-run-record-form",
        ".input-manifest-wizard-form",
        ".draft-manifest-wizard-form",
        ".acquire-section-lease-form",
        ".bridge-proposal-task-form",
        ".human-proposal-decision-form",
        ".create-section-revision-form",
        ".section-review-form",
        ".merge-section-form",
        ".freeze-contribution-ledger-form",
        ".review-evaluation-draft-form",
        ".review-attestation-form",
        ".review-finalize-form",
        ".review-reproduction-form",
      ]) {
        const guidedForms = page.locator(selector);
        for (let formIndex = 0; formIndex < await guidedForms.count(); formIndex += 1) {
          assert.equal(await guidedForms.nth(formIndex).evaluate(form => Boolean(form.closest(".developer-tools"))), false);
          assert.equal(await guidedForms.nth(formIndex).locator('textarea[name="payload"]').count(), 0);
          assert.equal(await guidedForms.nth(formIndex).locator(
            'input[name="manifest_id"], input[name="evidence_card_id"], input[name="citation_id"], input[name="experiment_plan_id"], input[name="run_record_id"], input[name="claim_id"], input[name="lease_id"], input[name="proposal_id"], input[name="section_revision_id"], input[name="expected_version"], input[name="fencing_token"]',
          ).count(), 0);
        }
      }
      const artifactLinks = page.locator(
        `a[href^="/api/papers/${credentials.paper_id}/artifacts/"]`,
      );
      for (let linkIndex = 0; linkIndex < await artifactLinks.count(); linkIndex += 1) {
        const href = await artifactLinks.nth(linkIndex).getAttribute("href");
        assert.match(href, /\/artifacts\/sha256:[0-9a-f]{64}$/);
      }
      for (const command of [
        "create_nakama_research_session_control",
        "resume_nakama_research_session_control",
        "replace_nakama_research_session_roster_control",
        "complete_nakama_research_session_control",
      ]) {
        const control = page.locator(`form.command-form[data-command="${command}"]`);
        await control.waitFor();
        assert.equal(await control.getAttribute("data-resource-id"), "");
        assert.equal(await control.locator('[name="child_id"]').count(), 0);
      }
      assert.deepEqual(
        await page.locator('.artifact-form select[name="media_type"] option').allTextContents(),
        [
          "application/x-bibtex",
          "text/csv; charset=utf-8",
          "application/json",
          "text/markdown; charset=utf-8",
          "application/pdf",
          "text/x-python; charset=utf-8",
          "image/svg+xml",
          "text/plain; charset=utf-8",
          "application/octet-stream",
          "application/zip",
          "application/gzip",
        ],
      );
      const finalityStatus = await page.locator(".hero .status").first().textContent();
      assert.match(finalityStatus || "", /(pending_finality|verified_finality)/);
      assert.match(
        finalityStatus || "",
        /(Verification pending \/ 等待终局验证|Verified scientific finality \/ 科学终局已验证)/,
      );
      assert.equal(await page.locator(".eligibility-grid li").count(), 4);
      await page.locator('.live-connection[data-state="live"]').waitFor({ timeout: 15000 });
      const renderedPhase = await page.locator(".live-phase").textContent();
      const materializationSummary = page.locator(".materialization-summary");
      await materializationSummary.waitFor();
      assert.equal(
        await materializationSummary.evaluate(node => Boolean(node.closest(".developer-tools"))),
        false,
      );
      const authorityState = await page.evaluate(async paperId => {
        const response = await fetch(`/api/papers/${encodeURIComponent(paperId)}/timeline?after_cursor=0&after_sequence=0`, {
          credentials: "same-origin",
          headers: { accept: "application/json" },
        });
        const value = await response.json();
        const currentRevisionId = value.paper_room?.paper?.current_revision_id ?? null;
        const currentRevision = (value.paper_room?.paper_revisions ?? []).find(
          revision => revision.revision_id === currentRevisionId,
        ) ?? null;
        return {
          ok: response.ok,
          phase: value.paper_room?.paper?.phase ?? "waiting_for_authority",
          materializationRoot: currentRevision?.section_materialization_root ?? null,
          releaseMaterializationRoot:
            currentRevision?.release_candidate?.section_materialization_root ?? null,
        };
      }, credentials.paper_id);
      assert.equal(authorityState.ok, true);
      assert.equal(
        renderedPhase,
        `Phase / 阶段: ${phaseLabels[authorityState.phase] || authorityState.phase}`,
      );
      if (authorityState.materializationRoot !== null) {
        assert.match(authorityState.materializationRoot, /^sha256:[0-9a-f]{64}$/);
        assert.match(
          await materializationSummary.textContent() || "",
          new RegExp(authorityState.materializationRoot.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
        );
      }
      if (authorityState.releaseMaterializationRoot !== null) {
        assert.equal(
          authorityState.releaseMaterializationRoot,
          authorityState.materializationRoot,
        );
      }
      assert.ok(await page.locator(".live-participants li").count() >= 1);
      const cursorState = await page.evaluate(paperId => JSON.parse(
        sessionStorage.getItem(`hepta.paper-raid.live-cursor.v1:${paperId}`),
      ));
      assert.equal(Number.isSafeInteger(cursorState.hepta) && cursorState.hepta >= 0, true);
      assert.deepEqual(Object.keys(cursorState), ["hepta"]);
    }

    const unexpectedFailures = requestFailures.filter(failure => {
      if (isExpectedRedirectAbort(failure)) return false;
      return !(index === 0
        && registration.lostResponse
        && failure.url === "/api/onboarding/human/register");
    });
    assert.deepEqual(consoleErrors, []);
    assert.deepEqual(pageErrors, []);
    assert.deepEqual(unexpectedFailures, []);
    return {
      contextIndex: index,
      playerId: session.player_id,
      sessionCookie: sessionCookie.value,
      generatedHumanKey: registration.generated,
      recoveredLostResponse: registration.lostResponse,
    };
  } finally {
    await context.close();
  }
}

const browser = await chromium.launch({ headless: true });
try {
  const results = await Promise.all([0, 1, 2].map(index => runContext(browser, index)));
  assert.equal(new Set(results.map(result => result.playerId)).size, 3);
  assert.equal(new Set(results.map(result => result.sessionCookie)).size, 3);
  assert.equal(results[0].generatedHumanKey ? results[0].recoveredLostResponse : true, true);
  console.log(JSON.stringify({
    schema: "hepta.paper_raid.browser_e2e.result.v1",
    isolated_contexts: 3,
    distinct_players: 3,
    distinct_http_only_sessions: 3,
    browser_storage_empty: true,
    response_loss_recovery_exercised: results[0].generatedHumanKey,
    lobby_reached: requireLobby,
    agent_rotation_continuity_ui_checked: requireLobby,
    paper_room_checked: credentials.paper_id !== null,
  }));
} finally {
  await browser.close();
}
