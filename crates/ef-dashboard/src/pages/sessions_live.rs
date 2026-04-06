use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, SessionListResponse, DeviceListResponse};
use crate::components::status_badge::StatusBadge;

fn truncate_id(s: &str) -> String {
    s.chars().take(8).collect()
}

/// Live session viewer — screenshot streaming via WebSocket + input relay.
#[component]
pub fn LiveSessionsPage() -> impl IntoView {
    let (selected_device, set_selected_device) = signal(Option::<(String, String)>::None);

    let devices = LocalResource::new(|| async {
        api::get::<DeviceListResponse>("/devices")
            .await
            .unwrap_or_default()
    });

    view! {
        <h1 style="margin-bottom: 1rem">"Live Device Viewer"</h1>

        {move || {
            if let Some((dev_id, dev_name)) = selected_device.get() {
                view! {
                    <DeviceViewer
                        device_id=dev_id.clone()
                        device_name=dev_name.clone()
                        on_close=move || set_selected_device.set(None)
                    />
                }.into_any()
            } else {
                view! {
                    <div class="card">
                        <div class="card-header"><h2>"Available Devices"</h2></div>
                        <p style="color: var(--text-muted); font-size: 0.85rem; margin-bottom: 1rem">
                            "Select an Android device to view its screen in real-time."
                        </p>
                        <Suspense fallback=move || view! { <div class="loading">"Loading…"</div> }>
                            {move || devices.get().as_deref().map(|data| {
                                let android_devices: Vec<_> = data.devices.iter()
                                    .filter(|d| d.os == "android" && (d.status == "available" || d.status == "busy"))
                                    .cloned()
                                    .collect();
                                if android_devices.is_empty() {
                                    return view! { <p class="empty-state">"No Android devices available for streaming"</p> }.into_any();
                                }
                                view! {
                                    <div class="table-wrap">
                                        <table>
                                            <thead><tr>
                                                <th>"Device"</th><th>"Serial"</th><th>"OS"</th><th>"Status"</th><th></th>
                                            </tr></thead>
                                            <tbody>
                                                {android_devices.into_iter().map(|d| {
                                                    let dev_id = d.id.clone();
                                                    let dev_name = d.name.clone();
                                                    let serial = d.serial.clone().unwrap_or_default();
                                                    let status = d.status.clone();
                                                    view! {
                                                        <tr>
                                                            <td>{d.name.clone()}</td>
                                                            <td class="mono">{serial}</td>
                                                            <td>{format!("Android {}", d.os_version)}</td>
                                                            <td><StatusBadge status=status/></td>
                                                            <td>
                                                                <button class="btn btn-primary btn-sm"
                                                                    on:click=move |_| set_selected_device.set(Some((dev_id.clone(), dev_name.clone())))>
                                                                    "▶ Stream"
                                                                </button>
                                                            </td>
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
                }.into_any()
            }
        }}
    }
}

/// Screenshot-based device live viewer with input controls.
#[component]
fn DeviceViewer(
    device_id: String,
    device_name: String,
    on_close: impl Fn() + Send + Sync + 'static,
) -> impl IntoView {
    let img_ref = NodeRef::<leptos::html::Img>::new();
    let (connected, set_connected) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (frame_count, set_frame_count) = signal(0u64);
    let (ws_handle, set_ws_handle) = signal(Option::<web_sys::WebSocket>::None);

    let dev_id = device_id.clone();
    Effect::new(move || {
        let dev_id = dev_id.clone();
        let set_connected = set_connected.clone();
        let set_error = set_error.clone();
        let set_frame_count = set_frame_count.clone();
        let set_ws_handle = set_ws_handle.clone();

        leptos::task::spawn_local(async move {
            let ws_url = format!("ws://localhost:6060/api/v1/devices/{dev_id}/stream");

            let ws = match web_sys::WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(_) => {
                    set_error.set(Some("Failed to create WebSocket".into()));
                    return;
                }
            };
            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

            // On open — send start_stream
            let ws_clone = ws.clone();
            let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
                set_connected.set(true);
                let msg = serde_json::json!({"type":"start_stream","session_id":""});
                let _ = ws_clone.send_with_str(&msg.to_string());
            }) as Box<dyn FnMut(_)>);
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            onopen.forget();

            // On message — binary = screenshot frame, text = control message
            let onmessage = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                // Binary frame — PNG screenshot
                if let Ok(buf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&buf);
                    let bytes = array.to_vec();
                    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(
                        &js_sys::Array::of1(&array.into()),
                        web_sys::BlobPropertyBag::new().type_("image/png"),
                    ).unwrap();
                    let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();

                    // Update image src
                    let window = web_sys::window().unwrap();
                    let document = window.document().unwrap();
                    if let Some(img) = document.get_element_by_id("stream-img") {
                        let old_src = img.get_attribute("src").unwrap_or_default();
                        img.set_attribute("src", &url).ok();
                        // Revoke old blob URL to prevent memory leak
                        if !old_src.is_empty() {
                            web_sys::Url::revoke_object_url(&old_src).ok();
                        }
                    }
                    set_frame_count.update(|c| *c += 1);
                }
            }) as Box<dyn FnMut(_)>);
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();

            let onerror = Closure::wrap(Box::new(move |_: web_sys::Event| {
                set_error.set(Some("WebSocket connection failed. Is the API server running?".into()));
            }) as Box<dyn FnMut(_)>);
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            onerror.forget();

            set_ws_handle.set(Some(ws));
        });
    });

    let send_input = move |event_type: &str, extra: Option<serde_json::Value>| {
        if let Some(ws) = ws_handle.get_untracked() {
            let mut evt = serde_json::json!({"type": event_type});
            if let Some(extra) = extra {
                evt.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            }
            let msg = serde_json::json!({"type":"input_event","event": evt});
            let _ = ws.send_with_str(&msg.to_string());
        }
    };

    let send_home = move |_| send_input("home", None);
    let send_back = move |_| send_input("back", None);
    let send_recent = move |_| send_input("recent", None);

    let full_id = device_id.clone();

    view! {
        <div class="session-layout">
            <div>
                <div class="card" style="margin-bottom: 1rem">
                    <div class="card-header">
                        <h2>{device_name.clone()}</h2>
                        <div>
                            <button class="btn btn-ghost btn-sm" on:click=move |_| {
                                // Close WebSocket on exit
                                if let Some(ws) = ws_handle.get_untracked() {
                                    let _ = ws.close();
                                }
                                on_close();
                            }>"✕ Close"</button>
                        </div>
                    </div>

                    <Show when=move || error.get().is_some()>
                        <div class="alert error">{move || error.get().unwrap_or_default()}</div>
                    </Show>

                    <div class="video-container" style="background: #000; text-align: center; min-height: 400px; display: flex; align-items: center; justify-content: center;">
                        <img id="stream-img" node_ref=img_ref
                             style="max-width: 100%; max-height: 70vh; object-fit: contain;"
                             alt="Device screen"
                        />
                    </div>
                </div>

                <div class="card">
                    <div class="card-header"><h2>"Input Controls"</h2></div>
                    <p style="color: var(--text-muted); font-size: 0.85rem">
                        "Send navigation commands to the device."
                    </p>
                    <div style="display: flex; gap: 0.5rem; margin-top: 0.75rem; flex-wrap: wrap">
                        <button class="btn btn-ghost btn-sm" on:click=send_home>"🏠 Home"</button>
                        <button class="btn btn-ghost btn-sm" on:click=send_back>"◀ Back"</button>
                        <button class="btn btn-ghost btn-sm" on:click=send_recent>"⧉ Recent"</button>
                    </div>
                </div>
            </div>

            <div>
                <div class="card">
                    <div class="card-header"><h2>"Stream Info"</h2></div>
                    <div style="font-size: 0.85rem; display: flex; flex-direction: column; gap: 0.5rem">
                        <div><strong>"Device: "</strong>{device_name}</div>
                        <div><strong>"ID: "</strong><span class="mono">{full_id}</span></div>
                        <div>
                            <strong>"Status: "</strong>
                            {move || if connected.get() {
                                view! { <span class="badge active">"Connected"</span> }.into_any()
                            } else {
                                view! { <span class="badge queued">"Connecting…"</span> }.into_any()
                            }}
                        </div>
                        <div><strong>"Frames: "</strong>{move || frame_count.get()}</div>
                    </div>
                </div>
            </div>
        </div>
    }
}
