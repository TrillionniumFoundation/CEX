import assert from "node:assert/strict";
import test from "node:test";
import { inboxFingerprint } from "../src/inbox.mjs";

test("inbox fingerprint changes only when the dedicated response changes", () => {
  const first = {
    schema: "hepta.paper_raid.agent_bridge.inbox.v1",
    tasks: [{ task_id: "task.1" }],
  };
  assert.equal(inboxFingerprint(first), inboxFingerprint({ ...first }));
  assert.notEqual(
    inboxFingerprint(first),
    inboxFingerprint({ ...first, tasks: [{ task_id: "task.2" }] }),
  );
});
