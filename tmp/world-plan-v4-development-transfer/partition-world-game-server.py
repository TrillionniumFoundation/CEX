from __future__ import annotations

from pathlib import Path
import hashlib
import json
import sys

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("world")
source_path = root / "trillionnium/crates/trnm-game-server/src/lib.rs"
parts_dir = source_path.parent / "lib_parts"
source = source_path.read_text(encoding="utf-8")

header_marker = "use axum::extract::DefaultBodyLimit;\n"
if source.count(header_marker) != 1:
    raise SystemExit(f"expected one header marker, found {source.count(header_marker)}")
header_start = source.index(header_marker)
header = source[:header_start].rstrip() + "\n\n"
body = source[header_start:]

specs = [
    ("authority_foundation.rs", "authority-foundation", "use axum::extract::DefaultBodyLimit;\n"),
    ("configuration_and_migrations.rs", "persistence-configuration", "pub struct AppStateConfig {\n"),
    ("terminal_recovery.rs", "terminal-recovery", "struct ApiError {\n"),
    ("operations_boundary.rs", "operations-boundary", "pub fn validate_operations_bind_addr"),
    ("fleet.rs", "fleet-fencing", "async fn lock_current_fleet_epoch(\n"),
    ("identity.rs", "identity-and-validation", "fn session_header(headers: &HeaderMap)"),
    ("application.rs", "application-adapters", "fn mission_for_map(map_id: &str)"),
    ("http_api.rs", "http-routing", "pub fn build_router(state: AppState)"),
    ("readiness.rs", "readiness", "async fn health() -> &'static str"),
    ("product_api.rs", "product-http", "async fn connect_campaign(\n"),
    ("actor_runtime.rs", "actor-runtime", "pub fn production_authority_tick_interval()"),
    ("persistence_tail.rs", "campaign-persistence", "async fn persist_campaign(\n"),
    ("tests.rs", "invariant-tests", "#[cfg(test)]\n"),
]

positions = []
for filename, _owner, marker in specs:
    count = body.count(marker)
    if count != 1:
        raise SystemExit(f"{filename}: expected one marker {marker!r}, found {count}")
    positions.append(body.index(marker))
if positions != sorted(positions) or len(set(positions)) != len(positions):
    raise SystemExit("partition markers are not strictly ordered")

parts_dir.mkdir(parents=True, exist_ok=True)
for old in parts_dir.glob("*.rs"):
    old.unlink()
records = []
for index, ((filename, owner, marker), start) in enumerate(zip(specs, positions)):
    end = positions[index + 1] if index + 1 < len(positions) else len(body)
    data = body[start:end]
    if not data.endswith("\n"):
        data += "\n"
    path = parts_dir / filename
    path.write_text(data, encoding="utf-8")
    raw = data.encode("utf-8")
    records.append({
        "path": f"lib_parts/{filename}",
        "owner": owner,
        "start_marker": marker.rstrip(),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
    })

includes = "".join(
    f"// Owns: {owner}. Textually included at crate root to preserve the reviewed API.\n"
    f"include!(\"lib_parts/{filename}\");\n\n"
    for filename, owner, _marker in specs
)
wrapper = (
    header
    + "// Correctness-critical implementation is partitioned into directly reviewed source\n"
      "// fragments. These are ordinary Git-tracked Rust files; no build script rewrites\n"
      "// runtime semantics and no generated source is compiled.\n\n"
    + includes.rstrip()
    + "\n"
)
source_path.write_text(wrapper, encoding="utf-8")
manifest = {
    "schema": "trnm_game_server_direct_source_partition_v1",
    "source": "src/lib.rs",
    "semantic_generation": False,
    "textual_include_reason": "preserve the existing crate-root API while reducing correctness review blast radius",
    "dependency_direction": ["protocol-domain", "application", "adapters", "runtime-bootstrap"],
    "parts": records,
}
(parts_dir / "manifest.json").write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)

if "trnm_game_server_lib_generated.rs" in wrapper:
    raise SystemExit("wrapper retained generated-source authority")
ordered = "".join(
    (parts_dir / filename).read_text(encoding="utf-8")
    for filename, _owner, _marker in specs
)
if ordered != body:
    raise SystemExit("partition did not preserve exact source body")
if max(record["bytes"] for record in records) >= len(body.encode("utf-8")) // 3:
    raise SystemExit("partition failed to reduce the catch-all review radius")
