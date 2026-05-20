# EdgeFabric - System Design Document

## 📌 Overview

EdgeFabric is a unified device cloud platform designed to provide scalable, real-time access to both **real and virtual devices** across Android, iOS, and IoT ecosystems.

This document describes the detailed system architecture, components, data flow, and design principles.

Refer to the **System Architecture Diagram** (Figma-style) for visual representation.

---

## 🧠 Design Goals

- Unified abstraction across device types
- Horizontal scalability (virtual + real devices)
- Low-latency interaction (WebRTC streaming)
- High reliability & fault tolerance
- CI/CD friendly
- Secure multi-tenant architecture

---

## 🏗️ High-Level Architecture

The system is divided into 5 primary layers:

1. User / CI Pipeline Layer
2. API Gateway & Auth Layer
3. Orchestration Layer
4. Device Control Plane (Core)
5. Device Infrastructure Layer

---

## 1️⃣ User / CI Pipeline Layer

### Components:
- Developers
- QA Engineers
- CI/CD systems (GitHub Actions, Jenkins)

### Responsibilities:
- Submit test jobs
- Request device sessions
- Monitor execution results

---

## 2️⃣ API Gateway & Authentication Layer

### Responsibilities:
- Request routing
- Authentication (OAuth / API keys)
- Rate limiting
- Multi-tenant isolation

### Key APIs:
- `/devices`
- `/sessions`
- `/tests/run`
- `/logs`
- `/stream`

---

## 3️⃣ Orchestration Layer

### Components:

#### 🔹 Device Scheduler
- Allocates devices based on:
  - Availability
  - Device type/version
  - Test priority

#### 🔹 Test Orchestrator
- Executes test workflows
- Handles retries
- Parallel execution

#### 🔹 Live Session Service
- Manages real-time sessions
- Handles WebRTC signaling
- Streams video + logs

---

## 4️⃣ Device Control Plane (Core)

This is the **heart of EdgeFabric**.

### Responsibilities:
- Device discovery
- Health monitoring
- Command execution
- Input injection
- Session lifecycle management

### Internal Modules:

#### 🔹 Device Manager
Tracks all connected devices (real + virtual)

#### 🔹 Command Engine
Executes commands:
- Install apps
- Run tests
- Capture logs

#### 🔹 Streaming Engine
- Frame capture
- Encoding (H.264)
- WebRTC delivery

#### 🔹 State Manager
- Tracks session/device state
- Handles failures & recovery

---

## 5️⃣ Device Infrastructure Layer

### A. Real Devices

#### Android:
- Controlled via ADB
- USB-connected to Linux hosts

#### iOS:
- Connected to macOS hosts
- Controlled via WebDriverAgent

#### IoT:
- Connected via:
  - Serial
  - SSH
  - Network

---

### B. Virtual Devices

#### Android Emulators:
- QEMU + KVM आधारित
- Snapshot boot
- Headless mode

#### iOS Simulators:
- CoreSimulator आधारित
- Runs as macOS processes

#### IoT Simulation:
- QEMU-based firmware emulation
- MQTT-based simulation
- Digital twin models

---

## 🔁 Data Flow

1. User submits test request
2. API Gateway authenticates
3. Scheduler selects device
4. Orchestrator triggers execution
5. Control Plane interacts with device
6. Streaming + logs sent to user
7. Results stored & returned

---

## ⚡ Streaming Architecture

```
Device → Frame Capture → Encoder → WebRTC → Browser
```

- Ultra low latency (<200ms)
- Supports live interaction

---

## 🔐 Security Model

- Session isolation
- Device reset after use
- Encrypted channels (TLS/WebRTC)
- Role-based access control

---

## 📦 Deployment Model

### Edge Nodes:
- Physical racks with devices
- Local agents connected to control plane

### Control Plane:
- Centralized (cloud or on-prem)

### Optional:
- Kubernetes for virtual devices

---

## 📊 Scalability Strategy

- Horizontal scaling for:
  - Virtual devices
  - Orchestration services
- Sharding device pools
- Region-based edge clusters

---

## ⚠️ Failure Handling

- Device health checks
- Auto-restart of emulators
- Session retry logic
- Graceful degradation

---

## 🚀 Future Enhancements

- AI-based scheduling
- Smart test routing
- Network simulation engine
- GPU acceleration
- Edge-distributed clusters
- **Electronic Circuit / PCB Design Studio** — AI-assisted + manual board
  design with realistic 3D visuals, drag-and-drop, and config-driven
  generation through to fabrication. See `PCB_DESIGN_INTEGRATION_PLAN.md`.
- **Chip Design (Silicon → Tape-Out)** — RTL workspace, verification,
  synthesis, PnR, and sign-off on open PDKs with FPGA flow parity and an
  AI co-pilot. See `CHIP_DESIGN_INTEGRATION_PLAN.md`.

---

## 🧩 Key Insight

> EdgeFabric acts as a **control plane for global device infrastructure**, abstracting physical and virtual devices into a single programmable platform.

---

## 📎 Reference

See the **EdgeFabric System Architecture Diagram** (dark theme, Figma-style) for visual mapping of all components.
