use leptos::prelude::*;
use gloo_storage::Storage as _;
use serde_json::json;

use crate::api::{self, QSimulateRequest, QSimulateResponse};

const DEFAULT_QASM: &str = "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\ncx q[0],q[1];\nmeasure q -> c;";

#[component]
pub fn QuantumDesignerPage() -> impl IntoView {
    let (mode, set_mode) = signal("code".to_string());
    let (qubits, set_qubits) = signal("2".to_string());
    let (qasm, set_qasm) = signal(DEFAULT_QASM.to_string());
    let (selected_program, set_selected_program) = signal("custom".to_string());
    let (agent_id, set_agent_id) = signal(String::new());

    // Fetch agents list to populate agent picker
    let agents = LocalResource::new(
        || async move { api::list_agents().await },
    );

    // Run action — submits circuit to the selected agent's simulator
    let run_result: RwSignal<Option<Result<QSimulateResponse, String>>> = RwSignal::new(None);
    let is_running = RwSignal::new(false);

    let run_circuit = move |_| {
        let current_qasm = qasm.get();
        let current_agent = agent_id.get();
        let current_program = selected_program.get();

        if current_agent.is_empty() {
            run_result.set(Some(Err("Select an agent/target first.".to_string())));
            return;
        }

        let (prog, qasm_src) = if current_program == "custom" {
            (None, Some(current_qasm))
        } else {
            (Some(current_program.clone()), None)
        };

        is_running.set(true);
        run_result.set(None);

        let req = QSimulateRequest {
            program: prog,
            qasm: qasm_src,
            shots: 1024,
        };

        let prog_label = current_program.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api::quantum_simulate(&current_agent, &req).await;

            // Persist result and append job entry to localStorage
            if let Ok(ref resp) = result {
                let job_id = format!("job-{}", js_sys::Date::now() as u64);
                let result_blob = json!({
                    "backend": resp.backend,
                    "shots": resp.shots,
                    "histogram": resp.histogram.iter().map(|b| json!({
                        "state": b.state,
                        "count": b.count,
                        "probability": b.probability,
                    })).collect::<Vec<_>>(),
                });
                let _ = gloo_storage::LocalStorage::set("ef_quantum_last_result", &result_blob);

                // Append to job log
                let mut log: Vec<serde_json::Value> =
                    gloo_storage::LocalStorage::get("ef_quantum_jobs").unwrap_or_default();
                log.push(json!({
                    "id": job_id,
                    "circuit": if prog_label == "custom" { "Custom QASM" } else { prog_label.as_str() },
                    "status": "Completed",
                    "backend": resp.backend.as_deref().unwrap_or("simulator"),
                    "shots": resp.shots.unwrap_or(1024),
                }));
                let _ = gloo_storage::LocalStorage::set("ef_quantum_jobs", &log);
            }

            run_result.set(Some(result));
            is_running.set(false);
        });
    };

    let visual_rows = move || {
        qubits.get().parse::<usize>().ok()
            .map(|count| {
                (0..count).map(|i| view! {
                    <div class="wire-row">
                        <span class="wire-label">{format!("q{i}")}</span>
                        <div class="wire-line"></div>
                        <span class="gate-box">"H"</span>
                        <span class="gate-box">"CX"</span>
                        <span class="gate-box">"M"</span>
                    </div>
                }).collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    // Update QASM when template program is chosen
    let on_program_change = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        let tmpl = match val.as_str() {
            "bell"  => "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\ncx q[0],q[1];\nmeasure q -> c;",
            "ghz3"  => "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nh q[0];\ncx q[0],q[1];\ncx q[1],q[2];\nmeasure q -> c;",
            "qft3"  => "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nh q[0];\ncp(pi/2) q[1],q[0];\ncp(pi/4) q[2],q[0];\nh q[1];\ncp(pi/2) q[2],q[1];\nh q[2];\nmeasure q -> c;",
            _       => DEFAULT_QASM,
        };
        set_selected_program.set(val);
        if tmpl != DEFAULT_QASM || selected_program.get() != "custom" {
            set_qasm.set(tmpl.to_string());
        }
    };

    view! {
        <div class="page-container quantum-designer-page">
            <div class="page-header">
                <div>
                    <h1>"Quantum Circuit Designer"</h1>
                    <p>"Author circuits visually or in OpenQASM and run them on a connected agent."</p>
                </div>
                <div class="mode-switch">
                    <button class:active=move || mode.get() == "visual"
                        on:click=move |_| set_mode.set("visual".to_string())>"Visual"</button>
                    <button class:active=move || mode.get() == "code"
                        on:click=move |_| set_mode.set("code".to_string())>"Code"</button>
                </div>
            </div>

            <div class="designer-layout">
                // ── control panel ───────────────────────────────────────────
                <section class="designer-panel control-panel">
                    <h2>"Circuit Setup"</h2>

                    <label class="field-row">
                        <span>"Template"</span>
                        <select on:change=on_program_change>
                            <option value="custom">"Custom QASM"</option>
                            <option value="bell">"Bell State"</option>
                            <option value="ghz3">"GHZ-3"</option>
                            <option value="qft3">"QFT-3"</option>
                        </select>
                    </label>

                    <label class="field-row">
                        <span>"Qubit Count"</span>
                        <input type="number" min="1" max="32" prop:value=qubits
                            on:input=move |ev| set_qubits.set(event_target_value(&ev))/>
                    </label>

                    <div class="gate-palette">
                        <span>"Gate Palette"</span>
                        <div class="gate-list">
                            {["H","X","Y","Z","S","T","CX","CCX","RZ","SWAP","M"].iter()
                                .map(|g| view! { <span class="gate-chip">{*g}</span> })
                                .collect::<Vec<_>>()}
                        </div>
                    </div>

                    <hr/>

                    <h2>"Run"</h2>

                    // Agent picker
                    <label class="field-row">
                        <span>"Target Agent"</span>
                        <Suspense fallback=move || view! { <select disabled><option>"Loading..."</option></select> }>
                            {move || {
                                agents.get().map(|res| {
                                    let list = (*res).clone().unwrap_or_default().agents;
                                    view! {
                                        <select on:change=move |ev| set_agent_id.set(event_target_value(&ev))>
                                            <option value="">"-- select agent --"</option>
                                            {list.into_iter().map(|a| {
                                                let label = a.hostname.or(a.name).unwrap_or_else(|| a.id.clone());
                                                let id = a.id.clone();
                                                view! { <option value=id>{label}</option> }
                                            }).collect::<Vec<_>>()}
                                        </select>
                                    }
                                })
                            }}
                        </Suspense>
                    </label>

                    <button class="run-btn"
                        disabled=move || is_running.get()
                        on:click=run_circuit>
                        {move || if is_running.get() { "Running..." } else { "Run Simulation" }}
                    </button>
                </section>

                // ── circuit editor ──────────────────────────────────────────
                <section class="designer-panel editor-panel">
                    <Show when=move || mode.get() == "visual">
                        <h2>"Visual Circuit"</h2>
                        <div class="circuit-canvas">
                            {visual_rows}
                        </div>
                    </Show>
                    <Show when=move || mode.get() == "code">
                        <h2>"OpenQASM Source"</h2>
                        <textarea class="qasm-editor" rows="18"
                            prop:value=qasm
                            on:input=move |ev| {
                                set_qasm.set(event_target_value(&ev));
                                set_selected_program.set("custom".to_string());
                            }>
                        </textarea>
                    </Show>
                </section>

                // ── results panel ───────────────────────────────────────────
                <section class="designer-panel results-panel-inline">
                    <h2>"Results"</h2>
                    {move || match run_result.get() {
                        None => view! {
                            <p class="hint">"Run a circuit to see measurement results here."</p>
                        }.into_any(),
                        Some(Err(e)) => view! {
                            <p class="error-msg">"Error: " {e}</p>
                        }.into_any(),
                        Some(Ok(resp)) => {
                            let shots = resp.shots.unwrap_or(1024);
                            let backend = resp.backend.clone().unwrap_or_else(|| "simulator".to_string());
                            view! {
                                <p class="run-meta">"Backend: " <strong>{backend}</strong> " | Shots: " <strong>{shots}</strong></p>
                                <div class="histogram-list">
                                    {resp.histogram.into_iter().map(|bin| {
                                        let width = (bin.probability * 100.0).min(100.0);
                                        view! {
                                            <div class="histogram-row">
                                                <code class="bitstring">{bin.state}</code>
                                                <div class="bar-track">
                                                    <div class="bar-fill" style:width=format!("{width:.1}%")></div>
                                                </div>
                                                <span class="bar-count">{format!("{} ({:.1}%)", bin.count, bin.probability * 100.0)}</span>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }
                    }}
                </section>
            </div>
        </div>

        <style>
            {r#"
                .quantum-designer-page {
                    padding: 2rem;
                    display: flex;
                    flex-direction: column;
                    gap: 1.5rem;
                }

                .page-header {
                    display: flex;
                    align-items: flex-end;
                    justify-content: space-between;
                    gap: 1rem;
                }

                .page-header h1 { margin: 0; color: var(--text-primary); }
                .page-header p { margin: 0.35rem 0 0; color: var(--text-secondary); }

                .mode-switch {
                    display: flex;
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    overflow: hidden;
                }

                .mode-switch button {
                    padding: 0.5rem 1rem;
                    border: none;
                    background: var(--bg-secondary);
                    color: var(--text-secondary);
                    cursor: pointer;
                    font-size: 0.9rem;
                }

                .mode-switch button.active {
                    background: var(--accent-color);
                    color: white;
                }

                .designer-layout {
                    display: grid;
                    grid-template-columns: 260px 1fr 300px;
                    gap: 1rem;
                    align-items: start;
                }

                @media (max-width: 1100px) {
                    .designer-layout { grid-template-columns: 1fr; }
                }

                .designer-panel {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1.25rem;
                    display: flex;
                    flex-direction: column;
                    gap: 0.9rem;
                }

                .designer-panel h2 {
                    margin: 0;
                    font-size: 0.95rem;
                    font-weight: 600;
                    color: var(--text-primary);
                }

                .field-row {
                    display: flex;
                    flex-direction: column;
                    gap: 0.35rem;
                    font-size: 0.85rem;
                    color: var(--text-secondary);
                }

                .field-row input,
                .field-row select {
                    padding: 0.55rem 0.7rem;
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    background: var(--bg-secondary);
                    color: var(--text-primary);
                    font-size: 0.9rem;
                }

                .gate-palette {
                    display: flex;
                    flex-direction: column;
                    gap: 0.5rem;
                    font-size: 0.85rem;
                    color: var(--text-secondary);
                }

                .gate-list {
                    display: flex;
                    flex-wrap: wrap;
                    gap: 0.4rem;
                }

                .gate-chip {
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    padding: 0.3rem 0.6rem;
                    font-size: 0.8rem;
                    font-family: monospace;
                    cursor: grab;
                    color: var(--text-primary);
                }

                hr { border: none; border-top: 1px solid var(--border-color); }

                .run-btn {
                    background: hsl(140, 60%, 38%);
                    color: white;
                    border: none;
                    border-radius: 8px;
                    padding: 0.75rem;
                    font-size: 0.95rem;
                    cursor: pointer;
                    font-weight: 600;
                    width: 100%;
                }

                .run-btn:disabled {
                    background: var(--bg-secondary);
                    color: var(--text-secondary);
                    cursor: not-allowed;
                }

                .circuit-canvas {
                    display: flex;
                    flex-direction: column;
                    gap: 0.75rem;
                    padding: 1rem;
                    background: var(--bg-secondary);
                    border-radius: 8px;
                    overflow-x: auto;
                }

                .wire-row {
                    display: flex;
                    align-items: center;
                    gap: 0.75rem;
                }

                .wire-label {
                    font-family: monospace;
                    font-size: 0.85rem;
                    color: var(--text-secondary);
                    width: 28px;
                    flex-shrink: 0;
                }

                .wire-line {
                    flex: 1;
                    height: 2px;
                    background: var(--border-color);
                }

                .gate-box {
                    background: var(--accent-color);
                    color: white;
                    border-radius: 6px;
                    padding: 0.3rem 0.55rem;
                    font-size: 0.8rem;
                    font-family: monospace;
                    font-weight: 600;
                }

                .qasm-editor {
                    width: 100%;
                    font-family: monospace;
                    font-size: 0.85rem;
                    background: hsl(220, 20%, 8%);
                    color: hsl(0, 0%, 88%);
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    padding: 1rem;
                    resize: vertical;
                    box-sizing: border-box;
                    line-height: 1.6;
                }

                .histogram-list {
                    display: flex;
                    flex-direction: column;
                    gap: 0.6rem;
                }

                .histogram-row {
                    display: flex;
                    align-items: center;
                    gap: 0.75rem;
                    font-size: 0.85rem;
                }

                .bitstring {
                    font-family: monospace;
                    background: var(--bg-secondary);
                    border-radius: 4px;
                    padding: 0.15rem 0.4rem;
                    min-width: 36px;
                    text-align: center;
                    color: var(--text-primary);
                }

                .bar-track {
                    flex: 1;
                    background: var(--bg-secondary);
                    border-radius: 999px;
                    height: 12px;
                    overflow: hidden;
                }

                .bar-fill {
                    height: 100%;
                    background: var(--accent-color);
                    border-radius: 999px;
                    transition: width 0.4s ease;
                }

                .bar-count { color: var(--text-secondary); min-width: 90px; text-align: right; }

                .hint, .run-meta { color: var(--text-secondary); font-size: 0.9rem; }
                .error-msg { color: hsl(0, 70%, 55%); font-size: 0.9rem; }
            "#}
        </style>
    }
}
