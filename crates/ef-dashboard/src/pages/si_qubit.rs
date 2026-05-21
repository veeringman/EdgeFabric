use leptos::prelude::*;

use crate::components::stat_card::StatCard;

#[derive(Clone, Copy)]
struct Qubit {
    id: &'static str,
    row: u32,
    col: u32,
    freq_ghz: f64,
    anharm_mhz: f64,
    t1_us: f64,
    t2_us: f64,
    readout_f_err: f64, // %
    role: &'static str, // "data" | "ancilla"
}

#[derive(Clone, Copy)]
struct Coupler {
    a: &'static str,
    b: &'static str,
    g_mhz: f64,
}

// 5-qubit linear chain (X-shape transmons) with bus resonator
const QUBITS: &[Qubit] = &[
    Qubit { id: "Q0", row: 1, col: 0, freq_ghz: 5.012, anharm_mhz: -312.0, t1_us: 84.0, t2_us: 71.0, readout_f_err: 1.2, role: "data" },
    Qubit { id: "Q1", row: 0, col: 1, freq_ghz: 5.182, anharm_mhz: -305.0, t1_us: 92.0, t2_us: 68.0, readout_f_err: 0.9, role: "ancilla" },
    Qubit { id: "Q2", row: 1, col: 2, freq_ghz: 4.954, anharm_mhz: -308.0, t1_us: 110.0, t2_us: 88.0, readout_f_err: 0.7, role: "data" },
    Qubit { id: "Q3", row: 2, col: 1, freq_ghz: 5.221, anharm_mhz: -301.0, t1_us: 76.0, t2_us: 61.0, readout_f_err: 1.4, role: "ancilla" },
    Qubit { id: "Q4", row: 1, col: 4, freq_ghz: 5.078, anharm_mhz: -310.0, t1_us: 98.0, t2_us: 79.0, readout_f_err: 1.0, role: "data" },
];

const COUPLERS: &[Coupler] = &[
    Coupler { a: "Q0", b: "Q1", g_mhz: 18.2 },
    Coupler { a: "Q1", b: "Q2", g_mhz: 17.9 },
    Coupler { a: "Q2", b: "Q3", g_mhz: 18.4 },
    Coupler { a: "Q2", b: "Q4", g_mhz: 18.1 },
];

// Layout: 600x420 viewBox; grid step
const STEP: f64 = 110.0;
const ORIGIN_X: f64 = 90.0;
const ORIGIN_Y: f64 = 70.0;

fn qubit_xy(q: &Qubit) -> (f64, f64) {
    (ORIGIN_X + q.col as f64 * STEP, ORIGIN_Y + q.row as f64 * STEP)
}

#[component]
pub fn SiliconQubitPage() -> impl IntoView {
    let selected: RwSignal<&'static str> = RwSignal::new("Q2");
    let show_readout = RwSignal::new(true);
    let show_drive = RwSignal::new(true);

    view! {
        <div class="page-container silicon-qubit-page">
            <div class="page-header">
                <div>
                    <span class="section-kicker">"Chip Design · Sample Project"</span>
                    <h1>"Superconducting Qubit Chip — SQC-Q5"</h1>
                    <p class="page-subtitle">
                        "5-qubit transmon test chip · X-mon geometry · Al/AlOx Josephson junctions · sapphire substrate · 20 mK operation."
                    </p>
                </div>
                <div class="rtl-actions">
                    <a href="/silicon/schematic?project=sqc-q5" class="btn btn-ghost btn-sm">"View Schematic"</a>
                    <a href="/silicon/stackup?project=sqc-q5" class="btn btn-ghost btn-sm">"View 3D Stack-up"</a>
                    <button class="btn btn-ghost btn-sm">"Export GDS"</button>
                    <button class="btn btn-ghost btn-sm">"Run EM Sim"</button>
                    <button class="btn btn-primary btn-sm">"Submit to Fab"</button>
                </div>
            </div>

            <div class="stats-grid">
                <StatCard label="Qubits" value=Signal::derive(|| "5".to_string())/>
                <StatCard label="Avg T₁" value=Signal::derive(|| "92 µs".to_string())/>
                <StatCard label="Avg T₂" value=Signal::derive(|| "73 µs".to_string())/>
                <StatCard label="Readout Fidelity" value=Signal::derive(|| "98.96 %".to_string())/>
            </div>

            <div class="qubit-layout">
                <section class="qubit-canvas-wrap">
                    <div class="qubit-toolbar">
                        <label class="fp-toggle">
                            <input type="checkbox" prop:checked=move || show_readout.get()
                                on:change=move |ev| show_readout.set(event_target_checked(&ev))/>
                            <span>"Readout Resonators"</span>
                        </label>
                        <label class="fp-toggle">
                            <input type="checkbox" prop:checked=move || show_drive.get()
                                on:change=move |ev| show_drive.set(event_target_checked(&ev))/>
                            <span>"Drive / Flux Lines"</span>
                        </label>
                    </div>
                    <svg viewBox="0 0 600 420" class="qubit-svg" preserveAspectRatio="xMidYMid meet">
                        // chip substrate
                        <rect x="20" y="20" width="560" height="380" rx="8" class="qchip-die"/>
                        // ground plane hatch
                        <defs>
                            <pattern id="gp" width="14" height="14" patternUnits="userSpaceOnUse" patternTransform="rotate(35)">
                                <line x1="0" y1="0" x2="0" y2="14" stroke="#1e293b" stroke-width="1"/>
                            </pattern>
                        </defs>
                        <rect x="20" y="20" width="560" height="380" rx="8" fill="url(#gp)" opacity="0.35"/>

                        // bus resonator (meandering CPW between qubits)
                        <path d="M 90 200 Q 145 240 200 200 T 310 200 T 420 200 T 530 200"
                              class="qbus"/>
                        <text x="300" y="232" text-anchor="middle" class="qlabel-muted">"bus CPW · ~6.8 GHz"</text>

                        // couplers (drawn as straight lines under qubit X-mons)
                        {COUPLERS.iter().map(|c| {
                            let a = QUBITS.iter().find(|q| q.id == c.a).unwrap();
                            let b = QUBITS.iter().find(|q| q.id == c.b).unwrap();
                            let (ax, ay) = qubit_xy(a);
                            let (bx, by) = qubit_xy(b);
                            view! {
                                <g>
                                    <line x1=ax y1=ay x2=bx y2=by class="qcoupler"/>
                                    <text x={(ax+bx)/2.0} y={(ay+by)/2.0 - 6.0} text-anchor="middle" class="qlabel-tiny">
                                        {format!("g/2π = {:.1} MHz", c.g_mhz)}
                                    </text>
                                </g>
                            }
                        }).collect::<Vec<_>>()}

                        // readout resonators (short hooks)
                        {move || show_readout.get().then(|| {
                            QUBITS.iter().map(|q| {
                                let (x, y) = qubit_xy(q);
                                let dx = 36.0_f64;
                                let dy = -36.0_f64;
                                view! {
                                    <g class="qreadout">
                                        <path d={format!("M {} {} q 12 -8 24 0 t 24 0 t 24 0", x + 22.0, y - 22.0)}/>
                                        <text x={x + dx + 30.0} y={y + dy - 8.0} class="qlabel-tiny">{format!("R{} · 7.{}GHz", &q.id[1..], q.id.chars().last().unwrap())}</text>
                                    </g>
                                }
                            }).collect::<Vec<_>>()
                        })}

                        // drive lines (from edge into qubit)
                        {move || show_drive.get().then(|| {
                            QUBITS.iter().map(|q| {
                                let (x, y) = qubit_xy(q);
                                // drive from nearest edge
                                let (sx, sy) = if q.col == 0 { (40.0, y) }
                                    else if q.col >= 4 { (560.0, y) }
                                    else if q.row == 0 { (x, 40.0) }
                                    else { (x, 380.0) };
                                view! {
                                    <g class="qdrive">
                                        <line x1=sx y1=sy x2=x y2=y/>
                                        <circle cx=sx cy=sy r=4.0_f64/>
                                    </g>
                                }
                            }).collect::<Vec<_>>()
                        })}

                        // qubits: X-mon cross
                        {QUBITS.iter().map(|q| {
                            let q = *q;
                            let (x, y) = qubit_xy(&q);
                            let is_sel = {
                                let selected = selected;
                                move || selected.get() == q.id
                            };
                            view! {
                                <g class="qubit" class:selected=is_sel
                                   on:click=move |_| selected.set(q.id)>
                                    // junction pad (center square)
                                    <rect x={x-7.0} y={y-7.0} width=14.0_f64 height=14.0_f64 class="qjunction"/>
                                    // four cross arms
                                    <rect x={x-26.0} y={y-6.0} width=52.0_f64 height=12.0_f64 rx=2.0_f64 class={if q.role == "ancilla" { "qxmon qxmon--anc" } else { "qxmon qxmon--data" }}/>
                                    <rect x={x-6.0} y={y-26.0} width=12.0_f64 height=52.0_f64 rx=2.0_f64 class={if q.role == "ancilla" { "qxmon qxmon--anc" } else { "qxmon qxmon--data" }}/>
                                    <text x=x y={y + 44.0} text-anchor="middle" class="qlabel">{q.id}</text>
                                </g>
                            }
                        }).collect::<Vec<_>>()}

                        // launchers / wire-bond pads along edges (decoration)
                        {(0..6).map(|i| {
                            let x = 60.0 + i as f64 * 100.0;
                            view! { <rect x={x-8.0} y=24.0_f64 width=16.0_f64 height=10.0_f64 class="qpad"/> }
                        }).collect::<Vec<_>>()}
                        {(0..6).map(|i| {
                            let x = 60.0 + i as f64 * 100.0;
                            view! { <rect x={x-8.0} y=386.0_f64 width=16.0_f64 height=10.0_f64 class="qpad"/> }
                        }).collect::<Vec<_>>()}
                    </svg>

                    <div class="fp-legend">
                        <span class="legend-item"><span class="dot dot-data"></span>"Data qubit"</span>
                        <span class="legend-item"><span class="dot dot-anc"></span>"Ancilla qubit"</span>
                        <span class="legend-item"><span class="dot dot-bus"></span>"Bus CPW"</span>
                        <span class="legend-item"><span class="dot dot-drv"></span>"Drive line"</span>
                        <span class="legend-item"><span class="dot dot-ro"></span>"Readout"</span>
                    </div>
                </section>

                <aside class="qubit-inspector">
                    <div class="fp-inspector-header">"Qubit Inspector"</div>
                    {move || {
                        let id = selected.get();
                        match QUBITS.iter().find(|q| q.id == id).copied() {
                            Some(q) => view! {
                                <div class="fp-props">
                                    <div class="prop"><span>"ID"</span><strong>{q.id}</strong></div>
                                    <div class="prop"><span>"Role"</span><strong>{q.role}</strong></div>
                                    <div class="prop"><span>"f₀₁"</span><strong>{format!("{:.3} GHz", q.freq_ghz)}</strong></div>
                                    <div class="prop"><span>"Anharm. α/2π"</span><strong>{format!("{:.0} MHz", q.anharm_mhz)}</strong></div>
                                    <div class="prop"><span>"T₁"</span><strong>{format!("{:.0} µs", q.t1_us)}</strong></div>
                                    <div class="prop"><span>"T₂*"</span><strong>{format!("{:.0} µs", q.t2_us)}</strong></div>
                                    <div class="prop"><span>"Readout err"</span><strong>{format!("{:.2} %", q.readout_f_err)}</strong></div>
                                </div>
                            }.into_any(),
                            None => view! { <p class="empty-state">"Select a qubit"</p> }.into_any(),
                        }
                    }}

                    <div class="fp-inspector-header">"Chip Parameters"</div>
                    <div class="fp-props">
                        <div class="prop"><span>"Substrate"</span><strong>"Sapphire (c-plane)"</strong></div>
                        <div class="prop"><span>"Metal"</span><strong>"Nb / Al"</strong></div>
                        <div class="prop"><span>"Junction"</span><strong>"Al/AlOx/Al (Dolan)"</strong></div>
                        <div class="prop"><span>"Eᴊ/Eᴄ"</span><strong>"~ 65"</strong></div>
                        <div class="prop"><span>"Die size"</span><strong>"5 × 5 mm"</strong></div>
                        <div class="prop"><span>"Operating temp"</span><strong>"~20 mK"</strong></div>
                        <div class="prop"><span>"Package"</span><strong>"Wire-bonded · 24-port"</strong></div>
                    </div>
                </aside>
            </div>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Qubit Parameters"</h2>
                </div>
                <table class="qor-table">
                    <thead>
                        <tr>
                            <th>"Qubit"</th>
                            <th>"Role"</th>
                            <th>"f₀₁ (GHz)"</th>
                            <th>"α/2π (MHz)"</th>
                            <th>"T₁ (µs)"</th>
                            <th>"T₂* (µs)"</th>
                            <th>"Readout err"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {QUBITS.iter().map(|q| view! {
                            <tr>
                                <td><code>{q.id}</code></td>
                                <td>{q.role}</td>
                                <td>{format!("{:.3}", q.freq_ghz)}</td>
                                <td>{format!("{:.0}", q.anharm_mhz)}</td>
                                <td>{format!("{:.0}", q.t1_us)}</td>
                                <td>{format!("{:.0}", q.t2_us)}</td>
                                <td>{format!("{:.2} %", q.readout_f_err)}</td>
                            </tr>
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </section>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Couplings"</h2>
                </div>
                <table class="qor-table">
                    <thead>
                        <tr><th>"Pair"</th><th>"g/2π (MHz)"</th><th>"Detuning Δ"</th><th>"Type"</th></tr>
                    </thead>
                    <tbody>
                        {COUPLERS.iter().map(|c| {
                            let qa = QUBITS.iter().find(|q| q.id == c.a).unwrap();
                            let qb = QUBITS.iter().find(|q| q.id == c.b).unwrap();
                            let delta = (qa.freq_ghz - qb.freq_ghz) * 1000.0;
                            view! {
                                <tr>
                                    <td><code>{format!("{}–{}", c.a, c.b)}</code></td>
                                    <td>{format!("{:.1}", c.g_mhz)}</td>
                                    <td>{format!("{:+.0} MHz", delta)}</td>
                                    <td>"Capacitive (transmon-transmon)"</td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </section>
        </div>

        <style>{r#"
            .silicon-qubit-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
            .silicon-qubit-page .page-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; flex-wrap: wrap; }
            .silicon-qubit-page .page-header h1 { margin: 0; }
            .silicon-qubit-page .page-subtitle { margin: 0.35rem 0 0; color: var(--text-secondary); max-width: 780px; }
            .qubit-layout { display: grid; grid-template-columns: minmax(0,1fr) 280px; gap: 0.75rem; }
            .qubit-canvas-wrap, .qubit-inspector, .silicon-section {
                background: var(--card-bg, var(--bg-card));
                border: 1px solid var(--border-color, var(--border));
                border-radius: 10px;
                padding: 1rem;
            }
            .qubit-toolbar { display: flex; gap: 0.75rem; margin-bottom: 0.6rem; flex-wrap: wrap; }
            .fp-toggle { display: inline-flex; gap: 0.35rem; align-items: center; color: var(--text-secondary); font-size: 0.85rem; }

            .qubit-svg { width: 100%; height: auto; display: block; background: #0a1226; border-radius: 6px; }
            .qchip-die { fill: #0f172a; stroke: #475569; stroke-width: 1.5; }

            .qbus { fill: none; stroke: #06b6d4; stroke-width: 2; opacity: 0.85; }
            .qcoupler { stroke: #94a3b8; stroke-width: 2; stroke-dasharray: 4 3; }

            .qubit { cursor: pointer; }
            .qjunction { fill: #facc15; stroke: #fde68a; stroke-width: 1; }
            .qxmon { stroke-width: 1.2; }
            .qxmon--data { fill: rgba(59,130,246,0.30); stroke: #3b82f6; }
            .qxmon--anc  { fill: rgba(168,85,247,0.30); stroke: #a855f7; }
            .qubit.selected .qxmon { stroke: #f1f5f9; stroke-width: 2.4; filter: drop-shadow(0 0 4px rgba(241,245,249,0.45)); }

            .qreadout path { fill: none; stroke: #22c55e; stroke-width: 1.4; opacity: 0.85; }
            .qdrive line { stroke: #f59e0b; stroke-width: 1.4; stroke-dasharray: 2 2; opacity: 0.85; }
            .qdrive circle { fill: #f59e0b; }
            .qpad { fill: #cbd5e1; opacity: 0.85; }

            .qlabel { fill: #e2e8f0; font-size: 12px; font-family: var(--font-mono); font-weight: 700; pointer-events: none; }
            .qlabel-muted { fill: #94a3b8; font-size: 11px; font-family: var(--font-mono); }
            .qlabel-tiny  { fill: #94a3b8; font-size: 10px; font-family: var(--font-mono); }

            .fp-legend { display: flex; gap: 1rem; margin-top: 0.6rem; flex-wrap: wrap; color: var(--text-secondary); font-size: 0.78rem; }
            .legend-item { display: inline-flex; gap: 0.35rem; align-items: center; }
            .dot { width: 10px; height: 10px; border-radius: 2px; display: inline-block; }
            .dot-data { background: #3b82f6; }
            .dot-anc  { background: #a855f7; }
            .dot-bus  { background: #06b6d4; }
            .dot-drv  { background: #f59e0b; }
            .dot-ro   { background: #22c55e; }

            .fp-inspector-header { font-size: 0.75rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.08em; margin: 0.4rem 0 0.5rem; }
            .fp-props { display: grid; gap: 0.35rem; }
            .prop { display: flex; justify-content: space-between; gap: 0.5rem; font-size: 0.85rem; }
            .prop span { color: var(--text-muted); }
            .prop strong { color: var(--text-primary); font-weight: 600; }

            .silicon-section .section-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.85rem; }
            .silicon-section .section-header h2 { margin: 0; font-size: 1rem; color: var(--text-primary); }
            .qor-table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
            .qor-table th, .qor-table td { padding: 0.55rem 0.6rem; border-bottom: 1px solid var(--border-color, var(--border)); text-align: left; }
            .qor-table th { color: var(--text-muted); font-weight: 600; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.06em; }
            .qor-table tr:last-child td { border-bottom: 0; }

            @media (max-width: 1024px) { .qubit-layout { grid-template-columns: 1fr; } }
        "#}</style>
    }
}
