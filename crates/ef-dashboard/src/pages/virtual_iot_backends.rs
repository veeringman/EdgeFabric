use leptos::prelude::*;

use crate::api::{self, CreateVirtualIotBackendRequest, UpdateVirtualIotBackendRequest,
    VirtualIotBackend, VirtualIotBackendEnvelope, VirtualIotBackendListResponse};
use crate::board_config::controllers;
use crate::components::status_badge::StatusBadge;

#[component]
pub fn VirtualIotBackendsPage() -> impl IntoView {
    // ── list refresh ─────────────────────────────────────────────────────────
    let (refresh, set_refresh) = signal(0u32);

    // ── create form ──────────────────────────────────────────────────────────
    let (name, set_name) = signal(String::new());
    let selected_controllers: RwSignal<Vec<String>> = RwSignal::new(vec!["esp32-devkit".to_string()]);
    let (region, set_region) = signal("local".to_string());
    let (auto_start, set_auto_start) = signal(true);
    let (saving, set_saving) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    // ── edit modal ───────────────────────────────────────────────────────────
    let editing: RwSignal<Option<VirtualIotBackend>> = RwSignal::new(None);
    let edit_name: RwSignal<String> = RwSignal::new(String::new());
    let edit_region: RwSignal<String> = RwSignal::new(String::new());
    let edit_controllers: RwSignal<Vec<String>> = RwSignal::new(vec![]);
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
            provider: "veeros-vm".to_string(),
            controller_type: ctypes[0].clone(),
            controller_types: ctypes,
            region: region.get_untracked(),
            auto_start: auto_start.get_untracked(),
        };

        leptos::task::spawn_local(async move {
            let result = api::post::<CreateVirtualIotBackendRequest, VirtualIotBackendEnvelope>(
                "/devices/virtual-iot/backends",
                &request,
            ).await;
            match result {
                Ok(_) => {
                    set_name.set(String::new());
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
            <div class="page-header">
                <h1>"Virtual IoT Backends"</h1>
                <a href="/virtual-iot/new" class="btn btn-secondary btn-sm">"Open IoT Designer"</a>
            </div>

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

            // ── Create Backend ───────────────────────────────────────────────
            <div class="card">
                <div class="card-header"><h2>"Create Backend"</h2></div>
                <div class="card-body">
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
                        <label>"Region"</label>
                        <input class="form-control"
                            prop:value=move || region.get()
                            on:input=move |ev| set_region.set(event_target_value(&ev)) />
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

            // ── Available Backends ───────────────────────────────────────────
            <div class="card">
                <div class="card-header"><h2>"Available Backends"</h2></div>
                <Transition fallback=move || view! { <div class="loading">"Loading backends…"</div> }>
                    {move || backends.get().map(|data| {
                        if data.backends.is_empty() {
                            return view! { <p class="empty-state">"No backends provisioned yet."</p> }.into_any();
                        }
                        view! {
                            <div class="table-wrap">
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Name"</th>
                                            <th>"Controllers"</th>
                                            <th>"Provider"</th>
                                            <th>"Region"</th>
                                            <th>"Status"</th>
                                            <th>"VM"</th>
                                            <th>"Shell"</th>
                                            <th>"Actions"</th>
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
                                            let ctypes_display = if backend.controller_types.is_empty() {
                                                backend.controller_type.clone()
                                            } else {
                                                backend.controller_types.join(", ")
                                            };
                                            let is_running = runtime_status == "running";
                                            let bid = StoredValue::new(backend.id.clone());
                                            let bval = StoredValue::new(backend.clone());
                                            view! {
                                                <tr>
                                                    <td><strong>{backend.name.clone()}</strong></td>
                                                    <td class="controllers-cell">{ctypes_display}</td>
                                                    <td>{backend.provider.clone()}</td>
                                                    <td>{backend.region.clone()}</td>
                                                    <td><StatusBadge status=runtime_status.clone()/></td>
                                                    <td class="mono truncate">{backend.vm_name.clone().unwrap_or_else(|| "—".into())}</td>
                                                    <td class="mono truncate">{shell_text}</td>
                                                    <td>
                                                        <div class="row-actions">
                                                            <Show
                                                                when=move || is_running
                                                                fallback=move || view! {
                                                                    <button class="btn btn-sm btn-success"
                                                                        on:click=move |_| {
                                                                            let id = bid.get_value();
                                                                            leptos::task::spawn_local(async move {
                                                                                let _ = api::post_empty(
                                                                                    &format!("/devices/virtual-iot/backends/{id}/start")
                                                                                ).await;
                                                                                set_refresh.update(|n| *n = n.wrapping_add(1));
                                                                            });
                                                                        }
                                                                    >"▶ Start"</button>
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
                                                                >"⏹ Stop"</button>
                                                            </Show>
                                                            <button class="btn btn-sm btn-secondary"
                                                                on:click=move |_| open_edit(bval.get_value())
                                                            >"✎ Edit"</button>
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
                                                            >"🗑 Delete"</button>
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
    }
}
