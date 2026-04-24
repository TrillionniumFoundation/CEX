#!/usr/bin/env bash
set -euo pipefail

bash -n ./scripts/reload-session-auth-runtime.sh
cat > /tmp/cex-session-auth-runtime-mock.py <<'PY'
#!/usr/bin/env python3
import http.server, json, sys
port = int(sys.argv[1])
name = sys.argv[2]
log_path = sys.argv[3]
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        with open(log_path, 'a', encoding='utf-8') as f:
            f.write(f"GET {self.path}\n")
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        if name == 'consumer':
            body = {
                "status": "ok",
                "service": name,
                "session_auth_issuer_registry": {"metadata": {"revision": "rev-live"}},
            }
        else:
            body = {
                "status": "ok",
                "service": name,
                "consumer_entry_session_auth": {"issuer_registry_metadata": {"revision": "rev-live"}},
            }
        self.wfile.write(json.dumps(body).encode())
    def do_POST(self):
        with open(log_path, 'a', encoding='utf-8') as f:
            f.write(f"POST {self.path}\n")
        body = {"status":"ok","service":name}
        if self.path.endswith('/validate'):
            body["valid"] = True
        if self.path.endswith('/reload'):
            body["reloaded"] = True
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())
    def log_message(self, format, *args):
        return
http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler).serve_forever()
PY
chmod +x /tmp/cex-session-auth-runtime-mock.py
: > /tmp/cex-session-auth-consumer.log
: > /tmp/cex-session-auth-matrix.log
python3 /tmp/cex-session-auth-runtime-mock.py 18090 consumer /tmp/cex-session-auth-consumer.log >/tmp/cex-session-auth-consumer.out 2>&1 &
PID1=$!
python3 /tmp/cex-session-auth-runtime-mock.py 18091 matrix /tmp/cex-session-auth-matrix.log >/tmp/cex-session-auth-matrix.out 2>&1 &
PID2=$!
trap 'kill $PID1 $PID2 >/dev/null 2>&1 || true' EXIT
sleep 1
./scripts/reload-session-auth-runtime.sh --action status --service both --consumer-base-url http://127.0.0.1:18090 --matrix-base-url http://127.0.0.1:18091 --consumer-token c1 --matrix-token m1 --compact >/tmp/cex-session-auth-status.json
./scripts/reload-session-auth-runtime.sh --action validate --service both --consumer-base-url http://127.0.0.1:18090 --matrix-base-url http://127.0.0.1:18091 --consumer-token c1 --matrix-token m1 --compact >/tmp/cex-session-auth-validate.json
./scripts/reload-session-auth-runtime.sh --action reload --service both --consumer-base-url http://127.0.0.1:18090 --matrix-base-url http://127.0.0.1:18091 --consumer-token c1 --matrix-token m1 --compact >/tmp/cex-session-auth-reload.json
python3 - <<'PY'
import json, pathlib
status_data = json.loads(pathlib.Path('/tmp/cex-session-auth-status.json').read_text())
validate_data = json.loads(pathlib.Path('/tmp/cex-session-auth-validate.json').read_text())
reload_data = json.loads(pathlib.Path('/tmp/cex-session-auth-reload.json').read_text())
for data, path in [
    (status_data, '/tmp/cex-session-auth-status.json'),
    (validate_data, '/tmp/cex-session-auth-validate.json'),
    (reload_data, '/tmp/cex-session-auth-reload.json'),
]:
    assert data['ok'] is True, path
assert reload_data['coordination']['mode'] == 'consumer_then_matrix'
assert reload_data['postStatus']['liveRevisionMatch'] is True
assert reload_data['postStatus']['consumerLiveRevision'] == 'rev-live'
assert reload_data['postStatus']['matrixLiveRevision'] == 'rev-live'
consumer = pathlib.Path('/tmp/cex-session-auth-consumer.log').read_text().strip().splitlines()
matrix = pathlib.Path('/tmp/cex-session-auth-matrix.log').read_text().strip().splitlines()
assert consumer == [
    'GET /v1/admin/session-auth/issuer-registry/status',
    'POST /v1/admin/session-auth/issuer-registry/validate',
    'POST /v1/admin/session-auth/issuer-registry/reload',
    'GET /v1/admin/session-auth/issuer-registry/status',
], consumer
assert matrix == [
    'GET /v1/admin/consumer-entry-session-auth/status',
    'POST /v1/admin/consumer-entry-session-auth/validate',
    'POST /v1/admin/consumer-entry-session-auth/reload',
    'GET /v1/admin/consumer-entry-session-auth/status',
], matrix
print('ok')
PY
