import { createHash } from "node:crypto";

export function inboxFingerprint(inbox) {
  return createHash("sha256").update(JSON.stringify(inbox)).digest("hex");
}
