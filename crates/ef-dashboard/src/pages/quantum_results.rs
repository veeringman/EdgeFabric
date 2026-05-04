use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

/// Quantum job results page.
/// Route: /quantum/results/:id
///
/// The `:id` param can be a real job UUID (future: fetch from API) or
/// "latest" to show the most recently run result stored in the page session.
/// For now we read results from localStorage (written by the designer on run).
#[component]
pub fn QuantumResultsPage() -> impl IntoView {
    let params = use_params_map();
    let job_id = move || {
        params.get().get("id").cloned().unwrap_or_else(|| "latest".to_string())
    };

    // Load result from localStorage (key: "ef_quantum_last_result")
    let stored_result = move || {
        gloo_storage::LocalStorage::get::<serde_json::Value>("ef_quantum_last_result").ok()
    };

    // Derive display data reactively from job_id + localStorage
    let result_data = Signal::derive(move || {
        let id = job_id();
        stored_result().map(|v| (id, v))
    });

    view! {
        <div class="page-container quantum-results-page">
            <div class="page-header">
                <div>
                    <h1>"Quantum Results"</h1>
                    <p>"Measurement histogram and raw output for job " <code>{move || job_id()}</code></p>
                </div>
                <div class="header-actions">
                    <button class="secondary-btn"
                        on:click=move |_| {
                            // Export JSON — copy raw result to clipboard
                            if let Some(v) = stored_result() {
                                let text = serde_json::to_string_pretty(&v).unwrap_or_default();
                                if let Some(window) = web_sys::window() {
                                    let _ = window.navigator().clipboard().write_text(&text);
                                }
                            }
                        }>
                        "Copy JSON"
                    </button>
                </div>
            </div>

            {move || match result_data.get() {
                None => view! {
                    <div class="empty-state">
                        <p>"No result available for this job."</p>
                        <p>"Run a circuit in the "
                            <a href="/quantum/designer">"Circuit Designer"</a>
                            " to see results here."
                        </p>
                    </div>
                }.into_any(),
                Some((_id, data)) => {
                    // Parse histogram from stored result JSON
                    let backend = data.get("backend").and_then(|v| v.as_str())
                        .unwrap_or("simulator").to_string();
                    let shots = data.get("shots").and_then(|v| v.as_u64()).unwrap_or(1024);

                    let bins: Vec<(String, u64, f64)> = data
                        .get("histogram")
                        .and_then(|h| h.as_array())
                        .map(|arr| {
                            arr.iter().filter_map(|b| {
                                let state = b.get("state").and_then(|v| v.as_str())?.to_string();
                                let count = b.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                                let prob  = b.get("probability").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                Some((state, count, prob))
                            }).collect()
                        })
                        .unwrap_or_default();

                    let raw = serde_json::to_string_pretty(&data).unwrap_or_default();

                    view! {
                        <div class="results-body">
                            <div class="summary-grid">
                                <SummaryCard label="Backend" value=backend/>
                                <SummaryCard label="Shots" value=shots.to_string()/>
                                <SummaryCard label="States Observed" value=bins.len().to_string()/>
                            </div>

                            <section class="results-panel">
                                <h2>"Measurement Distribution"</h2>
                                {if bins.is_empty() {
                                    view! { <p class="hint">"No histogram data."</p> }.into_any()
                                } else {
                                    view! {
                                        <div class="histogram-list">
                                            {bins.into_iter().map(|(state, count, prob)| {
                                                let width = (prob * 100.0).min(100.0);
                                                view! {
                                                    <div class="histogram-row">
                                                        <code class="bitstring">{state}</code>
                                                        <div class="bar-track">
                                                            <div class="bar-fill" style:width=format!("{width:.1}%")></div>
                                                        </div>
                                                        <span class="bar-count">{format!("{count} ({:.1}%)", prob * 100.0)}</span>
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }}
                            </section>

                            <section class="results-panel">
                                <h2>"Raw Result"</h2>
                                <pre class="result-code"><code>{raw}</code></pre>
                            </section>
                        </div>
                    }.into_any()
                }
            }}
        </div>

        <style>
            {r#"
                .quantum-results-page {
                    padding: 2rem;
                    display: flex;
                    flex-direction: column;
                    gap: 1.5rem;
                }

                .page-header {
                    display: flex;
                    align-items: flex-start;
                    justify-content: space-between;
                    gap: 1rem;
                }

                .page-header h1 { margin: 0; color: var(--text-primary); }
                .page-header p { margin: 0.35rem 0 0; color: var(--text-secondary); font-size: 0.9rem; }

                .header-actions { display: flex; gap: 0.75rem; }

                .results-body {
                    display: flex;
                    flex-direction: column;
                    gap: 1.25rem;
                }

                .summary-grid {
                    display: grid;
                    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
                    gap: 1rem;
                }

                .summary-card {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1rem 1.25rem;
                }

                .summary-card .label {
                    font-size: 0.8rem;
                    color: var(--text-secondary);
                    text-transform: uppercase;
                    letter-spacing: 0.07em;
                }

                .summary-card .value {
                    font-size: 1.5rem;
                    font-weight: 700;
                    color: var(--text-primary);
                    margin-top: 0.25rem;
                }

                .results-panel {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1.5rem;
                }

                .results-panel h2 {
                    margin: 0 0 1rem;
                    color: var(--text-primary);
                    font-size: 1rem;
                }

                .histogram-list {
                    display: flex;
                    flex-direction: column;
                    gap: 0.65rem;
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
                    min-width: 40px;
                    text-align: center;
                    color: var(--text-primary);
                }

                .bar-track {
                    flex: 1;
                    background: var(--bg-secondary);
                    border-radius: 999px;
                    height: 14px;
                    overflow: hidden;
                }

                .bar-fill {
                    height: 100%;
                    background: var(--accent-color);
                    border-radius: 999px;
                    transition: width 0.4s ease;
                }

                .bar-count {
                    color: var(--text-secondary);
                    min-width: 100px;
                    text-align: right;
                }

                .result-code {
                    background: hsl(220, 20%, 8%);
                    color: hsl(0, 0%, 85%);
                    border-radius: 8px;
                    padding: 1rem;
                    overflow-x: auto;
                    font-size: 0.82rem;
                    line-height: 1.6;
                }

                .empty-state {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 3rem 2rem;
                    text-align: center;
                    color: var(--text-secondary);
                }

                .empty-state a { color: var(--accent-color); }

                .secondary-btn {
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    color: var(--text-primary);
                    cursor: pointer;
                    padding: 0.6rem 0.9rem;
                    font-size: 0.9rem;
                }

                .hint { color: var(--text-secondary); font-size: 0.9rem; }
            "#}
        </style>
    }
}

#[component]
fn SummaryCard(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="summary-card">
            <div class="label">{label}</div>
            <div class="value">{value}</div>
        </div>
    }
}
