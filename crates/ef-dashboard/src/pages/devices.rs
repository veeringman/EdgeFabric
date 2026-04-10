use leptos::prelude::*;

use crate::api::{self, Device, DeviceListResponse};
use crate::components::device_icon::DeviceOsIcon;
use crate::components::status_badge::StatusBadge;

/// Device browser — list, filter, and manage devices.
#[component]
pub fn DevicesPage() -> impl IntoView {
    let (os_filter, set_os_filter) = signal(String::new());
    let (status_filter, set_status_filter) = signal(String::new());
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
        async move {
            api::get::<DeviceListResponse>("/devices").await.unwrap_or_default()
        }
    });

    let filtered = move || {
        devices.get().as_deref().map(|data| {
            let os = os_filter.get();
            let status = status_filter.get();
            data.devices
                .iter()
                .filter(|d| os.is_empty() || d.os.eq_ignore_ascii_case(&os))
                .filter(|d| status.is_empty() || d.status.eq_ignore_ascii_case(&status))
                .cloned()
                .collect::<Vec<_>>()
        })
    };

    let do_refresh = move |_| set_refresh.update(|n| *n += 1);

    view! {
        <h1 style="margin-bottom: 1rem">"Devices"</h1>

        <div class="filters">
            <div class="form-group">
                <label>"OS"</label>
                <select on:change=move |ev| set_os_filter.set(event_target_value(&ev))>
                    <option value="">"All"</option>
                    <option value="android">"Android"</option>
                    <option value="ios">"iOS"</option>
                    <option value="iot">"IoT"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Status"</label>
                <select on:change=move |ev| set_status_filter.set(event_target_value(&ev))>
                    <option value="">"All"</option>
                    <option value="available">"Available"</option>
                    <option value="busy">"Busy"</option>
                    <option value="offline">"Offline"</option>
                    <option value="maintenance">"Maintenance"</option>
                </select>
            </div>
            <button class="btn btn-ghost btn-sm" style="align-self: flex-end" on:click=do_refresh>"↻ Refresh"</button>
        </div>

        <div class="card">
            <Transition fallback=move || view! { <div class="loading">"Loading devices…"</div> }>
                {move || filtered().map(|devs| {
                    if devs.is_empty() {
                        return view! { <p class="empty-state">"No devices match the filters"</p> }.into_any();
                    }
                    view! { <DeviceTable devices=devs/> }.into_any()
                })}
            </Transition>
        </div>
    }
}

#[component]
fn DeviceTable(devices: Vec<Device>) -> impl IntoView {
    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"Name"</th>
                        <th>"Serial"</th>
                        <th>"OS"</th>
                        <th>"Version"</th>
                        <th>"Kind"</th>
                        <th>"Region"</th>
                        <th>"Status"</th>
                        <th>"Last Heartbeat"</th>
                    </tr>
                </thead>
                <tbody>
                    {devices.into_iter().map(|d| {
                        let status = d.status.clone();
                        view! {
                            <tr>
                                <td><strong>{d.name}</strong></td>
                                <td class="mono truncate">{d.serial.unwrap_or_else(|| "—".into())}</td>
                                <td><DeviceOsIcon os=d.os.clone()/> " " {d.os.clone()}</td>
                                <td>{d.os_version}</td>
                                <td>{d.kind}</td>
                                <td>{d.region}</td>
                                <td><StatusBadge status=status/></td>
                                <td class="mono">{d.last_heartbeat.unwrap_or_else(|| "—".into())}</td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}
