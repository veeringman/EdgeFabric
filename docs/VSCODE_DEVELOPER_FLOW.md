# VS Code Developer Flow (Microsoft Code)

This guide describes how developers can connect, build, and deploy through EdgeFabric directly from Visual Studio Code.

## 1) Prerequisites

- VS Code (latest stable)
- Access to EdgeFabric API endpoint (`/api/v1`)
- API key for your tenant
- Network path to registered runners and target devices

## 2) Authenticate From VS Code Terminal

Use your API key to exchange a JWT:

```bash
curl -sS -X POST "$EDGEFABRIC_URL/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"api_key":"'$EDGEFABRIC_API_KEY'"}'
```

Export the returned token:

```bash
export EF_TOKEN="<jwt-token>"
```

## 3) Submit iOS Build (macOS runners)

```bash
curl -sS -X POST "$EDGEFABRIC_URL/api/v1/builds/ios" \
  -H "Authorization: Bearer $EF_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "source_ref": "project-main",
    "project_path": "/Users/builder/work/my-ios-app",
    "scheme": "MyApp",
    "configuration": "Debug",
    "target_device_id": "<ios-device-uuid>",
    "required_runner_labels": ["ios"]
  }'
```

## 4) Submit IoT Build/Deploy (Linux runners)

ESP32C6 example:

```bash
curl -sS -X POST "$EDGEFABRIC_URL/api/v1/builds/iot/deploy" \
  -H "Authorization: Bearer $EF_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "source_ref": "iot-main",
    "project_path": "/home/builder/work/esp32c6-app",
    "target_device_id": "<iot-device-uuid>",
    "board_profile": "esp32c6",
    "deploy_command": "pio run -t upload",
    "required_runner_capabilities": ["linux_runner", "iot_toolchain", "esp32"],
    "required_runner_labels": ["iot"]
  }'
```

Raspberry Pi example:

```bash
curl -sS -X POST "$EDGEFABRIC_URL/api/v1/builds/iot/deploy" \
  -H "Authorization: Bearer $EF_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "source_ref": "iot-main",
    "project_path": "/home/builder/work/rpi-app",
    "target_device_id": "<rpi-device-uuid>",
    "board_profile": "rpi",
    "deploy_command": "cmake -S . -B build && cmake --build build",
    "required_runner_capabilities": ["linux_runner", "iot_toolchain", "rpi"],
    "required_runner_labels": ["iot"]
  }'
```

## 5) Submit Quantum Program (VeerOS hosts)

Virtual backend example:

```bash
curl -sS -X POST "$EDGEFABRIC_URL/api/v1/builds/quantum/run" \
  -H "Authorization: Bearer $EF_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "source_ref": "quantum-main",
    "project_path": "/home/builder/work/qprog",
    "run_command": "python3 main.py",
    "backend_kind": "virtual",
    "backend_name": "veeros-vqp-1",
    "required_runner_capabilities": ["veeros_host", "quantum_sdk", "quantum_simulator"],
    "required_runner_labels": ["quantum"]
  }'
```

Real backend example:

```bash
curl -sS -X POST "$EDGEFABRIC_URL/api/v1/builds/quantum/run" \
  -H "Authorization: Bearer $EF_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "source_ref": "quantum-main",
    "project_path": "/home/builder/work/qprog",
    "run_command": "python3 main.py",
    "backend_kind": "real",
    "backend_name": "ibm-q-backend",
    "required_runner_capabilities": ["veeros_host", "quantum_sdk", "quantum_hardware"],
    "required_runner_labels": ["quantum"]
  }'
```

## 6) Monitor, Logs, Artifacts

List builds:

```bash
curl -sS "$EDGEFABRIC_URL/api/v1/builds?limit=50&offset=0" \
  -H "Authorization: Bearer $EF_TOKEN"
```

Get build details:

```bash
curl -sS "$EDGEFABRIC_URL/api/v1/builds/<build-id>" \
  -H "Authorization: Bearer $EF_TOKEN"
```

Tail logs:

```bash
curl -sS "$EDGEFABRIC_URL/api/v1/builds/<build-id>/logs" \
  -H "Authorization: Bearer $EF_TOKEN"
```

List artifacts:

```bash
curl -sS "$EDGEFABRIC_URL/api/v1/builds/<build-id>/artifacts" \
  -H "Authorization: Bearer $EF_TOKEN"
```

## 7) Optional VS Code Tasks

You can create tasks that call these endpoints for one-click operations from the command palette.

Suggested task labels:

- `EdgeFabric: Submit iOS Build`
- `EdgeFabric: Submit IoT Deploy`
- `EdgeFabric: Submit Quantum Run`
- `EdgeFabric: Tail Build Logs`

## 8) Dashboard Compatibility

All submitted jobs appear in the Dashboard Builds page:

- iOS builds
- IoT deploy jobs (including Virtual IoT targets)
- Quantum program runs on VeerOS/quantum runners
