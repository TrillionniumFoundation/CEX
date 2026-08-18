#!/usr/bin/env bash
set -euo pipefail

service_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
repo_root=$(cd "$service_root/../.." && pwd -P)
scratch_dir=$(mktemp -d)
accessctl_dockerfile="$service_root/Dockerfile.accessctl"
accessctl_sbom="$service_root/docker/accessctl.sbom.cdx.json"
accessctl_sbom_binder="$service_root/scripts/bind-accessctl-runtime-sbom.sh"

cleanup() {
  case "$scratch_dir" in
    /tmp/tmp.*) rm -rf -- "$scratch_dir" ;;
    *) echo "refusing to remove unexpected boundary scratch path: $scratch_dir" >&2 ;;
  esac
}
trap cleanup EXIT INT TERM

for pattern in \
  "League""State" \
  "/bat""tle" \
  "forward_to_""cex_task" \
  "Alice"" fallback" \
  "keyword""_score" \
  "NAKAMA_""OPERATOR_TOKEN" \
  "HEPTA_""SERVICE_TOKEN" \
  "NAKAMA_""CONTROL_SIGNING_SEED"
do
  if rg -n --fixed-strings "$pattern" \
    "$service_root/src" \
    "$service_root/migrations" \
    "$service_root/deploy" \
    "$service_root/Dockerfile" \
    "$service_root/Cargo.toml"
  then
    echo "forbidden authority pattern detected" >&2
    exit 1
  fi
done
builder_section=$(sed -n \
  '/ AS builder$/,/^FROM scratch AS runtime-binary-export$/p' \
  "$service_root/Dockerfile")
if printf '%s\n' "$builder_section" \
  | rg -n 'COPY .*sbom\.cdx\.json|PAPER_RAID_BFF_(REVISION|SOURCE_TREE)|^(ARG|ENV) SOURCE_DATE_EPOCH'
then
  echo "runtime builder input graph contains SBOM/provenance/epoch material" >&2
  exit 1
fi
if ! printf '%s\n' "$builder_section" \
  | rg -q --fixed-strings \
    'RUN env -u SOURCE_DATE_EPOCH cargo build --locked --offline --release -p paper-raid-bff'
then
  echo "runtime builder does not explicitly remove SOURCE_DATE_EPOCH from Cargo" >&2
  exit 1
fi

if rg -n 'std::process::Command|tokio::process::Command' "$service_root/src"; then
  echo "production shell-out is forbidden" >&2
  exit 1
fi

browser="$service_root/src/browser.js"
html="$service_root/src/html.rs"
hepta="$service_root/src/hepta.rs"
app="$service_root/src/app.rs"
if rg -n 'localStorage|indexedDB|\.style' "$browser" || \
  rg -n '<input[^>]+name=\\?"agent_(private_key|seed|mnemonic)' "$html"
then
  echo "browser persistence or external Agent secret input detected" >&2
  exit 1
fi
if [[ $(rg -o 'sessionStorage' "$browser" | wc -l) -ne 4 ]] || \
   ! rg -q --fixed-strings 'sessionStorage.getItem(liveCursorKey(paperId))' "$browser" || \
   ! rg -q --fixed-strings 'sessionStorage.setItem(liveCursorKey(paperId), JSON.stringify({ hepta: cursor.hepta }))' "$browser" || \
   ! rg -q --fixed-strings 'sessionStorage.getItem(PLAYER_FOCUS_CONTEXT_KEY)' "$browser" || \
   ! rg -q --fixed-strings 'sessionStorage.setItem(PLAYER_FOCUS_CONTEXT_KEY, JSON.stringify(payload))' "$browser"
then
  echo "browser session storage is not limited to non-secret live cursors and focus context" >&2
  exit 1
fi

for required in \
  'hepta.paper_raid.bff_finality_availability.v1' \
  'AuthenticatedPaperReviewState' \
  'timeline_finality_projection_ref'
do
  if ! rg -q --fixed-strings "$required" "$app"; then
    echo "strict fail-closed finality boundary is missing: $required" >&2
    exit 1
  fi
done
for required in \
  'hepta.paper_raid.consumer_finality.v2' \
  'effective_evaluation_id' \
  'effective_reproduction_id' \
  'effective_appeal_resolution_id' \
  'pub(crate) struct AuthenticatedPaperRoom' \
  'pub(crate) struct AuthenticatedPaperReviewState' \
  'struct PaperRoomEnvelopeV3' \
  'struct PaperReviewStateEnvelopeV1' \
  '#[serde(deny_unknown_fields)]'
do
  if ! rg -q --fixed-strings "$required" "$hepta"; then
    echo "sealed authenticated Hepta read boundary is missing: $required" >&2
    exit 1
  fi
done
if rg -n --fixed-strings 'non_authoritative_consumer_finality' "$app"; then
  echo "non-authoritative availability must not reuse the consumer-finality schema" >&2
  exit 1
fi

for required in \
  'human-key-create-form' \
  'human-key-register-form' \
  'forget_current_in_memory_key_before_generating_another' \
  'Registration may already be committed' \
  'window.location.assign("/league/start")' \
  'agent_proof_nonce_must_equal_idempotency_key' \
  'sendCommand("create_agent_binding", null, null, payload)' \
  'agent_rotation_old_binding_mismatch' \
  '"rotate_agent_binding_key"' \
  '"submit_appeal"' \
  'PaperAppealSigningV1'
do
  if ! rg -q --fixed-strings "$required" "$browser" "$html" "$hepta"; then
    echo "required browser onboarding boundary is missing: $required" >&2
    exit 1
  fi
done

for required in \
  '"create_nakama_research_session_control"' \
  '"resume_nakama_research_session_control"' \
  '"replace_nakama_research_session_roster_control"' \
  '"complete_nakama_research_session_control"' \
  '"/v2/hepta/nakama/research-session-controls/create"' \
  '"/v2/hepta/nakama/research-session-controls/resume"' \
  '"/v2/hepta/nakama/research-session-controls/replace-roster"' \
  '"/v2/hepta/nakama/research-session-controls/complete"' \
  '"create_nakama_research_session_control_v2"' \
  '"resume_nakama_research_session_control_v2"' \
  '"replace_nakama_research_session_roster_control_v2"' \
  '"complete_nakama_research_session_control_v2"'
do
  if ! rg -q --fixed-strings "$required" "$hepta" "$html"; then
    echo "required fixed Nakama control mapping is missing: $required" >&2
    exit 1
  fi
done

cas="$service_root/src/cas.rs"
for required in \
  'application/x-bibtex' \
  'text/csv; charset=utf-8' \
  'application/json' \
  'text/markdown; charset=utf-8' \
  'application/pdf' \
  'text/x-python; charset=utf-8' \
  'image/svg+xml' \
  'text/plain; charset=utf-8' \
  'application/octet-stream'
do
  if ! rg -q --fixed-strings "$required" "$cas" "$html"; then
    echo "required exact Paper Collaboration Kernel media type is missing: $required" >&2
    exit 1
  fi
done

if [[ ! -f "$service_root/Dockerfile.dockerignore" ]] || \
   ! rg -q --fixed-strings 'org.trillionnium.source.tree' "$service_root/Dockerfile" || \
   ! rg -q --fixed-strings 'PAPER_RAID_BFF_SOURCE_TREE' "$service_root/scripts/check-image.sh"
then
  echo "BFF source-tree provenance or service-specific Docker context boundary is missing" >&2
  exit 1
fi
for required in \
  'CARGO_HTTP_TIMEOUT=600' \
  'CARGO_HTTP_LOW_SPEED_LIMIT=1' \
  'CARGO_NET_RETRY=5' \
  'rustc 1.95.0 (59807616e 2026-04-14)' \
  'cargo 1.95.0 (f2d3ce0bd 2026-03-21)' \
  'cargo fetch --locked' \
  'cargo build --locked --offline --release -p paper-raid-bff' \
  'AS runtime-binary-export' \
  'PAPER_RAID_BFF_RUNTIME_BINARY_SHA256' \
  'sha256sum --check --status' \
  'org.trillionnium.runtime-binary.sha256'
do
  if ! rg -q --fixed-strings -- "$required" "$service_root/Dockerfile"; then
    echo "bounded locked/offline Docker build gate is missing: $required" >&2
    exit 1
  fi
done

for required in \
  'cas://sha256/' \
  'artifact_sha256' \
  'digest_label_from_raw_sha256'
do
  if ! rg -q --fixed-strings "$required" "$service_root/src"; then
    echo "canonical Hepta/CAS digest boundary is missing: $required" >&2
    exit 1
  fi
done
if rg -n --fixed-strings 's3://' "$service_root/src"; then
  echo "storage-provider URI escaped the canonical CAS boundary" >&2
  exit 1
fi

for required in \
  'type: "file"' \
  '"bom-ref": "file:/paper-raid-bff"' \
  'name: "/paper-raid-bff"' \
  'runtime_binary_sha256' \
  '--runtime-sha256'
do
  if ! rg -q --fixed-strings -- "$required" "$service_root/scripts/generate-sbom.sh"; then
    echo "runtime binary SBOM component gate is missing: $required" >&2
    exit 1
  fi
done
if ! rg -q --fixed-strings 'runtime image binary differs from the SBOM-bound pinned-builder authority' \
  "$service_root/scripts/check-image.sh"
then
  echo "runtime binary image/SBOM equivalence gate is missing" >&2
  exit 1
fi
docker_lock_gate="$service_root/scripts/check-docker-lock.sh"
for required in \
  'cargo fetch \' \
  '--locked' \
  'cargo metadata \' \
  '--offline' \
  'paper-raid-bff committed minimal Docker lock verification: ok'
do
  if ! rg -q --fixed-strings -- "$required" "$docker_lock_gate"; then
    echo "committed Docker lock verification boundary is missing: $required" >&2
    exit 1
  fi
done
if rg -n '\bcargo[[:space:]]+(generate-lockfile|update)\b' "$docker_lock_gate"; then
  echo "Docker lock gate must not resolve against a moving registry index" >&2
  exit 1
fi
for required in \
  "scan_runtime_image \"\$repro_image_id\"" \
  'runtime_binary_authority_sha256' \
  'PAPER_RAID_BFF_RUNTIME_BINARY_SHA256' \
  "--build-arg \"SOURCE_DATE_EPOCH=\$source_date_epoch\"" \
  "source_context=\"\$scratch_dir/source\"" \
  "git -C \"\$repo_root\" archive \"\$revision\"" \
  'verify_source_unchanged' \
  "verify_tag_binding \"\$image_name\" \"\$image_id\"" \
  "scan_runtime_image \"\$image_id\"" \
  "if scan_runtime_image \"\$sentinel_image\"" \
  'timeout --signal=TERM --kill-after=30s 600s' \
  "\"\${bounded_buildx[@]}\""
do
  if ! rg -q --fixed-strings -- "$required" "$service_root/scripts/check-image.sh"; then
    echo "two-image pinned runtime authority gate is missing: $required" >&2
    exit 1
  fi
done

staging_image_builder="$repo_root/scripts/build-paper-raid-bff-image.sh"
[[ -x "$staging_image_builder" ]] || {
  echo "paper-raid-bff durable staging image builder is absent" >&2
  exit 1
}
for required in \
  '--image-ref <local-tag>' \
  'project-preflight.sh" --audit' \
  'paper-raid-bff-release-authority.lock' \
  'flock -n 9' \
  'PAPER_RAID_BFF_EXPORT_IMAGE_REF=$image_ref' \
  'services/paper-raid-bff/scripts/check-image.sh'
do
  if ! rg -q --fixed-strings -- "$required" "$staging_image_builder"; then
    echo "paper-raid-bff staging image builder contract drifted: $required" >&2
    exit 1
  fi
done
for required in \
  'export_image_owned=false' \
  'refusing to replace an existing local export image reference' \
  'sudo -n docker image tag "$image_id" "$export_image_ref"' \
  'verify_tag_binding "$export_image_ref" "$image_id"' \
  "'{{.Os}}/{{.Architecture}}'" \
  'platform=linux/amd64'
do
  if ! rg -q --fixed-strings -- "$required" "$service_root/scripts/check-image.sh"; then
    echo "paper-raid-bff verified local image export contract drifted: $required" >&2
    exit 1
  fi
done
if rg -n -- '(docker[[:space:]]+push|buildx[[:space:]]+build.*--push)' \
  "$staging_image_builder"
then
  echo "paper-raid-bff staging builder must retain a local image only" >&2
  exit 1
fi
bash -n "$staging_image_builder" "$service_root/scripts/check-image.sh"

runtime_generator="$service_root/scripts/generate-runtime-sbom.sh"
for required in \
  '--no-cache' \
  '--target runtime-binary-export' \
  'linux_amd64/paper-raid-bff' \
  "\$'linux_amd64\\nlinux_amd64/paper-raid-bff'" \
  "[[ -L \"\$runtime_binary\" ]]" \
  "\"\$scratch_dir/first\"|\"\$scratch_dir/second\")" \
  'runtime binary destination escaped the exact generator scratch slots' \
  'sudo -n chown -hR --' \
  "find \"\$destination\" -xdev" \
  'runtime generator scratch directory is not an owned regular directory' \
  'timeout --signal=TERM --kill-after=30s 600s' \
  "\"\${bounded_buildx[@]}\"" \
  'independent pinned-builder runtime binaries are not byte-deterministic' \
  'runtime binary embeds revision/tree/SBOM/self-hash material' \
  '--runtime-binary' \
  'runtime SBOM output must be the exact tracked BFF SBOM path' \
  'check-docker-lock.sh' \
  'verify_source_unchanged' \
  "git -C \"\$repo_root\" archive \"\$revision\"" \
  'mv -f --'
do
  if ! rg -q --fixed-strings -- "$required" "$runtime_generator"; then
    echo "pinned-builder runtime/SBOM generator gate is missing: $required" >&2
    exit 1
  fi
done

buildx_downloader="$service_root/scripts/download-pinned-buildx.sh"
for required in \
  'max_attempts=8' \
  '--continue-at -' \
  '--max-time 300' \
  "local_source=\${4:-}" \
  "[[ -L \"\$local_source\" || ! -f \"\$local_source\" || ! -r \"\$local_source\" ]]" \
  'actual_sha256=' \
  'sha256sum'
do
  if ! rg -q --fixed-strings -- "$required" "$buildx_downloader"; then
    echo "bounded resumable Buildx download gate is missing: $required" >&2
    exit 1
  fi
done
printf 'not-buildx\n' >"$scratch_dir/not-buildx"
ln -s "$scratch_dir/not-buildx" "$scratch_dir/buildx-symlink"
if bash "$buildx_downloader" \
  'https://invalid.example/buildx' \
  '0000000000000000000000000000000000000000000000000000000000000000' \
  "$scratch_dir/staged-buildx" \
  "$scratch_dir/buildx-symlink" >/dev/null 2>&1
then
  echo "pinned Buildx downloader accepted a local symlink" >&2
  exit 1
fi
if bash "$buildx_downloader" \
  'https://invalid.example/buildx' \
  '0000000000000000000000000000000000000000000000000000000000000000' \
  "$scratch_dir/staged-buildx" \
  "$scratch_dir/not-buildx" >/dev/null 2>&1
then
  echo "pinned Buildx downloader accepted a local checksum mismatch" >&2
  exit 1
fi
for caller in \
  "$service_root/scripts/check-image.sh" \
  "$service_root/scripts/check-browser-e2e.sh" \
  "$runtime_generator"
do
  if ! rg -q --fixed-strings 'download-pinned-buildx.sh' "$caller"; then
    echo "release gate bypasses the bounded resumable Buildx downloader: $caller" >&2
    exit 1
  fi
done
for caller in \
  "$service_root/scripts/check-image.sh" \
  "$runtime_generator"
do
  if ! rg -q --fixed-strings 'PAPER_RAID_BUILDX_BIN' "$caller"; then
    echo "release gate is missing the checksum-verified local Buildx cache input: $caller" >&2
    exit 1
  fi
done

[[ -f "$accessctl_dockerfile" && -f "$accessctl_sbom" \
  && -x "$accessctl_sbom_binder" ]] || {
  echo "paper-raid-accessctl image definition or SBOM is absent" >&2
  exit 1
}
for fragment in \
  'cargo build --locked --offline --release' \
  '--bin paper-raid-accessctl' \
  'ENTRYPOINT ["/paper-raid-accessctl"]' \
  'PAPER_RAID_ACCESSCTL_RUNTIME_BINARY_SHA256' \
  '"value": "bound-release-binary"' \
  '/usr/share/doc/paper-raid-accessctl/sbom.cdx.json'; do
  rg -q --fixed-strings -- "$fragment" "$accessctl_dockerfile" || {
    echo "paper-raid-accessctl image contract drifted: $fragment" >&2
    exit 1
  }
done
for fragment in \
  'accessctl SBOM output must be the exact tracked accessctl SBOM path' \
  'accessctl SBOM binding requires a clean committed source tree' \
  'independent pinned-builder accessctl binaries are not byte-deterministic' \
  'accessctl runtime binary embeds revision/tree/self-hash material' \
  '"bound-release-binary"' \
  'cmp -s "$scratch_dir/first.cdx.json" "$scratch_dir/second.cdx.json"' \
  'mv -f -- "$staged_output" "$resolved_output"'; do
  rg -q --fixed-strings -- "$fragment" "$accessctl_sbom_binder" || {
    echo "paper-raid-accessctl SBOM binding contract drifted: $fragment" >&2
    exit 1
  }
done
jq -e '
  .bomFormat == "CycloneDX"
  and .specVersion == "1.5"
  and (.components | length) == 1
  and .components[0]["bom-ref"] == "file:/paper-raid-accessctl"
  and .components[0].name == "/paper-raid-accessctl"
  and .components[0].type == "file"
  and (.components[0].hashes | length) == 1
  and .components[0].hashes[0].alg == "SHA-256"
  and (.components[0].hashes[0].content as $digest
    | ([.metadata.properties[]
        | select(.name == "org.trillionnium.release-state")
        | .value] as $states
      | ($states | length) == 1
      and (if $states[0] == "unbound-must-regenerate-from-release-binary"
           then $digest == ("0" * 64)
           elif $states[0] == "bound-release-binary"
           then ($digest | test("^[0-9a-f]{64}$"))
             and $digest != ("0" * 64)
           else false
           end)))
' "$accessctl_sbom" >/dev/null

node "$service_root/scripts/check-browser-crypto.mjs"
node "$service_root/scripts/check-player-language-focus.mjs"
bash "$service_root/scripts/check-observability-boundary.sh"

echo "paper-raid-bff boundary scan: ok"
