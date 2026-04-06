# EdgeFabric

**Unified device cloud to test, automate, and scale across real and virtual devices.**

---

## 🚀 Overview

EdgeFabric is a next-generation device cloud platform that enables developers and QA teams to run tests, debug applications, and automate workflows across:

- 📱 Real mobile devices (Android & iOS)
- 🧪 Virtual devices (emulators & simulators)
- 🌐 IoT devices and digital twins

It combines high-fidelity hardware testing with scalable virtual infrastructure, delivering speed, reliability, and cost efficiency.

---

## 🧠 Key Features

### 🔹 Unified Device Abstraction
Single API across all device types:
```rust
device.start()
device.install(app)
device.tap(x, y)
device.stream()
```

### 🔹 Hybrid Device Cloud
- Real devices for accuracy
- Virtual devices for scale
- Intelligent routing between both

### 🔹 Real-Time Streaming
- Low-latency WebRTC streaming
- Remote control via browser
- Live logs and debugging

### 🔹 Automation & CI/CD Integration
- Works with Jenkins, GitHub Actions, etc.
- Parallel test execution
- Scalable pipelines

### 🔹 IoT Support
- Firmware flashing
- Serial/SSH access
- Sensor simulation & digital twins

---

## 🏗️ Architecture

```
                ┌──────────────────────────────┐
                │        User / CI/CD          │
                └────────────┬─────────────────┘
                             │
                    API Gateway / Auth
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
 Device Scheduler     Test Orchestrator     Live Session
        │                    │                    │
        └──────────────┬─────┴─────┬──────────────┘
                       │           │
               Device Control Layer (Rust)
                       │
     ┌──────────────┬──────────────┬──────────────┐
     │              │              │              │
 Android Real   iOS Real      IoT Devices     Virtual Pool
```

---

## ⚙️ Tech Stack

- **Backend:** Rust (control plane), gRPC, REST
- **Streaming:** WebRTC (low latency)
- **Android:** ADB, Emulator (QEMU/KVM)
- **iOS:** Xcode, WebDriverAgent, Simulator
- **IoT:** MQTT, SSH, Serial, QEMU
- **Infra:** Kubernetes (optional), bare metal

---

## 🔁 Workflow

1. Build app (APK/IPA/Firmware)
2. Upload to EdgeFabric
3. Select device pool
4. Execute tests
5. View logs, video, metrics

---

## ⚡ Use Cases

- Mobile app testing at scale
- CI/CD automation pipelines
- Cross-device compatibility testing
- IoT firmware validation
- Performance & network simulation

---

## 🧩 Core Concepts

| Concept            | Description |
|------------------|-------------|
| Device Pool       | Group of devices (real/virtual) |
| Session           | Active interaction with a device |
| Scheduler         | Allocates devices dynamically |
| Control Plane     | Central orchestration system |
| Edge Node         | Physical device host |

---

## 🔐 Security

- Per-session isolation
- Secure device reset after use
- Encrypted communication (TLS/WebRTC)
- Network isolation (VLAN)

---

## 🚀 Roadmap

- [ ] Smart device allocation engine
- [ ] GPU-accelerated streaming
- [ ] Advanced network simulation
- [ ] AI-based test optimization
- [ ] Distributed edge clusters

---

## 🤝 Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

---

## 📄 License

MIT License

---

## 🌐 Vision

> EdgeFabric aims to become the **control plane for global device infrastructure**, enabling seamless testing across physical and virtual environments.
