#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

: "${HEPTA_TEST_DATABASE_URL:?HEPTA_TEST_DATABASE_URL is required for the live PostgreSQL release gate}"
cargo_gate="${HEPTA_CARGO_LOCK_FILE:-/tmp/trnm-paper-raid-cargo-gate.lock}"

cargo_locked() {
  flock -n "$cargo_gate" env -u SOURCE_DATE_EPOCH cargo "$@"
}

runtime_binary_sha256="$(jq -er '
  [.components[]? | select(.type == "file")] as $files
  | if (($files | length) == 1
      and $files[0].name == "/usr/local/bin/hepta-research-league"
      and ($files[0].hashes | length) == 1
      and $files[0].hashes[0].alg == "SHA-256")
    then $files[0].hashes[0].content
    else error("Hepta SBOM runtime binding is not canonical")
    end
' deploy/hepta-research-league/hepta-research-league.cdx.json)"
python3 scripts/verify-hepta-research-league-sbom.py \
  --sbom deploy/hepta-research-league/hepta-research-league.cdx.json \
  --runtime-sha256 "$runtime_binary_sha256" \
  --dockerfile services/hepta-research-league/Dockerfile \
  --cargo-lock services/hepta-research-league/docker/Cargo.lock \
  --rust-toolchain services/hepta-research-league/docker/rust-toolchain.manifest
bash scripts/check-hepta-research-league-release-structure.sh
python3 scripts/check-hepta-route-openapi-parity.py

python3 - <<'PY'
import yaml
import json
import hashlib
import pathlib
import re
import stat
import tomllib

for path in (
    "docs/openapi/hepta-research-league-v1.yaml",
    "docs/openapi/hepta-paper-raid-v2.yaml",
):
    with open(path, encoding="utf-8") as stream:
        yaml.safe_load(stream)

with open("docs/openapi/vendor/integration-artifact-bundle-v1.schema.json", encoding="utf-8") as stream:
    json.load(stream)
with open("docs/sdk-fixtures/integration-paper-raid-artifact-bundle-v1.json", encoding="utf-8") as stream:
    bundle = json.load(stream)
    assert bundle["schema"] == "paper-raid.artifact-bundle.v1"

vendor_root = pathlib.Path("vendor")
with open(vendor_root / "trnm-chain-vendor-manifest.json", encoding="utf-8") as stream:
    provenance = json.load(stream)
assert provenance["schema"] == "hepta.vendor.trnm_chain_crates.v1"
assert provenance["source_repository"] == "https://github.com/TrillionniumFoundation/Trillionnium-Chain.git"
assert provenance["source_commit"] == "e73d1a930991f0e308bf72854b334b6191c7fcc3"
assert provenance["license"] == "MIT"
assert provenance["update_policy"].strip()
assert provenance["packaging_context"] == {
    "workspace_inherited_crates": ["trnm-finality-types", "trnm-finality-verifier"],
    "workspace_inherited_fields": ["edition", "license", "authors"],
    "source_workspace_values": {
        "edition": "2021",
        "license": "MIT",
        "authors": ["Trillionnium Contributors"],
    },
    "vendor_workspace_values": {
        "edition": "2021",
        "license": "MIT",
        "authors": ["Qi Team"],
    },
    "accepted_metadata_difference": "Only Cargo package author metadata inherits from the embedding Hepta workspace; source bytes, runtime behavior, edition, license, versions, and package names remain identical to the pinned Chain subtrees.",
}
assert set(provenance["crates"]) == {
    "trnm-finality-types",
    "trnm-finality-verifier",
    "trnm-research-protocol",
}
expected_source_trees = {
    "trnm-finality-types": "31d7e3a141332055232e2d82260bdaa7bbf62d14",
    "trnm-finality-verifier": "ad3d2e1aacd5e6e6b3f0c520eb7f1dbfbebbf536",
    "trnm-research-protocol": "223cd8adcce3e6a3ab7bed24ffb919e0ae1fda56",
}

def git_object_id(kind, data):
    header = kind + b" " + str(len(data)).encode() + b"\0"
    return hashlib.sha1(header + data).digest()

def git_tree_id(root):
    entries = []
    for path in sorted(root.iterdir(), key=lambda item: item.name.encode()):
        mode = stat.S_IMODE(path.lstat().st_mode)
        if path.is_symlink():
            raise AssertionError(f"vendored symlink forbidden: {path}")
        if path.is_dir():
            git_mode = b"40000"
            object_id = git_tree_id(path)
        elif path.is_file():
            assert mode == 0o644, f"vendored file must be Git mode 100644: {path} ({mode:o})"
            git_mode = b"100644"
            object_id = git_object_id(b"blob", path.read_bytes())
        else:
            raise AssertionError(f"vendored special file forbidden: {path}")
        entries.append(git_mode + b" " + path.name.encode() + b"\0" + object_id)
    return git_object_id(b"tree", b"".join(entries))

credential_markers = (
    b"-----BEGIN PRIVATE KEY-----",
    b"-----BEGIN OPENSSH PRIVATE KEY-----",
    b"github_pat_",
    b"ghp_",
    b"AKIA",
)
for crate_name, crate in provenance["crates"].items():
    crate_root = vendor_root / crate_name
    assert crate["version"] == "0.1.0"
    assert crate["source_git_tree"] == expected_source_trees[crate_name]
    assert re.fullmatch(r"[0-9a-f]{64}", crate["content_manifest_sha256"])
    for path in crate_root.rglob("*"):
        assert not path.is_symlink(), f"vendored symlink forbidden: {path}"
        assert path.name != ".git", f"vendored Git metadata forbidden: {path}"
        assert not re.search(r"(^|[._-])(credentials?|id_rsa|\.env)([._-]|$)", path.name, re.I), path
    actual_files = sorted(
        path.relative_to(crate_root).as_posix()
        for path in crate_root.rglob("*")
        if path.is_file()
    )
    assert actual_files == sorted(crate["files"]), f"vendored file set drift: {crate_name}"
    digest_lines = []
    for relative_path in actual_files:
        data = (crate_root / relative_path).read_bytes()
        assert not any(marker in data for marker in credential_markers), relative_path
        digest = hashlib.sha256(data).hexdigest()
        assert digest == crate["files"][relative_path], f"vendored file drift: {crate_name}/{relative_path}"
        digest_lines.append(f"{digest}  {relative_path}\n")
    content_manifest = hashlib.sha256("".join(digest_lines).encode()).hexdigest()
    assert content_manifest == crate["content_manifest_sha256"], f"vendored tree drift: {crate_name}"
    assert git_tree_id(crate_root).hex() == expected_source_trees[crate_name], f"vendored Git tree drift: {crate_name}"

with open("Cargo.toml", "rb") as stream:
    workspace_dependencies = tomllib.load(stream)["workspace"]["dependencies"]
for crate_name in provenance["crates"]:
    dependency = workspace_dependencies[crate_name]
    assert dependency["path"] == f"vendor/{crate_name}"
    assert "git" not in dependency and "rev" not in dependency
lock_text = pathlib.Path("Cargo.lock").read_text(encoding="utf-8")
assert "Trillionnium-Chain.git" not in lock_text

with open("deploy/hepta-research-league/compose.yaml", encoding="utf-8") as stream:
    compose = yaml.safe_load(stream)
hepta = compose["services"]["hepta"]
assert hepta["healthcheck"]["test"] == [
    "CMD",
    "/usr/local/bin/hepta-research-league",
    "--probe-ready",
]
assert hepta["image"] == "${HEPTA_IMAGE:?immutable HEPTA_IMAGE digest is required}"
for required_env in (
    "HEPTA_NAKAMA_CONTROL_ISSUER_KEY_ID",
    "HEPTA_NAKAMA_CONTROL_ED25519_SEED_BASE64",
    "HEPTA_NAKAMA_BASE_URL",
    "HEPTA_NAKAMA_RUNTIME_HTTP_KEY",
    "HEPTA_CONSUMER_EDGE_ISSUER",
    "HEPTA_CONSUMER_EDGE_AUDIENCE",
    "HEPTA_CONSUMER_EDGE_ISSUER_KEY_ID",
    "HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEY_BASE64",
    "HEPTA_CONSUMER_EDGE_ED25519_PUBLIC_KEYS_JSON",
    "TRNM_NAKAMA_AUTHORITY_KEY_ID",
    "TRNM_NAKAMA_AUTHORITY_PUBLIC_KEY_BASE64",
    "TRNM_NAKAMA_AUTHORITY_PUBLIC_KEYS_JSON",
    "HEPTA_FINALITY_MODE",
):
    assert required_env in hepta["environment"]

PY

bash -n scripts/build-hepta-research-league-image.sh
python3 -c 'compile(open("scripts/generate-hepta-research-league-sbom.py", encoding="utf-8").read(), "scripts/generate-hepta-research-league-sbom.py", "exec")'
python3 -c 'compile(open("scripts/verify-hepta-research-league-sbom.py", encoding="utf-8").read(), "scripts/verify-hepta-research-league-sbom.py", "exec")'

test "$(sha256sum docs/openapi/vendor/integration-artifact-bundle-v1.schema.json | cut -d' ' -f1)" = \
  "aba8fd6d1059c59f63cdb258a2e507de1bed3ff74f935b7dd0214e4640ad9bb6"
test "$(sha256sum docs/sdk-fixtures/integration-paper-raid-artifact-bundle-v1.json | cut -d' ' -f1)" = \
  "9c2234c2c677307b262faa6be52a9855958003212fa0733e3c991af86df1555d"
test "$(sha256sum docs/sdk-fixtures/hepta-paper-collaboration-v3.json | cut -d' ' -f1)" = \
  "6a8c20dabaf2ff723a1db7e9742bcbd24f4d18bb17938f3695cac099c29d84ce"
test "$(sha256sum docs/sdk-fixtures/hepta-paper-review-v4.json | cut -d' ' -f1)" = \
  "b25dcbfcf3f9d5830ab8d2b32bdd36b2c073bca8a0bff1da6ba05fb85f6f17b4"
test "$(sha256sum docs/sdk-fixtures/trnm-nakama-research-control-golden-vectors-v2.json | cut -d' ' -f1)" = \
  "65f7869261f452dadbba9228dccafcba2fe4e6fa12875f96628d0395021197ea"
test "$(sha256sum docs/openapi/vendor/nakama-research-control-v2/spec.md | cut -d' ' -f1)" = \
  "5b6c51b2dc81307897b09b0cb16a233f88adddd82e9c1a8d7d918c82e9972839"
test "$(sha256sum docs/openapi/vendor/nakama-research-control-v2/control.schema.json | cut -d' ' -f1)" = \
  "52463e90e4bd6b5b8c55ada40a2b2ec76b494f3db721f2271d06e34647621f0a"
test "$(sha256sum docs/openapi/vendor/nakama-research-control-v2/rpc-request.schema.json | cut -d' ' -f1)" = \
  "709bd9749de8cdbf29b04312b6f0de982c4b9c383de189838d63760d39960615"
test "$(sha256sum docs/openapi/vendor/nakama-research-control-v2/rpc-response.schema.json | cut -d' ' -f1)" = \
  "cbb49b68793224494617b78580e164e716b47ec01deb29e49e381000134304c0"
jq -e \
  '.fixture_version == "hepta_sdk_fixtures_v1"
   and .agent_execution_mode == "external_only"
   and .top_level_modules == ["hepta","nakama","trnm"]
   and .invariants.nakama_match_authorization_is_ed25519_signed == true
   and .invariants.nakama_match_authorization_bearer_token == false' \
  docs/sdk-fixtures/hepta-research-league-v1.json >/dev/null

if rg -n \
  'model[_ -]?api[_ -]?key hosting|platform-owned agent|hosted agent loop|agent_execution_mode["=: ]+internal' \
  services/hepta-research-league docs/openapi/hepta-research-league-v1.yaml \
  docs/sdk-fixtures/hepta-research-league-v1.json; then
  echo "forbidden platform-hosted Agent semantics found" >&2
  exit 1
fi

test "$(rg -o '\"hepta\"|\"nakama\"|\"trnm\"' \
  docs/sdk-fixtures/hepta-research-league-v1.json | sort -u | wc -l)" -eq 3

node scripts/verify-hepta-paper-raid-v2-fixture.mjs \
  docs/sdk-fixtures/hepta-paper-raid-v2.json
node scripts/verify-hepta-paper-collaboration-v3-fixture.mjs \
  docs/sdk-fixtures/hepta-paper-collaboration-v3.json
node scripts/verify-hepta-paper-review-v4-fixture.mjs \
  docs/sdk-fixtures/hepta-paper-review-v4.json

cargo_locked fmt --all -- --check
cargo_locked test --locked -p hepta-research-league
cargo_locked check --locked --workspace
cargo_locked clippy --locked --workspace --all-targets -- -D warnings
