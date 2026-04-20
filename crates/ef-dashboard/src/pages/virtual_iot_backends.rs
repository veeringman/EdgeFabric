use leptos::prelude::*;

use crate::api::{self, CreateVirtualIotBackendRequest, VirtualIotBackendEnvelope, VirtualIotBackendListResponse};
use crate::components::status_badge::StatusBadge;

#[component]
pub fn VirtualIotBackendsPage() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);
    let (name, set_name) = signal(String::new());
    let (controller_type, set_controller_type) = signal("esp32-devkit".to_string());
    let (region, set_region) = signal("local".to_string());
    let (auto_start, set_auto_start) = signal(true);
    let (saving, set_saving) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let backends = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::get::<VirtualIotBackendListResponse>("/devices/virtual-iot/backends?available_only=false")
                .await
                .unwrap_or_default()
        }
    });

    let create_backend = move |_| {
        let backend_name = name.get_untracked();
        if backend_name.trim().is_empty() {
            set_error.set(Some("Backend name is required".into()));
            return;
        }
        set_saving.set(true);
        set_error.set(None);

        let request = CreateVirtualIotBackendRequest {
            name: backend_name,
            provider: "veeros-vm".to_string(),
            controller_type: controller_type.get_untracked(),
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

    view! {
        <div style="display:flex; flex-direction:column; gap:1rem;">
            <div style="display:flex; justify-content:space-between; align-items:center; gap:1rem;">
                <div>
                    <h1 style="margin-bottom:0.25rem;">"Virtual IoT Backends"</h1>
                    <p class="empty-state" style="margin:0; text-align:left;">"Provision VeerOS ESP32 backends here, then attach compatible ones from the IoT designer."</p>
                </div>
                <a href="/virtual-iot/new" class="btn btn-primary btn-sm">"Open IoT Designer"</a>
            </div>

            <div class="card">
                <div class="card-header"><h2>"Create Backend"</h2></div>
                <div style="padding:1rem; display:grid; grid-template-columns:repeat(5, minmax(0, 1fr)); gap:0.75rem; align-items:end;">
                    <div class="form-group">
                        <label>"Name"</label>
                        <input class="form-control" prop:value=move || name.get() on:input=move |ev| set_name.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-group">
                        <label>"Controller"</label>
                        <select class="form-control" on:change=move |ev| set_controller_type.set(event_target_value(&ev))>
                            <option value="esp32-devkit">"ESP32 DevKit"</option>
                            <option value="arduino-uno">"Arduino Uno"</option>
                            <option value="rpi-pico">"Raspberry Pi Pico"</option>
                            <option value="stm32-nucleo">"STM32 Nucleo"</option>
                            <option value="generic-mcu">"Generic MCU"</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>"Region"</label>
                        <input class="form-control" prop:value=move || region.get() on:input=move |ev| set_region.set(event_target_value(&ev)) />
                    </div>
                    <label style="display:flex; align-items:center; gap:0.5rem; margin-bottom:0.5rem; color:var(--text-muted);">
                        <input type="checkbox" prop:checked=move || auto_start.get() on:change=move |_| set_auto_start.update(|v| *v = !*v) />
                        <span>"Auto-start backend"</span>
                    </label>
                    <button class="btn btn-primary btn-sm" on:click=create_backend disabled=move || saving.get()>
                        {move || if saving.get() { "Provisioning…" } else { "Create Backend" }}
                    </button>
                </div>
                <Show when=move || error.get().is_some()>
                    <div class="alert error" style="margin:0 1rem 1rem 1rem;">{move || error.get().unwrap_or_default()}</div>
                </Show>
            </div>

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
                                            <th>"Controller"</th>
                                            <th>"Provider"</th>
                                            <th>"Region"</th>
                                            <th>"Status"</th>
                                            <th>"VM"</th>
                                            <th>"Shell"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {data.backends.into_iter().map(|backend| {
                                            let shell_text = match (backend.shell_host.clone(), backend.shell_port) {
                                                (Some(host), Some(port)) => format!("{}:{}", host, port),
                                                _ => "—".to_string(),
                                            };
                                            view! {
                                                <tr>
                                                    <td><strong>{backend.name}</strong></td>
                                                    <td>{backend.controller_type}</td>
                                                    <td>{backend.provider}</td>
                                                    <td>{backend.region}</td>
                                                    <td><StatusBadge status=backend.status.clone()/></td>
                                                    <td class="mono truncate">{backend.vm_name.unwrap_or_else(|| "—".into())}</td>
                                                    <td class="mono truncate">{shell_text}</td>
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