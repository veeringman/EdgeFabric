#!/usr/bin/env bash
# EdgeFabric — End-to-End Virtual IoT Shell Test
#
# Verifies the full live-device-viewer shell path:
#   1. API + agent are healthy
#   2. A virtual_iot device exists (reuses existing or fails with a clear error)
#   3. A folded virtual_iot backend is created (via POST /devices/virtual-iot/backends)
#   4. The backend is attached to the device
#   5. GET /devices/{id}/virtual-iot/shell returns live metadata
#   6. WebSocket /devices/{id}/virtual-iot/shell/ws opens, receives a non-error frame
#
# Usage:
#   ./scripts/e2e-shell.sh                  # auto-pick first virtual_iot device
#   ./scripts/e2e-shell.sh <device_id>      # use specific device
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
source "$SCRIPT_DIR/ports.env"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

API="$EF_URL_API"
API_KEY="$EF_DEV_API_KEY"
H_KEY=(-H "X-Api-Key: $API_KEY")
H_JSON=(-H "Content-Type: application/json")

curl_json() { curl -sS "${H_KEY[@]}" "${H_JSON[@]}" "$@"; }

require() { command -v "$1" >/dev/null || { err "missing dependency: $1"; exit 1; }; }
require curl
require jq
require python3

# ── 1. Health checks ─────────────────────────────────────────────
info "Checking API health @ $API ..."
curl -sf "$API/health" >/dev/null || { err "API not healthy at $API/health"; exit 1; }
ok "API healthy"

# ── 2. Pick device ───────────────────────────────────────────────
DEVICE_ID="${1:-}"
if [[ -z "$DEVICE_ID" ]]; then
  info "Auto-picking a virtual_iot device..."
  DEVICE_ID="$(PGPASSWORD=edgefabric psql -h localhost -U edgefabric -d edgefabric -At -c \
    "select d.id from devices d join virtual_iot_configs c on c.device_id=d.id where d.status='available' order by d.updated_at desc limit 1;")"
  if [[ -z "$DEVICE_ID" ]]; then
    err "No virtual_iot device found. Create one via the dashboard or API first."
    exit 1
  fi
fi
ok "Using device: $DEVICE_ID"

DEVICE_JSON="$(curl_json "$API/api/v1/devices/$DEVICE_ID")"
echo "$DEVICE_JSON" | jq -r '{id: .device.id, name: .device.name, kind: .device.kind, os: .device.os, status: .device.status}' || true

CONTROLLER_TYPE="$(PGPASSWORD=edgefabric psql -h localhost -U edgefabric -d edgefabric -At -c \
  "select controller_type from virtual_iot_configs where device_id='$DEVICE_ID';")"
[[ -z "$CONTROLLER_TYPE" ]] && CONTROLLER_TYPE="xiao-esp32c6"
info "Controller type: $CONTROLLER_TYPE"

# ── 3. Ensure a backend exists and is attached ───────────────────
EXISTING_BACKEND="$(PGPASSWORD=edgefabric psql -h localhost -U edgefabric -d edgefabric -At -c \
  "select id from virtual_iot_backends where attached_device_id='$DEVICE_ID' limit 1;")"

if [[ -n "$EXISTING_BACKEND" ]]; then
  ok "Reusing attached backend: $EXISTING_BACKEND"
  BACKEND_ID="$EXISTING_BACKEND"
else
  BACKEND_NAME="e2e-$(date +%s)"
  info "Creating backend '$BACKEND_NAME' (provider=veeros-vm, controller=$CONTROLLER_TYPE)..."
  CREATE_BODY=$(jq -cn \
    --arg n "$BACKEND_NAME" \
    --arg ct "$CONTROLLER_TYPE" \
    '{name:$n, provider:"veeros-vm", controller_type:$ct, controller_types:[$ct], region:"local", auto_start:true}')
  CREATE_RESP="$(curl_json -X POST -d "$CREATE_BODY" "$API/api/v1/devices/virtual-iot/backends")"
  BACKEND_ID="$(echo "$CREATE_RESP" | jq -r '.backend.id // empty')"
  if [[ -z "$BACKEND_ID" ]]; then
    err "Backend create failed:"
    echo "$CREATE_RESP" | jq .
    exit 1
  fi
  ok "Created backend: $BACKEND_ID"

  info "Attaching backend to device..."
  ATTACH_BODY=$(jq -cn --arg b "$BACKEND_ID" '{backend_id:$b}')
  ATTACH_RESP="$(curl_json -X POST -d "$ATTACH_BODY" "$API/api/v1/devices/$DEVICE_ID/virtual-iot/backend/attach")"
  echo "$ATTACH_RESP" | jq '{backend_status: .backend.status, attached: .backend.attached_device_id}' || true
  ok "Attached"
fi

# ── 4. Shell info ────────────────────────────────────────────────
info "GET /virtual-iot/shell ..."
SHELL_INFO="$(curl_json "$API/api/v1/devices/$DEVICE_ID/virtual-iot/shell")"
echo "$SHELL_INFO" | jq '{host, port, connect_command, vm_name, runtime_status: .backend.config.runtime_status}' 2>/dev/null || echo "$SHELL_INFO"
HTTP_OK="$(curl -s -o /dev/null -w '%{http_code}' "${H_KEY[@]}" "$API/api/v1/devices/$DEVICE_ID/virtual-iot/shell")"
if [[ "$HTTP_OK" != "200" ]]; then
  err "shell-info returned HTTP $HTTP_OK"; exit 1
fi
ok "shell-info HTTP 200"

# ── 5. WebSocket smoke test (2 frames) ───────────────────────────
info "Connecting WebSocket..."
WS_PORT="$EF_PORT_API_HTTP"
python3 - <<PY
import asyncio, json, sys
import websockets

url = "ws://127.0.0.1:${WS_PORT}/api/v1/devices/${DEVICE_ID}/virtual-iot/shell/ws"
headers = [("X-Api-Key", "${API_KEY}")]

async def go():
    got_ready = False
    got_output = False
    try:
        async with websockets.connect(url, additional_headers=headers, open_timeout=5, close_timeout=5) as ws:
            print("WS_CONNECTED")
            for i in range(4):
                try:
                    frame = await asyncio.wait_for(ws.recv(), timeout=5)
                except asyncio.TimeoutError:
                    print(f"FRAME{i}: timeout"); break
                if isinstance(frame, (bytes, bytearray)):
                    print(f"FRAME{i} bin len={len(frame)}")
                    got_output = True
                else:
                    print(f"FRAME{i} txt: {frame[:300]}")
                    try:
                        j = json.loads(frame)
                        if j.get("type") == "error":
                            print("ERROR_FRAME", j.get("message"))
                            sys.exit(2)
                        if j.get("type") == "shell_ready":
                            got_ready = True
                    except Exception:
                        got_output = True
    except websockets.exceptions.ConnectionClosedOK:
        pass
    except Exception as e:
        print("WS_FAIL", type(e).__name__, e)
        if not (got_ready or got_output):
            sys.exit(3)
    if got_ready or got_output:
        print("RESULT_OK ready={} output={}".format(got_ready, got_output))
        sys.exit(0)
    print("RESULT_FAIL no frames received"); sys.exit(4)

asyncio.run(go())
PY
rc=$?

if [[ $rc -eq 0 ]]; then
  ok "E2E shell test PASSED"
  exit 0
elif [[ $rc -eq 2 ]]; then
  err "E2E shell test FAILED — backend surfaced an error frame"
  exit 2
else
  err "E2E shell test FAILED — WebSocket failed (rc=$rc)"
  exit $rc
fi
