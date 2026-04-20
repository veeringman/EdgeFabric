use leptos::prelude::*;
use serde_json::Value;

use crate::api::{
    self, AttachVirtualIotBackendRequest, CreateVirtualIotRequest, UpdateVirtualIotConfigRequest,
    VirtualIotBackendListResponse, VirtualIotConfigResponse,
};
use crate::board_config::{controller_accessories, controllers, find_controller, sensor_categories, ControllerProfile};
use crate::components::board_layout::render_catalog_board_layout;

/// IoT Designer page — create or edit a virtual IoT device.
#[component]
pub fn VirtualIotDesignerPage() -> impl IntoView {
    // Check if editing — pull ?edit=<device_id> from URL
    let edit_device_id = {
        let params = leptos_router::hooks::use_query_map();
        let p = params.get_untracked();
        p.get("edit")
    };
    let is_edit = edit_device_id.is_some();
    let edit_id = edit_device_id.clone().unwrap_or_default();

    // Form signals
    let (device_name, set_device_name) = signal(String::new());
    let (region, set_region) = signal("local".to_string());
    let (controller_type, set_controller_type) = signal("esp32-devkit".to_string());
    let (sim_rate_ms, set_sim_rate_ms) = signal(500i32);
    let (enabled_sensors, set_enabled_sensors) = signal(Vec::<String>::new());
    let (selected_backend_id, set_selected_backend_id) = signal(String::new());
    let (attached_backend_id, set_attached_backend_id) = signal(String::new());
    let (saving, set_saving) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (loaded, set_loaded) = signal(!is_edit); // if creating, no load needed

    let compatible_backends = LocalResource::new(move || {
        let controller = controller_type.get();
        let selected_region = region.get();
        async move {
            let path = format!(
                "/devices/virtual-iot/backends?controller_type={}&region={}&available_only=false",
                controller, selected_region
            );
            api::get::<VirtualIotBackendListResponse>(&path)
                .await
                .unwrap_or_default()
        }
    });

    // In edit mode, fetch existing config
    if is_edit {
        let edit_id_c = edit_id.clone();
        leptos::task::spawn_local(async move {
            // Fetch device info
            match api::get::<Value>(&format!("/devices/{edit_id_c}")).await {
                Ok(val) => {
                    if let Some(dev) = val.get("device") {
                        if let Some(n) = dev.get("name").and_then(|v| v.as_str()) {
                            set_device_name.set(n.to_string());
                        }
                        if let Some(r) = dev.get("region").and_then(|v| v.as_str()) {
                            set_region.set(r.to_string());
                        }
                    }
                }
                Err(e) => set_error.set(Some(format!("Failed to load device: {e}"))),
            }
            // Fetch virtual iot config
            match api::get::<VirtualIotConfigResponse>(&format!("/devices/{edit_id_c}/virtual-iot/config")).await {
                Ok(resp) => {
                    set_controller_type.set(resp.config.controller_type);
                    set_sim_rate_ms.set(resp.config.sim_rate_ms);
                    if let Some(backend_id) = resp.config.backend_id {
                        set_selected_backend_id.set(backend_id.clone());
                        set_attached_backend_id.set(backend_id);
                    }
                    if let Ok(sensors) = serde_json::from_value::<Vec<String>>(resp.config.enabled_sensors) {
                        set_enabled_sensors.set(sensors);
                    }
                    set_loaded.set(true);
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to load config: {e}")));
                    set_loaded.set(true);
                }
            }
        });
    }

    // Toggle a sensor in the enabled list
    let toggle_sensor = move |sensor_id: String| {
        set_enabled_sensors.update(|list| {
            if let Some(pos) = list.iter().position(|s| s == &sensor_id) {
                list.remove(pos);
            } else {
                list.push(sensor_id);
            }
        });
    };

    // Save handler
    let edit_id_save = edit_id.clone();
    let on_save = move |_: web_sys::MouseEvent| {
        let name = device_name.get_untracked();
        if name.trim().is_empty() {
            set_error.set(Some("Device name is required".into()));
            return;
        }
        set_saving.set(true);
        set_error.set(None);

        let name = name.clone();
        let ct = controller_type.get_untracked();
        let reg = region.get_untracked();
        let rate = sim_rate_ms.get_untracked();
        let sensors = enabled_sensors.get_untracked();
        let backend_id = selected_backend_id.get_untracked();
        let current_backend_id = attached_backend_id.get_untracked();
        let is_edit = is_edit;
        let edit_id = edit_id_save.clone();

        leptos::task::spawn_local(async move {
            let result = if is_edit {
                api::patch::<UpdateVirtualIotConfigRequest, Value>(
                    &format!("/devices/{edit_id}/virtual-iot/config"),
                    &UpdateVirtualIotConfigRequest {
                        controller_type: Some(ct),
                        enabled_sensors: Some(sensors),
                        sim_rate_ms: Some(rate),
                    },
                ).await
            } else {
                api::post::<CreateVirtualIotRequest, Value>(
                    "/devices/virtual-iot",
                    &CreateVirtualIotRequest {
                        name,
                        controller_type: ct,
                        region: reg,
                        sim_rate_ms: rate,
                        enabled_sensors: sensors,
                    },
                ).await
            };

            match result {
                Ok(resp) => {
                    let device_id = if is_edit {
                        edit_id.clone()
                    } else {
                        resp.get("device")
                            .and_then(|d| d.get("id"))
                            .and_then(|id| id.as_str())
                            .unwrap_or_default()
                            .to_string()
                    };

                    if !device_id.is_empty() {
                        if backend_id.is_empty() && !current_backend_id.is_empty() {
                            let _ = api::post::<Value, Value>(
                                &format!("/devices/{device_id}/virtual-iot/backend/detach"),
                                &serde_json::json!({}),
                            ).await;
                        } else if !backend_id.is_empty() && backend_id != current_backend_id {
                            let _ = api::post::<AttachVirtualIotBackendRequest, Value>(
                                &format!("/devices/{device_id}/virtual-iot/backend/attach"),
                                &AttachVirtualIotBackendRequest { backend_id: backend_id.clone() },
                            ).await;
                        }
                    }

                    // Navigate to devices page
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href("/devices");
                    }
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_saving.set(false);
                }
            }
        });
    };

    view! {
        <div class="iot-designer-page">
            <div class="iot-designer-header">
                <a href="/devices" class="btn btn-ghost btn-sm">"← Back to Devices"</a>
                <h1>{if is_edit { "Edit Virtual IoT" } else { "IoT Designer" }}</h1>
                <div class="iot-designer-actions">
                    <a href="/devices" class="btn btn-ghost btn-sm">"Cancel"</a>
                    <button
                        class="btn btn-primary btn-sm"
                        on:click=on_save
                        disabled=move || saving.get()
                    >
                        {move || if saving.get() { "Saving…" } else if is_edit { "Update" } else { "Save Controller" }}
                    </button>
                </div>
            </div>

            <Show when=move || error.get().is_some()>
                <div class="alert error" style="margin-bottom: 1rem;">
                    {move || error.get().unwrap_or_default()}
                </div>
            </Show>

            <Show when=move || !loaded.get()>
                <p class="empty-state">"Loading configuration…"</p>
            </Show>

            <Show when=move || loaded.get()>
                <div class="iot-designer-layout">
                    // ── Left: Board Preview ──
                    <div class="iot-designer-preview">
                        <div class="card">
                            <div class="card-header"><h2>"Board Preview"</h2></div>
                            <div class="board-preview-container">
                                <BoardPreview controller_type=controller_type/>
                            </div>
                            <div class="controller-specs">
                                {move || {
                                    let ct = controller_type.get();
                                    let profile = find_controller(&ct);
                                    let accessory_summary = controller_accessories(&ct)
                                        .into_iter()
                                        .map(|accessory| format!("{} ({})", accessory.label, accessory.interface))
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    view! {
                                        <div class="spec-row"><span class="spec-label">"Flash"</span><span class="spec-value">{profile.flash.as_str()}</span></div>
                                        <div class="spec-row"><span class="spec-label">"RAM"</span><span class="spec-value">{profile.ram.as_str()}</span></div>
                                        <div class="spec-row"><span class="spec-label">"Connectivity"</span><span class="spec-value">{profile.connectivity.as_str()}</span></div>
                                        {if accessory_summary.is_empty() {
                                            view! { <></> }.into_any()
                                        } else {
                                            view! {
                                                <div class="spec-row"><span class="spec-label">"Accessories"</span><span class="spec-value">{accessory_summary}</span></div>
                                            }.into_any()
                                        }}
                                    }
                                }}
                            </div>
                        </div>
                    </div>

                    // ── Right: Configuration Panel ──
                    <div class="iot-designer-config">
                        <div class="card">
                            <div class="card-header"><h2>"Configuration"</h2></div>
                            <div style="padding: 1rem; display: flex; flex-direction: column; gap: 1rem;">
                                // Device name
                                <div class="form-group">
                                    <label>"Device Name"</label>
                                    <input
                                        type="text"
                                        class="form-control"
                                        placeholder="my-esp32-sim"
                                        prop:value=move || device_name.get()
                                        on:input=move |ev| set_device_name.set(event_target_value(&ev))
                                        disabled=is_edit
                                    />
                                </div>
                                // Region
                                <div class="form-group">
                                    <label>"Region"</label>
                                    <select
                                        class="form-control"
                                        on:change=move |ev| set_region.set(event_target_value(&ev))
                                        disabled=is_edit
                                    >
                                        <option value="local" selected=move || region.get() == "local">"local"</option>
                                        <option value="default" selected=move || region.get() == "default">"default"</option>
                                        <option value="edge" selected=move || region.get() == "edge">"edge"</option>
                                    </select>
                                </div>
                                // Controller type
                                <div class="form-group">
                                    <label>"Controller"</label>
                                    <select
                                        class="form-control"
                                        on:change=move |ev| set_controller_type.set(event_target_value(&ev))
                                    >
                                        {controllers().iter().map(|c| {
                                            let id = c.id.clone();
                                            let name = c.name.clone();
                                            let id_c = id.clone();
                                            view! {
                                                <option value={id} selected=move || controller_type.get() == id_c>{name}</option>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </select>
                                </div>
                                // Sim rate
                                <div class="form-group">
                                    <label>"Simulation Rate (ms)"</label>
                                    <input
                                        type="number"
                                        class="form-control"
                                        min="100"
                                        max="5000"
                                        step="100"
                                        prop:value=move || sim_rate_ms.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                                set_sim_rate_ms.set(v.clamp(100, 5000));
                                            }
                                        }
                                    />
                                </div>

                                <div class="form-group">
                                    <label>"Compatible Backend"</label>
                                    <select
                                        class="form-control"
                                        on:change=move |ev| set_selected_backend_id.set(event_target_value(&ev))
                                    >
                                        <option value="">"No backend attached"</option>
                                        {move || compatible_backends.get().map(|data| {
                                            data.backends.into_iter().map(|backend| {
                                                let option_value = backend.id.clone();
                                                let label = format!(
                                                    "{} · {} · {}{}",
                                                    backend.name,
                                                    backend.provider,
                                                    backend.region,
                                                    if backend.attached_device_id.is_some() && backend.attached_device_id.as_deref() != Some(edit_id.as_str()) {
                                                        " · attached"
                                                    } else {
                                                        ""
                                                    }
                                                );
                                                let selected_value = option_value.clone();
                                                view! {
                                                    <option value={option_value.clone()} selected=move || selected_backend_id.get() == selected_value>{label}</option>
                                                }
                                            }).collect::<Vec<_>>()
                                        }).unwrap_or_default()}
                                    </select>
                                    <div style="display:flex; gap:0.75rem; margin-top:0.5rem; align-items:center;">
                                        <a href="/virtual-iot/backends" class="btn btn-ghost btn-sm">"Manage Backends"</a>
                                        <Show when=move || is_edit && !selected_backend_id.get().is_empty()>
                                            <a href={move || format!("/virtual-iot/shell?device={}", edit_id)} class="btn btn-primary btn-sm">"Open VeerOS Shell"</a>
                                        </Show>
                                    </div>
                                </div>

                                // ── Sensor Categories ──
                                <div class="sensor-categories">
                                    <h3 style="margin: 0 0 0.5rem 0; font-size: 0.95rem; color: #c9d1d9;">"Sensors"</h3>
                                    {sensor_categories().iter().map(|cat| {
                                        let cat_name = cat.name.clone();
                                        view! {
                                            <div class="sensor-category">
                                                <div class="sensor-category-header">{cat_name}</div>
                                                <div class="sensor-checkbox-group">
                                                    {cat.sensors.iter().map(|sensor| {
                                                        let sid = sensor.id.clone();
                                                        let label = sensor.label.clone();
                                                        let sid_toggle = sid.clone();
                                                        let sid_check = sid.clone();
                                                        view! {
                                                            <label class="sensor-checkbox">
                                                                <input
                                                                    type="checkbox"
                                                                    prop:checked=move || enabled_sensors.get().contains(&sid_check)
                                                                    on:change=move |_| toggle_sensor(sid_toggle.clone())
                                                                />
                                                                <span>{label}</span>
                                                            </label>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>

                                // Sensor count
                                <div style="text-align: right; color: #8b949e; font-size: 0.82rem;">
                                    {move || format!("{} sensor(s) enabled", enabled_sensors.get().len())}
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Inline SVG board preview that swaps based on controller type.
#[component]
fn BoardPreview(controller_type: ReadSignal<String>) -> impl IntoView {
    move || {
        let ct = controller_type.get();
        let profile = find_controller(&ct);
        let chip_label = profile.chip_label.as_str();
        let has_wifi = profile.has_wifi;
        let pin_count = profile.pin_count;
        let board_color = profile.board_color.as_str();
        let board_title = profile.name.as_str();
        view! {
            <svg viewBox="0 0 260 360" class="board-shell-svg" xmlns="http://www.w3.org/2000/svg">
                <defs>
                    <linearGradient id="pcb-sheen" x1="0" y1="0" x2="1" y2="1">
                        <stop offset="0%" stop-color="rgba(255,255,255,0.18)"/>
                        <stop offset="28%" stop-color="rgba(255,255,255,0.02)"/>
                        <stop offset="100%" stop-color="rgba(0,0,0,0.18)"/>
                    </linearGradient>
                    <pattern id="pcb-grid" width="18" height="18" patternUnits="userSpaceOnUse">
                        <path d="M18 0H0V18" fill="none" stroke="rgba(255,255,255,0.03)" stroke-width="1"/>
                    </pattern>
                    <filter id="chip-shadow" x="-20%" y="-20%" width="140%" height="140%">
                        <feDropShadow dx="0" dy="3" stdDeviation="3" flood-color="rgba(0,0,0,0.45)"/>
                    </filter>
                </defs>

                <rect x="10" y="10" width="240" height="340" rx="18" fill={board_color} stroke="#111826" stroke-width="2"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-grid)"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-sheen)"/>

                <circle cx="28" cy="28" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>
                <circle cx="232" cy="28" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>
                <circle cx="28" cy="332" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>
                <circle cx="232" cy="332" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>

                <text x="130" y="30" text-anchor="middle" fill="rgba(255,255,255,0.85)" font-size="8" font-family="sans-serif">{board_title}</text>

                {if let Some(layout_view) = render_catalog_board_layout(profile, chip_label) {
                    layout_view
                } else {
                    view! {
                        <>
                            <rect x="22" y="24" width="216" height="312" rx="12" fill={board_color} stroke="#1f2937" stroke-width="2"/>
                            <rect x="70" y="90" width="120" height="82" rx="5" fill="#111827" stroke="#5f6872" stroke-width="1.2" filter="url(#chip-shadow)"/>
                            <text x="130" y="134" text-anchor="middle" fill="#d1dae2" font-size="9" font-family="monospace">{chip_label}</text>
                            {(0..pin_count).map(|i| {
                                let y = 50 + i * 16;
                                view! {
                                    <>
                                        <rect x="28" y={y} width="12" height="8" rx="1" fill="#d4af37"/>
                                        <rect x="220" y={y} width="12" height="8" rx="1" fill="#d4af37"/>
                                    </>
                                }
                            }).collect::<Vec<_>>()}
                        </>
                    }.into_any()
                }}

                {if has_wifi {
                    view! {
                        <g transform="translate(176, 44)">
                            <rect x="0" y="0" width="54" height="18" rx="4" fill="#0b315a" stroke="#2b6cb0" stroke-width="0.8"/>
                            <text x="27" y="12.5" text-anchor="middle" fill="#a5d8ff" font-size="7" font-family="sans-serif">"Wireless"</text>
                        </g>
                    }.into_any()
                } else {
                    view! { <g/> }.into_any()
                }}
            </svg>
        }
    }
}
