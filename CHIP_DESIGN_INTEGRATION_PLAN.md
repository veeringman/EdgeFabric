# EdgeFabric Chip Design (Silicon → Tape-Out) Integration Plan

## Overview

Add an **end-to-end chip design** capability to EdgeFabric, covering the full
flow from architectural intent through **tape-out**. Same philosophy as the
PCB plan: AI-assisted + manual, realistic visuals, drag-and-drop where it
makes sense, and config / spec driven adaptation.

Target both ASIC and FPGA flows, with a strong open-silicon path (e.g.
SkyWater 130nm / IHP 130nm via OpenROAD) so users can realistically go from
"new chip idea" to fabricable GDSII.

## Goals

- AI-assisted RTL generation from natural-language / spec input
- Full manual mode: RTL editing, constraints, floorplanning, PnR review
- Cutting-edge visuals: interactive floorplan, congestion heatmaps, 3D die view
- Drag-and-drop floorplan / macro placement and pin assignment
- Config / spec driven SoC composition (CPU + bus + peripherals from YAML)
- Reproducible flows: pinned tool versions, container/runner backed
- Tape-out outputs: GDSII, LEF/DEF, LVS/DRC clean reports, sign-off packs
- FPGA flow parity (Yosys + nextpnr + open targets first)
- Integration with EdgeFabric runners for long-running synth/PnR jobs

## Non-Goals (Phase 1)

- Replacing commercial sign-off (Calibre, ICV, PrimeTime) on advanced nodes
- Analog/mixed-signal full custom layout (digital + macro-based first)
- Foundry-specific PDKs that we are not licensed to redistribute

## Personas

- Digital designer: writes RTL, runs synth + PnR, iterates on QoR
- SoC architect: composes systems from blocks via spec
- Verification engineer: drives simulation, coverage, formal
- Physical design engineer: floorplan, CTS, routing, sign-off
- Student / hobbyist: open-PDK tape-out via shuttle programs
- AI co-pilot: assists at every step with reversible, reviewable actions

## Architecture

### Frontend (`ef-dashboard`) — Leptos WASM + WebGL/WebGPU

**New Pages**

1. **Silicon Studio Overview** (`pages/si_studio.rs`)
   - Project list, active runs (synth/PnR), tape-out queue
   - Status of long-running EDA jobs on runners
   - Entry points: "New from spec", "New blank", "Import RTL repo"

2. **RTL Workspace** (`pages/si_rtl.rs`)
   - Multi-file editor (Verilog/SystemVerilog/VHDL/Chisel/SpinalHDL)
   - Module hierarchy explorer
   - Linting (Verilator / Slang) with inline diagnostics
   - AI co-pilot panel for generating / refactoring RTL

3. **Verification** (`pages/si_verify.rs`)
   - Testbench manager (cocotb, UVM-lite, Verilator)
   - Coverage dashboard (line, toggle, FSM, functional)
   - Waveform viewer (VCD/FST) with bookmarks and AI-explained signals
   - Formal property runs (SymbiYosys) summary

4. **Synthesis & Constraints** (`pages/si_synth.rs`)
   - Constraint editor (SDC) with timing groups
   - Synthesis runs (Yosys / OpenROAD / commercial adapter)
   - QoR snapshot per run: area, timing, power, gate count
   - Diff QoR across runs and against baseline

5. **Floorplan & PnR** (`pages/si_pnr.rs`)
   - Interactive floorplan canvas with macro drag-and-drop
   - Pin/IO placement editor
   - Power planning visualization (rings, straps, vias)
   - Congestion + density heatmaps (WebGL)
   - Routing run controls + iteration loop

6. **Sign-Off & Tape-Out** (`pages/si_signoff.rs`)
   - DRC / LVS / antenna / ERC report viewer
   - Timing sign-off summary (STA across corners)
   - GDSII / OASIS / LEF / DEF artifact browser
   - Tape-out checklist + "submit to shuttle" workflow

7. **AI Co-Pilot** (`pages/si_copilot.rs`)
   - Conversational assistant across RTL, verification, PD
   - "Build me a RISC-V Zicsr core with a 2-cycle FPU and AXI4-Lite"
   - Plans → reversible actions → approval → commit to revision history

8. **PDK & IP Library** (`pages/si_libraries.rs`)
   - Open PDK browser (SkyWater 130, IHP SG13G2, GF180MCU, etc.)
   - Standard cell + macro library inspector
   - Tenant-private hard IP registration (with NDA/access controls)

**New Components** (`crates/ef-dashboard/src/components`)

- `FloorplanCanvas` — WebGL2 interactive floorplan
- `DieView3D` — WebGPU 3D die / package visualization
- `CongestionHeatmap` — overlay on floorplan
- `WaveformViewer` — VCD/FST with virtual scrolling
- `RTLEditor` — Monaco-style editor with HDL lints
- `QoRDeltaTable` — per-metric diff with sparklines
- `AICopilotChat` — shared chat shell (also used by PCB plan)

### Backend Crates

- **`ef-si-core`**: shared types — modules, nets, constraints, libs, runs
- **`ef-si-flow`**: orchestrates RTL → synth → PnR → sign-off pipelines
- **`ef-si-tools`**: thin Rust adapters around Yosys, OpenROAD, Verilator,
  cocotb, nextpnr, SymbiYosys, Magic, Netgen, KLayout
- **`ef-si-pdk`**: PDK metadata + access control (open + private)
- **`ef-si-spec`**: SoC composition engine (spec → RTL + integration)
- **`ef-si-ai`**: AI orchestration for chip design tasks
- **`ef-api` additions**: `/api/v1/si/*` REST + WebSocket endpoints
- **Runner integration**: long jobs (synth/PnR/STA) run on EdgeFabric runners
  with containerized tool images; results stream live to the dashboard

### Storage

- Projects as structured docs in Postgres + object store for large artifacts
  (netlists, GDSII, reports, waveforms)
- Per-project revision graph (Git-backed for source-of-truth RTL is allowed)
- Run records with full reproducibility: tool versions, PDK rev, seed, env

### AI Strategy

- Same provider abstraction as PCB plan; shared `AICopilotChat` shell
- Tool-using agents with strict schemas:
  - `generate_module`, `refactor_rtl`, `write_testbench`, `add_assertion`,
    `propose_constraints`, `tune_synth_strategy`, `place_macro`,
    `route_net`, `analyze_qor`, `explain_violation`, `prepare_tapeout`
- All AI changes are atomic edits committed to revision history
- Spec → SoC pipeline: NL spec → structured SoC manifest → block selection
  → integration RTL → testbench scaffolding → synth/PnR loop with AI tuning

## API Surface (Phase 1)

- `POST   /api/v1/si/projects` — create project
- `GET    /api/v1/si/projects` — list projects (tenant scoped)
- `GET    /api/v1/si/projects/{id}` — project detail
- `POST   /api/v1/si/projects/{id}/rtl/commands` — apply RTL edit
- `POST   /api/v1/si/projects/{id}/runs/synth` — launch synthesis run
- `POST   /api/v1/si/projects/{id}/runs/pnr` — launch PnR run
- `POST   /api/v1/si/projects/{id}/runs/signoff` — DRC/LVS/STA pack
- `POST   /api/v1/si/projects/{id}/tapeout` — assemble tape-out pack
- `POST   /api/v1/si/projects/{id}/ai/plan` — request AI plan
- `POST   /api/v1/si/projects/{id}/ai/apply` — apply approved AI plan
- `GET    /api/v1/si/pdks` — list accessible PDKs
- `WS     /api/v1/si/projects/{id}/runs/{run_id}/log` — live tool logs
- `WS     /api/v1/si/projects/{id}/events` — collab + run status stream

## Data Model (initial migrations)

- `si_projects` (id, tenant_id, name, target_kind: asic|fpga, current_revision)
- `si_revisions` (id, project_id, parent_revision_id, author, message, created_at)
- `si_documents` (revision_id, kind: rtl|tb|sdc|floorplan|run_report, blob_ref)
- `si_runs` (id, project_id, revision_id, kind: lint|sim|synth|pnr|sta|drc|lvs,
            status, runner_id, tool_versions_json, started_at, finished_at)
- `si_artifacts` (id, run_id, kind, blob_ref, size, checksum)
- `si_pdks` (id, name, version, vendor, access: open|tenant_private)
- `si_tapeouts` (id, project_id, revision_id, shuttle, status, submitted_at)
- `si_ai_actions` (id, project_id, revision_id, intent, plan_json, status)

## Rollout Phases

### Phase S1: Foundations
- [ ] Workspace crates skeleton: `ef-si-core`, `ef-si-flow`, `ef-si-tools`
- [ ] DB migrations for projects / revisions / runs / artifacts
- [ ] REST scaffolding under `/api/v1/si/*`
- [ ] Dashboard nav entry + empty Silicon Studio page
- [ ] Runner image profile: Yosys + Verilator + OpenROAD + KLayout + Magic

### Phase S2: RTL + Verification MVP
- [ ] Multi-file RTL editor with module hierarchy
- [ ] Verilator-backed lint + compile diagnostics
- [ ] cocotb / Verilator simulation runs on runners
- [ ] Waveform (VCD/FST) viewer in dashboard
- [ ] Coverage summary v1 (line + toggle)

### Phase S3: Open-PDK Synth + PnR MVP
- [ ] Yosys synthesis run with QoR capture
- [ ] OpenROAD PnR run (floorplan → routing) on open PDK (sky130 first)
- [ ] Floorplan + congestion visualization
- [ ] DRC/LVS via KLayout + Netgen, report ingestion
- [ ] GDSII artifact browser

### Phase S4: FPGA Flow Parity
- [ ] Yosys + nextpnr flow for ECP5 / iCE40 / Gowin targets
- [ ] Bitstream artifact + timing report ingestion
- [ ] One-click "build + flash + smoke test" via EdgeFabric runners

### Phase S5: Spec / Config Driven SoC Composition
- [ ] SoC manifest schema (CPU, bus, peripherals, memory map)
- [ ] Block catalog with capability metadata
- [ ] Generator: manifest → top-level RTL + testbench scaffolding
- [ ] Parameter-driven bus/peripheral instantiation

### Phase S6: AI Co-Pilot
- [ ] Tool-use agent with `ef-si-ai`
- [ ] RTL generation + refactor with diff review
- [ ] Auto-write testbenches and assertions
- [ ] Constraint and synth strategy tuning loops
- [ ] AI-explained timing/DRC violations with proposed fixes

### Phase S7: Advanced PD + Realistic Visuals
- [ ] Interactive macro placement + pin assignment
- [ ] Power planning visualization (rings, straps, IR drop preview)
- [ ] WebGPU 3D die / package view
- [ ] Multi-corner STA dashboard

### Phase S8: Sign-Off & Tape-Out
- [ ] Sign-off pack assembly (GDSII + LEF/DEF + reports + manifest)
- [ ] Tape-out checklist with hard gating
- [ ] Shuttle adapters (e.g. Efabless / open MPW programs)
- [ ] Reproducibility manifest: tool versions, PDK rev, seeds, env hashes

### Phase S9: Commercial PDK / Tool Bridges (Optional)
- [ ] Adapter interface for licensed synth/PnR/STA tools
- [ ] Per-tenant license enforcement and audit
- [ ] Sign-off pack parity with commercial flows

## Quality & Compliance

- Every run is fully reproducible from `si_runs.tool_versions_json` + revision
- AI actions are reversible and logged for audit
- Tenant + PDK access controls enforced at API and storage layers
- Tape-out submissions require explicit human approval and checklist pass
- NDA-bound PDKs and IP must never leak via shared library scopes

## Open Questions

- How aggressive can we be with in-browser visualization for GDSII at scale?
- Containerized vs. native tool execution on runners — default policy
- Default open PDK(s) to ship first (sky130 is the obvious starting point)
- Pricing model for long synth/PnR jobs and AI token usage
- Where AI-generated RTL fits in IP provenance and licensing

## References

- Companion plan: `PCB_DESIGN_INTEGRATION_PLAN.md`
- Existing pattern: `QUANTUM_INTEGRATION_PLAN.md`
- System design overview: `EdgeFabric_System_Design.md`
