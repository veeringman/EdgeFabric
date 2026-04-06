use leptos::prelude::*;

use crate::api::{self, DeviceListResponse, JobListResponse, SessionListResponse};
use crate::components::stat_card::StatCard;

/// Dashboard overview — quick stats + recent activity.
#[component]
pub fn OverviewPage() -> impl IntoView {
    let devices = LocalResource::new(|| async {
        api::get::<DeviceListResponse>("/devices").await.unwrap_or_default()
    });
    let sessions = LocalResource::new(|| async {
        api::get::<SessionListResponse>("/sessions").await.unwrap_or_default()
    });
    let jobs = LocalResource::new(|| async {
        api::get::<JobListResponse>("/jobs").await.unwrap_or_default()
    });

    let total_devices = move || {
        devices.get().as_deref().map(|d| d.total.to_string()).unwrap_or("…".into())
    };
    let available_devices = move || {
        devices.get().as_deref()
            .map(|d| d.devices.iter().filter(|dev| dev.status == "available").count().to_string())
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

    view! {
        <h1 style="margin-bottom: 1rem">"Dashboard"</h1>

        <div class="stats-grid">
            <StatCard label="Total Devices" value=Signal::derive(total_devices) class="info"/>
            <StatCard label="Available" value=Signal::derive(available_devices) class="success"/>
            <StatCard label="Active Sessions" value=Signal::derive(active_sessions) class="warning"/>
            <StatCard label="Test Jobs" value=Signal::derive(total_jobs)/>
        </div>

        <div class="grid-2">
            <div class="card">
                <div class="card-header">
                    <h2>"Recent Devices"</h2>
                </div>
                <Suspense fallback=move || view! { <div class="loading">"Loading…"</div> }>
                    {move || devices.get().as_deref().map(|data| {
                        if data.devices.is_empty() {
                            return view! { <p class="empty-state">"No devices registered"</p> }.into_any();
                        }
                        let devs: Vec<_> = data.devices.iter().take(5).cloned().collect();
                        view! {
                            <div class="table-wrap">
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Name"</th>
                                            <th>"OS"</th>
                                            <th>"Status"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {devs.into_iter().map(|d| {
                                            let status = d.status.clone();
                                            view! {
                                                <tr>
                                                    <td>{d.name}</td>
                                                    <td>{os_icon(&d.os)} " " {d.os}</td>
                                                    <td><crate::components::status_badge::StatusBadge status=status/></td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    })}
                </Suspense>
            </div>

            <div class="card">
                <div class="card-header">
                    <h2>"Recent Jobs"</h2>
                </div>
                <Suspense fallback=move || view! { <div class="loading">"Loading…"</div> }>
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
                </Suspense>
            </div>
        </div>
    }
}

fn os_icon(os: &str) -> &'static str {
    match os.to_lowercase().as_str() {
        "android" => "🤖",
        "ios" => "🍎",
        _ => "📟",
    }
}
