# Device Registration & Network Discovery

EdgeFabric supports three categories of devices: **Android**, **iOS**, and **IoT** (Raspberry Pi, Arduino, ESP32, etc.). Devices can be discovered automatically or registered manually.

---

## Automatic Discovery

The EdgeFabric agent scans for devices periodically (default: every 5 seconds in development, 30 seconds in production). Configure via `config/development.toml`:

```toml
[agent]
device_scan_interval_seconds = 5
```

### Discovery Methods

| Platform | Connection | Method | Auto-detected Capabilities |
|----------|-----------|--------|---------------------------|
| Android | USB | `adb devices -l` | `["adb"]` |
| Android | WiFi (ADB over TCP) | `adb connect <ip>:5555` | `["adb"]` |
| iOS | USB | `xcrun xctrace` / `idevice_id -l` | `["usb"]` |
| iOS | Simulator | `xcrun simctl list --json` | `["simctl"]` |
| IoT | USB Serial | `/dev/ttyACM*`, `/dev/cu.usbmodem*` | `["serial", "usb"]` |
| IoT | WiFi (mDNS) | `avahi-browse _edgefabric._tcp` | `["mdns", "ssh"]` |

---

## IoT / Raspberry Pi via WiFi (mDNS)

### Prerequisites

The Pi and the machine running `ef-agent` must be on the **same local network** (WiFi or LAN).

### Step 1: Install Avahi on the Pi

```bash
sudo apt update && sudo apt install -y avahi-daemon avahi-utils
sudo systemctl enable avahi-daemon
sudo systemctl start avahi-daemon
```

### Step 2: Advertise the EdgeFabric Service

Create a service file on the Pi:

```bash
sudo tee /etc/avahi/services/edgefabric.service <<'EOF'
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name>Pi5-Lab</name>
  <service>
    <type>_edgefabric._tcp</type>
    <port>22</port>
    <txt-record>model=Raspberry Pi 5</txt-record>
    <txt-record>os=iot</txt-record>
    <txt-record>version=bookworm</txt-record>
  </service>
</service-group>
EOF

sudo systemctl restart avahi-daemon
```

### Step 3: Verify Discovery

On the agent machine:

```bash
avahi-browse -t _edgefabric._tcp -r
```

You should see the Pi listed. The agent will auto-discover it on the next scan cycle and register it with the control plane.

### Supported mDNS Service Types

The agent scans for these service types:

- `_edgefabric._tcp` — EdgeFabric-aware devices (primary)
- `_mqtt._tcp` — MQTT brokers
- `_coap._udp` — CoAP endpoints
- `_http._tcp` — HTTP services

---

## Android via WiFi (ADB over TCP)

### Step 1: Connect Once via USB

```bash
adb devices                     # Verify USB connection
adb tcpip 5555                  # Switch device to TCP mode
adb disconnect                  # Optional: disconnect USB
```

### Step 2: Connect Over WiFi

```bash
# Find the device's IP (Settings → About Phone → IP Address)
adb connect 192.168.1.105:5555
```

The agent will detect the WiFi-connected device via `adb devices` and register it normally. The device IP is also extracted for remote streaming.

### Persistent WiFi ADB

To auto-reconnect on boot, add to the device's `init.rc` or use a Tasker profile:

```bash
setprop service.adb.tcp.port 5555
stop adbd
start adbd
```

---

## iOS Streaming Requirements

iOS streaming requires **ffmpeg** installed on the macOS edge node:

```bash
brew install ffmpeg
```

The pipeline: `ffmpeg -f avfoundation` → captures iOS screen via USB → encodes H.264 via `libx264` (ultrafast/zerolatency) → streams to dashboard.

Android uses `scrcpy` (outputs H.264 directly, no ffmpeg needed).

---

## Manual Device Registration

### Via API

```bash
# Login first
TOKEN=$(curl -s http://localhost:6060/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin"}' | jq -r .token)

# Register a device
curl -X POST http://localhost:6060/api/v1/devices \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Raspberry Pi 5 - Kitchen",
    "serial": "pi5-10000000abcdef",
    "os": "iot",
    "os_version": "bookworm",
    "kind": "real",
    "region": "local",
    "capabilities": ["ssh", "gpio", "camera", "mdns"]
  }'
```

### Via CLI

```bash
ef device list                              # List all devices
ef device list --os iot --status available  # Filter IoT devices
ef device health <device-id>                # Check heartbeat health
```

---

## Heartbeat & Offline Detection

Once registered, the agent sends heartbeats every 10 seconds (configurable). The API server runs a **stale device checker** every 15 seconds — any device with no heartbeat for 60 seconds is automatically marked **offline**.

```toml
# config/default.toml
[agent]
heartbeat_interval_seconds = 10
```

---

## Device API Reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/v1/devices` | Register new device |
| `GET` | `/api/v1/devices` | List devices (filters: `?os=iot&status=available&region=local`) |
| `GET` | `/api/v1/devices/:id` | Get device details |
| `PUT` | `/api/v1/devices/:id` | Update device |
| `DELETE` | `/api/v1/devices/:id` | Remove device |
| `PUT` | `/api/v1/devices/:id/status` | Change status |
| `POST` | `/api/v1/devices/:id/heartbeat` | Send heartbeat |
| `GET` | `/api/v1/devices/:id/health` | Check health |
| `PUT` | `/api/v1/devices/:id/preference` | Set preference (e.g., bezel style) |
