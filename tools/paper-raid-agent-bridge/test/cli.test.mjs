import assert from "node:assert/strict";
import { PassThrough, Writable } from "node:stream";
import test from "node:test";
import { main, readPairingCodeFromInput } from "../src/cli.mjs";

test("pairing code stdin reader returns one line without echoing it", async () => {
  const secret = "prg1.hidden-pair-code";
  const input = new PassThrough();
  let displayed = "";
  const output = new Writable({
    write(chunk, _encoding, callback) {
      displayed += chunk.toString("utf8");
      callback();
    },
  });
  input.end(`${secret}\n`);
  assert.equal(await readPairingCodeFromInput(input, output), secret);
  assert.equal(displayed.includes(secret), false);
});

test("CLI rejects pairing code argv before reading config", async () => {
  await assert.rejects(
    main([
      "pair",
      "--config",
      "/does/not/exist",
      "--pairing-code",
      "prg1.argv-secret",
    ]),
    /unsupported flag --pairing-code/,
  );
});
