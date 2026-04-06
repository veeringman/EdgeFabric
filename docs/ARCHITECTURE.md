<p align="center">
  <img src="assets/EdgeFabric_Logo.png" alt="EdgeFabric" width="360"/>
</p>

<h1 align="center">Architecture & Diagrams</h1>

<p align="center">
  Visual reference for EdgeFabric's system topology, network layout, and component interactions.
</p>

---

## Table of Contents

- [System Architecture Overview](#system-architecture-overview)
- [Network Architecture](#network-architecture)
- [Layered Component Model](#layered-component-model)
- [Device Control Flow](#device-control-flow)
- [Streaming Pipeline](#streaming-pipeline)
- [Session Lifecycle](#session-lifecycle)
- [Deployment Topology](#deployment-topology)

---

## System Architecture Overview

> Full visual reference — see the high-resolution diagram below.

<p align="center">
  <img src="assets/EdgeFabric_SystemArchitecture.png" alt="EdgeFabric System Architecture" width="900"/>
</p>

The platform is organized in five horizontal layers, each with clear responsibilities:

```mermaid
graph TB
    subgraph "Layer 1 — Users & CI"
        DEV[Developers / QA]
        CI[CI/CD Pipelines<br/>GitHub Actions · Jenkins]
    end

    subgraph "Layer 2 — Ingress"
        GW[API Gateway]
        AUTH[Auth Service<br/>OAuth · API Keys]
    end

    subgraph "Layer 3 — Orchestration"
        SCHED[Device Scheduler]
        ORCH[Test Orchestrator]
        LIVE[Live Session Service]
    end

    subgraph "Layer 4 — Control Plane  (Rust Core)"
        DM[Device Manager]
        CMD[Command Engine]
        STREAM[Streaming Engine]
        STATE[State Manager]
    end

    subgraph "Layer 5 — Device Infrastructure"
        AND[Android<br/>ADB · USB]
        IOS[iOS<br/>WDA · Xcode]
        IOT[IoT<br/>MQTT · Serial · SSH]
        VIRT[Virtual Pool<br/>QEMU/KVM · Simulators]
    end

    DEV --> GW
    CI --> GW
    GW --> AUTH
    AUTH --> SCHED
    AUTH --> ORCH
    AUTH --> LIVE
    SCHED --> DM
    ORCH --> CMD
    LIVE --> STREAM
    DM --> STATE
    CMD --> STATE
    STREAM --> STATE
    STATE --> AND
    STATE --> IOS
    STATE --> IOT
    STATE --> VIRT
```

---

## Network Architecture

> Full visual reference — see the high-resolution diagram below.

<p align="center">
  <img src="assets/EdgeFabric_NetworkArchitecture.png" alt="EdgeFabric Network Architecture" width="900"/>
</p>

```mermaid
graph TB
    subgraph "Public Ingress"
        GLB[Global Load Balancer<br/>CDN · WAF · Anycast DNS]
    end

    subgraph "API & Control Plane Network"
        APIGW[API Gateway]
        AUTHSVC[Auth Service]
        APISVC[API Services]
    end

    subgraph "Real-Time Streaming Network"
        WEBRTC[WebRTC Signaling Server]
        TURN[TURN Servers]
    end

    subgraph "Edge Nodes Network"
        AGENT[EdgeFabric Agent]
        USB[USB Hubs]
    end

    GLB --> APIGW
    GLB --> WEBRTC
    APIGW --> AUTHSVC
    APIGW --> APISVC
    APISVC -->|gRPC / TLS| AGENT
    WEBRTC --> TURN
    TURN -->|Media Relay| AGENT
    AGENT --> USB

    subgraph "US Region"
        US_CP[Control Plane]
        US_DEV[Android Devices<br/>ADB over USB/TCP]
    end

    subgraph "India Region"
        IN_CP[Scheduler / Orchestrator]
        IN_DEV[iOS Devices<br/>usbmuxd / TCP Bridge]
    end

    subgraph "EU Region"
        EU_CP[EdgeFabric Agent]
        EU_DEV[Virtual Devices<br/>Emulator / Simulator]
    end

    AGENT -->|Secure VPN| US_CP
    AGENT -->|Secure VPN| IN_CP
    AGENT -->|Secure VPN| EU_CP
    US_CP --> US_DEV
    IN_CP --> IN_DEV
    EU_CP --> EU_DEV
```

---

## Layered Component Model

A detailed breakdown of every module inside the control plane.

```mermaid
classDiagram
    class DeviceManager {
        +register_device()
        +deregister_device()
        +list_devices()
        +get_health()
    }

    class CommandEngine {
        +install_app()
        +run_test()
        +capture_screenshot()
        +inject_input()
    }

    class StreamingEngine {
        +start_capture()
        +encode_h264()
        +deliver_webrtc()
        +stop_capture()
    }

    class StateManager {
        +track_session()
        +recover_failure()
        +persist_state()
    }

    class DeviceScheduler {
        +find_available()
        +allocate()
        +release()
    }

    class TestOrchestrator {
        +enqueue_job()
        +parallel_dispatch()
        +collect_results()
        +retry()
    }

    class LiveSessionService {
        +open_session()
        +signal_webrtc()
        +close_session()
    }

    DeviceScheduler --> DeviceManager
    TestOrchestrator --> CommandEngine
    LiveSessionService --> StreamingEngine
    DeviceManager --> StateManager
    CommandEngine --> StateManager
    StreamingEngine --> StateManager
```

---

## Device Control Flow

End-to-end flow for a test execution request.

```mermaid
sequenceDiagram
    actor User
    participant GW as API Gateway
    participant Auth as Auth Service
    participant Sched as Scheduler
    participant Orch as Orchestrator
    participant Ctrl as Control Plane
    participant Dev as Device

    User->>GW: POST /tests/run
    GW->>Auth: Validate token
    Auth-->>GW: OK
    GW->>Sched: Request device (type, OS, version)
    Sched->>Ctrl: Query available devices
    Ctrl-->>Sched: Device list
    Sched->>Ctrl: Allocate device
    Ctrl-->>Sched: Device locked
    Sched-->>GW: Device assigned
    GW->>Orch: Start test job
    Orch->>Ctrl: Install app
    Ctrl->>Dev: ADB install / WDA install
    Dev-->>Ctrl: Installed
    Orch->>Ctrl: Execute test suite
    Ctrl->>Dev: Run commands
    Dev-->>Ctrl: Results + logs
    Ctrl-->>Orch: Execution complete
    Orch-->>GW: Job result
    GW-->>User: 200 OK + results
    Ctrl->>Dev: Reset device
```

---

## Streaming Pipeline

Low-latency WebRTC streaming from device to browser.

```mermaid
graph LR
    A[Device Screen] -->|Frame Capture| B[Capture Agent<br/>scrcpy / mirroring]
    B -->|Raw Frames| C[H.264 Encoder<br/>hardware or software]
    C -->|Encoded Stream| D[WebRTC Packager<br/>RTP / SRTP]
    D -->|Signaling| E[Signaling Server]
    D -->|Media| F[TURN Server<br/>relay if needed]
    E -->|SDP/ICE| G[Browser Client]
    F -->|Media Relay| G
    G -->|User Input| H[Input Relay]
    H -->|Touch / Key events| A
```

**Target Latency:** < 200 ms end-to-end

---

## Session Lifecycle

State machine for a device session.

```mermaid
stateDiagram-v2
    [*] --> Requested
    Requested --> Queued : Device pool has capacity
    Requested --> Rejected : No capacity / auth failure
    Queued --> Allocating : Scheduler picks device
    Allocating --> Active : Device locked & ready
    Allocating --> Failed : Device unreachable
    Active --> Streaming : WebRTC connected
    Active --> Testing : Test job running
    Streaming --> Active : Stream ended
    Testing --> Active : Test complete
    Active --> Releasing : User ends session / timeout
    Releasing --> Resetting : Wipe & restore device
    Resetting --> Idle : Ready for next session
    Failed --> [*]
    Rejected --> [*]
    Idle --> [*]
```

---

## Deployment Topology

```mermaid
graph TB
    subgraph "Cloud / On-Prem Control Plane"
        API[API Gateway<br/>+ Auth]
        SVC[Orchestration Services]
        DB[(PostgreSQL<br/>State Store)]
        REDIS[(Redis<br/>Session Cache)]
        S3[(Object Store<br/>Artifacts · Logs)]
    end

    subgraph "Edge Node — Rack A"
        AGENT_A[EdgeFabric Agent]
        HUB_A[USB Hub]
        DEV_A1[Pixel 7]
        DEV_A2[Galaxy S24]
        DEV_A3[IoT Board]
    end

    subgraph "Edge Node — Rack B  (macOS)"
        AGENT_B[EdgeFabric Agent]
        HUB_B[USB Hub]
        DEV_B1[iPhone 15]
        DEV_B2[iPad Pro]
    end

    subgraph "Virtual Pool (K8s Cluster)"
        VM1[Android Emulator x N]
        VM2[iOS Simulator x N]
        VM3[QEMU IoT x N]
    end

    API --> SVC
    SVC --> DB
    SVC --> REDIS
    SVC --> S3
    SVC -->|gRPC + TLS| AGENT_A
    SVC -->|gRPC + TLS| AGENT_B
    SVC -->|K8s API| VM1
    SVC -->|K8s API| VM2
    SVC -->|K8s API| VM3
    AGENT_A --> HUB_A
    HUB_A --> DEV_A1
    HUB_A --> DEV_A2
    HUB_A --> DEV_A3
    AGENT_B --> HUB_B
    HUB_B --> DEV_B1
    HUB_B --> DEV_B2
```

---

<p align="center"><i>Back to <a href="../README.md">README</a> · <a href="SYSTEM_DESIGN.md">System Design</a> · <a href="API_REFERENCE.md">API Reference</a></i></p>
