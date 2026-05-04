use leptos::prelude::*;

use crate::api::{self, DiscoveredHost};

#[component]
pub fn DiscoveredHostsPage() -> impl IntoView {
    let (quantum_filter, set_quantum_filter) = signal(false);

    let hosts = LocalResource::new(move || async move {
        api::fetch_discovered_hosts(quantum_filter.get()).await
    });

    view! {
        <div class="page-container discovered-hosts-page">
            <div class="page-header">
                <div>
                    <h1>"VeerOS Hosts"</h1>
                    <p>"VeerOS instances discovered via network advertisement."</p>
                </div>
                <div class="header-actions">
                    <label class="toggle-label">
                        <input type="checkbox"
                            prop:checked=quantum_filter
                            on:change=move |ev| set_quantum_filter.set(event_target_checked(&ev))/>
                        <span>"Quantum only"</span>
                    </label>
                    <button class="secondary-btn"
                        on:click=move |_| hosts.refetch()>
                        "Refresh"
                    </button>
                </div>
            </div>

            <Suspense fallback=move || view! { <div class="loading-state">"Scanning for hosts..."</div> }>
                {move || {
                    hosts.get().map(|res| match (*res).clone() {
                        Err(e) => view! {
                            <div class="error-state">
                                <p>"Failed to load hosts: " {e}</p>
                                <p class="hint">"Make sure ef-agent is running and can reach the network."</p>
                            </div>
                        }.into_any(),
                        Ok(data) if data.hosts.is_empty() => view! {
                            <div class="empty-state">
                                <div class="empty-icon">"📡"</div>
                                <h2>"No VeerOS hosts discovered"</h2>
                                <p>"VeerOS hosts advertise themselves via UDP multicast on your local network."</p>
                                <p>"Start a VeerOS VM with " <code>"veer advertise"</code> " or enable auto-advertisement in the VM config."</p>
                            </div>
                        }.into_any(),
                        Ok(data) => view! {
                            <div class="hosts-grid">
                                {data.hosts.into_iter().map(|host| view! {
                                    <HostCard host=host/>
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any(),
                    })
                }}
            </Suspense>
        </div>

        <style>
            {r#"
                .discovered-hosts-page {
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
                    flex-wrap: wrap;
                }

                .page-header h1 { margin: 0; color: var(--text-primary); }
                .page-header p  { margin: 0.35rem 0 0; color: var(--text-secondary); font-size: 0.9rem; }

                .header-actions {
                    display: flex;
                    align-items: center;
                    gap: 0.75rem;
                }

                .toggle-label {
                    display: flex;
                    align-items: center;
                    gap: 0.4rem;
                    font-size: 0.88rem;
                    color: var(--text-secondary);
                    cursor: pointer;
                }

                .hosts-grid {
                    display: grid;
                    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
                    gap: 1.25rem;
                }

                /* host card */
                .host-card {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 12px;
                    padding: 1.25rem;
                    display: flex;
                    flex-direction: column;
                    gap: 0.85rem;
                    transition: border-color 0.2s;
                }

                .host-card:hover { border-color: var(--accent-color); }

                .host-card-header {
                    display: flex;
                    align-items: flex-start;
                    justify-content: space-between;
                    gap: 0.5rem;
                }

                .host-name {
                    font-weight: 700;
                    font-size: 1.05rem;
                    color: var(--text-primary);
                    margin: 0;
                }

                .host-id {
                    font-size: 0.78rem;
                    color: var(--text-secondary);
                    font-family: monospace;
                    margin: 0.15rem 0 0;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    max-width: 200px;
                }

                .badge-row {
                    display: flex;
                    gap: 0.4rem;
                    flex-wrap: wrap;
                }

                .badge {
                    display: inline-flex;
                    align-items: center;
                    gap: 0.25rem;
                    padding: 0.2rem 0.55rem;
                    border-radius: 999px;
                    font-size: 0.75rem;
                    font-weight: 600;
                }

                .badge-qpu { background: hsl(45, 90%, 20%); color: hsl(45, 90%, 75%); border: 1px solid hsl(45, 70%, 35%); }
                .badge-sim { background: hsl(210, 70%, 15%); color: hsl(210, 90%, 75%); border: 1px solid hsl(210, 60%, 35%); }
                .badge-verified { background: hsl(140, 50%, 15%); color: hsl(140, 80%, 65%); border: 1px solid hsl(140, 50%, 30%); }
                .badge-unverified { background: hsl(0, 0%, 12%); color: hsl(0, 0%, 60%); border: 1px solid hsl(0, 0%, 25%); }

                .host-meta {
                    display: grid;
                    grid-template-columns: auto 1fr;
                    gap: 0.3rem 0.75rem;
                    font-size: 0.85rem;
                }

                .meta-label { color: var(--text-secondary); }
                .meta-value { color: var(--text-primary); font-family: monospace; }

                .cap-chips {
                    display: flex;
                    flex-wrap: wrap;
                    gap: 0.35rem;
                }

                .cap-chip {
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 999px;
                    padding: 0.15rem 0.55rem;
                    font-size: 0.75rem;
                    color: var(--text-secondary);
                }

                .host-actions {
                    display: flex;
                    gap: 0.5rem;
                    margin-top: 0.25rem;
                }

                .action-btn {
                    flex: 1;
                    padding: 0.5rem;
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    background: var(--bg-secondary);
                    color: var(--text-primary);
                    cursor: pointer;
                    font-size: 0.83rem;
                    text-align: center;
                    text-decoration: none;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    gap: 0.3rem;
                }

                .action-btn:hover { border-color: var(--accent-color); color: var(--accent-color); }
                .action-btn.primary { background: var(--accent-color); border-color: var(--accent-color); color: white; }
                .action-btn.primary:hover { opacity: 0.9; }

                .last-seen {
                    font-size: 0.78rem;
                    color: var(--text-secondary);
                    text-align: right;
                }

                .loading-state, .empty-state, .error-state {
                    padding: 3rem 2rem;
                    text-align: center;
                    color: var(--text-secondary);
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 12px;
                }

                .empty-icon { font-size: 2.5rem; margin-bottom: 0.75rem; }
                .empty-state h2 { color: var(--text-primary); margin: 0 0 0.5rem; }
                .empty-state code { background: var(--bg-secondary); padding: 0.1rem 0.4rem; border-radius: 4px; }
                .error-state { border-color: hsl(0, 70%, 40%); }
                .hint { font-size: 0.85rem; margin-top: 0.5rem; }

                .secondary-btn {
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    color: var(--text-primary);
                    cursor: pointer;
                    padding: 0.6rem 0.9rem;
                    font-size: 0.9rem;
                }
            "#}
        </style>
    }
}

// ─── Host Card ─────────────────────────────────────────────────────────────────

#[component]
fn HostCard(host: DiscoveredHost) -> impl IntoView {
    let shell_url = format!("http://{}:{}", host.ip_address, host.port);
    let quantum_capable = host.quantum_capable;
    let has_qpu = host.capabilities.iter().any(|c| c == "quantum_hardware");
    let hostname = host.hostname.clone();
    let host_id_short = if host.host_id.len() > 16 {
        format!("{}…", &host.host_id[..16])
    } else {
        host.host_id.clone()
    };

    view! {
        <div class="host-card">
            <div class="host-card-header">
                <div>
                    <p class="host-name">{hostname}</p>
                    <p class="host-id" title=host.host_id.clone()>{host_id_short}</p>
                </div>
                <div class="badge-row">
                    {if has_qpu {
                        view! { <span class="badge badge-qpu">"⚛ QPU"</span> }.into_any()
                    } else if quantum_capable {
                        view! { <span class="badge badge-sim">"⚛ SIM"</span> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    {if host.verified {
                        view! { <span class="badge badge-verified">"✓ Verified"</span> }.into_any()
                    } else {
                        view! { <span class="badge badge-unverified">"Unverified"</span> }.into_any()
                    }}
                </div>
            </div>

            <div class="host-meta">
                <span class="meta-label">"IP"</span>
                <span class="meta-value">{format!("{}:{}", host.ip_address, host.port)}</span>

                {host.os_version.map(|v| view! {
                    <span class="meta-label">"OS"</span>
                    <span class="meta-value">{v}</span>
                })}

                {if quantum_capable {
                    let q = host.quantum_qubits.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string());
                    let backends = host.quantum_backends.join(", ");
                    view! {
                        <span class="meta-label">"Qubits"</span>
                        <span class="meta-value">{q}</span>
                        <span class="meta-label">"Backends"</span>
                        <span class="meta-value">{backends}</span>
                    }.into_any()
                } else {
                    view! { <span></span><span></span> }.into_any()
                }}
            </div>

            {if !host.capabilities.is_empty() {
                view! {
                    <div class="cap-chips">
                        {host.capabilities.iter().map(|c| view! {
                            <span class="cap-chip">{c.clone()}</span>
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}

            <div class="host-actions">
                <a href=shell_url.clone() target="_blank" class="action-btn primary">
                    "Open Shell"
                </a>
                {if quantum_capable {
                    view! {
                        <a href="/quantum/designer" class="action-btn">"Circuit Designer"</a>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </div>

            <div class="last-seen">
                "Last seen: " {host.last_seen_at.split('T').next().unwrap_or("unknown").to_string()}
            </div>
        </div>
    }
}
