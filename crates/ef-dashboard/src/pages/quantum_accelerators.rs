use leptos::prelude::*;

use crate::api::{self, QBackend};

#[component]
pub fn QuantumAcceleratorsPage() -> impl IntoView {
    // Fetch catalog backends from real API
    let backends = LocalResource::new(
        || async move { api::fetch_quantum_backends().await },
    );

    // Local custom backends added by user (stored client-side for now)
    let (custom, set_custom) = signal(Vec::<CustomBackend>::new());
    let (show_form, set_show_form) = signal(false);
    let (name, set_name) = signal(String::new());
    let (backend_type, set_backend_type) = signal("simulator".to_string());
    let (endpoint, set_endpoint) = signal(String::new());
    let (max_qubits, set_max_qubits) = signal("12".to_string());

    let add_custom = move |_| {
        let parsed = max_qubits.get().parse::<u32>().unwrap_or(12);
        set_custom.update(|v| v.push(CustomBackend {
            id: format!("custom-{}", v.len() + 1),
            name: if name.get().is_empty() { "Custom Backend".to_string() } else { name.get() },
            backend_type: backend_type.get(),
            endpoint: endpoint.get(),
            max_qubits: parsed,
        }));
        set_show_form.set(false);
        set_name.set(String::new());
        set_endpoint.set(String::new());
        set_max_qubits.set("12".to_string());
    };

    view! {
        <div class="page-container quantum-accelerators-page">
            <div class="page-header">
                <div>
                    <h1>"Quantum Accelerators"</h1>
                    <p>"Simulators and cloud-connected quantum hardware available to this platform."</p>
                </div>
                <button class="primary-btn"
                    on:click=move |_| set_show_form.set(!show_form.get())>
                    {move || if show_form.get() { "Close" } else { "Add Custom" }}
                </button>
            </div>

            <Show when=move || show_form.get()>
                <div class="form-panel">
                    <h3>"Register Custom Backend"</h3>
                    <div class="form-grid">
                        <label>
                            <span>"Display Name"</span>
                            <input type="text" prop:value=name
                                on:input=move |ev| set_name.set(event_target_value(&ev))/>
                        </label>
                        <label>
                            <span>"Backend Type"</span>
                            <select on:change=move |ev| set_backend_type.set(event_target_value(&ev))>
                                <option value="simulator">"Simulator"</option>
                                <option value="ibm">"IBM Quantum"</option>
                                <option value="braket">"AWS Braket"</option>
                                <option value="azure">"Azure Quantum"</option>
                                <option value="ionq">"IonQ"</option>
                            </select>
                        </label>
                        <label>
                            <span>"API Endpoint (optional)"</span>
                            <input type="text" placeholder="https://..." prop:value=endpoint
                                on:input=move |ev| set_endpoint.set(event_target_value(&ev))/>
                        </label>
                        <label>
                            <span>"Max Qubits"</span>
                            <input type="number" min="1" max="1000" prop:value=max_qubits
                                on:input=move |ev| set_max_qubits.set(event_target_value(&ev))/>
                        </label>
                    </div>
                    <div class="form-actions">
                        <button class="secondary-btn" on:click=move |_| set_show_form.set(false)>"Cancel"</button>
                        <button class="primary-btn" on:click=add_custom>"Save"</button>
                    </div>
                </div>
            </Show>

            // Platform backends from API
            <section class="acc-section">
                <h2>"Platform Backends"</h2>
                <Suspense fallback=move || view! { <p class="loading">"Loading backends..."</p> }>
                    {move || {
                        backends.get().map(|res| match (*res).clone() {
                            Err(e) => view! {
                                <p class="error-msg">"Error: " {e}</p>
                            }.into_any(),
                            Ok(data) => view! {
                                <div class="accelerator-grid">
                                    {data.backends.into_iter().map(|b| view! {
                                        <CatalogCard backend=b/>
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any(),
                        })
                    }}
                </Suspense>
            </section>

            // User-added custom backends
            <Show when=move || !custom.get().is_empty()>
                <section class="acc-section">
                    <h2>"Custom Backends"</h2>
                    <div class="accelerator-grid">
                        {move || custom.get().into_iter().map(|b| {
                            let del_id = b.id.clone();
                            view! {
                                <div class="accelerator-card custom-card">
                                    <div class="accelerator-card-header">
                                        <div>
                                            <h3>{b.name.clone()}</h3>
                                            <p>{format!("{} backend", b.backend_type)}</p>
                                        </div>
                                        <StatusPill status="custom".to_string()/>
                                    </div>
                                    <div class="accelerator-meta">
                                        <div>
                                            <span>"Max Qubits"</span>
                                            <strong>{b.max_qubits}</strong>
                                        </div>
                                        {if b.endpoint.is_empty() { None } else { Some(view! {
                                            <div>
                                                <span>"Endpoint"</span>
                                                <strong class="mono">{b.endpoint.clone()}</strong>
                                            </div>
                                        })}}
                                    </div>
                                    <div class="card-actions">
                                        <button class="danger-btn" on:click=move |_| {
                                            set_custom.update(|v| v.retain(|x| x.id != del_id));
                                        }>"Remove"</button>
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </section>
            </Show>
        </div>

        <style>
            {r#"
                .quantum-accelerators-page {
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

                .page-header h1,
                .accelerator-card h3,
                .acc-section h2 {
                    margin: 0;
                    color: var(--text-primary);
                }

                .acc-section h2 {
                    font-size: 1rem;
                    font-weight: 600;
                    margin-bottom: 1rem;
                }

                .page-header p,
                .accelerator-card p,
                .accelerator-meta span {
                    margin: 0.35rem 0 0;
                    color: var(--text-secondary);
                    font-size: 0.9rem;
                }

                .form-panel,
                .accelerator-card {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1.25rem;
                }

                .form-panel h3 {
                    margin: 0 0 1rem;
                    color: var(--text-primary);
                }

                .form-grid,
                .accelerator-grid {
                    display: grid;
                    gap: 1rem;
                    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
                }

                .form-grid label {
                    display: flex;
                    flex-direction: column;
                    gap: 0.4rem;
                    color: var(--text-primary);
                    font-size: 0.9rem;
                }

                .form-grid input,
                .form-grid select {
                    padding: 0.7rem 0.8rem;
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    background: var(--bg-secondary);
                    color: var(--text-primary);
                }

                .form-actions,
                .card-actions,
                .accelerator-card-header {
                    display: flex;
                    justify-content: space-between;
                    gap: 0.75rem;
                    align-items: center;
                }

                .form-actions {
                    margin-top: 1rem;
                    justify-content: flex-end;
                }

                .accelerator-meta {
                    display: grid;
                    grid-template-columns: repeat(2, minmax(0, 1fr));
                    gap: 0.75rem;
                    margin: 1rem 0;
                }

                .primary-btn, .secondary-btn, .danger-btn {
                    border: none;
                    border-radius: 8px;
                    cursor: pointer;
                    padding: 0.7rem 1rem;
                    font-size: 0.9rem;
                }

                .primary-btn { background: var(--accent-color); color: white; }
                .secondary-btn { background: var(--bg-secondary); color: var(--text-primary); border: 1px solid var(--border-color); }
                .danger-btn { background: hsl(0, 70%, 50%); color: white; }

                .status-pill {
                    border-radius: 999px;
                    color: white;
                    font-size: 0.75rem;
                    font-weight: 600;
                    padding: 0.2rem 0.6rem;
                    white-space: nowrap;
                }

                .qv-badge {
                    font-size: 0.75rem;
                    border-radius: 6px;
                    padding: 0.15rem 0.4rem;
                    background: hsla(210, 80%, 50%, 0.15);
                    color: hsl(210, 80%, 60%);
                    border: 1px solid hsla(210, 80%, 50%, 0.3);
                }

                .acc-available { border-left: 3px solid hsl(140, 60%, 45%); }
                .acc-offline   { border-left: 3px solid hsl(0, 70%, 50%); }

                .mono { font-family: monospace; font-size: 0.8rem; }

                .loading { color: var(--text-secondary); }
                .error-msg { color: hsl(0, 70%, 55%); }
            "#}
        </style>
    }
}

// ─── local types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct CustomBackend {
    id: String,
    name: String,
    backend_type: String,
    endpoint: String,
    max_qubits: u32,
}

impl CustomBackend {
    fn endpoint(&self) -> Option<String> {
        if self.endpoint.is_empty() { None } else { Some(self.endpoint.clone()) }
    }
}

// ─── sub-components ──────────────────────────────────────────────────────────

#[component]
fn CatalogCard(backend: QBackend) -> impl IntoView {
    let pill_color = if backend.available {
        "hsl(140, 60%, 45%)"
    } else {
        "hsl(0, 70%, 50%)"
    };
    let status_text = if backend.available { "available" } else { "offline" };
    let side_class = if backend.available { "acc-available" } else { "acc-offline" };
    let qubits = backend.qubits.map(|q| q.to_string()).unwrap_or_else(|| "—".to_string());
    let qv_text = backend.quantum_volume.map(|v| format!("QV {v}"));
    let desc = backend.description.clone();

    view! {
        <div class=format!("accelerator-card {side_class}")>
            <div class="accelerator-card-header">
                <div>
                    <h3>{backend.name}</h3>
                    <p>{desc}</p>
                </div>
                <span class="status-pill" style:background=pill_color>{status_text}</span>
            </div>
            <div class="accelerator-meta">
                <div>
                    <span>"Provider"</span>
                    <strong>{backend.provider}</strong>
                </div>
                <div>
                    <span>"Type"</span>
                    <strong>{backend.backend_type}</strong>
                </div>
                <div>
                    <span>"Qubits"</span>
                    <strong>{qubits}</strong>
                </div>
                {qv_text.map(|qv| view! {
                    <div>
                        <span>"Quantum Volume"</span>
                        <span class="qv-badge">{qv}</span>
                    </div>
                })}
            </div>
        </div>
    }
}

#[component]
fn StatusPill(status: String) -> impl IntoView {
    let color = match status.as_str() {
        "available" => "hsl(140, 60%, 45%)",
        "busy" => "hsl(38, 90%, 52%)",
        "offline" => "hsl(0, 70%, 50%)",
        "custom" => "hsl(270, 60%, 55%)",
        _ => "hsl(0, 0%, 45%)",
    };
    view! {
        <span class="status-pill" style:background=color>{status}</span>
    }
}
