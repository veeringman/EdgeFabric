//! Chip Test / Verification / Simulation / Emulation backends.
//!
//! Two backend kinds:
//!   * `vfpga`  — VeerOS veer-vm "virtual FPGA": software-defined chip
//!                (cycle-accurate RTL simulator + waveform + JTAG-over-IPC).
//!   * `fpga`   — Hardware FPGA card attached to a host agent.
//!
//! Both expose the same conceptual interface (load bitstream / RTL, run
//! testbench, capture waveforms, attach debugger) so users can move a design
//! from veer-vm to silicon-equivalent FPGA without changing the harness.

use leptos::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Vfpga,
    Fpga,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Status {
    Idle,
    Booting,
    Running,
    Error,
    Offline,
}

#[derive(Clone, Debug, PartialEq)]
struct Backend {
    id: u32,
    name: String,
    kind: Kind,
    status: Status,
    host: String,        // agent / box hosting it
    target: String,      // e.g. "veer-vm v0.9 (rv32imc)" or "Xilinx XCVU13P"
    capabilities: Vec<&'static str>,
    project: Option<String>,
    util_pct: u32,
    uptime: String,
    notes: String,
}

fn seed() -> Vec<Backend> {
    vec![
        Backend {
            id: 1,
            name: "veer-vm-local".into(),
            kind: Kind::Vfpga,
            status: Status::Running,
            host: "local · macbook-pro".into(),
            target: "veer-vm 0.9 · rv32imc · 200 kLUT eq.".into(),
            capabilities: vec!["RTL sim", "Gate sim", "Formal", "Waveform", "JTAG-IPC"],
            project: Some("riscv-core-v1".into()),
            util_pct: 42,
            uptime: "2h 14m".into(),
            notes: "primary dev sandbox".into(),
        },
        Backend {
            id: 2,
            name: "veer-vm-cluster-01".into(),
            kind: Kind::Vfpga,
            status: Status::Idle,
            host: "agent: build-01 · 10.0.4.21".into(),
            target: "veer-vm 0.9 · 1.2M LUT eq. · 32 cores".into(),
            capabilities: vec!["RTL sim", "Gate sim", "Coverage", "Waveform"],
            project: None,
            util_pct: 4,
            uptime: "12d 7h".into(),
            notes: "shared CI farm".into(),
        },
        Backend {
            id: 3,
            name: "vcu118-rack-A".into(),
            kind: Kind::Fpga,
            status: Status::Running,
            host: "agent: lab-rack-a · 10.0.7.12".into(),
            target: "Xilinx Virtex-UltraScale+ XCVU9P · VCU118".into(),
            capabilities: vec!["Emulation", "In-circuit", "JTAG", "ChipScope"],
            project: Some("pqc-kyber".into()),
            util_pct: 68,
            uptime: "3d 1h".into(),
            notes: "primary PQC emulator".into(),
        },
        Backend {
            id: 4,
            name: "alveo-u280-prod".into(),
            kind: Kind::Fpga,
            status: Status::Booting,
            host: "agent: dc-rack-3 · 10.0.9.4".into(),
            target: "Xilinx Alveo U280 · HBM2 8 GB · PCIe Gen4".into(),
            capabilities: vec!["Emulation", "Accelerator", "DMA", "JTAG"],
            project: None,
            util_pct: 0,
            uptime: "—".into(),
            notes: "loading bitstream…".into(),
        },
        Backend {
            id: 5,
            name: "veer-vm-quantum-sim".into(),
            kind: Kind::Vfpga,
            status: Status::Idle,
            host: "agent: q-sim-01 · 10.0.6.18".into(),
            target: "veer-vm 0.9 · classical co-sim · SQC-Q5 harness".into(),
            capabilities: vec!["RTL sim", "Mixed-signal", "Waveform"],
            project: Some("sqc-q5".into()),
            util_pct: 0,
            uptime: "9h 22m".into(),
            notes: "qubit control co-simulation".into(),
        },
        Backend {
            id: 6,
            name: "arty-a7-bench".into(),
            kind: Kind::Fpga,
            status: Status::Error,
            host: "agent: bench-02 · 10.0.7.31".into(),
            target: "Xilinx Artix-7 35T · Digilent Arty".into(),
            capabilities: vec!["In-circuit", "JTAG", "UART"],
            project: None,
            util_pct: 0,
            uptime: "—".into(),
            notes: "JTAG cable disconnected".into(),
        },
    ]
}

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Vfpga => "veer-vm · vFPGA",
        Kind::Fpga => "Hardware FPGA",
    }
}

fn kind_class(k: Kind) -> &'static str {
    match k {
        Kind::Vfpga => "kind-vfpga",
        Kind::Fpga => "kind-fpga",
    }
}

fn status_label(s: Status) -> &'static str {
    match s {
        Status::Idle => "Idle",
        Status::Booting => "Booting",
        Status::Running => "Running",
        Status::Error => "Error",
        Status::Offline => "Offline",
    }
}

fn status_class(s: Status) -> &'static str {
    match s {
        Status::Idle => "st-idle",
        Status::Booting => "st-booting",
        Status::Running => "st-running",
        Status::Error => "st-error",
        Status::Offline => "st-offline",
    }
}

#[component]
pub fn SiliconBackendsPage() -> impl IntoView {
    let backends = RwSignal::new(seed());
    let next_id = RwSignal::new(7_u32);
    let filter_kind: RwSignal<Option<Kind>> = RwSignal::new(None);
    let filter_status: RwSignal<Option<Status>> = RwSignal::new(None);
    let show_create = RwSignal::new(false);

    // create form state
    let f_name = RwSignal::new(String::new());
    let f_kind = RwSignal::new("vfpga".to_string());
    let f_host = RwSignal::new(String::new());
    let f_target = RwSignal::new(String::new());
    let f_caps: RwSignal<HashMap<&'static str, bool>> = RwSignal::new({
        let mut m = HashMap::new();
        for c in ["RTL sim", "Gate sim", "Formal", "Coverage", "Emulation", "In-circuit", "Waveform", "JTAG"] {
            m.insert(c, c == "RTL sim" || c == "Waveform");
        }
        m
    });

    let do_create = move |_| {
        let name = f_name.get().trim().to_string();
        if name.is_empty() {
            return;
        }
        let kind = if f_kind.get() == "fpga" { Kind::Fpga } else { Kind::Vfpga };
        let caps: Vec<&'static str> = f_caps
            .get()
            .into_iter()
            .filter(|(_, v)| *v)
            .map(|(k, _)| k)
            .collect();
        let host = {
            let h = f_host.get();
            if h.trim().is_empty() { "local".into() } else { h }
        };
        let target = {
            let t = f_target.get();
            if t.trim().is_empty() {
                match kind {
                    Kind::Vfpga => "veer-vm 0.9 · rv32imc".into(),
                    Kind::Fpga => "Generic FPGA".into(),
                }
            } else {
                t
            }
        };
        let id = next_id.get();
        next_id.set(id + 1);
        backends.update(|b| {
            b.push(Backend {
                id,
                name,
                kind,
                status: Status::Booting,
                host,
                target,
                capabilities: caps,
                project: None,
                util_pct: 0,
                uptime: "—".into(),
                notes: "provisioning…".into(),
            });
        });
        f_name.set(String::new());
        f_host.set(String::new());
        f_target.set(String::new());
        show_create.set(false);
    };

    let toggle_cap = move |c: &'static str| {
        f_caps.update(|m| {
            let v = m.get(c).copied().unwrap_or(false);
            m.insert(c, !v);
        });
    };

    let set_status = move |id: u32, s: Status| {
        backends.update(|list| {
            if let Some(b) = list.iter_mut().find(|b| b.id == id) {
                b.status = s;
                if matches!(s, Status::Idle | Status::Offline | Status::Error) {
                    b.util_pct = 0;
                    if matches!(s, Status::Offline | Status::Error) {
                        b.uptime = "—".into();
                    }
                }
            }
        });
    };

    let delete_backend = move |id: u32| {
        backends.update(|b| b.retain(|x| x.id != id));
    };

    // derived stats
    let total = move || backends.with(|b| b.len());
    let running = move || backends.with(|b| b.iter().filter(|x| x.status == Status::Running).count());
    let vfpga_cnt = move || backends.with(|b| b.iter().filter(|x| x.kind == Kind::Vfpga).count());
    let fpga_cnt = move || backends.with(|b| b.iter().filter(|x| x.kind == Kind::Fpga).count());

    view! {
        <style>{BACKENDS_CSS}</style>

        <div class="si-be">
            <header class="si-be-hdr">
                <div>
                    <span class="si-be-kicker">"Chip Design · Backends"</span>
                    <h1>"Test / Verification / Simulation / Emulation"</h1>
                    <p class="si-be-sub">
                        "Manage chip-design backends. " <b>"veer-vm"</b>
                        " runs software-defined chips (virtual FPGA). "
                        <b>"Hardware FPGA"</b>
                        " backends attach to physical boards on lab/agent hosts."
                    </p>
                </div>
                <button class="si-be-btn primary"
                    on:click=move |_| show_create.update(|v| *v = !*v)>
                    {move || if show_create.get() { "▾ Close" } else { "+ New backend" }}
                </button>
            </header>

            // ---- summary stats ----
            <div class="si-be-stats">
                <div class="si-be-stat">
                    <span class="lbl">"Backends"</span>
                    <span class="val">{total}</span>
                </div>
                <div class="si-be-stat">
                    <span class="lbl">"Running"</span>
                    <span class="val running">{running}</span>
                </div>
                <div class="si-be-stat">
                    <span class="lbl">"veer-vm (vFPGA)"</span>
                    <span class="val vfpga">{vfpga_cnt}</span>
                </div>
                <div class="si-be-stat">
                    <span class="lbl">"Hardware FPGA"</span>
                    <span class="val fpga">{fpga_cnt}</span>
                </div>
            </div>

            // ---- create form ----
            {move || if show_create.get() {
                view! {
                    <section class="si-be-create">
                        <h3>"Provision a new backend"</h3>
                        <div class="si-be-form">
                            <label>
                                <span>"Backend kind"</span>
                                <select
                                    on:change=move |ev| f_kind.set(event_target_value(&ev))
                                    prop:value=move || f_kind.get()
                                >
                                    <option value="vfpga">"veer-vm · Virtual FPGA (software)"</option>
                                    <option value="fpga">"Hardware FPGA (physical board)"</option>
                                </select>
                            </label>
                            <label>
                                <span>"Name"</span>
                                <input type="text" placeholder="e.g. veer-vm-dev-01 or vcu118-rack-A"
                                    prop:value=move || f_name.get()
                                    on:input=move |ev| f_name.set(event_target_value(&ev))/>
                            </label>
                            <label>
                                <span>"Host / Agent"</span>
                                <input type="text" placeholder="local · or agent:<host>"
                                    prop:value=move || f_host.get()
                                    on:input=move |ev| f_host.set(event_target_value(&ev))/>
                            </label>
                            <label class="full">
                                <span>{move || if f_kind.get() == "fpga" {
                                    "Target device (board / part)"
                                } else {
                                    "Target VM profile"
                                }}</span>
                                <input type="text"
                                    placeholder=move || if f_kind.get() == "fpga" {
                                        "e.g. Xilinx XCVU9P · VCU118"
                                    } else {
                                        "e.g. veer-vm 0.9 · rv32imc · 200 kLUT eq."
                                    }
                                    prop:value=move || f_target.get()
                                    on:input=move |ev| f_target.set(event_target_value(&ev))/>
                            </label>
                            <div class="si-be-caps full">
                                <span class="caps-lbl">"Capabilities"</span>
                                <div class="caps-grid">
                                    {["RTL sim", "Gate sim", "Formal", "Coverage", "Emulation", "In-circuit", "Waveform", "JTAG"].iter().map(|c| {
                                        let cc = *c;
                                        view! {
                                            <label class="cap-chk">
                                                <input type="checkbox"
                                                    prop:checked=move || f_caps.with(|m| m.get(cc).copied().unwrap_or(false))
                                                    on:change=move |_| toggle_cap(cc)/>
                                                <span>{cc}</span>
                                            </label>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>
                            <div class="si-be-actions full">
                                <button class="si-be-btn ghost" on:click=move |_| show_create.set(false)>"Cancel"</button>
                                <button class="si-be-btn primary" on:click=do_create>"Provision backend"</button>
                            </div>
                        </div>
                    </section>
                }.into_any()
            } else { ().into_any() }}

            // ---- filter bar ----
            <div class="si-be-filters">
                <span class="flt-lbl">"Filter:"</span>
                <button class="flt"
                    class:active=move || filter_kind.get().is_none()
                    on:click=move |_| filter_kind.set(None)>"All kinds"</button>
                <button class="flt"
                    class:active=move || matches!(filter_kind.get(), Some(Kind::Vfpga))
                    on:click=move |_| filter_kind.set(Some(Kind::Vfpga))>"veer-vm"</button>
                <button class="flt"
                    class:active=move || matches!(filter_kind.get(), Some(Kind::Fpga))
                    on:click=move |_| filter_kind.set(Some(Kind::Fpga))>"Hardware FPGA"</button>
                <span class="flt-sep"/>
                <button class="flt"
                    class:active=move || filter_status.get().is_none()
                    on:click=move |_| filter_status.set(None)>"Any status"</button>
                <button class="flt"
                    class:active=move || matches!(filter_status.get(), Some(Status::Running))
                    on:click=move |_| filter_status.set(Some(Status::Running))>"Running"</button>
                <button class="flt"
                    class:active=move || matches!(filter_status.get(), Some(Status::Idle))
                    on:click=move |_| filter_status.set(Some(Status::Idle))>"Idle"</button>
                <button class="flt"
                    class:active=move || matches!(filter_status.get(), Some(Status::Error))
                    on:click=move |_| filter_status.set(Some(Status::Error))>"Error"</button>
            </div>

            // ---- backend cards ----
            <div class="si-be-grid">
                {move || {
                    let fk = filter_kind.get();
                    let fs = filter_status.get();
                    backends.with(|list| {
                        list.iter()
                            .filter(|b| fk.map_or(true, |k| k == b.kind))
                            .filter(|b| fs.map_or(true, |s| s == b.status))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .into_iter()
                    .map(|b| {
                        let id = b.id;
                        let kc = kind_class(b.kind);
                        let sc = status_class(b.status);
                        view! {
                            <article class={format!("si-be-card {kc}")}>
                                <header class="card-hdr">
                                    <div class="card-title">
                                        <span class={format!("kind-badge {kc}")}>{kind_label(b.kind)}</span>
                                        <h3>{b.name.clone()}</h3>
                                    </div>
                                    <span class={format!("st-badge {sc}")}>{status_label(b.status)}</span>
                                </header>
                                <div class="card-meta">
                                    <div><span>"Host"</span><b>{b.host.clone()}</b></div>
                                    <div><span>"Target"</span><b>{b.target.clone()}</b></div>
                                    <div><span>"Project"</span><b>{b.project.clone().unwrap_or_else(|| "—".into())}</b></div>
                                    <div><span>"Util"</span>
                                        <div class="util-bar">
                                            <div class="util-fill" style={format!("width:{}%", b.util_pct)}/>
                                            <span class="util-txt">{format!("{}%", b.util_pct)}</span>
                                        </div>
                                    </div>
                                    <div><span>"Uptime"</span><b>{b.uptime.clone()}</b></div>
                                </div>
                                <div class="caps-pills">
                                    {b.capabilities.iter().map(|c| view! { <span class="cap-pill">{*c}</span> }).collect_view()}
                                </div>
                                <p class="card-notes">{b.notes.clone()}</p>
                                <footer class="card-actions">
                                    {match b.status {
                                        Status::Running => view! {
                                            <button class="si-be-btn warn" on:click=move |_| set_status(id, Status::Idle)>"⏸ Stop"</button>
                                        }.into_any(),
                                        Status::Error | Status::Offline => view! {
                                            <button class="si-be-btn primary" on:click=move |_| set_status(id, Status::Booting)>"⟳ Recover"</button>
                                        }.into_any(),
                                        _ => view! {
                                            <button class="si-be-btn primary" on:click=move |_| set_status(id, Status::Running)>"▶ Start"</button>
                                        }.into_any(),
                                    }}
                                    <button class="si-be-btn ghost">"↪ Attach project"</button>
                                    <button class="si-be-btn ghost">"≡ Open console"</button>
                                    <button class="si-be-btn ghost">"⌖ Waveform"</button>
                                    <button class="si-be-btn danger" on:click=move |_| delete_backend(id)>"✕"</button>
                                </footer>
                            </article>
                        }
                    }).collect_view()
                }}
            </div>

            // ---- explanatory side strip ----
            <section class="si-be-explain">
                <div>
                    <span class="ex-badge kind-vfpga">"veer-vm"</span>
                    <p>"VeerOS veer-vm runs the chip as a "<b>"software-defined FPGA"</b>". "
                       "Same RTL/bitstream interface as a real card; cycle-accurate; fast to spin up; "
                       "ideal for unit tests, regression, formal proofs and CI farms."</p>
                </div>
                <div>
                    <span class="ex-badge kind-fpga">"Hardware FPGA"</span>
                    <p>"Physical FPGA on a lab/agent host (Xilinx, Lattice, Intel). "
                       "Used for emulation, in-circuit validation, signal-integrity testing, "
                       "and as a drop-in pre-silicon platform for the PQC accelerator and RISC-V cores."</p>
                </div>
            </section>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const BACKENDS_CSS: &str = r#"
.si-be { padding:1.2rem 1.6rem; color:#dbe2f1; background:#0a0f1c; min-height:100%; }
.si-be-hdr { display:flex; justify-content:space-between; align-items:flex-start; gap:1rem;
    padding-bottom:1rem; border-bottom:1px solid #1d2740; margin-bottom:1rem; }
.si-be-kicker { font-size:.72rem; color:#7e8db0; text-transform:uppercase; letter-spacing:.08em; }
.si-be-hdr h1 { font-size:1.4rem; margin:.2rem 0 .35rem;
    background:linear-gradient(90deg,#9fb6ff,#d39bff); -webkit-background-clip:text;
    background-clip:text; color:transparent; }
.si-be-sub { color:#8c98b3; font-size:.85rem; max-width:780px; margin:0; }
.si-be-sub b { color:#cdd6ee; font-weight:600; }

.si-be-btn { background:#1a2540; color:#cdd6ee; border:1px solid #2a3654;
    padding:.42rem .8rem; border-radius:6px; font-size:.78rem; cursor:pointer;
    transition:all .15s; white-space:nowrap; }
.si-be-btn:hover { background:#243358; border-color:#3a4c7a; transform:translateY(-1px); }
.si-be-btn.primary { background:linear-gradient(135deg,#3d52a0,#7b4ec2);
    border-color:#9fb6ff; color:#fff; box-shadow:0 0 12px rgba(123,78,194,.35); }
.si-be-btn.ghost { background:transparent; }
.si-be-btn.warn { background:#5a4a1f; border-color:#c1a14a; }
.si-be-btn.danger { background:#5a1f2c; border-color:#c14a6b; color:#ffd0d6; }

.si-be-stats { display:grid; grid-template-columns:repeat(4, 1fr); gap:.8rem; margin-bottom:1rem; }
.si-be-stat { background:linear-gradient(180deg,#121a2e,#0c1322); border:1px solid #1d2740;
    border-radius:8px; padding:.7rem .9rem; display:flex; flex-direction:column; gap:.25rem; }
.si-be-stat .lbl { font-size:.7rem; color:#7e8db0; text-transform:uppercase; letter-spacing:.06em; }
.si-be-stat .val { font-size:1.6rem; font-weight:600; color:#cdd6ee; font-family:ui-monospace,monospace; }
.si-be-stat .val.running { color:#5fd7b7; }
.si-be-stat .val.vfpga { color:#9fb6ff; }
.si-be-stat .val.fpga { color:#ffb86b; }

.si-be-create { background:#0e1526; border:1px solid #2a3654; border-radius:10px;
    padding:1rem 1.2rem; margin-bottom:1rem; }
.si-be-create h3 { margin:0 0 .8rem; font-size:.95rem; color:#cdd6ee; }
.si-be-form { display:grid; grid-template-columns:1fr 1fr; gap:.7rem 1rem; }
.si-be-form label { display:flex; flex-direction:column; gap:.25rem; font-size:.75rem; color:#9eaac8; }
.si-be-form label.full { grid-column:1 / -1; }
.si-be-form input, .si-be-form select { background:#1a2540; color:#dbe2f1;
    border:1px solid #2a3654; border-radius:5px; padding:.4rem .55rem; font-size:.82rem; }
.si-be-caps { grid-column:1 / -1; }
.caps-lbl { font-size:.75rem; color:#9eaac8; }
.caps-grid { display:grid; grid-template-columns:repeat(4, 1fr); gap:.35rem; margin-top:.35rem; }
.cap-chk { display:flex; align-items:center; gap:.35rem; font-size:.78rem; color:#cdd6ee;
    background:#141d33; border:1px solid #233258; border-radius:5px; padding:.3rem .5rem; cursor:pointer; }
.cap-chk:hover { background:#1c2848; }
.si-be-actions { display:flex; gap:.5rem; justify-content:flex-end; }

.si-be-filters { display:flex; flex-wrap:wrap; align-items:center; gap:.35rem; margin-bottom:.8rem; }
.flt-lbl { font-size:.74rem; color:#7e8db0; margin-right:.3rem; }
.flt { background:#141d33; color:#9eaac8; border:1px solid #233258; border-radius:14px;
    padding:.22rem .65rem; font-size:.72rem; cursor:pointer; transition:all .15s; }
.flt:hover { background:#1c2848; color:#cdd6ee; }
.flt.active { background:linear-gradient(135deg,#3d52a0,#7b4ec2); color:#fff;
    border-color:#9fb6ff; box-shadow:0 0 10px rgba(123,78,194,.4); }
.flt-sep { width:1px; height:16px; background:#2a3654; margin:0 .35rem; }

.si-be-grid { display:grid; grid-template-columns:repeat(auto-fill, minmax(380px, 1fr));
    gap:.9rem; }
.si-be-card { background:linear-gradient(180deg,#121a2e,#0c1322); border:1px solid #1d2740;
    border-radius:10px; padding:.85rem 1rem; display:flex; flex-direction:column; gap:.55rem;
    transition:all .15s; }
.si-be-card:hover { border-color:#3a4c7a; transform:translateY(-1px);
    box-shadow:0 4px 22px rgba(0,0,0,.4); }
.si-be-card.kind-vfpga { border-left:3px solid #6e8cf2; }
.si-be-card.kind-fpga  { border-left:3px solid #ffb86b; }
.card-hdr { display:flex; justify-content:space-between; align-items:flex-start; gap:.5rem; }
.card-title h3 { margin:.2rem 0 0; font-size:1rem; color:#dbe2f1; font-family:ui-monospace,monospace; }
.kind-badge { font-size:.62rem; padding:.15rem .5rem; border-radius:10px;
    text-transform:uppercase; letter-spacing:.06em; font-weight:600; }
.kind-badge.kind-vfpga { background:rgba(110,140,242,.15); color:#9fb6ff; border:1px solid #3d52a0; }
.kind-badge.kind-fpga  { background:rgba(255,184,107,.15); color:#ffd0a0; border:1px solid #a06a30; }

.st-badge { font-size:.68rem; padding:.18rem .6rem; border-radius:10px; font-weight:600;
    text-transform:uppercase; letter-spacing:.05em; }
.st-badge.st-running { background:rgba(95,215,183,.18); color:#5fd7b7; border:1px solid #2f8a72; }
.st-badge.st-idle    { background:rgba(159,182,255,.12); color:#9fb6ff; border:1px solid #3d52a0; }
.st-badge.st-booting { background:rgba(255,209,102,.16); color:#ffd166; border:1px solid #a07f30; }
.st-badge.st-error   { background:rgba(255,107,140,.18); color:#ff8aa3; border:1px solid #a04060; }
.st-badge.st-offline { background:rgba(120,140,180,.12); color:#7e8db0; border:1px solid #3a4c7a; }

.card-meta { display:grid; grid-template-columns:1fr 1fr; gap:.35rem .8rem;
    font-size:.76rem; }
.card-meta > div { display:flex; flex-direction:column; gap:.1rem; min-width:0; }
.card-meta span { color:#7e8db0; font-size:.66rem; text-transform:uppercase; letter-spacing:.05em; }
.card-meta b { color:#cdd6ee; font-weight:500; font-family:ui-monospace,monospace;
    overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }

.util-bar { position:relative; background:#141d33; border:1px solid #233258; border-radius:4px;
    height:14px; overflow:hidden; }
.util-fill { background:linear-gradient(90deg,#3d52a0,#7b4ec2); height:100%; }
.util-txt { position:absolute; inset:0; text-align:center; font-size:.65rem; color:#fff;
    line-height:14px; font-family:ui-monospace,monospace; }

.caps-pills { display:flex; flex-wrap:wrap; gap:.25rem; }
.cap-pill { font-size:.66rem; background:#141d33; color:#9eaac8; border:1px solid #233258;
    border-radius:8px; padding:.12rem .5rem; }

.card-notes { margin:0; font-size:.74rem; color:#8c98b3; font-style:italic; }
.card-actions { display:flex; flex-wrap:wrap; gap:.3rem; padding-top:.4rem;
    border-top:1px solid #1d2740; }

.si-be-explain { display:grid; grid-template-columns:1fr 1fr; gap:.8rem; margin-top:1.4rem; }
.si-be-explain > div { background:#0e1526; border:1px solid #1d2740; border-radius:8px;
    padding:.8rem 1rem; }
.si-be-explain p { margin:.4rem 0 0; font-size:.78rem; color:#9eaac8; line-height:1.45; }
.si-be-explain b { color:#cdd6ee; }
.ex-badge { display:inline-block; font-size:.64rem; padding:.18rem .55rem; border-radius:10px;
    text-transform:uppercase; letter-spacing:.06em; font-weight:600; }
.ex-badge.kind-vfpga { background:rgba(110,140,242,.15); color:#9fb6ff; border:1px solid #3d52a0; }
.ex-badge.kind-fpga  { background:rgba(255,184,107,.15); color:#ffd0a0; border:1px solid #a06a30; }
"#;
