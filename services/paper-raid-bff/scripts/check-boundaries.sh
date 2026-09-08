#!/usr/bin/env bash
set -euo pipefail

service_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
repo_root=$(cd "$service_root/../.." && pwd -P)
scratch_dir=$(mktemp -d)

cleanup() {
  case "$scratch_dir" in
    /tmp/tmp.*) rm -rf -- "$scratch_dir" ;;
    *) echo "refusing to remove unexpected boundary scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT INT TERM

require_file() {
  [[ -f "$1" ]] || {
    echo "required boundary file is absent: $1" >&2
    exit 1
  }
}

require_executable() {
  [[ -x "$1" ]] || {
    echo "required boundary executable is absent: $1" >&2
    exit 1
  }
}

require_marker() {
  local file=$1
  local marker=$2
  require_file "$file"
  rg -q --fixed-strings -- "$marker" "$file" || {
    echo "required boundary marker is absent: file=$file marker=$marker" >&2
    exit 1
  }
}

for command in bash cargo git jq node python3 rg rustc sed; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "required boundary command is absent: $command" >&2
    exit 1
  }
done

python3 "$repo_root/scripts/check-rust-toolchain-convergence.py" >/dev/null

bff_dockerfile="$service_root/Dockerfile"
accessctl_dockerfile="$service_root/Dockerfile.accessctl"
accessctl_sbom="$service_root/docker/accessctl.sbom.cdx.json"
accessctl_sbom_binder="$service_root/scripts/bind-accessctl-runtime-sbom.sh"
browser="$service_root/src/browser.js"
html="$service_root/src/html.rs"
hepta="$service_root/src/hepta.rs"
app="$service_root/src/app.rs"
cas="$service_root/src/cas.rs"

for path in \
  "$bff_dockerfile" \
  "$accessctl_dockerfile" \
  "$accessctl_sbom" \
  "$browser" \
  "$html" \
  "$hepta" \
  "$app" \
  "$cas"; do
  require_file "$path"
done
require_executable "$accessctl_sbom_binder"

# Paper Raid is an edge/BFF. It must not regain gameplay, matching, platform
# Agent, or privileged Hepta/Nakama authority.
for pattern in \
  'LeagueState' \
  '/battle' \
  'forward_to_cex_task' \
  'Alice fallback' \
  'keyword_score' \
  'NAKAMA_OPERATOR_TOKEN' \
  'HEPTA_SERVICE_TOKEN' \
  'NAKAMA_CONTROL_SIGNING_SEED'; do
  if rg -n --fixed-strings -- "$pattern" \
    "$service_root/src" \
    "$service_root/migrations" \
    "$service_root/deploy" \
    "$service_root/Cargo.toml"; then
    echo "forbidden Paper Raid authority marker detected: $pattern" >&2
    exit 1
  fi
done

if rg -n 'std::process::Command|tokio::process::Command' "$service_root/src"; then
  echo "production shell-out is forbidden in Paper Raid service source" >&2
  exit 1
fi

if rg -n 'localStorage|indexedDB|\.style' "$browser" || \
   rg -n '<input[^>]+name=\\?"agent_(private_key|seed|mnemonic)' "$html"; then
  echo "browser persistence or external Agent secret input detected" >&2
  exit 1
fi

# Session storage is limited to non-secret resumability and focus context.
[[ $(rg -o 'sessionStorage' "$browser" | wc -l) -eq 4 ]] || {
  echo "browser sessionStorage surface expanded" >&2
  exit 1
}
for marker in \
  'sessionStorage.getItem(liveCursorKey(paperId))' \
  'sessionStorage.setItem(liveCursorKey(paperId), JSON.stringify({ hepta: cursor.hepta }))' \
  'sessionStorage.getItem(PLAYER_FOCUS_CONTEXT_KEY)' \
  'sessionStorage.setItem(PLAYER_FOCUS_CONTEXT_KEY, JSON.stringify(payload))'; do
  require_marker "$browser" "$marker"
done

# Authenticated research/finality reads remain sealed and typed.
for marker in \
  'hepta.paper_raid.bff_finality_availability.v1' \
  'AuthenticatedPaperReviewState' \
  'timeline_finality_projection_ref'; do
  require_marker "$app" "$marker"
done
for marker in \
  'hepta.paper_raid.consumer_finality.v2' \
  'effective_evaluation_id' \
  'effective_reproduction_id' \
  'effective_appeal_resolution_id' \
  'pub(crate) struct AuthenticatedPaperRoom' \
  'pub(crate) struct AuthenticatedPaperReviewState' \
  '#[serde(deny_unknown_fields)]'; do
  require_marker "$hepta" "$marker"
done
if rg -n --fixed-strings 'non_authoritative_consumer_finality' "$app"; then
  echo "non-authoritative availability reused the consumer-finality schema" >&2
  exit 1
fi

# Browser key creation/rotation and fixed Nakama control mapping remain explicit.
for marker in \
  'human-key-create-form' \
  'human-key-register-form' \
  'forget_current_in_memory_key_before_generating_another' \
  'agent_proof_nonce_must_equal_idempotency_key' \
  'sendCommand("create_agent_binding", null, null, payload)' \
  'agent_rotation_old_binding_mismatch' \
  '"rotate_agent_binding_key"' \
  '"submit_appeal"' \
  'PaperAppealSigningV1'; do
  rg -q --fixed-strings -- "$marker" "$browser" "$html" "$hepta" || {
    echo "browser onboarding/key boundary marker is absent: $marker" >&2
    exit 1
  }
done
for marker in \
  '"create_nakama_research_session_control"' \
  '"resume_nakama_research_session_control"' \
  '"replace_nakama_research_session_roster_control"' \
  '"complete_nakama_research_session_control"' \
  '"/v2/hepta/nakama/research-session-controls/create"' \
  '"/v2/hepta/nakama/research-session-controls/resume"' \
  '"/v2/hepta/nakama/research-session-controls/replace-roster"' \
  '"/v2/hepta/nakama/research-session-controls/complete"'; do
  rg -q --fixed-strings -- "$marker" "$hepta" "$html" || {
    echo "fixed Nakama control mapping is absent: $marker" >&2
    exit 1
  }
done

# CAS is canonical and provider-neutral.
for media_type in \
  'application/x-bibtex' \
  'text/csv; charset=utf-8' \
  'application/json' \
  'text/markdown; charset=utf-8' \
  'application/pdf' \
  'text/x-python; charset=utf-8' \
  'image/svg+xml' \
  'text/plain; charset=utf-8' \
  'application/octet-stream'; do
  rg -q --fixed-strings -- "$media_type" "$cas" "$html" || {
    echo "canonical CAS media type is absent: $media_type" >&2
    exit 1
  }
done
if rg -n --fixed-strings 's3://' "$service_root/src"; then
  echo "storage-provider URI escaped the canonical CAS boundary" >&2
  exit 1
fi
for marker in 'artifact_sha256' 'digest_label_from_raw_sha256' 'cas://sha256/'; do
  rg -q --fixed-strings -- "$marker" "$service_root/src" || {
    echo "canonical CAS digest marker is absent: $marker" >&2
    exit 1
  }
done

# The runtime build DAG is source-only until the binary exists.
builder_section=$(sed -n '/ AS builder$/,/^FROM scratch AS runtime-binary-export$/p' "$bff_dockerfile")
if printf '%s\n' "$builder_section" | rg -n 'COPY .*sbom\.cdx\.json|PAPER_RAID_BFF_(REVISION|SOURCE_TREE)|^(ARG|ENV) SOURCE_DATE_EPOCH'; then
  echo "runtime builder input graph contains SBOM/provenance/epoch material" >&2
  exit 1
fi

for dockerfile in "$bff_dockerfile" "$accessctl_dockerfile"; do
  first_from=$(rg -m1 '^FROM ' "$dockerfile")
  [[ "$first_from" == *@sha256:* ]] || {
    echo "builder base is not digest-pinned: $dockerfile" >&2
    exit 1
  }
  runtime_from=$(rg '^FROM gcr\.io/distroless/' "$dockerfile")
  [[ "$runtime_from" == *@sha256:* ]] || {
    echo "runtime base is not digest-pinned: $dockerfile" >&2
    exit 1
  }
  for marker in \
    'RUSTUP_TOOLCHAIN=1.98.1' \
    'rustup toolchain install 1.98.1' \
    'commit-hash: 48a229ceaefd4985c50990b14116b6d856af0985' \
    'cargo fetch --locked' \
    'cargo build --locked --offline --release' \
    'runtime-binary-export' \
    'sha256sum --check --status' \
    'org.trillionnium.rust-toolchain="1.98.1"' \
    'org.trillionnium.rust-release-commit="48a229ceaefd4985c50990b14116b6d856af0985"'; do
    require_marker "$dockerfile" "$marker"
  done
done
require_marker "$bff_dockerfile" 'env -u SOURCE_DATE_EPOCH cargo build --locked --offline --release -p paper-raid-bff'
require_marker "$bff_dockerfile" 'PAPER_RAID_BFF_RUNTIME_BINARY_SHA256'
require_marker "$accessctl_dockerfile" '--bin paper-raid-accessctl'
require_marker "$accessctl_dockerfile" 'PAPER_RAID_ACCESSCTL_RUNTIME_BINARY_SHA256'
require_marker "$accessctl_dockerfile" 'ENTRYPOINT ["/paper-raid-accessctl"]'

# Release helpers must remain local-only, bounded and checksum-verifying.
staging_image_builder="$repo_root/scripts/build-paper-raid-bff-image.sh"
image_checker="$service_root/scripts/check-image.sh"
runtime_generator="$service_root/scripts/generate-runtime-sbom.sh"
buildx_downloader="$service_root/scripts/download-pinned-buildx.sh"
browser_a11y="$service_root/scripts/check-browser-mobile-a11y.sh"
for path in \
  "$staging_image_builder" \
  "$image_checker" \
  "$runtime_generator" \
  "$buildx_downloader" \
  "$browser_a11y"; do
  require_executable "$path"
  bash -n "$path"
done
if rg -n -- '(docker[[:space:]]+push|buildx[[:space:]]+build.*--push)' "$staging_image_builder"; then
  echo "Paper Raid staging builder must retain a local image only" >&2
  exit 1
fi
for marker in \
  'max_attempts=8' \
  '--continue-at -' \
  '--max-time 300' \
  'actual_sha256=' \
  'sha256sum'; do
  require_marker "$buildx_downloader" "$marker"
done
for caller in "$image_checker" "$browser_a11y" "$runtime_generator"; do
  require_marker "$caller" 'download-pinned-buildx.sh'
  require_marker "$caller" 'PAPER_RAID_BUILDX_BIN'
done

printf 'not-buildx\n' >"$scratch_dir/not-buildx"
ln -s "$scratch_dir/not-buildx" "$scratch_dir/buildx-symlink"
if bash "$buildx_downloader" \
  'https://invalid.example/buildx' \
  '0000000000000000000000000000000000000000000000000000000000000000' \
  "$scratch_dir/staged-buildx" \
  "$scratch_dir/buildx-symlink" >/dev/null 2>&1; then
  echo "pinned Buildx downloader accepted a local symlink" >&2
  exit 1
fi
if bash "$buildx_downloader" \
  'https://invalid.example/buildx' \
  '0000000000000000000000000000000000000000000000000000000000000000' \
  "$scratch_dir/staged-buildx" \
  "$scratch_dir/not-buildx" >/dev/null 2>&1; then
  echo "pinned Buildx downloader accepted a checksum mismatch" >&2
  exit 1
fi

# Minimal Docker dependency closure is checked behaviorally, not by grepping its
# implementation. This also verifies the executed compiler release identity.
"$service_root/scripts/check-docker-lock.sh"

jq -e '
  .bomFormat == "CycloneDX"
  and .specVersion == "1.5"
  and (.components | length) == 1
  and .components[0]["bom-ref"] == "file:/paper-raid-accessctl"
  and .components[0].name == "/paper-raid-accessctl"
  and .components[0].type == "file"
  and (.components[0].hashes | length) == 1
  and .components[0].hashes[0].alg == "SHA-256"
  and ([.metadata.properties[] | select(.name == "org.trillionnium.release-state")] | length) == 1
' "$accessctl_sbom" >/dev/null

# Specialized behavior and UI contracts remain the source of truth.
"$service_root/scripts/check-practice-unranked-boundary.sh"
"$service_root/scripts/check-quick-raid-boundary.sh"
node "$service_root/scripts/check-practice-player-ui.mjs"
node "$service_root/scripts/check-practice-agent-bridge.mjs"
node "$service_root/scripts/check-browser-crypto.mjs"
node "$service_root/scripts/check-player-language-focus.mjs"
node "$service_root/scripts/check-p1-accessibility.mjs"
node --check "$service_root/browser-e2e/mock-hepta.mjs"
node --check "$service_root/browser-e2e/mobile-a11y.mjs"

for marker in \
  'Accessibility.getFullAXTree' \
  'Object.freeze({ width: 390, height: 844 })' \
  'Object.freeze({ width: 430, height: 932 })' \
  'rect.width >= 24 && rect.height >= 24' \
  'developer_json_fallback_used: false' \
  'production_bridge_pairing_proved: false' \
  'production_bridge_execution_proved: false'; do
  require_marker "$service_root/browser-e2e/mobile-a11y.mjs" "$marker"
done
for marker in \
  'verify(null, assertionSigningBytes' \
  'seenAssertionIds.has(claim.assertion_id)' \
  'claim.operation !== routeOperations.get(canonicalPath)' \
  'PAPER_RAID_BFF_A11Y_AGENT_DIGEST' \
  'PAPER_RAID_BFF_A11Y_CONSUMER_PUBLIC_KEY_B64'; do
  require_marker "$service_root/browser-e2e/mock-hepta.mjs" "$marker"
done
for marker in \
  'runner_name="$run_id-chromium"' \
  'docker rm -f "$runner_name"' \
  'source_snapshot_sha256' \
  'browser accessibility source identity changed before evidence sealing'; do
  require_marker "$browser_a11y" "$marker"
done

"$service_root/scripts/check-observability-boundary.sh"

echo "paper-raid-bff structured boundary scan: ok"
