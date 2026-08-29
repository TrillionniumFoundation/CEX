#!/usr/bin/env node
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { writeReleaseArtifacts } from "../src/release.mjs";

function argumentsFor(argv) {
  const flags = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index];
    const value = argv[index + 1];
    if (!name?.startsWith("--") || value === undefined || value.startsWith("--")) {
      throw new Error("usage: build-release --version SEMVER --sequence N --out DIRECTORY");
    }
    if (flags.has(name)) throw new Error(`duplicate flag ${name}`);
    flags.set(name, value);
  }
  for (const name of flags.keys()) {
    if (!["--version", "--sequence", "--out"].includes(name)) {
      throw new Error(`unsupported flag ${name}`);
    }
  }
  return flags;
}

const flags = argumentsFor(process.argv.slice(2));
const version = flags.get("--version");
const sequence = Number(flags.get("--sequence"));
const outputDirectory = flags.get("--out");
if (!version || !Number.isSafeInteger(sequence) || !outputDirectory) {
  throw new Error("usage: build-release --version SEMVER --sequence N --out DIRECTORY");
}
const sourceRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const result = await writeReleaseArtifacts({
  sourceRoot,
  outputDirectory,
  version,
  sequence,
});
process.stdout.write(
  `${JSON.stringify({
    package: result.packagePath,
    manifest: result.manifestPath,
    signature_required: `${result.manifestPath}.sig`,
  })}\n`,
);
