use leptos::prelude::*;
use leptos_router::components::A;
use gloo_storage::Storage as _;

#[derive(Clone, Debug, PartialEq)]
struct QJob {
    id: String,
    circuit: String,
    status: String,
    backend: String,
    shots: u32,
}

fn load_jobs() -> Vec<QJob> {
    gloo_storage::LocalStorage::get::<Vec<serde_json::Value>>("ef_quantum_jobs")
        .ok()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            Some(QJob {
                id:      v.get("id")?.as_str()?.to_string(),
                circuit: v.get("circuit").and_then(|x| x.as_str()).unwrap_or("Custom").to_string(),
                status:  v.get("status").and_then(|x| x.as_str()).unwrap_or("Completed").to_string(),
                backend: v.get("backend").and_then(|x| x.as_str()).unwrap_or("simulator").to_string(),
                shots:   v.get("shots").and_then(|x| x.as_u64()).unwrap_or(1024) as u32,
            })
        })
        .collect()
}

#[component]
pub fn QuantumJobsPage() -> impl IntoView {
    let (filter, set_filter) = signal("All".to_string());
    let jobs: RwSignal<Vec<QJob>> = RwSignal::new(load_jobs());

    let filtered = Signal::derive(move || {
        let f = filter.get();
        jobs.get().into_iter().filter(move |j| {
            f == "All" || j.status.to_lowercase() == f.to_lowercase()
        }).collect::<Vec<_>>()
    });

    let filters = ["All", "Running", "Completed", "Failed", "Queued"];

    view! {
        <div class="page-container quantum-jobs-page">
            <div class="page-header">
                <div>
                    <h1>"Quantum Jobs"</h1>
                    <p>"Review active executions, queue status, and completed runs."</p>
                </div>
                <a href="/quantum/designer" class="primary-btn">"New Job"</a>
            </div>

            <div class="jobs-shell">
                <div class="jobs-toolbar">
                    {filters.iter().map(|f| {
                        let label = *f;
                        view! {
                            <button
                                class=move || if filter.get() == label { "filter-btn active" } else { "filter-btn" }
                                on:click=move |_| set_filter.set(label.to_string())>
                                {label}
                            </button>
                        }
                    }).collect::<Vec<_>>()}
                    <button class="filter-btn refresh-btn"
                        on:click=move |_| jobs.set(load_jobs())>
                        "Refresh"
                    </button>
                    <span class="job-count">
                        {move || {
                            let n = filtered.get().len();
                            format!("{n} job{}", if n == 1 { "" } else { "s" })
                        }}
                    </span>
                </div>

                {move || if filtered.get().is_empty() {
                    view! {
                        <div class="empty-jobs">
                            <p>"No jobs found."</p>
                            <p>"Run a circuit in the "
                                <a href="/quantum/designer">"Circuit Designer"</a>
                                " to submit your first job."
                            </p>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="jobs-table">
                            <table>
                                <thead>
                                    <tr>
                                        <th>"Job ID"</th>
                                        <th>"Circuit"</th>
                                        <th>"Status"</th>
                                        <th>"Backend"</th>
                                        <th>"Shots"</th>
                                        <th>"Result"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || filtered.get().into_iter().map(|job| {
                                        let job_id = job.id.clone();
                                        view! {
                                            <tr>
                                                <td><code>{job.id}</code></td>
                                                <td>{job.circuit}</td>
                                                <td><StatusBadge status=job.status/></td>
                                                <td><span class="backend-tag">{job.backend}</span></td>
                                                <td>{job.shots}</td>
                                                <td>
                                                    <A href=format!("/quantum/results/{job_id}")>"Open"</A>
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </div>
        </div>

        <style>
            {r#"
                .quantum-jobs-page {
                    padding: 2rem;
                    display: flex;
                    flex-direction: column;
                    gap: 1.5rem;
                }

                .page-header {
                    display: flex;
                    justify-content: space-between;
                    gap: 1rem;
                    align-items: flex-end;
                }

                .page-header h1 { margin: 0; color: var(--text-primary); }
                .page-header p { margin: 0.35rem 0 0; color: var(--text-secondary); }

                .jobs-shell {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1rem;
                }

                .jobs-toolbar {
                    display: flex;
                    align-items: center;
                    gap: 0.5rem;
                    flex-wrap: wrap;
                    margin-bottom: 1rem;
                }

                .filter-btn {
                    padding: 0.4rem 0.9rem;
                    border: 1px solid var(--border-color);
                    border-radius: 999px;
                    background: var(--bg-secondary);
                    color: var(--text-secondary);
                    cursor: pointer;
                    font-size: 0.85rem;
                }

                .filter-btn.active {
                    background: var(--accent-color);
                    color: white;
                    border-color: var(--accent-color);
                }

                .refresh-btn { margin-left: 0.5rem; }

                .job-count {
                    margin-left: auto;
                    color: var(--text-secondary);
                    font-size: 0.85rem;
                }

                .jobs-table table {
                    width: 100%;
                    border-collapse: collapse;
                }

                .jobs-table th,
                .jobs-table td {
                    padding: 0.85rem 0.75rem;
                    text-align: left;
                    border-bottom: 1px solid var(--border-color);
                    font-size: 0.9rem;
                }

                .jobs-table th { color: var(--text-secondary); font-weight: 600; }
                .jobs-table a { color: var(--accent-color); text-decoration: none; }

                .jobs-table code,
                .backend-tag {
                    background: var(--bg-secondary);
                    border-radius: 999px;
                    padding: 0.2rem 0.6rem;
                    font-size: 0.82rem;
                }

                .status-badge {
                    display: inline-block;
                    border-radius: 999px;
                    padding: 0.2rem 0.6rem;
                    color: white;
                    font-size: 0.78rem;
                    font-weight: 600;
                }

                .primary-btn {
                    background: var(--accent-color);
                    color: white;
                    border: none;
                    border-radius: 8px;
                    cursor: pointer;
                    padding: 0.65rem 1rem;
                    text-decoration: none;
                    font-size: 0.9rem;
                }

                .empty-jobs {
                    padding: 3rem 2rem;
                    text-align: center;
                    color: var(--text-secondary);
                }

                .empty-jobs a { color: var(--accent-color); }
            "#}
        </style>
    }
}

#[component]
fn StatusBadge(status: String) -> impl IntoView {
    let color = match status.as_str() {
        "Completed" | "completed" => "hsl(140, 60%, 45%)",
        "Running"   | "running"   => "hsl(38, 90%, 52%)",
        "Queued"    | "queued"    => "hsl(210, 70%, 50%)",
        "Failed"    | "failed"    => "hsl(0, 70%, 50%)",
        _ => "hsl(0, 0%, 45%)",
    };
    view! {
        <span class="status-badge" style:background=color>{status}</span>
    }
}
