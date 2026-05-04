<p align="left">
  <img src="docs/assets/EdgeFabric_Logo.png" alt="EdgeFabric" width="420"/>
</p>

<h3 align="left">Unified device cloud to test, automate, and scale across real and virtual devices.</h3>

<p align="left">
  <a href="docs/SYSTEM_DESIGN.md">System Design</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture & Diagrams</a> ·
  <a href="docs/API_REFERENCE.md">API Reference</a>
</p>

---

## Overview

EdgeFabric is a next-generation device cloud platform that enables developers and QA teams to run tests, debug applications, and automate workflows across:

- **Real mobile devices** — Android & iOS
- **Virtual devices** — Emulators & simulators
- **IoT devices** — Hardware boards & digital twins

Built in **Rust** for performance, safety, and low-level device control.

## Latest Updates (2026-05-04)

- iOS coordinate mapping fixed to use logical points for WebDriverAgent input alignment.
- iOS model enrichment now maps ProductType values to bezel profiles for accurate device framing.
- VeerOS host discovery dedupe improved in DB and dashboard merge path.
- VeerOS shell page updated to xterm-based UX and raw-byte websocket input/output handling.
- Dashboard auth behavior clarified: device/discovery endpoints require valid JWT or API key.

---

## Architecture at a Glance

<p align="center">
  <img src="docs/assets/EdgeFabric_SystemArchitecture.png" alt="System Architecture" width="800"/>
</p>

```mermaid
graph TB
    subgraph "Users & CI"
        DEV[Developers / QA]
        CI[CI/CD Pipelines]
    end

    subgraph "Ingress"
        GW[API Gateway + Auth]
    end

    subgraph "Orchestration"
        SCHED[Device Scheduler]
        ORCH[Test Orchestrator]
        LIVE[Live Session Service]
    end

    subgraph "Control Plane — Rust"
        DM[Device Manager]
        CMD[Command Engine]
        STREAM[Streaming Engine]
        STATE[State Manager]
    end

    subgraph "Devices"
        AND[Android Real/Virtual]
        IOS[iOS Real/Virtual]
        IOT[IoT / Digital Twins]
    end

    DEV --> GW
    CI --> GW
    GW --> SCHED
    GW --> ORCH
    GW --> LIVE
    SCHED --> DM
    ORCH --> CMD
    LIVE --> STREAM
    DM --> STATE
    CMD --> STATE
    STREAM --> STATE
    STATE --> AND
    STATE --> IOS
    STATE --> IOT
```

> See [Architecture & Diagrams](docs/ARCHITECTURE.md) for full schematics including network topology and deployment layout.

---

## Key Features

| Feature | Description |
|---------|-------------|
| **Unified Device Abstraction** | Single API across Android, iOS, IoT — real and virtual |
| **Hybrid Device Cloud** | Real devices for accuracy, virtual for scale, intelligent routing |
| **Real-Time Streaming** | < 200 ms latency WebRTC streaming with remote control |
| **CI/CD Native** | First-class GitHub Actions, Jenkins, GitLab CI integration |
| **Parallel Testing** | Distribute test suites across device pools automatically |
| **IoT Support** | Firmware flashing, serial/SSH access, sensor simulation |
| **Secure Multi-Tenancy** | Per-session isolation, device wipe, RBAC, TLS everywhere |

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Core platform | **Rust** (Axum, tonic, tokio) |
| API | REST + gRPC (Protocol Buffers) |
| Streaming | WebRTC (webrtc-rs) |
| Android control | ADB, scrcpy |
| iOS control | WebDriverAgent, Xcode tools |
| IoT | MQTT (rumqttc), SSH, Serial |
| State store | PostgreSQL + Redis |
| Messaging | NATS |
| Virtual devices | QEMU/KVM, Kubernetes |

---

## Workflow

```mermaid
graph LR
    A[Build App<br/>APK / IPA / Firmware] --> B[Upload to EdgeFabric]
    B --> C[Select Device Pool]
    C --> D[Execute Tests<br/>or Start Live Session]
    D --> E[View Results<br/>Logs · Video · Metrics]
```

---

## Core Concepts

| Concept | Description |
|---------|-------------|
| **Device Pool** | Group of real/virtual devices matching a filter |
| **Session** | Active, isolated interaction with a single device |
| **Scheduler** | Allocates devices by availability, type, priority |
| **Control Plane** | Central orchestration — the Rust core |
| **Edge Node** | Physical rack hosting real devices + local agent |

---

## Network Architecture

<p align="center">
  <img src="docs/assets/EdgeFabric_NetworkArchitecture.png" alt="Network Architecture" width="800"/>
</p>

> Multi-region edge deployment with secure VPN tunnels, global load balancing, and CDN/WAF at the perimeter. See [Architecture & Diagrams](docs/ARCHITECTURE.md#network-architecture) for details.

---

## Project Structure

```
EdgeFabric/
├── Cargo.toml                  # Workspace root
├── README.md
├── docs/
│   ├── SYSTEM_DESIGN.md        # Detailed system design
│   ├── ARCHITECTURE.md         # Diagrams & schematics
│   ├── API_REFERENCE.md        # REST & gRPC API spec
│   └── assets/                 # Images, diagrams, PPTX
│       ├── EdgeFabric_Logo.png
│       ├── EdgeFabric_SystemArchitecture.png
│       └── EdgeFabric_NetworkArchitecture.png
├── crates/
│   ├── ef-core/                # Shared types, traits, errors
│   ├── ef-api/                 # REST + gRPC API server
│   ├── ef-scheduler/           # Device allocation engine
│   ├── ef-orchestrator/        # Test job orchestration
│   ├── ef-control/             # Device control plane
│   ├── ef-streaming/           # WebRTC streaming engine
│   ├── ef-agent/               # Edge node agent binary
│   └── ef-cli/                 # Developer CLI tool
├── proto/                      # Protocol Buffer definitions
├── config/                     # Environment configs
└── tests/                      # Integration tests
```

---

## Getting Started

### Quick Start (One-Liner)

**macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/veeringman/EdgeFabric/main/scripts/bootstrap.sh | bash
cd ~/edgefabric
./scripts/setup-mac.sh
```

**Linux (Ubuntu/Debian):**
```bash
curl -fsSL https://raw.githubusercontent.com/veeringman/EdgeFabric/main/scripts/bootstrap.sh | bash
cd ~/edgefabric
./scripts/install.sh
./scripts/build.sh
./scripts/deploy.sh all
```

**Windows (PowerShell — run as Administrator):**
```powershell
irm https://raw.githubusercontent.com/veeringman/EdgeFabric/main/scripts/bootstrap.ps1 | iex
cd $HOME\edgefabric
.\scripts\build.ps1
.\scripts\deploy.ps1 all
```

### Step by Step

If you already have the repo cloned:

```bash
git clone https://github.com/veeringman/EdgeFabric.git
cd EdgeFabric
```

Then follow the platform-specific instructions below.

---

## Prerequisites

| Tool | Version | macOS | Linux | Windows |
|------|---------|-------|-------|---------|
| **Rust** | >= 1.85 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` | same | [rustup.rs](https://rustup.rs) |
| **protoc** | any | `brew install protobuf` | `apt install protobuf-compiler` | `winget install Google.Protobuf` |
| **Docker** | any | `brew install --cask docker` | `curl -fsSL https://get.docker.com \| sh` | Docker Desktop |
| **adb** | any | `brew install --cask android-platform-tools` | `apt install android-tools-adb` | `winget install Google.PlatformTools` |
| **Xcode CLI** | any | `xcode-select --install` | N/A | N/A |
| **libimobiledevice** | any | `brew install libimobiledevice` | N/A | N/A |

Or run the automated installer:

```bash
./scripts/install.sh              # macOS / Linux
.\scripts\install.ps1             # Windows (PowerShell)
./scripts/install.sh --agent-only # edge agent node only
```

---

## Building

```bash
# Debug build (all crates)
./scripts/build.sh

# Release build
./scripts/build.sh --release

# Build specific targets
./scripts/build.sh agent          # ef-agent only
./scripts/build.sh api            # ef-api only
./scripts/build.sh cli            # ef CLI only
./scripts/build.sh dashboard      # WASM dashboard via trunk

# Or use cargo directly
cargo build                       # debug
cargo build --release             # release
cargo build -p ef-agent           # single crate
```

**Windows:**
```powershell
.\scripts\build.ps1               # debug, all
.\scripts\build.ps1 -Release      # release
.\scripts\build.ps1 agent         # agent only
```

**Note:** On machines with <= 8 GB RAM, limit parallelism:
```bash
CARGO_BUILD_JOBS=2 ./scripts/build.sh
```

---

## Deploying

### Local Development (Full Stack)

```bash
# 1. Start infrastructure (Postgres, Redis, NATS, CoTURN)
./scripts/deploy.sh infra

# 2. Run database migrations
DATABASE_URL="postgres://edgefabric:edgefabric@localhost:5432/edgefabric" ./scripts/migrate.sh up

# 3. Start the API server (starts infra automatically if needed)
./scripts/deploy.sh api

# 4. Start the edge agent
./scripts/deploy.sh agent

# Or do it all at once:
./scripts/deploy.sh all
```

### Docker Compose (Full Stack)

```bash
docker compose up -d                    # infra + API
docker compose --profile full up -d     # infra + API + agent
docker compose down -v                  # tear down
```

### Agent-Only Deployment (Edge Node)

When the API server runs elsewhere (e.g., cloud), deploy just the agent:

```bash
# On the edge node (Mac / Linux)
./scripts/install.sh --agent-only
./scripts/build.sh agent

EF__AGENT__CONTROL_PLANE_URL=https://api.example.com:6060 \
EF__AGENT__AGENT_ID=macbook-lab-01 \
EF__AGENT__REGION=us-west \
  ./scripts/deploy.sh agent
```

### macOS One-Shot (Setup + Build + Deploy)

```bash
./scripts/setup-mac.sh                  # full stack on Mac
./scripts/setup-mac.sh --agent-only     # agent only
./scripts/setup-mac.sh --release        # release build
./scripts/setup-mac.sh --step check     # scan for connected devices
```

### Managing Services

```bash
./scripts/deploy.sh status              # check health of all services
./scripts/deploy.sh stop                # stop everything
tail -f logs/ef-api.log                 # watch API logs
tail -f logs/ef-agent.log              # watch agent logs
```

### Service Endpoints

| Service | URL |
|---------|-----|
| API (native) | http://127.0.0.1:6060 |
| API (Docker) | http://localhost:8080 |
| Swagger UI (native) | http://127.0.0.1:6060/swagger-ui |
| Health check | http://127.0.0.1:6060/health |
| gRPC | localhost:50051 |
| NATS monitoring | http://localhost:8222 |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `EF_RUN_MODE` | `development` | Config profile: `development` or `production` |
| `EF__AGENT__AGENT_ID` | hostname | Unique agent identifier |
| `EF__AGENT__REGION` | `local` | Region label for device scheduling |
| `EF__AGENT__CONTROL_PLANE_URL` | `http://localhost:6060` | API server URL |
| `EF__AGENT__TLS_SKIP_VERIFY` | `true` (dev) | Skip TLS cert verification |
| `EF__DATABASE__URL` | (see config) | PostgreSQL connection string |
| `EF__AUTH__JWT_SECRET` | (see config) | JWT signing secret |
| `EF__STREAMING__TURN_PASSWORD` | (see config) | TURN server credential |

---

## Device Setup

### Android Devices

1. Enable **Developer Options**: Settings → About Phone → tap Build Number 7 times
2. Enable **USB Debugging**: Settings → Developer Options → USB Debugging
3. Connect via USB and authorize the computer when prompted
4. Verify: `adb devices -l`

### iOS / iPadOS Devices

1. Enable **Developer Mode** (iOS 16+): Settings → Privacy & Security → Developer Mode → On → Restart
2. Connect via USB (Lightning or USB-C cable)
3. Tap **Trust** on the "Trust This Computer?" dialog and enter your passcode
4. Verify:
   ```bash
   idevice_id -l                            # list UDIDs
   ideviceinfo -k DeviceName                # device name
   xcrun xctrace list devices               # Xcode device list
   ```
5. **Optional (full Xcode):** For app install/testing via `xcrun`, install Xcode from the App Store

### iOS / iPadOS Simulators

1. Install Xcode from the App Store
2. Boot a simulator:
   ```bash
   xcrun simctl list devices available       # see available devices
   xcrun simctl boot "iPhone 15 Pro"         # boot one
   ```
3. EdgeFabric auto-discovers booted simulators and tags them as `virtual` with `simctl` capability

### IoT Devices

EdgeFabric discovers IoT devices via **mDNS** (Bonjour / Avahi):

- **macOS**: Uses built-in `dns-sd` — no install needed
- **Linux**: Install `avahi-utils` (`apt install avahi-utils`)
- Devices must advertise `_edgefabric._tcp` or generic services (`_mqtt._tcp`, `_http._tcp`, `_coap._udp`)

For custom IoT boards (Raspberry Pi, ESP32, Arduino):
1. Ensure the device is on the same network
2. Register an mDNS service or configure SSH access
3. EdgeFabric agent will discover via mDNS scan

### Checking Connected Devices

```bash
# Full device check (macOS)
./scripts/setup-mac.sh --step check

# Or manually:
adb devices -l                           # Android
idevice_id -l                            # iOS (USB)
xcrun xctrace list devices               # iOS (Xcode)
xcrun simctl list devices booted         # simulators
dns-sd -B _edgefabric._tcp              # IoT (macOS)
avahi-browse -t -r _edgefabric._tcp     # IoT (Linux)
```

---

## Security

| Control | Implementation |
|---------|---------------|
| Transport | TLS 1.3 (API), DTLS + SRTP (WebRTC) |
| Authentication | OAuth 2.0 / JWT / API keys |
| Session isolation | VLAN segmentation, container namespaces |
| Device sanitization | Full wipe + snapshot restore per session |
| Access control | RBAC with tenant-scoped permissions |
| Audit | Immutable operation log |

---

## Roadmap

- [ ] Smart AI-powered device scheduling
- [ ] GPU-accelerated streaming
- [ ] Advanced network simulation (latency, packet loss)
- [ ] Visual regression testing
- [ ] Distributed edge clusters with autonomous failover
- [ ] Plugin system for custom device protocols

---

## Documentation

| Document | Description |
|----------|-------------|
| [System Design](docs/SYSTEM_DESIGN.md) | Detailed design: layers, modules, data flow, security |
| [Architecture & Diagrams](docs/ARCHITECTURE.md) | All schematics: system, network, deployment, sequences |
| [API Reference](docs/API_REFERENCE.md) | REST endpoints, gRPC services, error codes |
| [Getting Started](#getting-started) | Quick start, install, build, deploy instructions |
| [Device Setup](#device-setup) | Android, iOS/iPadOS, IoT device preparation |

---

## Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

---

## License

MIT License

---

<p align="center">
  <img src="docs/assets/EdgeFabric_Logo.png" alt="EdgeFabric" width="180"/>
  <br/>
  <i>The control plane for global device infrastructure.</i>
</p>
