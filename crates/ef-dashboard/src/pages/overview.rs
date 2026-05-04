use leptos::prelude::*;

use crate::api::{self, JobListResponse, SessionListResponse};
use crate::components::device_icon::DeviceOsIcon;
use crate::components::stat_card::StatCard;

/// Dashboard overview — quick stats + recent activity.
#[component]
pub fn OverviewPage() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);

    // Auto-refresh every 5 seconds
    Effect::new(move || {
        let handle = gloo_timers::callback::Interval::new(5_000, move || {
            set_refresh.update(|n| *n = n.wrapping_add(1));
        });
        std::mem::forget(handle);
    });

    let devices = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::fetch_all_devices().await }
    });
    let sessions = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::get::<SessionListResponse>("/sessions").await.unwrap_or_default() }
    });
    let jobs = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::get::<JobListResponse>("/jobs").await.unwrap_or_default() }
    });

    let total_devices = move || {
        devices.get().as_deref().map(|d| d.total.to_string()).unwrap_or("…".into())
    };
    let available_devices = move || {
        devices.get().as_deref()
            .map(|d| d.devices.iter().filter(|dev| dev.status == "available" || dev.status == "online").count().to_string())
            .unwrap_or("…".into())
    };
    let active_sessions = move || {
        sessions.get().as_deref()
            .map(|s| s.sessions.iter().filter(|sess| sess.status == "active").count().to_string())
            .unwrap_or("…".into())
    };
    let total_jobs = move || {
        jobs.get().as_deref().map(|j| j.total.to_string()).unwrap_or("…".into())
    };
    let quantum_veeros = move || {
        devices.get().as_deref()
            .map(|d| d.devices.iter().filter(|dev| {
                dev.os == "veeros" && dev.capabilities.as_array()
                    .map(|a| a.iter().any(|v| v.as_str().map(|s| s.starts_with("quantum")).unwrap_or(false)))
                    .unwrap_or(false)
            }).count().to_string())
            .unwrap_or("…".into())
    };

    view! {
        <h1 style="margin-bottom: 1rem">"Dashboard"</h1>

        <div class="stats-grid">
            <StatCard label="Total Devices" value=Signal::derive(total_devices) class="info"/>
            <StatCard label="Available" value=Signal::derive(available_devices) class="success"/>
            <StatCard label="Active Sessions" value=Signal::derive(active_sessions) class="warning"/>
            <StatCard label="Test Jobs" value=Signal::derive(total_jobs)/>
            <StatCard label="Quantum VeerOS Hosts" value=Signal::derive(quantum_veeros) class="info"/>
        </div>

        <div class="grid-2">
            <div class="card">
                <div class="card-header">
                    <h2>"Recent Devices"</h2>
                </div>
                <Transition fallback=move || view! { <div class="loading">"Loading…"</div> }>
                    {move || devices.get().as_deref().map(|data| {
                        if data.devices.is_empty() {
                            return view! { <p class="empty-state">"No devices registered"</p> }.into_any();
                        }
                        let mut sorted: Vec<_> = data.devices.clone();
                        sorted.sort_by(|a, b| b.last_heartbeat.cmp(&a.last_heartbeat));
                        let devs: Vec<_> = sorted.into_iter().take(5).collect();
                        view! {
                            <div class="table-wrap">
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Name"</th>
                                            <th>"OS"</th>
                                            <th>"Version"</th>
                                            <th>"Status"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {devs.into_iter().map(|d| {
                                            let status = d.status.clone();
                                            let is_veeros = d.os == "veeros";
                                            let caps: Vec<String> = d.capabilities.as_array()
                                                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                                .unwrap_or_default();
                                            let is_qpu = caps.iter().any(|c| c == "quantum_hardware");
                                            let is_qsim = !is_qpu && caps.iter().any(|c| c.starts_with("quantum"));
                                            view! {
                                                <tr>
                                                    <td>
                                                        {d.name}
                                                        {if is_veeros && is_qpu {
                                                            view! { <span class="badge badge-qpu" style="margin-left:0.4rem;font-size:0.7rem">"QPU"</span> }.into_any()
                                                        } else if is_veeros && is_qsim {
                                                            view! { <span class="badge badge-sim" style="margin-left:0.4rem;font-size:0.7rem">"SIM"</span> }.into_any()
                                                        } else {
                                                            view! { }.into_any()
                                                        }}
                                                    </td>
                                                    <td><DeviceOsIcon os=d.os.clone()/> " " {d.os}</td>
                                                    <td>{d.os_version}</td>
                                                    <td><crate::components::status_badge::StatusBadge status=status/></td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    })}
                </Transition>
            </div>

            <div class="card">
                <div class="card-header">
                    <h2>"Recent Jobs"</h2>
                </div>
                <Transition fallback=move || view! { <div class="loading">"Loading…"</div> }>
                    {move || jobs.get().as_deref().map(|data| {
                        if data.jobs.is_empty() {
                            return view! { <p class="empty-state">"No test jobs yet"</p> }.into_any();
                        }
                        let recent: Vec<_> = data.jobs.iter().take(5).cloned().collect();
                        view! {
                            <div class="table-wrap">
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Suite"</th>
                                            <th>"Status"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {recent.into_iter().map(|j| {
                                            let status = j.status.clone();
                                            view! {
                                                <tr>
                                                    <td class="mono truncate">{j.test_suite}</td>
                                                    <td><crate::components::status_badge::StatusBadge status=status/></td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    })}
                </Transition>
            </div>
        </div>
    }
}
