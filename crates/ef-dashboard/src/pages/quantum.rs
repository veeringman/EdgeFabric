use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{self, QBackend};
use crate::components::stat_card::StatCard;

#[component]
pub fn QuantumPage() -> impl IntoView {
    // Fetch backend catalog from real API
    let backends = Resource::new(
        || (),
        |_| async move { api::fetch_quantum_backends().await },
    );

    // Derived stats from backend data
    let total_backends = Signal::derive(move || {
        backends.get().and_then(|r| r.ok()).map(|r| r.backends.len()).unwrap_or(0).to_string()
    });
    let available_backends = Signal::derive(move || {
        backends.get().and_then(|r| r.ok())
            .map(|r| r.backends.iter().filter(|b| b.available).count())
            .unwrap_or(0)
            .to_string()
    });

    view! {
        <div class="page-container quantum-page">
            <div class="page-header">
                <h1>"Quantum Computing"</h1>
                <p class="page-subtitle">
                    "Manage accelerators, design circuits, and run jobs from one dashboard."
                </p>
            </div>

            <div class="stats-grid">
                <StatCard label="Backends" value=total_backends/>
                <StatCard label="Available" value=available_backends/>
                <StatCard label="Executions Today" value=Signal::derive(move || "—".to_string())/>
            </div>

            <section class="quantum-section">
                <div class="section-header">
                    <h2>"Quick Actions"</h2>
                </div>
                <div class="quick-actions">
                    <a href="/quantum/designer" class="action-card">
                        <span class="action-kicker">"Build"</span>
                        <h3>"Circuit Designer"</h3>
                        <p>"Switch between visual editing and OpenQASM code."</p>
                    </a>
                    <a href="/quantum/accelerators" class="action-card">
                        <span class="action-kicker">"Configure"</span>
                        <h3>"Accelerators"</h3>
                        <p>"Register simulators and cloud backends."</p>
                    </a>
                    <a href="/quantum/jobs" class="action-card">
                        <span class="action-kicker">"Execute"</span>
                        <h3>"Jobs"</h3>
                        <p>"Track submissions, queue depth, and status."</p>
                    </a>
                    <a href="/quantum/results/latest" class="action-card">
                        <span class="action-kicker">"Inspect"</span>
                        <h3>"Results"</h3>
                        <p>"Review histograms and measurement distributions."</p>
                    </a>
                </div>
            </section>

            <section class="quantum-section">
                <div class="section-header">
                    <h2>"Backend Health"</h2>
                    <A href="/quantum/accelerators">"Manage"</A>
                </div>
                <Suspense fallback=move || view! { <p class="loading">"Loading backends..."</p> }>
                    {move || {
                        backends.get().map(|res| match res {
                            Err(e) => view! {
                                <p class="error-msg">"Failed to load backends: " {e}</p>
                            }.into_any(),
                            Ok(data) => view! {
                                <div class="accelerators-grid">
                                    {data.backends.into_iter().map(|b| view! {
                                        <BackendCard backend=b/>
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any(),
                        })
                    }}
                </Suspense>
            </section>
        </div>

        <style>
            {r#"
                .quantum-page {
                    display: flex;
                    flex-direction: column;
                    gap: 1.5rem;
                    padding: 2rem;
                }

                .page-header h1,
                .section-header h2 {
                    margin: 0;
                    color: var(--text-primary);
                }

                .page-subtitle {
                    margin: 0.5rem 0 0;
                    color: var(--text-secondary);
                }

                .stats-grid,
                .quick-actions,
                .accelerators-grid {
                    display: grid;
                    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
                    gap: 1rem;
                }

                .quantum-section {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1.5rem;
                }

                .section-header {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    margin-bottom: 1rem;
                }

                .section-header a {
                    color: var(--accent-color);
                    text-decoration: none;
                }

                .action-card {
                    display: block;
                    background: linear-gradient(180deg, var(--bg-secondary), var(--card-bg));
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1.25rem;
                    text-decoration: none;
                    color: var(--text-primary);
                }

                .action-card h3 { margin: 0.4rem 0; }

                .action-card p,
                .action-kicker {
                    margin: 0;
                    color: var(--text-secondary);
                }

                .action-kicker {
                    font-size: 0.8rem;
                    text-transform: uppercase;
                    letter-spacing: 0.08em;
                }

                .accelerator-card {
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1rem;
                }

                .accelerator-card h3 {
                    margin: 0;
                    color: var(--text-primary);
                }

                .accelerator-top,
                .accelerator-row {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    gap: 1rem;
                }

                .accelerator-top { margin-bottom: 0.9rem; }

                .accelerator-row {
                    color: var(--text-secondary);
                    padding-top: 0.5rem;
                }

                .provider-tag {
                    font-size: 0.75rem;
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 999px;
                    padding: 0.15rem 0.55rem;
                    color: var(--text-secondary);
                }

                .status-dot {
                    width: 10px;
                    height: 10px;
                    border-radius: 50%;
                    display: inline-block;
                    margin-left: 0.5rem;
                }

                .loading, .error-msg {
                    color: var(--text-secondary);
                    padding: 1rem 0;
                }

                .error-msg { color: hsl(0, 70%, 55%); }
            "#}
        </style>
    }
}

#[component]
fn BackendCard(backend: QBackend) -> impl IntoView {
    let dot_color = if backend.available {
        "hsl(140, 60%, 45%)"
    } else {
        "hsl(0, 70%, 50%)"
    };
    let status_text = if backend.available { "available" } else { "offline" };
    let qubits = backend.qubits.map(|q| q.to_string()).unwrap_or_else(|| "—".to_string());
    let qv = backend.quantum_volume.map(|qv| format!("QV {qv}")).unwrap_or_else(|| "—".to_string());

    view! {
        <div class="accelerator-card">
            <div class="accelerator-top">
                <h3>{backend.name}</h3>
                <span>
                    {status_text}
                    <span class="status-dot" style:background=dot_color></span>
                </span>
            </div>
            <div class="accelerator-row">
                <span>"Provider"</span>
                <span class="provider-tag">{backend.provider}</span>
            </div>
            <div class="accelerator-row">
                <span>"Qubits"</span>
                <strong>{qubits}</strong>
            </div>
            <div class="accelerator-row">
                <span>"Quantum Volume"</span>
                <strong>{qv}</strong>
            </div>
        </div>
    }
}

