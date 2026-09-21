use leptos::prelude::*;
use std::collections::HashMap;

use crate::api::{self, AgentListResponse, CreateVirtualIotBackendRequest, UpdateVirtualIotBackendRequest,
    VirtualIotBackend, VirtualIotBackendEnvelope, VirtualIotBackendListResponse};
use crate::board_config::{controllers, sensor_profile};
use crate::components::board_twin_3d::BoardTwin3d;
use crate::components::status_badge::StatusBadge;

fn extract_host_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next().unwrap_or_default();
    let host = host_port.split(':').next().unwrap_or_default().trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn backend_host_ip(backend: &VirtualIotBackend) -> String {
    backend
        .shell_host
        .clone()
        .or_else(|| backend.agent_base_url.as_deref().and_then(extract_host_from_url))
        .unwrap_or_else(|| "—".to_string())
}

fn is_local_host_label(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "local" | "localhost" | "127.0.0.1" | "::1"
    )
}

fn format_agent_host_label(backend: &VirtualIotBackend) -> String {
    let host_ip = backend_host_ip(backend);
    let host_missing = host_ip == "—";
    let host_is_local = is_local_host_label(&host_ip);

    match backend.agent_id.as_deref() {
        None => {
            if host_missing || host_is_local {
                "local".to_string()
            } else {
                format!("local · {host_ip}")
            }
        }
        Some(id) => {
            let agent_label = if id.len() > 8 {
                format!("agent:{}", &id[..8])
            } else {
                format!("agent:{id}")
            };

            if host_missing {
                agent_label
            } else {
                format!("{agent_label} · {host_ip}")
            }
        }
    }
}

#[component]
pub fn VirtualIotBackendsPage() -> impl IntoView {
    // ── list refresh ─────────────────────────────────────────────────────────
    let (refresh, set_refresh) = signal(0u32);

    // ── create form ──────────────────────────────────────────────────────────
    let (name, set_name) = signal(String::new());
    let selected_controllers: RwSignal<Vec<String>> = RwSignal::new(vec!["esp32-devkit".to_string()]);
    let (region, set_region) = signal("local".to_string());
    let (agent_id, set_agent_id) = signal(String::new());
    let (auto_start, set_auto_start) = signal(true);
    let (saving, set_saving) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    // ── 3D twin preview (first selected controller + representative sensors) ──
    let (twin_controller, set_twin_controller) = signal("esp32-devkit".to_string());
    let (twin_sensors, _) = signal(vec![
        "temperature".to_string(),
        "humidity".to_string(),
        "accel_xyz".to_string(),
        "ambient_light".to_string(),
        "gps".to_string(),
        "battery_voltage".to_string(),
        "wifi_rssi".to_string(),
    ]);
    let (twin_values, set_twin_values) = signal(HashMap::<String, String>::new());
    Effect::new(move || {
        let first = selected_controllers
            .get()
            .first()
            .cloned()
            .unwrap_or_else(|| "esp32-devkit".to_string());
        set_twin_controller.set(first);
        let map: HashMap<String, String> = twin_sensors
            .get_untracked()
            .iter()
            .map(|s| (s.clone(), sensor_profile(s).preview.clone()))
            .collect();
        set_twin_values.set(map);
    });

    // ── edit modal ───────────────────────────────────────────────────────────
    let editing: RwSignal<Option<VirtualIotBackend>> = RwSignal::new(None);
    let edit_name: RwSignal<String> = RwSignal::new(String::new());
    let edit_region: RwSignal<String> = RwSignal::new(String::new());
    let edit_controllers: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let edit_agent_id: RwSignal<String> = RwSignal::new(String::new());
    let edit_host_ip: RwSignal<String> = RwSignal::new(String::new());
    let edit_saving: RwSignal<bool> = RwSignal::new(false);
    let edit_error: RwSignal<Option<String>> = RwSignal::new(None);

    // ── backend list ─────────────────────────────────────────────────────────
    let backends = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::get::<VirtualIotBackendListResponse>("/devices/virtual-iot/backends?available_only=false")
                .await
                .unwrap_or_default()
        }
    });
    let agents = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::get::<AgentListResponse>("/agents?only_online=true")
                .await
                .unwrap_or_default()
        }
    });

    // ── create handlers ──────────────────────────────────────────────────────
    let toggle_create_ctrl = move |id: String| {
        selected_controllers.update(|list| {
            if let Some(pos) = list.iter().position(|s| s == &id) {
                list.remove(pos);
            } else {
                list.push(id);
            }
        });
    };

    let create_backend = move |_| {
        let backend_name = name.get_untracked();
        if backend_name.trim().is_empty() {
            set_error.set(Some("Backend name is required".into()));
            return;
        }
        let ctypes = selected_controllers.get_untracked();
        if ctypes.is_empty() {
            set_error.set(Some("Select at least one controller type".into()));
            return;
        }
        set_saving.set(true);
        set_error.set(None);

        let request = CreateVirtualIotBackendRequest {
            name: backend_name,
            provider: "veer-vm".to_string(),
            controller_type: ctypes[0].clone(),
            controller_types: ctypes,
            region: region.get_untracked(),
            auto_start: auto_start.get_untracked(),
            agent_id: {
                let selected = agent_id.get_untracked();
                if selected.trim().is_empty() {
                    None
                } else {
                    Some(selected)
                }
            },
        };

        leptos::task::spawn_local(async move {
            let result = api::post::<CreateVirtualIotBackendRequest, VirtualIotBackendEnvelope>(
                "/devices/virtual-iot/backends",
                &request,
            ).await;
            match result {
                Ok(_) => {
                    set_name.set(String::new());
                    set_agent_id.set(String::new());
                    set_saving.set(false);
                    set_refresh.update(|n| *n = n.wrapping_add(1));
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_saving.set(false);
                }
            }
        });
    };

    // ── edit handlers ────────────────────────────────────────────────────────
    let open_edit = move |b: VirtualIotBackend| {
        edit_name.set(b.name.clone());
        edit_region.set(b.region.clone());
        edit_agent_id.set(b.agent_id.clone().unwrap_or_else(|| "local".to_string()));
        edit_host_ip.set(backend_host_ip(&b));
        edit_controllers.set(if b.controller_types.is_empty() {
            vec![b.controller_type.clone()]
        } else {
            b.controller_types.clone()
        });
        edit_error.set(None);
        editing.set(Some(b));
    };

    let close_edit = move |_| editing.set(None);

    let save_edit = move |_| {
        let Some(b) = editing.get_untracked() else { return; };
        let ctypes = edit_controllers.get_untracked();
        if ctypes.is_empty() {
            edit_error.set(Some("Select at least one controller type".into()));
            return;
        }
        edit_saving.set(true);
        edit_error.set(None);
        let id = b.id.clone();
        let req = UpdateVirtualIotBackendRequest {
            name: Some(edit_name.get_untracked()),
            region: Some(edit_region.get_untracked()),
            controller_types: Some(ctypes),
        };
        leptos::task::spawn_local(async move {
            let path = format!("/devices/virtual-iot/backends/{id}");
            let result = api::patch::<UpdateVirtualIotBackendRequest, serde_json::Value>(&path, &req).await;
            edit_saving.set(false);
            match result {
                Ok(_) => {
                    editing.set(None);
                    set_refresh.update(|n| *n = n.wrapping_add(1));
                }
                Err(e) => edit_error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="page virtual-iot-backends-page">
            <div class="page-header backends-page-header">
                <div>
                    <h1>"Virtual IoT Backends"</h1>
                    <p class="page-subtitle">"Provision backends, bind them to agents, and route Virtual IoT workloads reliably."</p>
                </div>
                <a href="/virtual-iot/new" class="btn btn-secondary btn-sm">"Open IoT Designer"</a>
            </div>

            <Transition fallback=move || view! { <div class="loading">"Loading backend summary…"</div> }>
                {move || backends.get().map(|data| {
                    let total = data.backends.len();
                    let running = data.backends.iter().filter(|b| {
                        b.config
                            .get("runtime_status")
                            .and_then(serde_json::Value::as_str)
                            == Some("running")
                    }).count();
                    let attached = data.backends.iter().filter(|b| b.attached_device_id.is_some()).count();
                    let agent_bound = data.backends.iter().filter(|b| b.agent_id.is_some()).count();
                    view! {
                        <div class="backend-kpis">
                            <div class="backend-kpi-card"><span>"Total"</span><strong>{total.to_string()}</strong></div>
                            <div class="backend-kpi-card"><span>"Running"</span><strong>{running.to_string()}</strong></div>
                            <div class="backend-kpi-card"><span>"Attached"</span><strong>{attached.to_string()}</strong></div>
                            <div class="backend-kpi-card"><span>"Agent Bound"</span><strong>{agent_bound.to_string()}</strong></div>
                        </div>
                    }
                })}
            </Transition>

            // ── Edit Modal ───────────────────────────────────────────────────
            <Show when=move || editing.get().is_some()>
                <div class="modal-overlay" on:click=close_edit>
                    <div class="modal" on:click=|ev| ev.stop_propagation()>
                        <div class="modal-header">
                            <h2>"Edit Backend"</h2>
                            <button class="btn-icon" on:click=close_edit>"✕"</button>
                        </div>
                        <div class="modal-body">
                            <div class="form-group">
                                <label>"Name"</label>
                                <input class="form-control"
                                    prop:value=move || edit_name.get()
                                    on:input=move |ev| edit_name.set(event_target_value(&ev)) />
                            </div>
                            <div class="form-group">
                                <label>"Region"</label>
                                <input class="form-control"
                                    prop:value=move || edit_region.get()
                                    on:input=move |ev| edit_region.set(event_target_value(&ev)) />
                            </div>
                            <div class="form-group">
                                <label>"Assigned Agent"</label>
                                <input class="form-control" prop:value=move || edit_agent_id.get() readonly=true />
                                <small style="color:var(--text-muted);">"Agent binding is set at create time."</small>
                            </div>
                            <div class="form-group">
                                <label>"Host/IP"</label>
                                <input class="form-control" prop:value=move || edit_host_ip.get() readonly=true />
                            </div>
                            <div class="form-group">
                                <label>"Supported Controllers"</label>
                                <div class="controller-grid">
                                    {controllers().iter().map(|c| {
                                        let cid = StoredValue::new(c.id.clone());
                                        let label = c.name.clone();
                                        view! {
                                            <label class="controller-chip">
                                                <input type="checkbox"
                                                    prop:checked=move || edit_controllers.get().contains(&cid.get_value())
                                                    on:change=move |_| {
                                                        let id = cid.get_value();
                                                        edit_controllers.update(|list| {
                                                            if let Some(pos) = list.iter().position(|s| *s == id) {
                                                                list.remove(pos);
                                                            } else {
                                                                list.push(id);
                                                            }
                                                        });
                                                    }
                                                />
                                                <span>{label}</span>
                                            </label>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                            <Show when=move || edit_error.get().is_some()>
                                <div class="alert error">{move || edit_error.get().unwrap_or_default()}</div>
                            </Show>
                        </div>
                        <div class="modal-footer">
                            <button class="btn btn-secondary btn-sm" on:click=close_edit>"Cancel"</button>
                            <button class="btn btn-primary btn-sm" on:click=save_edit
                                disabled=move || edit_saving.get()>
                                {move || if edit_saving.get() { "Saving…" } else { "Save Changes" }}
                            </button>
                        </div>
                    </div>
                </div>
            </Show>

            <div class="backends-layout">
                // ── Create Backend ───────────────────────────────────────────
                <div class="card backends-create-card">
                    <div class="card-header"><h2>"Create Backend"</h2></div>
                    <div class="card-body backends-create-body">
                    <div class="form-group">
                        <label>"Name"</label>
                        <input class="form-control" placeholder="my-backend"
                            prop:value=move || name.get()
                            on:input=move |ev| set_name.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-group">
                        <label>"Supported Controllers"</label>
                        <div class="controller-grid">
                            {controllers().iter().map(|c| {
                                let cid = StoredValue::new(c.id.clone());
                                let label = c.name.clone();
                                view! {
                                    <label class="controller-chip">
                                        <input type="checkbox"
                                            prop:checked=move || selected_controllers.get().contains(&cid.get_value())
                                            on:change=move |_| toggle_create_ctrl(cid.get_value())
                                        />
                                        <span>{label}</span>
                                    </label>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                    <div class="form-group">
                        <label>"3D Twin Preview"</label>
                        <BoardTwin3d
                            controller=twin_controller
                            enabled_sensors=twin_sensors
                            sensor_values=twin_values
                            mini=true
                        />
                        <small style="color:var(--text-muted);">"Live preview of the first selected controller."</small>
                    </div>
                    <div class="form-group">
                        <label>"Region"</label>
                        <input class="form-control"
                            prop:value=move || region.get()
                            on:input=move |ev| set_region.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-group">
                        <label>"Agent"</label>
                        <select class="form-control"
                            on:change=move |ev| set_agent_id.set(event_target_value(&ev))>
                            <option value="">"Local control-plane runtime"</option>
                            {move || agents.get().map(|data| {
                                data.agents.iter().cloned().map(|agent| {
                                    let value = agent.id.clone();
                                    let label = format!(
                                        "{} ({}) · {}",
                                        agent.name,
                                        agent.region,
                                        agent.streaming_url
                                            .as_deref()
                                            .and_then(extract_host_from_url)
                                            .unwrap_or_else(|| "unknown-host".to_string())
                                    );
                                    let selected_value = value.clone();
                                    view! {
                                        <option value={value.clone()} selected=move || agent_id.get() == selected_value>{label}</option>
                                    }
                                }).collect::<Vec<_>>()
                            }).unwrap_or_default()}
                        </select>
                        <small style="color:var(--text-muted);">"Choose a remote agent for host-bound routing, or keep local runtime."</small>
                    </div>
                    <div class="form-group">
                        <label>"Host/IP"</label>
                        <input class="form-control" readonly=true prop:value=move || {
                            let selected = agent_id.get();
                            if selected.is_empty() {
                                return "local".to_string();
                            }
                            agents
                                .get()
                                .and_then(|data| {
                                    data.agents
                                        .iter()
                                        .find(|agent| agent.id == selected)
                                        .and_then(|agent| {
                                            agent
                                                .streaming_url
                                                .as_deref()
                                                .and_then(extract_host_from_url)
                                        })
                                })
                                .unwrap_or_else(|| "unknown-host".to_string())
                        } />
                    </div>
                    <label style="display:flex; align-items:center; gap:0.5rem; margin-bottom:0.5rem; color:var(--text-muted);">
                        <input type="checkbox"
                            prop:checked=move || auto_start.get()
                            on:change=move |_| set_auto_start.update(|v| *v = !*v) />
                        <span>"Auto-start backend"</span>
                    </label>
                    <button class="btn btn-primary btn-sm" on:click=create_backend
                        disabled=move || saving.get()>
                        {move || if saving.get() { "Provisioning…" } else { "Create Backend" }}
                    </button>
                </div>
                <Show when=move || error.get().is_some()>
                    <div class="alert error" style="margin:0 1rem 1rem 1rem;">
                        {move || error.get().unwrap_or_default()}
                    </div>
                </Show>
                </div>

                // ── Available Backends ───────────────────────────────────────
                <div class="card backends-list-card">
                    <div class="card-header backends-list-header">
                        <h2>"Available Backends"</h2>
                        <span class="backends-list-hint">"Start/stop, edit metadata, and remove stale VM backends."</span>
                    </div>
                    <Transition fallback=move || view! { <div class="loading">"Loading backends…"</div> }>
                        {move || backends.get().map(|data| {
                        if data.backends.is_empty() {
                            return view! { <p class="empty-state">"No backends provisioned yet."</p> }.into_any();
                        }
                        view! {
                            <div class="table-wrap">
                                <table class="backends-table">
                                    <colgroup>
                                        <col class="col-name"/>
                                        <col class="col-ctrls"/>
                                        <col class="col-provider"/>
                                        <col class="col-region"/>
                                        <col class="col-agent"/>
                                        <col class="col-status"/>
                                        <col class="col-vm"/>
                                        <col class="col-shell"/>
                                        <col class="col-actions"/>
                                    </colgroup>
                                    <thead>
                                        <tr>
                                            <th>"Name"</th>
                                            <th>"Controllers"</th>
                                            <th>"Provider"</th>
                                            <th>"Region"</th>
                                            <th>"Agent / Host"</th>
                                            <th>"Status"</th>
                                            <th>"VM"</th>
                                            <th>"Shell"</th>
                                            <th class="th-actions">"Actions"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {data.backends.iter().cloned().map(|backend| {
                                            let shell_text = match (backend.shell_host.clone(), backend.shell_port) {
                                                (Some(host), Some(port)) => format!("{}:{}", host, port),
                                                _ => "—".to_string(),
                                            };
                                            let runtime_status = backend
                                                .config
                                                .get("runtime_status")
                                                .and_then(serde_json::Value::as_str)
                                                .unwrap_or("unknown")
                                                .to_string();
                                            let ctypes_list: Vec<String> = if backend.controller_types.is_empty() {
                                                vec![backend.controller_type.clone()]
                                            } else {
                                                backend.controller_types.clone()
                                            };
                                            let is_running = runtime_status == "running";
                                            let agent_host_label = format_agent_host_label(&backend);
                                            let bid = StoredValue::new(backend.id.clone());
                                            let bval = StoredValue::new(backend.clone());
                                            view! {
                                                <tr>
                                                    <td class="cell-name"><strong>{backend.name.clone()}</strong></td>
                                                    <td class="cell-ctrls">
                                                        <div class="chip-row">
                                                            {ctypes_list.into_iter().map(|c| view! {
                                                                <span class="chip">{c}</span>
                                                            }).collect::<Vec<_>>()}
                                                        </div>
                                                    </td>
                                                    <td>{backend.provider.clone()}</td>
                                                    <td>{backend.region.clone()}</td>
                                                    <td class="cell-mono">
                                                        <span class="cell-mono-text">{agent_host_label}</span>
                                                    </td>
                                                    <td><StatusBadge status=runtime_status.clone()/></td>
                                                    <td class="cell-mono">
                                                        <span class="cell-mono-text">
                                                            {backend.vm_name.clone().unwrap_or_else(|| "—".into())}
                                                        </span>
                                                    </td>
                                                    <td class="cell-mono">
                                                        <span class="cell-mono-text">{shell_text}</span>
                                                    </td>
                                                    <td class="cell-actions">
                                                        <div class="row-actions">
                                                            <Show
                                                                when=move || is_running
                                                                fallback=move || view! {
                                                                    <button class="btn btn-sm btn-success"
                                                                        on:click=move |_| {
                                                                            let id = bid.get_value();
                                                                            leptos::logging::log!("[backends] Start clicked id={}", id);
                                                                            leptos::task::spawn_local(async move {
                                                                                leptos::logging::log!("[backends] POST /start id={}", id);
                                                                                let res = api::post_empty(
                                                                                    &format!("/devices/virtual-iot/backends/{id}/start")
                                                                                ).await;
                                                                                leptos::logging::log!("[backends] /start result: {:?}", res);
                                                                                set_refresh.update(|n| *n = n.wrapping_add(1));
                                                                            });
                                                                        }
                                                                    >"Start"</button>
                                                                }>
                                                                <button class="btn btn-sm btn-warning"
                                                                    on:click=move |_| {
                                                                        let id = bid.get_value();
                                                                        leptos::task::spawn_local(async move {
                                                                            let _ = api::post_empty(
                                                                                &format!("/devices/virtual-iot/backends/{id}/stop")
                                                                            ).await;
                                                                            set_refresh.update(|n| *n = n.wrapping_add(1));
                                                                        });
                                                                    }
                                                                >"Stop"</button>
                                                            </Show>
                                                            <button class="btn btn-sm btn-secondary"
                                                                on:click=move |_| open_edit(bval.get_value())
                                                            >"Edit"</button>
                                                            <button class="btn btn-sm btn-danger"
                                                                on:click=move |_| {
                                                                    let id = bid.get_value();
                                                                    let confirmed = web_sys::window()
                                                                        .and_then(|w| w.confirm_with_message(
                                                                            "Delete this backend? This will destroy the VM."
                                                                        ).ok())
                                                                        .unwrap_or(false);
                                                                    if confirmed {
                                                                        leptos::task::spawn_local(async move {
                                                                            let _ = api::delete(
                                                                                &format!("/devices/virtual-iot/backends/{id}")
                                                                            ).await;
                                                                            set_refresh.update(|n| *n = n.wrapping_add(1));
                                                                        });
                                                                    }
                                                                }
                                                            >"Delete"</button>
                                                        </div>
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
            </div>
        </div>
    }
}
