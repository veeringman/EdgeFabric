# EdgeFabric PCB & Electronic Circuit Design Integration Plan

## Overview

Add a first-class **Electronic Circuit & PCB Design** capability to EdgeFabric.
The goal is a KiCad-class workflow, but cutting-edge: AI-assisted automation,
realistic visuals, full drag-and-drop interaction, and config / spec driven
generation that can produce manufacturing-ready outputs.

This complements the existing Virtual IoT, real device, and quantum tracks by
letting users go from "I need a board for this idea" all the way to fabricated
hardware that can then be onboarded as a normal EdgeFabric device.

## Goals

- AI-assisted board design (natural language and spec → schematic → layout)
- Fully manual mode with professional-grade editors (schematic + PCB)
- Realistic, GPU-accelerated 3D board / component visualization
- Drag-and-drop part placement, routing assistance, and live DRC/ERC
- Spec / config driven adaptation (YAML/JSON inputs, target profiles, BOM rules)
- Multi-user, multi-tenant projects with versioned history
- Manufacturing export: Gerber, drill, BOM, pick-and-place, ODB++, IPC-2581
- Direct handoff into EdgeFabric runners for firmware build + on-device test
- Open data formats so projects round-trip with KiCad / external EDA tools

## Non-Goals (Phase 1)

- Replacing high-end commercial EDA (Altium, Cadence) for HDI/RF specialty boards
- Full analog SPICE-level simulation in the browser (use external solvers)
- Component sourcing/purchasing workflow (only BOM export + supplier links)

## Personas

- Hardware engineer: full manual schematic + PCB control
- IoT product lead: spec-driven "design me a sensor node" flow
- Firmware engineer: needs board metadata + pinout to drive code generation
- Operator / QA: tracks revisions, fabrication batches, test coverage
- AI co-pilot: assists across all of the above

## Architecture

### Frontend (`ef-dashboard`) — Leptos WASM + WebGL/WebGPU

**New Pages**

1. **Hardware Studio Overview** (`pages/hw_studio.rs`)
   - Project list, recently opened boards, fabrication status
   - Quick stats: projects, revisions, open DRC errors, in-flight fab orders
   - Entry points: "New from spec", "New blank", "Import KiCad / Eagle"

2. **Schematic Editor** (`pages/hw_schematic.rs`)
   - Hierarchical sheets, buses, net classes
   - Drag-and-drop symbol library with search and AI-suggested parts
   - Live ERC (Electrical Rules Check) with inline annotations
   - Cross-probing to PCB editor

3. **PCB Layout Editor** (`pages/hw_pcb.rs`)
   - 2D layout with layer stack visualization
   - GPU-accelerated 3D view (WebGPU + glTF component models)
   - Drag-place parts, push-and-shove + autoroute (with manual override)
   - Live DRC, length matching, differential pair tools
   - Realistic rendering: solder mask, silkscreen, copper, finished components

4. **AI Co-Pilot** (`pages/hw_copilot.rs`)
   - Conversational design assistant
   - "Design a battery-powered BLE sensor node with 3 GPIOs and USB-C charge"
   - Suggests reference designs, parts, schematics, and layout strategies
   - Explains DRC errors, proposes fixes, can apply them on user approval

5. **Library Manager** (`pages/hw_library.rs`)
   - Symbols, footprints, 3D models
   - Tenant-local + shared community libraries
   - Part vetting (lifecycle, sourcing, datasheet links)

6. **BOM & Manufacturing** (`pages/hw_bom.rs`)
   - BOM viewer with supplier links + cost roll-up
   - Generate fabrication packs (Gerber/drill/pick-and-place)
   - Submit to integrated fab partners (JLCPCB, PCBWay, OSH Park — adapters)
   - Track fab order status, returned units, serial assignment

7. **Board → Device Onboarding** (`pages/hw_onboard.rs`)
   - Once a board is fabricated, register it as an EdgeFabric IoT device
   - Auto-populate capabilities from board metadata (MCU, peripherals)
   - Auto-suggest firmware targets and starter test suites

**New Components** (`crates/ef-dashboard/src/components`)

- `SchematicCanvas` — SVG/Canvas hybrid with hierarchical zoom
- `PCBCanvas2D` — high-perf 2D layout (WebGL2)
- `PCBView3D` — WebGPU 3D viewer with realistic materials
- `LayerStackControl` — interactive stackup editor
- `PartPalette` — searchable parts panel with AI suggestions
- `DRCPanel` — DRC/ERC issue list with cross-probe
- `NetlistViewer` — inspectable netlist tree
- `AICopilotChat` — chat with action buttons ("apply patch", "explain")

### Backend Crates

- **`ef-eda-core`**: shared types — schematics, footprints, nets, layers, designs
- **`ef-eda-render`**: SVG / WebGL / WebGPU render helpers (no I/O)
- **`ef-eda-rules`**: DRC/ERC rule engine, configurable rule packs
- **`ef-eda-router`**: autoroute + push-and-shove (Rust port / wraps existing OSS)
- **`ef-eda-fabrication`**: Gerber, NC drill, pick-and-place, ODB++, IPC-2581 exporters
- **`ef-eda-ai`**: AI orchestration — prompts, planners, tool-use for design tasks
- **`ef-eda-import`**: KiCad, Eagle, Altium importers (best-effort)
- **`ef-api` additions**: `/api/v1/hw/*` REST + WebSocket endpoints

### Storage

- Projects stored as structured documents (Postgres + object store for blobs)
- Git-like revision graph per board (commits, branches, diffs)
- Library assets in object store (footprints, 3D models, datasheets)

### AI Strategy

- Model-agnostic provider layer (OpenAI / Anthropic / local llama.cpp)
- Tool-using agents with strict schemas:
  - `add_part`, `connect_nets`, `place_part`, `route_net`, `run_drc`,
    `generate_fabrication`, `import_reference_design`
- All AI actions are reversible (atomic edits in revision history)
- Spec → design pipeline: NL spec → structured intent → reference design pick
  → schematic synthesis → constraint-driven placement → routing → DRC loop

## API Surface (Phase 1)

- `POST   /api/v1/hw/projects` — create project
- `GET    /api/v1/hw/projects` — list projects (tenant scoped)
- `GET    /api/v1/hw/projects/{id}` — project detail
- `POST   /api/v1/hw/projects/{id}/schematic/commands` — apply schematic edit
- `POST   /api/v1/hw/projects/{id}/pcb/commands` — apply PCB edit
- `POST   /api/v1/hw/projects/{id}/drc` — run DRC
- `POST   /api/v1/hw/projects/{id}/fabrication` — generate fab pack
- `POST   /api/v1/hw/projects/{id}/ai/plan` — request AI plan for goal
- `POST   /api/v1/hw/projects/{id}/ai/apply` — apply approved AI plan
- `GET    /api/v1/hw/libraries` / `POST` — manage symbol/footprint libraries
- `WS     /api/v1/hw/projects/{id}/events` — live collaboration + DRC stream

## Data Model (initial migrations)

- `hw_projects` (id, tenant_id, name, current_revision, created_at)
- `hw_revisions` (id, project_id, parent_revision_id, author, message, created_at)
- `hw_documents` (revision_id, kind: schematic|pcb|stackup|rules|bom, blob_ref)
- `hw_libraries` (id, tenant_id, name, scope: tenant|shared, version)
- `hw_library_items` (id, library_id, kind: symbol|footprint|model3d, blob_ref)
- `hw_fab_orders` (id, project_id, revision_id, vendor, status, tracking_ref)
- `hw_ai_actions` (id, project_id, revision_id, intent, plan_json, status)

## Rollout Phases

### Phase H1: Foundations
- [ ] Workspace crates skeleton: `ef-eda-core`, `ef-eda-rules`, `ef-eda-fabrication`
- [ ] DB migrations for projects / revisions / libraries
- [ ] REST scaffolding under `/api/v1/hw/*`
- [ ] Dashboard nav entry + empty Hardware Studio page

### Phase H2: Manual Schematic MVP
- [ ] Schematic editor canvas with pan/zoom and selection
- [ ] Symbol library with starter parts (passives, MCUs, connectors)
- [ ] Wire drawing + net naming + buses
- [ ] ERC v1 (unconnected pins, duplicate refs, power conflicts)
- [ ] KiCad schematic import (best effort)

### Phase H3: Manual PCB MVP
- [ ] Layer stack editor (2-layer + 4-layer presets)
- [ ] Footprint placement with snapping and rotation
- [ ] Manual routing + via insertion
- [ ] DRC v1 (clearance, track width, hole/annular ring)
- [ ] Gerber + drill export, BOM CSV, pick-and-place CSV

### Phase H4: Realistic 3D + UX Polish
- [ ] WebGPU 3D viewer with PBR materials (mask, copper, silkscreen)
- [ ] Realistic component 3D models (glTF library)
- [ ] Cross-probe schematic ↔ PCB
- [ ] Multi-user live collaboration (cursors, locks)

### Phase H5: Spec / Config Driven Design
- [ ] YAML/JSON board spec schema (MCU, peripherals, power, form factor)
- [ ] Reference design catalog keyed by spec capability vector
- [ ] One-shot "spec → starting schematic + placement" generator
- [ ] Parameter-driven sub-circuit blocks (regulator, USB-C, BLE module)

### Phase H6: AI Co-Pilot
- [ ] Tool-use agent with `ef-eda-ai`
- [ ] Chat panel with diff-style proposals + approve/reject
- [ ] AI-driven DRC explanation and auto-fix suggestions
- [ ] AI-assisted part picking with lifecycle + sourcing awareness

### Phase H7: Autoroute & Advanced Tools
- [ ] Interactive push-and-shove router
- [ ] Length matching + differential pairs
- [ ] Net classes, design rule packs per class
- [ ] High-speed constraint editor

### Phase H8: Manufacturing & Fab Partners
- [ ] ODB++ and IPC-2581 exporters
- [ ] Fab partner adapter framework (JLCPCB, PCBWay, OSH Park)
- [ ] Order tracking + cost estimation
- [ ] Returned-board serialization → device onboarding hook

### Phase H9: Device Loop Closure
- [ ] Auto-onboard fabricated board as EdgeFabric IoT device
- [ ] Pinout / capability metadata → firmware starter projects
- [ ] One-click "build firmware + flash + smoke test" on a runner

## Quality & Compliance

- All AI edits go through revision history and are diff-reviewable
- Tenant isolation on every project, library, and fab order
- Audit log for AI actions and fabrication submissions
- Export reproducibility: same revision → byte-identical Gerber set

## Open Questions

- Build vs. wrap for autoroute (write Rust, port FreeRouting, or vendor commercial?)
- License posture for community library content
- Where to draw the line between in-browser solve and server-side jobs
- Pricing model for AI tokens consumed during co-pilot use

## References

- Existing pattern: `QUANTUM_INTEGRATION_PLAN.md`
- Companion plan: `CHIP_DESIGN_INTEGRATION_PLAN.md`
- System design overview: `EdgeFabric_System_Design.md`
