use leptos::prelude::*;
use serde::Serialize;

use crate::api::{self, DeviceListResponse};
use crate::components::status_badge::StatusBadge;

/// Admin panel — tenant management, device pools, API key management.
#[component]
pub fn AdminPage() -> impl IntoView {
    let (active_tab, set_active_tab) = signal("devices");

    view! {
        <h1 style="margin-bottom: 1rem">"Admin Panel"</h1>

        <div class="filters" style="margin-bottom: 1rem">
            <button
                class="btn btn-sm"
                class:btn-primary=move || active_tab.get() == "devices"
                class:btn-ghost=move || active_tab.get() != "devices"
                on:click=move |_| set_active_tab.set("devices")
            >"Device Management"</button>
            <button
                class="btn btn-sm"
                class:btn-primary=move || active_tab.get() == "apikeys"
                class:btn-ghost=move || active_tab.get() != "apikeys"
                on:click=move |_| set_active_tab.set("apikeys")
            >"API Keys"</button>
            <button
                class="btn btn-sm"
                class:btn-primary=move || active_tab.get() == "tenants"
                class:btn-ghost=move || active_tab.get() != "tenants"
                on:click=move |_| set_active_tab.set("tenants")
            >"Tenants"</button>
        </div>

        {move || match active_tab.get() {
            "devices" => view! { <DeviceManagement/> }.into_any(),
            "apikeys" => view! { <ApiKeyManagement/> }.into_any(),
            "tenants" => view! { <TenantManagement/> }.into_any(),
            _ => view! { <DeviceManagement/> }.into_any(),
        }}
    }
}

#[component]
fn DeviceManagement() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);
    let (show_add, set_show_add) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let devices = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::get::<DeviceListResponse>("/devices").await.unwrap_or_default()
        }
    });

    let do_refresh = move |_| set_refresh.update(|n| *n += 1);

    view! {
        <Show when=move || error.get().is_some()>
            <div class="alert error">{move || error.get().unwrap_or_default()}</div>
        </Show>

        <div class="card">
            <div class="card-header">
                <h2>"Device Pool"</h2>
                <div style="display: flex; gap: 0.5rem">
                    <button class="btn btn-primary btn-sm" on:click=move |_| set_show_add.update(|v| *v = !*v)>
                        {move || if show_add.get() { "Cancel" } else { "+ Add Device" }}
                    </button>
                    <button class="btn btn-ghost btn-sm" on:click=do_refresh>"↻"</button>
                </div>
            </div>

            <Show when=move || show_add.get()>
                <AddDeviceForm on_created=move || {
                    set_show_add.set(false);
                    set_refresh.update(|n| *n += 1);
                } on_error=move |e| set_error.set(Some(e))/>
            </Show>

            <Transition fallback=move || view! { <div class="loading">"Loading…"</div> }>
                {move || devices.get().as_deref().map(|data| {
                    if data.devices.is_empty() {
                        return view! { <p class="empty-state">"No devices"</p> }.into_any();
                    }
                    let devs = data.devices.clone();
                    let set_refresh = set_refresh;
                    view! {
                        <div class="table-wrap">
                            <table>
                                <thead>
                                    <tr>
                                        <th>"Name"</th>
                                        <th>"OS"</th>
                                        <th>"Kind"</th>
                                        <th>"Status"</th>
                                        <th>"Region"</th>
                                        <th>"Actions"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {devs.into_iter().map(|d| {
                                        let status = d.status.clone();
                                        let device_id = d.id.clone();
                                        let set_refresh = set_refresh;
                                        let on_delete = move |_| {
                                            let did = device_id.clone();
                                            let set_refresh = set_refresh;
                                            leptos::task::spawn_local(async move {
                                                let _ = api::delete(&format!("/devices/{did}")).await;
                                                set_refresh.update(|n| *n += 1);
                                            });
                                        };
                                        view! {
                                            <tr>
                                                <td><strong>{d.name}</strong></td>
                                                <td>{d.os}</td>
                                                <td>{d.kind}</td>
                                                <td><StatusBadge status=status/></td>
                                                <td>{d.region}</td>
                                                <td>
                                                    <button class="btn btn-danger btn-sm" on:click=on_delete>"Delete"</button>
                                                </td>
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
    }
}

#[derive(Serialize)]
struct AddDeviceReq {
    name: String,
    serial: Option<String>,
    os: String,
    os_version: String,
    kind: String,
    region: String,
}

#[component]
fn AddDeviceForm(
    on_created: impl Fn() + Send + Sync + 'static + Clone,
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let (serial, set_serial) = signal(String::new());
    let (os, set_os) = signal("android".to_string());
    let (os_version, set_os_version) = signal(String::new());
    let (kind, set_kind) = signal("real".to_string());
    let (region, set_region) = signal("default".to_string());
    let (loading, set_loading) = signal(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let on_created = on_created.clone();
        let on_error = on_error.clone();
        set_loading.set(true);

        let req = AddDeviceReq {
            name: name.get_untracked(),
            serial: { let s = serial.get_untracked(); if s.is_empty() { None } else { Some(s) } },
            os: os.get_untracked(),
            os_version: os_version.get_untracked(),
            kind: kind.get_untracked(),
            region: region.get_untracked(),
        };

        leptos::task::spawn_local(async move {
            match api::post::<_, serde_json::Value>("/devices", &req).await {
                Ok(_) => on_created(),
                Err(e) => on_error(e),
            }
            set_loading.set(false);
        });
    };

    view! {
        <form on:submit=submit style="display: flex; gap: 0.75rem; align-items: flex-end; flex-wrap: wrap; margin-bottom: 1rem; padding: 1rem; background: var(--bg-primary); border-radius: var(--radius)">
            <div class="form-group">
                <label>"Name"</label>
                <input type="text" placeholder="Pixel 7" prop:value=name on:input=move |ev| set_name.set(event_target_value(&ev))/>
            </div>
            <div class="form-group">
                <label>"Serial"</label>
                <input type="text" placeholder="Optional" prop:value=serial on:input=move |ev| set_serial.set(event_target_value(&ev))/>
            </div>
            <div class="form-group">
                <label>"OS"</label>
                <select on:change=move |ev| set_os.set(event_target_value(&ev))>
                    <option value="android">"Android"</option>
                    <option value="ios">"iOS"</option>
                    <option value="iot">"IoT"</option>
                    <option value="ssh">"SSH host"</option>
                    <option value="veeros">"VeerOS"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Version"</label>
                <input type="text" placeholder="14" prop:value=os_version on:input=move |ev| set_os_version.set(event_target_value(&ev))/>
            </div>
            <div class="form-group">
                <label>"Kind"</label>
                <select on:change=move |ev| set_kind.set(event_target_value(&ev))>
                    <option value="real">"Real"</option>
                    <option value="emulator">"Emulator"</option>
                    <option value="simulator">"Simulator"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Region"</label>
                <input type="text" prop:value=region on:input=move |ev| set_region.set(event_target_value(&ev))/>
            </div>
            <button type="submit" class="btn btn-primary btn-sm" disabled=move || loading.get()>
                {move || if loading.get() { "Adding…" } else { "Add Device" }}
            </button>
        </form>
    }
}

#[component]
fn ApiKeyManagement() -> impl IntoView {
    view! {
        <div class="card">
            <div class="card-header">
                <h2>"API Keys"</h2>
                <button class="btn btn-primary btn-sm">"+ Generate Key"</button>
            </div>
            <div class="table-wrap">
                <table>
                    <thead>
                        <tr>
                            <th>"Key Prefix"</th>
                            <th>"Created"</th>
                            <th>"Last Used"</th>
                            <th>"Status"</th>
                            <th>"Actions"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td class="mono">"ef_live_a3b8…"</td>
                            <td>"2026-04-01"</td>
                            <td>"2026-04-07"</td>
                            <td><StatusBadge status="active".to_string()/></td>
                            <td><button class="btn btn-danger btn-sm">"Revoke"</button></td>
                        </tr>
                        <tr>
                            <td class="mono">"ef_test_7cf2…"</td>
                            <td>"2026-03-15"</td>
                            <td>"2026-04-06"</td>
                            <td><StatusBadge status="active".to_string()/></td>
                            <td><button class="btn btn-danger btn-sm">"Revoke"</button></td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    }
}

#[component]
fn TenantManagement() -> impl IntoView {
    view! {
        <div class="card">
            <div class="card-header">
                <h2>"Tenants"</h2>
                <button class="btn btn-primary btn-sm">"+ Add Tenant"</button>
            </div>
            <p class="empty-state">"Tenant management requires super-admin privileges. Configure tenants via the CLI or API."</p>
        </div>
    }
}
