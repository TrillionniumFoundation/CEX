#!/usr/bin/env bash
set -euo pipefail

bash -n ./scripts/activate-session-auth-runtime.sh
bash -n ./scripts/reload-session-auth-runtime.sh
bash -n ./scripts/read-session-auth-runtime-activation-status.sh
bash -n ./activate-session-auth-runtime.sh
bash -n ./read-session-auth-runtime-activation-status.sh
bash -n ./scripts/rollback-session-auth-runtime.sh
bash -n ./rollback-session-auth-runtime.sh
bash -n ./scripts/read-session-auth-runtime-history.sh
bash -n ./read-session-auth-runtime-history.sh
bash -n ./scripts/session-auth-runtime.sh
bash -n ./session-auth-runtime.sh

cat > /tmp/cex-session-auth-activation-mock.py <<'PY'
#!/usr/bin/env python3
import http.server, json, pathlib, sys
port = int(sys.argv[1])
name = sys.argv[2]
live_file = pathlib.Path(sys.argv[3])
fail_once_file = pathlib.Path(sys.argv[4])
log_file = pathlib.Path(sys.argv[5])

def read_revision():
    if not live_file.exists():
        return None
    try:
        return json.loads(live_file.read_text(encoding='utf-8')).get('revision')
    except Exception:
        return None

class Handler(http.server.BaseHTTPRequestHandler):
    def _write_json(self, status, body):
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())

    def do_GET(self):
        log_file.write_text(log_file.read_text(encoding='utf-8') + f"GET {self.path}\n", encoding='utf-8') if log_file.exists() else log_file.write_text(f"GET {self.path}\n", encoding='utf-8')
        revision = read_revision()
        if name == 'consumer':
            body = {
                'status': 'ok',
                'session_auth_issuer_registry': {
                    'metadata': {'revision': revision}
                }
            }
        else:
            body = {
                'status': 'ok',
                'consumer_entry_session_auth': {
                    'issuer_registry_metadata': {'revision': revision}
                }
            }
        self._write_json(200, body)

    def do_POST(self):
        log_file.write_text(log_file.read_text(encoding='utf-8') + f"POST {self.path}\n", encoding='utf-8') if log_file.exists() else log_file.write_text(f"POST {self.path}\n", encoding='utf-8')
        if self.path.endswith('/reload') and name == 'matrix' and fail_once_file.exists():
            fail_once_file.unlink()
            self._write_json(500, {'status': 'forced_matrix_reload_failure'})
            return
        body = {'status': 'ok'}
        if self.path.endswith('/reload'):
            body['reloaded'] = True
        if self.path.endswith('/validate'):
            body['valid'] = True
        self._write_json(200, body)

    def log_message(self, fmt, *args):
        return

http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler).serve_forever()
PY
chmod +x /tmp/cex-session-auth-activation-mock.py

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"; kill ${PID1:-} ${PID2:-} >/dev/null 2>&1 || true' EXIT

LAYOUT_ROOT="$TMP_DIR/layout"
CANDIDATE_DIR="$LAYOUT_ROOT/session-auth-candidates"
LIVE_FILE="$LAYOUT_ROOT/session-auth-issuer-registry.json"
APPROVAL_FILE="$LAYOUT_ROOT/session-auth-issuer-registry-approved-revisions.json"
CANDIDATE_OK="$CANDIDATE_DIR/candidate-ok.json"
CANDIDATE_FAIL="$CANDIDATE_DIR/candidate-fail.json"
MATRIX_FAIL_ONCE="$TMP_DIR/matrix-fail-once"
CONSUMER_LOG="$TMP_DIR/consumer.log"
MATRIX_LOG="$TMP_DIR/matrix.log"
SUMMARY_OK="$TMP_DIR/summary-ok.json"
SUMMARY_FAIL="$TMP_DIR/summary-fail.json"
ROLLBACK_SUMMARY="$TMP_DIR/rollback-summary.json"
BACKUP_DIR="$TMP_DIR/backups"
HISTORY_FILE="$TMP_DIR/history.jsonl"

mkdir -p "$CANDIDATE_DIR" "$BACKUP_DIR"

cat > "$LIVE_FILE" <<'JSON'
{"version":1,"revision":"sess-reg-live-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}
JSON
cat > "$CANDIDATE_OK" <<'JSON'
{"version":1,"revision":"sess-reg-live-b","issuers":{"matrix-entry-adapter":{"activeKeyId":"v2","keys":{"v2":"secret-b"}}}}
JSON
cat > "$CANDIDATE_FAIL" <<'JSON'
{"version":1,"revision":"sess-reg-live-c","issuers":{"matrix-entry-adapter":{"activeKeyId":"v3","keys":{"v3":"secret-c"}}}}
JSON
cat > "$APPROVAL_FILE" <<'JSON'
{"version":1,"revision":"approval-a","approved_revisions":["sess-reg-live-b","sess-reg-live-c"]}
JSON
: > "$CONSUMER_LOG"
: > "$MATRIX_LOG"

python3 /tmp/cex-session-auth-activation-mock.py 18190 consumer "$LIVE_FILE" "$TMP_DIR/no-fail-consumer" "$CONSUMER_LOG" >/tmp/cex-session-auth-activation-consumer.out 2>&1 &
PID1=$!
python3 /tmp/cex-session-auth-activation-mock.py 18191 matrix "$LIVE_FILE" "$MATRIX_FAIL_ONCE" "$MATRIX_LOG" >/tmp/cex-session-auth-activation-matrix.out 2>&1 &
PID2=$!
sleep 1

SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
./activate-session-auth-runtime.sh --print-defaults --compact >/tmp/cex-session-auth-activation-defaults.txt
python3 - <<'PY' "$LAYOUT_ROOT" "$LIVE_FILE" "$APPROVAL_FILE" "$BACKUP_DIR"
import json, pathlib, sys
layout_root, live_file, approval_file, backup_dir = sys.argv[1:5]
defaults = json.loads(pathlib.Path('/tmp/cex-session-auth-activation-defaults.txt').read_text())
assert defaults['kind'] == 'session-auth-runtime-activation-defaults'
assert defaults['layoutRoot'] == layout_root
assert defaults['liveFile'] == live_file
assert defaults['approvedRevisionsFile'] == approval_file
assert defaults['backupDir'] == backup_dir
PY

SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_OK" \
./activate-session-auth-runtime.sh \
  --candidate-file candidate-ok.json \
  --consumer-base-url http://127.0.0.1:18190 \
  --matrix-base-url http://127.0.0.1:18191 \
  --consumer-token c1 \
  --matrix-token m1 \
  --compact >/tmp/cex-session-auth-activation-ok.json

python3 - <<'PY' "$SUMMARY_OK" "$LIVE_FILE"
import json, pathlib, sys
summary = json.loads(pathlib.Path(sys.argv[1]).read_text())
live = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert summary['kind'] == 'session-auth-runtime-activation-summary'
assert summary['overall']['status'] == 'activated'
assert summary['activation']['exitCode'] == 0
assert summary['activation']['body']['ok'] is True
assert summary['activation']['body']['postStatus']['liveRevisionMatch'] is True
assert live['revision'] == 'sess-reg-live-b'
PY
./read-session-auth-runtime-activation-status.sh --summary-file "$SUMMARY_OK" --compact >/tmp/cex-session-auth-activation-read-ok.txt
./read-session-auth-runtime-activation-status.sh --summary-file "$SUMMARY_OK" --field overall.status >/tmp/cex-session-auth-activation-read-ok-status.txt
./read-session-auth-runtime-activation-status.sh --summary-file "$SUMMARY_OK" --require-converged >/tmp/cex-session-auth-activation-read-ok-full.txt
python3 - <<'PY'
import pathlib
compact = pathlib.Path('/tmp/cex-session-auth-activation-read-ok.txt').read_text()
status = pathlib.Path('/tmp/cex-session-auth-activation-read-ok-status.txt').read_text().strip()
assert 'status=activated' in compact
assert 'converged=true' in compact
assert status == 'activated'
PY

printf 'fail_once\n' > "$MATRIX_FAIL_ONCE"
set +e
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
./activate-session-auth-runtime.sh \
  --candidate-file candidate-fail.json \
  --consumer-base-url http://127.0.0.1:18190 \
  --matrix-base-url http://127.0.0.1:18191 \
  --consumer-token c1 \
  --matrix-token m1 \
  --compact >/tmp/cex-session-auth-activation-fail.json
FAIL_CODE=$?
set -e
if [[ "$FAIL_CODE" -eq 0 ]]; then
  echo "expected activation failure exit code" >&2
  exit 1
fi

python3 - <<'PY' "$SUMMARY_FAIL" "$LIVE_FILE"
import json, pathlib, sys
summary = json.loads(pathlib.Path(sys.argv[1]).read_text())
live = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert summary['overall']['status'] == 'activation_failed_rolled_back'
assert summary['activation']['exitCode'] != 0
assert summary['rollback']['exitCode'] == 0
assert summary['rollback']['body']['ok'] is True
assert summary['rollback']['body']['postStatus']['liveRevisionMatch'] is True
assert live['revision'] == 'sess-reg-live-b'
PY
./read-session-auth-runtime-activation-status.sh --summary-file "$SUMMARY_FAIL" --field overall.status >/tmp/cex-session-auth-activation-read-fail-status.txt
set +e
./read-session-auth-runtime-activation-status.sh --summary-file "$SUMMARY_FAIL" --fail-on-status activation_failed_rolled_back >/tmp/cex-session-auth-activation-read-fail.txt
READER_FAIL_CODE=$?
set -e
if [[ "$READER_FAIL_CODE" -eq 0 ]]; then
  echo "expected reader fail-on-status failure" >&2
  exit 1
fi
python3 - <<'PY'
import pathlib
status = pathlib.Path('/tmp/cex-session-auth-activation-read-fail-status.txt').read_text().strip()
assert status == 'activation_failed_rolled_back'
PY

cat > "$TMP_DIR/manual-rollback-target.json" <<'JSON'
{"version":1,"revision":"sess-reg-live-a","issuers":{"matrix-entry-adapter":{"activeKeyId":"v1","keys":{"v1":"secret-a"}}}}
JSON
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_ROLLBACK_SUMMARY_FILE="$ROLLBACK_SUMMARY" \
./rollback-session-auth-runtime.sh \
  --backup-file "$TMP_DIR/manual-rollback-target.json" \
  --consumer-base-url http://127.0.0.1:18190 \
  --matrix-base-url http://127.0.0.1:18191 \
  --consumer-token c1 \
  --matrix-token m1 \
  --compact >/tmp/cex-session-auth-rollback.json
python3 - <<'PY' "$ROLLBACK_SUMMARY" "$LIVE_FILE"
import json, pathlib, sys
summary = json.loads(pathlib.Path(sys.argv[1]).read_text())
live = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert summary['kind'] == 'session-auth-runtime-rollback-summary'
assert summary['overall']['status'] == 'rolled_back'
assert summary['rollback']['exitCode'] == 0
assert summary['rollback']['body']['postStatus']['liveRevisionMatch'] is True
assert live['revision'] == 'sess-reg-live-a'
PY
./read-session-auth-runtime-history.sh --history-file "$HISTORY_FILE" --compact >/tmp/cex-session-auth-history-compact.txt
./read-session-auth-runtime-history.sh --history-file "$HISTORY_FILE" --latest --field action >/tmp/cex-session-auth-history-latest-action.txt
./read-session-auth-runtime-history.sh --history-file "$HISTORY_FILE" --action activation --status activated --limit 1 --field candidateRevision >/tmp/cex-session-auth-history-activated-revision.txt
./read-session-auth-runtime-history.sh --history-file "$HISTORY_FILE" --summary --compact >/tmp/cex-session-auth-history-summary-compact.txt
./read-session-auth-runtime-history.sh --history-file "$HISTORY_FILE" --summary --field counts.matchedEntries >/tmp/cex-session-auth-history-summary-count.txt
./read-session-auth-runtime-history.sh --history-file "$HISTORY_FILE" --summary --require-latest-converged >/tmp/cex-session-auth-history-summary.txt
./session-auth-runtime.sh history --history-file "$HISTORY_FILE" --summary --field overall.status >/tmp/cex-session-auth-runtime-history-status.txt
./session-auth-runtime.sh last --summary-file "$SUMMARY_FAIL" --field overall.status >/tmp/cex-session-auth-runtime-last-status.txt
./session-auth-runtime.sh status --service both --consumer-base-url http://127.0.0.1:18190 --matrix-base-url http://127.0.0.1:18191 --consumer-token c1 --matrix-token m1 --compact >/tmp/cex-session-auth-runtime-status.json
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --examples >/tmp/cex-session-auth-runtime-examples.txt
./session-auth-runtime.sh --print-run-command >/tmp/cex-session-auth-runtime-run-command.txt
./session-auth-runtime.sh --help-json >/tmp/cex-session-auth-runtime-help.json
./session-auth-runtime.sh --summary-json >/tmp/cex-session-auth-runtime-summary.json
./session-auth-runtime.sh --summary-compact >/tmp/cex-session-auth-runtime-summary-compact.txt
./session-auth-runtime.sh --summary-field overall.surfaceCount >/tmp/cex-session-auth-runtime-summary-field.txt
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --schema >/tmp/cex-session-auth-runtime-schema.json
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --status-json >/tmp/cex-session-auth-runtime-status-top.json
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --status-compact >/tmp/cex-session-auth-runtime-status-compact.txt
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --status-field overall.lastKnownStatus >/tmp/cex-session-auth-runtime-status-field.txt
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --doctor-json >/tmp/cex-session-auth-runtime-doctor.json
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --doctor >/tmp/cex-session-auth-runtime-doctor.txt
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --doctor-compact >/tmp/cex-session-auth-runtime-doctor-compact.txt
SESSION_AUTH_RUNTIME_LAYOUT_ROOT="$LAYOUT_ROOT" \
SESSION_AUTH_RUNTIME_BACKUP_DIR="$BACKUP_DIR" \
SESSION_AUTH_RUNTIME_HISTORY_FILE="$HISTORY_FILE" \
SESSION_AUTH_RUNTIME_SUMMARY_FILE="$SUMMARY_FAIL" \
CONSUMER_ENTRY_BASE_URL=http://127.0.0.1:18190 \
MATRIX_ENTRY_BASE_URL=http://127.0.0.1:18191 \
CONSUMER_ENTRY_INGRESS_TOKEN=c1 \
MATRIX_ENTRY_INGRESS_TOKEN=m1 \
./session-auth-runtime.sh --doctor-field overall.status >/tmp/cex-session-auth-runtime-doctor-field.txt
set +e
./read-session-auth-runtime-history.sh --history-file "$HISTORY_FILE" --summary --fail-on-latest-status rolled_back >/tmp/cex-session-auth-history-summary-fail.txt
SUMMARY_FAIL_CODE=$?
set -e
if [[ "$SUMMARY_FAIL_CODE" -eq 0 ]]; then
  echo "expected history summary fail-on-latest-status failure" >&2
  exit 1
fi
python3 - <<'PY' "$HISTORY_FILE"
import json, pathlib, sys
history_lines = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines() if line.strip()]
assert len(history_lines) == 3
assert history_lines[0]['action'] == 'activation'
assert history_lines[0]['overall']['status'] == 'activated'
assert history_lines[1]['action'] == 'activation'
assert history_lines[1]['overall']['status'] == 'activation_failed_rolled_back'
assert history_lines[2]['action'] == 'rollback'
assert history_lines[2]['overall']['status'] == 'rolled_back'
PY
python3 - <<'PY'
import json, pathlib
compact = pathlib.Path('/tmp/cex-session-auth-history-compact.txt').read_text()
latest_action = pathlib.Path('/tmp/cex-session-auth-history-latest-action.txt').read_text().strip()
revision = pathlib.Path('/tmp/cex-session-auth-history-activated-revision.txt').read_text().strip()
summary_compact = pathlib.Path('/tmp/cex-session-auth-history-summary-compact.txt').read_text()
summary_count = pathlib.Path('/tmp/cex-session-auth-history-summary-count.txt').read_text().strip()
wrapper_history_status = pathlib.Path('/tmp/cex-session-auth-runtime-history-status.txt').read_text().strip()
wrapper_last_status = pathlib.Path('/tmp/cex-session-auth-runtime-last-status.txt').read_text().strip()
wrapper_status_json = pathlib.Path('/tmp/cex-session-auth-runtime-status.json').read_text()
examples_text = pathlib.Path('/tmp/cex-session-auth-runtime-examples.txt').read_text()
run_command = pathlib.Path('/tmp/cex-session-auth-runtime-run-command.txt').read_text().strip()
help_json = json.loads(pathlib.Path('/tmp/cex-session-auth-runtime-help.json').read_text())
summary_json = json.loads(pathlib.Path('/tmp/cex-session-auth-runtime-summary.json').read_text())
summary_compact_top = pathlib.Path('/tmp/cex-session-auth-runtime-summary-compact.txt').read_text()
summary_field = pathlib.Path('/tmp/cex-session-auth-runtime-summary-field.txt').read_text().strip()
schema_json = json.loads(pathlib.Path('/tmp/cex-session-auth-runtime-schema.json').read_text())
status_top_json = json.loads(pathlib.Path('/tmp/cex-session-auth-runtime-status-top.json').read_text())
doctor_json = json.loads(pathlib.Path('/tmp/cex-session-auth-runtime-doctor.json').read_text())
doctor_text = pathlib.Path('/tmp/cex-session-auth-runtime-doctor.txt').read_text()
status_compact = pathlib.Path('/tmp/cex-session-auth-runtime-status-compact.txt').read_text()
status_field = pathlib.Path('/tmp/cex-session-auth-runtime-status-field.txt').read_text().strip()
doctor_compact = pathlib.Path('/tmp/cex-session-auth-runtime-doctor-compact.txt').read_text()
doctor_field = pathlib.Path('/tmp/cex-session-auth-runtime-doctor-field.txt').read_text().strip()
assert 'action=rollback' in compact
assert 'action=activation' in compact
assert latest_action == 'rollback'
assert revision == 'sess-reg-live-b'
assert 'status=rolled_back' in summary_compact
assert 'matched=3' in summary_compact
assert summary_count == '3'
assert wrapper_history_status == 'rolled_back'
assert wrapper_last_status == 'activation_failed_rolled_back'
assert '"ok": true' in wrapper_status_json or '"ok":true' in wrapper_status_json
assert './scripts/session-auth-runtime.sh --status-compact' in examples_text
assert run_command == './scripts/session-auth-runtime.sh --status-compact'
assert help_json['kind'] == 'session-auth-runtime-help'
assert help_json['commands']['activate']['delegatesTo'].endswith('/activate-session-auth-runtime.sh')
assert 'examples' in help_json['surfaces']
assert 'printRunCommand' in help_json['surfaces']
assert 'summaryJson' in help_json['surfaces']
assert 'recommendedConsumption' in help_json
assert help_json['related']['catalogEntryPath'] == 'catalogEntry'
assert help_json['related']['catalogEntryContractPath'] == 'contracts.catalogEntry'
assert help_json['related']['metaDiscoverabilityPath'] == 'metaDiscoverability'
assert help_json['related']['metaDiscoverabilityContractPath'] == 'contracts.metaDiscoverability'
assert help_json['related']['surfaceCapabilitiesPath'] == 'surfaceCapabilities'
assert help_json['related']['surfaceCapabilitiesContractPath'] == 'contracts.surfaceCapabilities'
assert help_json['related']['stabilityPolicyPath'] == 'stabilityPolicy'
assert help_json['related']['stabilityPolicyContractPath'] == 'contracts.stabilityPolicy'
assert help_json['related']['surfaceProfilesPath'] == 'surfaceProfiles'
assert help_json['related']['surfaceProfilesContractPath'] == 'contracts.surfaceProfiles'
assert help_json['related']['consumerProfilesPath'] == 'consumerProfiles'
assert help_json['related']['consumerProfilesContractPath'] == 'contracts.consumerProfiles'
assert help_json['related']['profileSelectionGuidePath'] == 'profileSelectionGuide'
assert help_json['related']['profileSelectionGuideContractPath'] == 'contracts.profileSelectionGuide'
assert help_json['related']['profileSelectionTracePath'] == 'profileSelectionTrace'
assert help_json['related']['profileSelectionTraceContractPath'] == 'contracts.profileSelectionTrace'
assert help_json['related']['lifecyclePath'] == 'lifecycle'
assert help_json['related']['lifecycleContractPath'] == 'contracts.lifecycle'
assert help_json['related']['maturityPath'] == 'maturity'
assert help_json['related']['maturityContractPath'] == 'contracts.maturity'
assert help_json['related']['compatibilityPolicyPath'] == 'compatibilityPolicy'
assert help_json['related']['compatibilityPolicyContractPath'] == 'contracts.compatibilityPolicy'
assert help_json['related']['contractGovernancePath'] == 'contractGovernance'
assert help_json['related']['contractGovernanceContractPath'] == 'contracts.contractGovernance'
assert help_json['related']['surfaceLifecycleMatrixPath'] == 'surfaceLifecycleMatrix'
assert help_json['related']['surfaceLifecycleMatrixContractPath'] == 'contracts.surfaceLifecycleMatrix'
assert help_json['related']['contractStatusMatrixPath'] == 'contractStatusMatrix'
assert help_json['related']['contractStatusMatrixContractPath'] == 'contracts.contractStatusMatrix'
assert help_json['related']['summarySurfaceGuidePath'] == 'summarySurfaceGuide'
assert help_json['related']['summarySurfaceGuideContractPath'] == 'contracts.summarySurfaceGuide'
assert help_json['recommendedConsumption']['recommendedRunCommand'] == './scripts/session-auth-runtime.sh --status-compact'
assert help_json['recommendedConsumption']['metaDiscoverabilityPath'] == 'metaDiscoverability'
assert help_json['recommendedConsumption']['catalogEntryPath'] == 'catalogEntry'
assert help_json['recommendedConsumption']['surfaceCapabilitiesPath'] == 'surfaceCapabilities'
assert help_json['recommendedConsumption']['stabilityPolicyPath'] == 'stabilityPolicy'
assert help_json['recommendedConsumption']['surfaceProfilesPath'] == 'surfaceProfiles'
assert help_json['recommendedConsumption']['consumerProfilesPath'] == 'consumerProfiles'
assert help_json['recommendedConsumption']['profileSelectionGuidePath'] == 'profileSelectionGuide'
assert help_json['recommendedConsumption']['profileSelectionTracePath'] == 'profileSelectionTrace'
assert help_json['recommendedConsumption']['lifecyclePath'] == 'lifecycle'
assert help_json['recommendedConsumption']['maturityPath'] == 'maturity'
assert help_json['recommendedConsumption']['compatibilityPolicyPath'] == 'compatibilityPolicy'
assert help_json['recommendedConsumption']['contractGovernancePath'] == 'contractGovernance'
assert help_json['recommendedConsumption']['surfaceLifecycleMatrixPath'] == 'surfaceLifecycleMatrix'
assert help_json['recommendedConsumption']['contractStatusMatrixPath'] == 'contractStatusMatrix'
assert summary_json['kind'] == 'session-auth-runtime-summary'
assert summary_json['catalogEntry']['entryId'] == 'session-auth-runtime'
assert summary_json['catalogEntry']['primaryCommand'] == './scripts/session-auth-runtime.sh --summary-json'
assert summary_json['catalogEntry']['preferredReadOrder'][0] == 'metaDiscoverability'
assert summary_json['catalogEntry']['preferredReadOrder'][2] == 'lifecycle'
assert summary_json['catalogEntry']['preferredReadOrder'][4] == 'compatibilityPolicy'
assert summary_json['catalogEntry']['preferredReadOrder'][5] == 'contractGovernance'
assert summary_json['catalogEntry']['preferredReadOrder'][6] == 'surfaceLifecycleMatrix'
assert summary_json['catalogEntry']['preferredReadOrder'][7] == 'contractStatusMatrix'
assert summary_json['metaDiscoverability']['catalogEntryPath'] == 'catalogEntry'
assert summary_json['metaDiscoverability']['surfaceCapabilitiesPath'] == 'surfaceCapabilities'
assert summary_json['metaDiscoverability']['surfaceProfilesPath'] == 'surfaceProfiles'
assert summary_json['metaDiscoverability']['consumerProfilesContractPath'] == 'contracts.consumerProfiles'
assert summary_json['metaDiscoverability']['profileSelectionGuidePath'] == 'profileSelectionGuide'
assert summary_json['metaDiscoverability']['profileSelectionTraceContractPath'] == 'contracts.profileSelectionTrace'
assert summary_json['metaDiscoverability']['lifecyclePath'] == 'lifecycle'
assert summary_json['metaDiscoverability']['maturityContractPath'] == 'contracts.maturity'
assert summary_json['metaDiscoverability']['compatibilityPolicyPath'] == 'compatibilityPolicy'
assert summary_json['metaDiscoverability']['compatibilityPolicyContractPath'] == 'contracts.compatibilityPolicy'
assert summary_json['metaDiscoverability']['contractGovernancePath'] == 'contractGovernance'
assert summary_json['metaDiscoverability']['contractGovernanceContractPath'] == 'contracts.contractGovernance'
assert summary_json['metaDiscoverability']['surfaceLifecycleMatrixPath'] == 'surfaceLifecycleMatrix'
assert summary_json['metaDiscoverability']['surfaceLifecycleMatrixContractPath'] == 'contracts.surfaceLifecycleMatrix'
assert summary_json['metaDiscoverability']['contractStatusMatrixPath'] == 'contractStatusMatrix'
assert summary_json['metaDiscoverability']['contractStatusMatrixContractPath'] == 'contracts.contractStatusMatrix'
assert summary_json['surfaceCapabilities']['summaryCompact']['glance'] is True
assert summary_json['surfaceCapabilities']['doctor']['stable'] is False
assert summary_json['surfaceProfiles']['machine_reader']['entrySurface'] == 'summaryJson'
assert summary_json['consumerProfiles']['shell_operator']['recommendedStart'] == 'summaryCompact'
assert summary_json['profileSelectionGuide']['ifYouNeed']['machineReadableContract'] == 'machine_parser'
assert summary_json['profileSelectionTrace']['machine_parser']['selectedSurface'] == 'summaryJson'
assert summary_json['profileSelectionTrace']['shell_operator']['surfaceProfile'] == 'shell_glance'
assert summary_json['lifecycle']['currentPhase'] == 'self-describing-operator-cli'
assert summary_json['maturity']['machineReadable']['level'] == 'stable'
assert summary_json['compatibilityPolicy']['contractStrategy'] == 'additive-with-announced-deprecation'
assert summary_json['compatibilityPolicy']['deprecation']['mode'] == 'announce-then-remove'
assert summary_json['contractGovernance']['governedBlocks'][0] == 'lifecycle'
assert summary_json['contractGovernance']['changeManagement']['requiresSmokeCoverage'] is True
assert summary_json['surfaceLifecycleMatrix']['doctor']['status'] == 'best_effort'
assert summary_json['surfaceLifecycleMatrix']['summaryJson']['preferred'] is True
assert summary_json['contractStatusMatrix']['surfaceLifecycleMatrix']['governedBy'] == 'contractGovernance'
assert summary_json['contractStatusMatrix']['stabilityPolicy']['status'] == 'stable'
assert summary_json['stabilityPolicy']['additiveOnly'] is True
assert 'summaryJson' in summary_json['stabilityPolicy']['preferredMachineReadableSurfaces']
assert summary_json['summarySurfaceGuide']['summary']['json'] == './scripts/session-auth-runtime.sh --summary-json'
assert summary_json['summarySurfaceGuide']['doctor']['fieldExample'] == './scripts/session-auth-runtime.sh --doctor-field overall.status'
assert summary_json['overall']['commandCount'] == 7
assert summary_json['overall']['surfaceCount'] >= 15
assert summary_json['overall']['compatibilityReady'] is True
assert summary_json['overall']['contractGovernanceReady'] is True
assert summary_json['overall']['surfaceLifecycleMatrixReady'] is True
assert summary_json['overall']['contractStatusMatrixReady'] is True
assert 'status=ok' in summary_compact_top
assert 'discoverability=true' in summary_compact_top
assert summary_field.isdigit()
assert int(summary_field) >= 15
assert 'statusJson' in help_json['surfaces']
assert 'doctorJson' in help_json['surfaces']
assert schema_json['kind'] == 'session-auth-runtime-schema'
assert schema_json['commands']['status']['fixedArgs'] == ['--action', 'status']
assert 'statusJson' in schema_json['contracts']
assert 'doctorJson' in schema_json['contracts']
assert 'recommendedConsumption' in schema_json['contracts']
assert 'summaryJson' in schema_json['contracts']
assert 'summarySurfaceGuide' in schema_json['contracts']
assert 'metaDiscoverability' in schema_json['contracts']
assert 'catalogEntry' in schema_json['contracts']
assert 'surfaceCapabilities' in schema_json['contracts']
assert 'surfaceProfiles' in schema_json['contracts']
assert 'consumerProfiles' in schema_json['contracts']
assert 'profileSelectionGuide' in schema_json['contracts']
assert 'profileSelectionTrace' in schema_json['contracts']
assert 'lifecycle' in schema_json['contracts']
assert 'maturity' in schema_json['contracts']
assert 'compatibilityPolicy' in schema_json['contracts']
assert 'contractGovernance' in schema_json['contracts']
assert 'surfaceLifecycleMatrix' in schema_json['contracts']
assert 'contractStatusMatrix' in schema_json['contracts']
assert 'stabilityPolicy' in schema_json['contracts']
assert schema_json['recommendedConsumption']['contractPath'] == 'contracts.recommendedConsumption'
assert schema_json['recommendedConsumption']['summarySurfaceGuideContractPath'] == 'contracts.summarySurfaceGuide'
assert schema_json['recommendedConsumption']['metaDiscoverabilityContractPath'] == 'contracts.metaDiscoverability'
assert schema_json['recommendedConsumption']['catalogEntryContractPath'] == 'contracts.catalogEntry'
assert schema_json['recommendedConsumption']['surfaceCapabilitiesContractPath'] == 'contracts.surfaceCapabilities'
assert schema_json['recommendedConsumption']['stabilityPolicyContractPath'] == 'contracts.stabilityPolicy'
assert schema_json['recommendedConsumption']['surfaceProfilesContractPath'] == 'contracts.surfaceProfiles'
assert schema_json['recommendedConsumption']['consumerProfilesContractPath'] == 'contracts.consumerProfiles'
assert schema_json['recommendedConsumption']['profileSelectionGuideContractPath'] == 'contracts.profileSelectionGuide'
assert schema_json['recommendedConsumption']['profileSelectionTraceContractPath'] == 'contracts.profileSelectionTrace'
assert schema_json['recommendedConsumption']['lifecycleContractPath'] == 'contracts.lifecycle'
assert schema_json['recommendedConsumption']['maturityContractPath'] == 'contracts.maturity'
assert schema_json['recommendedConsumption']['compatibilityPolicyContractPath'] == 'contracts.compatibilityPolicy'
assert schema_json['recommendedConsumption']['contractGovernanceContractPath'] == 'contracts.contractGovernance'
assert schema_json['recommendedConsumption']['surfaceLifecycleMatrixContractPath'] == 'contracts.surfaceLifecycleMatrix'
assert schema_json['recommendedConsumption']['contractStatusMatrixContractPath'] == 'contracts.contractStatusMatrix'
assert status_top_json['kind'] == 'session-auth-runtime-status'
assert status_top_json['overall']['liveStatusOk'] is True
assert status_top_json['overall']['lastKnownStatus'] == 'activation_failed_rolled_back'
assert 'status=ok' in status_compact
assert 'lastStatus=activation_failed_rolled_back' in status_compact
assert status_field == 'activation_failed_rolled_back'
assert doctor_json['kind'] == 'session-auth-runtime-doctor'
assert doctor_json['checks']['helpersExecutable'] is True
assert 'status=ok' in doctor_compact
assert 'helpers=true' in doctor_compact
assert doctor_field == 'ok'
assert 'status: ' in doctor_text
assert 'helpersExecutable: True' in doctor_text
PY

echo ok
