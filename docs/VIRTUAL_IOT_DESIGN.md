# Virtual IoT — High-Level Design

> **Status:** Design phase · Not yet implemented  
> **Phase:** P17 (see TODO.md)  
> **Author:** EdgeFabric team  
> **Updated:** 2026-04-10

---

## 1. Overview

Virtual IoT is a new device category in EdgeFabric that lets users spin up a fully software-emulated microcontroller with a configurable set of sensors — no physical hardware required. The controller generates realistic synthetic sensor data, and the Live Device Viewer renders a **graphical circuit-board shell** with animated sensor widgets (gauges, sparklines, LED grids, GPS tracker) alongside the existing session controls.

### User Flow

1. **Navigate to `/virtual-iot/new`** — a dedicated full-page **IoT Designer** (not a modal).  
2. **Pick a controller profile** (ESP32, Arduino, RPi Pico, etc.) — the page renders a live preview of the PCB board shell SVG on the left.  
3. **Toggle sensors** — checkboxes grouped by category (Environmental, Motion, Light, Location, Power, Audio, Connectivity). Enabled sensors appear as mini-widget previews on the right.  
4. **Set metadata** — device name, region, simulation rate.  
5. **Save** — `POST /api/v1/devices/virtual-iot` → device + config persisted to DB.  
6. The saved virtual IoT device **immediately appears in the Devices list** (`/devices`) with `os = virtual_iot`, `kind = virtual`, and status `available`.  
7. **Edit later** — `/virtual-iot/:id/edit` reopens the same designer with the saved config pre-populated. Changes are persisted via `PATCH`.  
8. From the **Devices page**, the user clicks the device row to open a Live Session (`/sessions/live`) where the graphical emulator runs.

**Design goals:**
- Dedicated configuration page with live board preview — not a modal  
- Saved configs persist in DB and appear as normal devices in the device list  
- Predefined catalog of common sensors; user enables the ones they need  
- Sensor data is streamed over the same WebSocket path as screen frames (`/api/v1/devices/:id/stream`)  
- Graphical in-browser widgets give an instant visual of simulated readings  
- Fully scriptable via the CLI (`ef devices virtual-iot create ...`)  
- Lays groundwork for scenario injection (P17.x future work)

---

## 2. Controller Profiles

| Profile ID | Display Name | Flash (KB) | RAM (KB) | Connectivity | Notes |
|---|---|---|---|---|---|
| `esp32-devkit` | ESP32 DevKit V1 | 4096 | 520 | WiFi + BLE | Most popular; default |
| `arduino-uno` | Arduino Uno R3 | 32 | 2 | UART only | Classic maker board |
| `rpi-pico` | Raspberry Pi Pico | 2048 | 264 | UART + SPI | RP2040 chip |
| `stm32-nucleo` | STM32 Nucleo-F4 | 1024 | 192 | UART + SPI + I²C | ARM Cortex-M4 |
| `generic-mcu` | Generic MCU | 512 | 64 | UART | Baseline template |

Each profile drives the SVG board art rendered in the Live Device Viewer shell.

---

## 3. Sensor Catalog

Users enable sensors from the following predefined list at device-creation time. Enabled sensors are stored in the DB and drive the emulator task.

### 3.1 Environmental
| Sensor ID | Metric | Unit | Range | Default Sim |
|---|---|---|---|---|
| `temperature` | Temperature | °C | −40 to 85 | 25 ± noise |
| `humidity` | Relative Humidity | % RH | 0–100 | 55 ± noise |
| `pressure` | Atmospheric Pressure | hPa | 300–1100 | 1013 ± noise |
| `air_quality_co2` | CO₂ Concentration | ppm | 400–5000 | 450 ± noise |
| `air_quality_tvoc` | Total VOC | ppb | 0–1000 | 100 ± noise |

### 3.2 Motion
| Sensor ID | Metric | Unit | Range |
|---|---|---|---|
| `accel_xyz` | Acceleration X/Y/Z | g | −16 to +16 |
| `gyro_xyz` | Angular Rate X/Y/Z | °/s | −2000 to +2000 |
| `magnetometer` | Magnetic Field X/Y/Z | μT | −4800 to +4800 |

### 3.3 Light & Proximity
| Sensor ID | Metric | Unit |
|---|---|---|
| `ambient_light` | Illuminance | lux |
| `uv_index` | UV Index | 0–11+ |
| `proximity` | Object Distance | mm |

### 3.4 Location
| Sensor ID | Metric | Notes |
|---|---|---|
| `gps` | Latitude / Longitude / Altitude / Speed | Simulated path or fixed point |

### 3.5 Power & Electrical
| Sensor ID | Metric | Unit |
|---|---|---|
| `battery_voltage` | Battery Voltage | V |
| `current` | Supply Current | mA |
| `gpio_states` | GPIO Pin D0–D13, A0–A5 | HIGH / LOW |
| `adc_raw` | ADC Channel 0–3 | 0–4095 (12-bit) |

### 3.6 Audio
| Sensor ID | Metric | Unit |
|---|---|---|
| `sound_level` | Ambient Sound Level | dB SPL |

### 3.7 Connectivity (meta-sensors)
| Sensor ID | Metric | Unit |
|---|---|---|
| `wifi_rssi` | WiFi Signal Strength | dBm |
| `ble_scan` | Nearby BLE Beacons | RSSI + UUID list |

---

## 4. Architecture

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ Browser (ef-dashboard Leptos WASM)                                            │
│                                                                               │
│  ┌── /virtual-iot/new  (IoT Designer Page) ─────────────────────────────┐    │
│  │                                                                       │    │
│  │  ┌──────────────────┐   ┌─────────────────────────────────────────┐  │    │
│  │  │ Board Preview    │   │ Config Panel                            │  │    │
│  │  │ (live SVG shell  │   │  Controller dropdown ─────────────────  │  │    │
│  │  │  updates on      │   │  Device name / region / rate ─────────  │  │    │
│  │  │  controller      │   │  Sensor checklist (by category):        │  │    │
│  │  │  selection)      │   │    ☑ Temperature  ☑ Humidity  ☐ CO₂     │  │    │
│  │  │                  │   │    ☑ Accel XYZ    ☐ Gyro XYZ            │  │    │
│  │  │  GPIO LED grid   │   │    ☑ GPIO States  ☑ WiFi RSSI          │  │    │
│  │  │  preview (static)│   │                                         │  │    │
│  │  └──────────────────┘   │  [ Save Controller ]  [ Cancel ]        │  │    │
│  │                         └─────────────────────────────────────────┘  │    │
│  └──────────────────────────────┬────────────────────────────────────────┘    │
│                                  │                                            │
│                    POST /api/v1/devices/virtual-iot                           │
│                    (or PATCH .../config on edit)                              │
│                                  │                                            │
│                                  ▼                                            │
│  ┌── /devices  (Device List) ───────────────────────────────────────────┐    │
│  │  Device now appears with os=virtual_iot, kind=virtual, chip icon    │    │
│  │  Click row → navigates to /sessions/live?device_id=...              │    │
│  │  │ "+ New Virtual IoT" button → links to /virtual-iot/new          │    │
│  └──┼───────────────────────────────────────────────────────────────────┘    │
│     │                                                                        │
│     ▼                                                                        │
│  ┌── /sessions/live  (Live Device Viewer) ──────────────────────────────┐   │
│  │  VirtualIotBoard                                                      │   │
│  │  ┌──────────────┐  ┌─────────────────────────────────────┐           │   │
│  │  │ Board Shell  │  │ Sensor Panel (live animated)         │           │   │
│  │  │ (SVG PCB art)│  │  ┌────────────┐  ┌──────────────┐  │           │   │
│  │  │  GPIO LEDs   │  │  │ Gauge (T,H)│  │ Sparkline(g) │  │           │   │
│  │  │  Chip label  │  │  └────────────┘  └──────────────┘  │           │   │
│  │  └──────────────┘  │  ┌────────────┐  ┌──────────────┐  │           │   │
│  │                    │  │ GPS Tracker│  │ LED Grid     │  │           │   │
│  │                    │  └────────────┘  └──────────────┘  │           │   │
│  │                    └─────────────────────────────────────┘           │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│         ▲ WebSocket ws://host/api/v1/devices/:id/stream                      │
└─────────┼────────────────────────────────────────────────────────────────────┘
          │
┌─────────┴─────────────────────────────────────────────────────────┐
│ ef-api (Axum)                                                      │
│  POST /api/v1/devices/virtual-iot  →  create device + config in DB │
│  PATCH /api/v1/devices/:id/virtual-iot/config → update config      │
│  WebSocket handler  →  spawns / connects to emulator task          │
└─────────┬─────────────────────────────────────────────────────────┘
          │ NATS subject: ef.iot.{device_id}.sensors
┌─────────┴─────────────────────────────────────────────────────────┐
│ ef-agent (Virtual IoT emulator task)                               │
│  VirtualIotEmulator::new(config) → spawn async task               │
│  Every tick (configurable interval, default 500 ms):              │
│    • Generate sensor readings (noise models per sensor)            │
│    • Publish SensorFrame JSON to NATS                              │
│    • Control plane relays to WebSocket clients                     │
└─────────┬─────────────────────────────────────────────────────────┘
          │ DB read on startup
┌─────────┴─────────────────────────────────────────────────────────┘
│ ef-db / PostgreSQL                                                  │
│  devices table: os = 'virtual_iot', kind = 'virtual'               │
│  virtual_iot_configs: device_id, controller_type, sensors (JSON)   │
└────────────────────────────────────────────────────────────────────┘
```

---

## 5. Data Contract — SensorFrame

Sensor data is streamed as JSON over the existing WebSocket, distinguished from screen frames by a leading JSON envelope (screen frames are binary).

```json
{
  "type": "sensor_frame",
  "device_id": "c3ac380b-...",
  "ts": 1712345678.123,
  "readings": {
    "temperature":   { "value": 24.7,  "unit": "°C" },
    "humidity":      { "value": 58.2,  "unit": "%" },
    "pressure":      { "value": 1012.4,"unit": "hPa" },
    "accel_xyz":     { "x": 0.12, "y": -0.05, "z": 9.81, "unit": "g" },
    "gyro_xyz":      { "x": 1.2,  "y": -0.3,  "z": 0.6,  "unit": "°/s" },
    "gpio_states":   { "D0": 1, "D1": 0, "D2": 1, "A0": 3210 },
    "wifi_rssi":     { "value": -68, "unit": "dBm" },
    "gps":           { "lat": 12.9716, "lng": 77.5946, "alt": 920.0, "speed": 0.0 }
  }
}
```

The existing WebSocket message handler in `sessions_live.rs` checks `type == "sensor_frame"` and routes to signal updates for each enabled sensor.

---

## 6. Noise Models

Each sensor uses a parameterized stochastic model to generate realistic readings:

| Model | Formula | Used by |
|---|---|---|
| Gaussian noise | `base + σ × N(0,1)` | Temperature, humidity, pressure, RSSI |
| Random walk | `prev + δ × N(0,1)` clamped | Accelerometer, gyroscope, magnetometer |
| Step function | Occasional step change ± Δ | GPIO, battery discharge |
| Sine + noise | `base + A·sin(2πt/T) + σ·N(0,1)` | Sound level, GPS path simulation |

Noise parameters are per-sensor defaults, overridable via scenario TOML files (future P17.x).

---

## 7. Board Shell SVG Design

Each controller profile has a corresponding SVG template in `ef-dashboard/assets/boards/`. The SVG includes:

- **PCB background**: green FR4-style rectangle with mounting holes  
- **Chip die**: labeled IC package in the center  
- **Pin headers**: numbered left + right (or top + bottom for Arduino)  
- **LED indicators**: one per GPIO (D-pins), animated `fill-opacity` driven by `gpio_states`  
- **USB connector**: bottom edge  
- **Reset button**: small SVG `<rect>` with hover glow  
- **Connectivity badge**: WiFi/BLE indicator icon (profile-dependent)

The SVG is inlined as a Leptos `view!` component (`BoardShell`) parameterized by `controller_type` and `gpio_state` signals.

---

## 8. IoT Designer Page (`/virtual-iot/new` & `/virtual-iot/:id/edit`)

New page file: `crates/ef-dashboard/src/pages/virtual_iot_designer.rs`  
New route entries in `main.rs`:
```rust
<Route path=path!("/virtual-iot/new")     view=move || view! { <AuthGuard><Shell><VirtualIotDesignerPage/></Shell></AuthGuard> }/>
<Route path=path!("/virtual-iot/:id/edit") view=move || view! { <AuthGuard><Shell><VirtualIotDesignerPage/></Shell></AuthGuard> }/>
```

### Page Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  ← Back to Devices           IoT Designer           [ Save ] [Cancel] │
├───────────────────────────────┬──────────────────────────────────────┤
│                               │                                      │
│   Board Preview (40%)         │   Configuration Panel (60%)          │
│                               │                                      │
│   ┌───────────────────────┐  │   Device Name: [______________]     │
│   │                       │  │   Region:      [local ▾]             │
│   │    SVG Board Shell    │  │   Controller:  [ESP32 DevKit V1 ▾]  │
│   │    (updates live      │  │   Sim Rate:    [500] ms              │
│   │     when controller   │  │                                      │
│   │     dropdown changes) │  │   ── Sensors ───────────────────     │
│   │                       │  │                                      │
│   │    GPIO LED grid      │  │   ▸ Environmental                    │
│   │    (static preview,   │  │     ☑ Temperature  ☑ Humidity        │
│   │     all LEDs off)     │  │     ☐ Pressure     ☐ CO₂   ☐ TVOC   │
│   │                       │  │                                      │
│   └───────────────────────┘  │   ▸ Motion                           │
│                               │     ☑ Accelerometer XYZ              │
│   Controller specs:           │     ☐ Gyroscope XYZ                  │
│   Flash: 4096 KB              │     ☐ Magnetometer                   │
│   RAM:   520 KB               │                                      │
│   Conn:  WiFi + BLE           │   ▸ Light & Proximity                │
│                               │     ☐ Ambient Light  ☐ UV Index     │
│                               │     ☐ Proximity                      │
│                               │                                      │
│                               │   ▸ Location                         │
│                               │     ☐ GPS                            │
│                               │                                      │
│                               │   ▸ Power & Electrical               │
│                               │     ☑ GPIO States   ☐ ADC Raw       │
│                               │     ☐ Battery Voltage ☐ Current      │
│                               │                                      │
│                               │   ▸ Audio                            │
│                               │     ☐ Sound Level                    │
│                               │                                      │
│                               │   ▸ Connectivity                     │
│                               │     ☑ WiFi RSSI     ☐ BLE Scan      │
│                               │                                      │
└───────────────────────────────┴──────────────────────────────────────┘
```

### Behaviour

| Action | Effect |
|---|---|
| Change controller dropdown | Board preview SVG swaps to selected profile; specs text updates |
| Toggle a sensor checkbox | Sensor is added/removed from `enabled_sensors` list; widget mini-preview appears/disappears below board (optional future enhancement) |
| Click **Save** | `POST /api/v1/devices/virtual-iot` (create) or `PATCH /api/v1/devices/:id/virtual-iot/config` (edit) → on success, navigate to `/devices` |
| Click **Cancel** | Navigate back to `/devices` without saving |
| Open page with `:id` param | Fetch existing config via `GET /api/v1/devices/:id/virtual-iot/config`, pre-populate all fields |

### Leptos Signals

```rust
let (device_name, set_device_name) = signal(String::new());
let (region, set_region) = signal("local".to_string());
let (controller_type, set_controller_type) = signal("esp32-devkit".to_string());
let (sim_rate_ms, set_sim_rate_ms) = signal(500u32);
let (enabled_sensors, set_enabled_sensors) = signal(Vec::<String>::new());
let (saving, set_saving) = signal(false);
let (error, set_error) = signal(Option::<String>::None);
```

### Integration with Device List

The **Devices page** (`/devices`) gains:
1. **OS filter** — new option `"virtual_iot"` → `"Virtual IoT"` in the dropdown
2. **"+ New Virtual IoT"** button — navigates to `/virtual-iot/new`
3. **Device list row** — virtual IoT devices show a chip/circuit icon (`DeviceOsIcon` for `"virtual_iot"`), controller type in the "Model" column, and sensor count badge
4. **Row click** for virtual IoT → either opens live session (same as other devices) or an edit link to `/virtual-iot/:id/edit` via a context menu / secondary button

---

## 9. Frontend Sensor Widget Components

All in `crates/ef-dashboard/src/components/iot/`:

| Component | Description |
|---|---|
| `VirtualIotBoard` | Top-level component for Live Viewer; conditionally rendered when `is_virtual_iot` |
| `BoardShell` | Renders SVG board art + GPIO LED grid (reused in designer preview + live viewer) |
| `SensorGauge` | Semi-circular SVG gauge for scalar sensors (T, H, P, UV, etc.) |
| `SensorSparkline` | 60-point rolling time-series SVG path (accelerometer, sound) |
| `GpioLedGrid` | Grid of colored dots for D0–D13 + A0–A5 states |
| `GpsTracker` | Minimal lat/lng dot on a static tile background |
| `ConnectivityBadge` | WiFi RSSI bar icon or BLE scan list |
| `SensorTogglePanel` | Per-session toggle to show/hide individual sensor widgets |
| `SensorCategoryChecklist` | Grouped checkbox list used in IoT Designer config panel |
| `ControllerSpecsCard` | Flash / RAM / Connectivity display for selected profile |

Sensor values are held in Leptos signals, updated per `sensor_frame` WebSocket message (live viewer) or static defaults (designer preview).

---

## 10. Database Migration

New table added in migration `20260411000008_virtual_iot_configs.sql`:

```sql
CREATE TABLE virtual_iot_configs (
    device_id       UUID PRIMARY KEY REFERENCES devices(id) ON DELETE CASCADE,
    controller_type TEXT NOT NULL DEFAULT 'esp32-devkit',
    enabled_sensors JSONB NOT NULL DEFAULT '[]',
    sim_rate_ms     INTEGER NOT NULL DEFAULT 500,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

The `devices` table uses `os = 'virtual_iot'` and `kind = 'virtual'`.

---

## 11. API Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/v1/devices/virtual-iot` | Create a new virtual IoT device + emulator config |
| `GET` | `/api/v1/devices/:id/virtual-iot/config` | Get emulator config for a device |
| `PATCH` | `/api/v1/devices/:id/virtual-iot/config` | Update sensors / rate / controller type |
| `DELETE` | `/api/v1/devices/:id` | Existing endpoint — also tears down emulator task |

Request body for `POST`:
```json
{
  "name": "my-esp32-sim",
  "controller_type": "esp32-devkit",
  "region": "local",
  "sim_rate_ms": 500,
  "enabled_sensors": ["temperature", "humidity", "accel_xyz", "wifi_rssi", "gpio_states"]
}
```

---

## 12. ef-core Type Changes

New `Os` variant:
```rust
pub enum Os {
    Android, Ios, Iot, Ssh,
    VirtualIot,   // ← new
}
```

New types in `ef-core/src/virtual_iot.rs`:
```rust
pub enum ControllerType { Esp32Devkit, ArduinoUno, RpiPico, Stm32Nucleo, GenericMcu }
pub enum SensorKind {
    Temperature, Humidity, Pressure, AirQualityCo2, AirQualityTvoc,
    AccelXyz, GyroXyz, Magnetometer,
    AmbientLight, UvIndex, Proximity,
    Gps, BatteryVoltage, Current, GpioStates, AdcRaw,
    SoundLevel, WifiRssi, BleScan,
}
pub struct VirtualIotConfig {
    pub controller_type: ControllerType,
    pub enabled_sensors: Vec<SensorKind>,
    pub sim_rate_ms: u32,
}
pub struct SensorReading { ... }
pub struct SensorFrame {
    pub device_id: Uuid,
    pub ts: f64,
    pub readings: HashMap<String, SensorReading>,
}
```

---

## 13. ef-agent Emulator Task

New file: `crates/ef-agent/src/virtual_iot.rs`

```
VirtualIotEmulator
  ├── config: VirtualIotConfig
  ├── state: HashMap<SensorKind, f64>   // current simulated value per sensor
  └── run() → async loop:
        tick every sim_rate_ms
        for each enabled sensor:
          apply noise model → new reading
        serialize SensorFrame as JSON
        publish to NATS ef.iot.{device_id}.sensors
        (API relays to WS clients in af-api stream handler)
```

---

## 14. Dashboard Integration Points

### New files
| File | Purpose |
|---|---|
| `pages/virtual_iot_designer.rs` | IoT Designer page component |
| `components/iot/mod.rs` | Module re-exports for IoT widget components |
| `components/iot/board_shell.rs` | SVG board renderer (shared: designer preview + live viewer) |
| `components/iot/sensor_gauge.rs` | Gauge widget |
| `components/iot/sensor_sparkline.rs` | Sparkline widget |
| `components/iot/gpio_led_grid.rs` | GPIO LED grid widget |
| `components/iot/gps_tracker.rs` | GPS dot widget |
| `components/iot/connectivity_badge.rs` | WiFi/BLE widget |
| `components/iot/sensor_category_checklist.rs` | Sensor checkbox groups (designer) |
| `components/iot/controller_specs_card.rs` | Controller specs display (designer) |

### Changes to existing files

**`main.rs`** — add two new routes:
```rust
<Route path=path!("/virtual-iot/new")      view=…/>
<Route path=path!("/virtual-iot/:id/edit")  view=…/>
```

**`pages/mod.rs`** — add `pub mod virtual_iot_designer;`

**`components/mod.rs`** — add `pub mod iot;`

**`pages/devices.rs`**:
1. Add `"virtual_iot"` option in OS filter dropdown
2. Add "+ New Virtual IoT" button linking to `/virtual-iot/new`
3. Show chip icon + controller type + sensor count for virtual IoT rows

**`components/device_icon.rs`** — add `"virtual_iot"` SVG icon (chip/circuit)

**`pages/sessions_live.rs`**:
1. Detect `device_os == "virtual_iot"` → set `is_virtual_iot = true`
2. Fetch config via `GET /devices/:id/virtual-iot/config` to know which sensors are enabled
3. Add sensor reading signals per sensor kind
4. In WebSocket `onmessage` handler: parse `type == "sensor_frame"`, dispatch to sensor signals
5. In `view!`: render `<VirtualIotBoard>` instead of terminal or phone shell when `is_virtual_iot`

**`style/main.css`**:
1. `.iot-designer-layout` — two-column grid (board preview + config panel)
2. `.iot-designer-preview` — board preview pane (40% width)
3. `.iot-designer-config` — config panel pane (60% width)
4. `.sensor-category` — collapsible category header
5. `.sensor-checkbox-group` — grid of sensor checkboxes
6. `.virtual-iot-layout` — live viewer grid layout for board + sensor panels
7. `.sensor-gauge` — SVG gauge styles
8. `.sensor-sparkline` — sparkline canvas styles
9. `.gpio-led-grid` — LED dot grid
10. `.board-shell` — PCB background, pin header labels
11. `.controller-specs` — specs card in designer preview

---

## 15. Future Scope (not in Phase 17)

| Item | Description |
|---|---|
| Scenario injection | Load TOML scenario files that control sensor pattern over time |
| Hardware-in-loop bridge | Connect emulator to real UART/MQTT hardware to mirror actual readings |
| Sensor recording + playback | Record a session of sensor frames; replay on demand |
| Multi-sensor correlation | Correlated noise (e.g., pressure drops when temperature rises) |
| Alert rules on sensor thresholds | Trigger EdgeFabric test actions when sensor crosses a threshold |
| Custom GPIO scripting | JS/Lua microprogram controlling GPIO sequence |

---

## 16. Out of Scope

- Actual firmware execution (no CPU emulation like QEMU for MCUs in Phase 17)  
- Real device passthrough (bridging to physical hardware over USB/UART is a separate IoT category)  
- Network stack simulation (TCP/IP, CoAP, MQTT protocol emulation)
