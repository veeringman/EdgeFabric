use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, DeviceListResponse};
use crate::components::status_badge::StatusBadge;

/// Live session viewer — screenshot streaming via WebSocket + input relay.
#[component]
pub fn LiveSessionsPage() -> impl IntoView {
    let (selected_device, set_selected_device) = signal(Option::<(String, String, String)>::None);

    let devices = LocalResource::new(|| async {
        api::get::<DeviceListResponse>("/devices")
            .await
            .unwrap_or_default()
    });

    view! {
        <h1 style="margin-bottom: 1rem">"Live Device Viewer"</h1>

        {move || {
            if let Some((dev_id, dev_name, dev_os)) = selected_device.get() {
                view! {
                    <DeviceViewer
                        device_id=dev_id.clone()
                        device_name=dev_name.clone()
                        device_os=dev_os.clone()
                        on_close=move || set_selected_device.set(None)
                    />
                }.into_any()
            } else {
                view! {
                    <div class="card">
                        <div class="card-header"><h2>"Available Devices"</h2></div>
                        <p style="color: var(--text-muted); font-size: 0.85rem; margin-bottom: 1rem">
                            "Select a device to view its screen in real-time and interact with it."
                        </p>
                        <Suspense fallback=move || view! { <div class="loading">"Loading…"</div> }>
                            {move || devices.get().as_deref().map(|data| {
                                let streamable: Vec<_> = data.devices.iter()
                                    .filter(|d| d.status == "available" || d.status == "busy")
                                    .cloned()
                                    .collect();
                                if streamable.is_empty() {
                                    return view! { <p class="empty-state">"No devices available for streaming"</p> }.into_any();
                                }
                                view! {
                                    <div class="device-grid">
                                        {streamable.into_iter().map(|d| {
                                            let dev_id = d.id.clone();
                                            let dev_name = d.name.clone();
                                            let dev_os = d.os.clone();
                                            let os_icon = if d.os == "android" { "🤖" } else if d.os == "ios" { "🍎" } else if d.os == "iot" { "🔌" } else { "📱" };
                                            let status = d.status.clone();
                                            let os_label = if d.os == "android" { "Android".to_string() }
                                                else if d.os == "ios" { "iOS".to_string() }
                                                else if d.os == "iot" { "IoT".to_string() }
                                                else { d.os.clone() };
                                            let status = d.status.clone();
                                            view! {
                                                <div class="device-card" on:click=move |_| {
                                                    set_selected_device.set(Some((dev_id.clone(), dev_name.clone(), dev_os.clone())));
                                                }>
                                                    <div class="device-card-icon">{os_icon}</div>
                                                    <div class="device-card-name">{d.name.clone()}</div>
                                                    <div class="device-card-info">
                                                        {format!("{} {}", os_label, d.os_version)}
                                                    </div>
                                                    <div class="device-card-serial">{d.serial.clone().unwrap_or_default()}</div>
                                                    <StatusBadge status=status/>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
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

/// Realistic phone-shaped device viewer with touch interaction.
#[component]
fn DeviceViewer(
    device_id: String,
    device_name: String,
    device_os: String,
    on_close: impl Fn() + Send + Sync + 'static,
) -> impl IntoView {
    let img_ref = NodeRef::<leptos::html::Img>::new();
    let (connected, set_connected) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (frame_count, set_frame_count) = signal(0u64);
    let (ws_handle, set_ws_handle) = signal(Option::<web_sys::WebSocket>::None);
    let (resolution, set_resolution) = signal([1080u32, 2340]);
    let (fps, set_fps) = signal(0.0f64);
    let (touch_start, set_touch_start) = signal(Option::<(f64, f64, f64)>::None);
    let (serial_lines, set_serial_lines) = signal(Vec::<String>::new());
    let (shell_input, set_shell_input) = signal(String::new());
    let (cmd_history, set_cmd_history) = signal(Vec::<String>::new());
    let (history_idx, set_history_idx) = signal(Option::<usize>::None);

    let dev_id = device_id.clone();
    let is_android = device_os == "android";
    let is_iot = device_os == "iot";

    // FPS counter
    let last_frame_time = std::cell::Cell::new(0.0f64);
    let fps_counter = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::new()));

    Effect::new(move || {
        let dev_id = dev_id.clone();
        let set_connected = set_connected.clone();
        let set_error = set_error.clone();
        let set_frame_count = set_frame_count.clone();
        let set_ws_handle = set_ws_handle.clone();
        let set_resolution = set_resolution.clone();
        let set_fps = set_fps.clone();
        let fps_counter = fps_counter.clone();

        leptos::task::spawn_local(async move {
            let host = web_sys::window()
                .and_then(|w| w.location().hostname().ok())
                .unwrap_or_else(|| "localhost".to_string());
            let ws_url = format!("ws://{host}:6060/api/v1/devices/{dev_id}/stream");

            let ws = match web_sys::WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(_) => {
                    set_error.set(Some("Failed to create WebSocket".into()));
                    return;
                }
            };
            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

            let ws_clone = ws.clone();
            let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
                set_connected.set(true);
                let msg = serde_json::json!({"type":"start_stream","session_id":""});
                let _ = ws_clone.send_with_str(&msg.to_string());
            }) as Box<dyn FnMut(_)>);
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            onopen.forget();

            let onmessage = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                if let Ok(buf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&buf);
                    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(
                        &js_sys::Array::of1(&array.into()),
                        web_sys::BlobPropertyBag::new().type_("image/jpeg"),
                    ).unwrap();
                    let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();

                    let window = web_sys::window().unwrap();
                    let document = window.document().unwrap();
                    if let Some(img) = document.get_element_by_id("stream-img") {
                        let old_src = img.get_attribute("src").unwrap_or_default();
                        img.set_attribute("src", &url).ok();
                        if !old_src.is_empty() {
                            web_sys::Url::revoke_object_url(&old_src).ok();
                        }
                    }
                    set_frame_count.update(|c| *c += 1);

                    // Calculate FPS
                    let now = js_sys::Date::now();
                    let mut times = fps_counter.borrow_mut();
                    times.push(now);
                    // Keep last 2 seconds of timestamps
                    times.retain(|t| now - t < 2000.0);
                    if times.len() > 1 {
                        set_fps.set(times.len() as f64 / ((now - times[0]) / 1000.0));
                    }
                } else if let Ok(text) = evt.data().dyn_into::<js_sys::JsString>() {
                    let s: String = text.into();
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&s) {
                        match msg.get("type").and_then(|t| t.as_str()) {
                            Some("stream_ready") => {
                                if let Some(res) = msg.get("resolution").and_then(|r| r.as_array()) {
                                    if res.len() == 2 {
                                        let w = res[0].as_u64().unwrap_or(1080) as u32;
                                        let h = res[1].as_u64().unwrap_or(2340) as u32;
                                        set_resolution.set([w, h]);
                                    }
                                }
                            }
                            Some("serial_data") => {
                                if let Some(data) = msg.get("data").and_then(|d| d.as_str()) {
                                    set_serial_lines.update(|lines| {
                                        lines.push(data.to_string());
                                        // Keep last 500 lines
                                        if lines.len() > 500 {
                                            let drain = lines.len() - 500;
                                            lines.drain(..drain);
                                        }
                                    });
                                    set_frame_count.update(|c| *c += 1);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();

            let onerror = Closure::wrap(Box::new(move |_: web_sys::Event| {
                set_error.set(Some("WebSocket connection failed".into()));
            }) as Box<dyn FnMut(_)>);
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            onerror.forget();

            set_ws_handle.set(Some(ws));
        });
    });

    // ── Input helpers ──────────────────────────────────────────────

    let send_input = move |event_type: &str, extra: Option<serde_json::Value>| {
        if let Some(ws) = ws_handle.get_untracked() {
            let mut evt = serde_json::json!({"type": event_type});
            if let Some(extra) = extra {
                if let Some(obj) = extra.as_object() {
                    evt.as_object_mut().unwrap().extend(obj.clone());
                }
            }
            let msg = serde_json::json!({"type":"input_event","event": evt});
            let _ = ws.send_with_str(&msg.to_string());
        }
    };

    // Map screen coordinates from the displayed image to actual device pixels.
    let map_coords = move |client_x: f64, client_y: f64| -> Option<(i32, i32)> {
        let window = web_sys::window()?;
        let document = window.document()?;
        let img = document.get_element_by_id("stream-img")?;
        let rect = img.get_bounding_client_rect();

        let img_w = rect.width();
        let img_h = rect.height();
        if img_w < 1.0 || img_h < 1.0 {
            return None;
        }

        let rel_x = (client_x - rect.left()) / img_w;
        let rel_y = (client_y - rect.top()) / img_h;

        if rel_x < 0.0 || rel_x > 1.0 || rel_y < 0.0 || rel_y > 1.0 {
            return None;
        }

        let [dev_w, dev_h] = resolution.get_untracked();
        // The capture is downscaled to 540px wide, maintain aspect ratio
        let scale_w = dev_w as f64;
        let scale_h = dev_h as f64;

        Some(((rel_x * scale_w) as i32, (rel_y * scale_h) as i32))
    };

    // Mouse/touch event handlers for tap and swipe
    let on_pointer_down = move |evt: web_sys::PointerEvent| {
        evt.prevent_default();
        let now = js_sys::Date::now();
        set_touch_start.set(Some((evt.client_x() as f64, evt.client_y() as f64, now)));
        // Capture pointer for smooth tracking
        if let Some(target) = evt.target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(evt.pointer_id());
            }
        }
    };

    let on_pointer_up = move |evt: web_sys::PointerEvent| {
        evt.prevent_default();
        if let Some((sx, sy, start_time)) = touch_start.get_untracked() {
            let ex = evt.client_x() as f64;
            let ey = evt.client_y() as f64;
            let dx = ex - sx;
            let dy = ey - sy;
            let dist = (dx * dx + dy * dy).sqrt();
            let elapsed = js_sys::Date::now() - start_time;

            if dist < 10.0 {
                // Tap
                if let Some((x, y)) = map_coords(ex, ey) {
                    send_input("tap", Some(serde_json::json!({"x": x, "y": y})));
                }
            } else {
                // Swipe
                if let (Some((x1, y1)), Some((x2, y2))) = (map_coords(sx, sy), map_coords(ex, ey)) {
                    let duration = (elapsed as i64).max(100).min(1000);
                    send_input("swipe", Some(serde_json::json!({
                        "x1": x1, "y1": y1, "x2": x2, "y2": y2, "duration": duration
                    })));
                }
            }
        }
        set_touch_start.set(None);
    };

    let on_pointer_cancel = move |_: web_sys::PointerEvent| {
        set_touch_start.set(None);
    };

    let send_home = move |_| send_input("home", None);
    let send_back = move |_| send_input("back", None);
    let send_recent = move |_| send_input("recent", None);
    let send_power = move |_| send_input("key", Some(serde_json::json!({"keycode": "KEYCODE_POWER"})));
    let send_vol_up = move |_| send_input("key", Some(serde_json::json!({"keycode": "KEYCODE_VOLUME_UP"})));
    let send_vol_down = move |_| send_input("key", Some(serde_json::json!({"keycode": "KEYCODE_VOLUME_DOWN"})));

    let full_id = device_id.clone();

    view! {
        <div class="viewer-layout">
            // ── IoT serial terminal OR Phone simulator ──
            {if is_iot {
                view! {
                    <div class="card" style="flex: 1; min-width: 0;">
                        <div class="card-header">
                            <h2>"🔌 Serial Console"</h2>
                            <button class="btn btn-ghost btn-sm" on:click=move |_| {
                                if let Some(ws) = ws_handle.get_untracked() {
                                    let _ = ws.close();
                                }
                                on_close();
                            }>"✕ Close"</button>
                        </div>
                        <Show
                            when=move || connected.get()
                            fallback=|| view! {
                                <div class="loading">"Connecting to serial port…"</div>
                            }
                        >
                            <div class="log-viewer" id="iot-terminal" style="max-height: 600px; min-height: 400px; overflow-y: auto; font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace; font-size: 0.82rem; background: #0d1117; color: #c9d1d9; padding: 0.5rem; border-radius: 6px;">
                                {move || {
                                    // Auto-scroll to bottom
                                    if let Some(w) = web_sys::window() {
                                        if let Some(d) = w.document() {
                                            if let Some(el) = d.get_element_by_id("iot-terminal") {
                                                let _ = el.set_scroll_top(el.scroll_height());
                                            }
                                        }
                                    }
                                    serial_lines.get().into_iter().map(|line| {
                                        view! { <div class="log-line" style="white-space: pre-wrap; word-break: break-all; line-height: 1.5; padding: 1px 0;">{line}</div> }
                                    }).collect::<Vec<_>>()
                                }}
                            </div>
                            // ── Shell command input ──
                            <div style="display: flex; gap: 0.5rem; margin-top: 0.5rem; align-items: center;">
                                <span style="color: #58a6ff; font-family: monospace; font-weight: 600; flex-shrink: 0;">"$"</span>
                                <input
                                    type="text"
                                    class="form-control"
                                    placeholder="Type a command…"
                                    style="flex: 1; font-family: 'JetBrains Mono', monospace; font-size: 0.82rem; background: #161b22; color: #c9d1d9; border: 1px solid #30363d; border-radius: 4px; padding: 0.4rem 0.6rem;"
                                    prop:value=move || shell_input.get()
                                    on:input=move |ev| {
                                        set_shell_input.set(event_target_value(&ev));
                                        set_history_idx.set(None);
                                    }
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        if ev.key() == "Enter" {
                                            let cmd = shell_input.get_untracked();
                                            if !cmd.trim().is_empty() {
                                                if let Some(ws) = ws_handle.get_untracked() {
                                                    let msg = serde_json::json!({
                                                        "type": "send_text",
                                                        "data": format!("{}\r\n", cmd)
                                                    });
                                                    let _ = ws.send_with_str(&msg.to_string());
                                                }
                                                set_cmd_history.update(|h| h.push(cmd));
                                                set_shell_input.set(String::new());
                                                set_history_idx.set(None);
                                            }
                                        } else if ev.key() == "ArrowUp" {
                                            ev.prevent_default();
                                            let hist = cmd_history.get_untracked();
                                            if !hist.is_empty() {
                                                let idx = match history_idx.get_untracked() {
                                                    Some(i) if i > 0 => i - 1,
                                                    Some(0) => 0,
                                                    _ => hist.len() - 1,
                                                };
                                                set_shell_input.set(hist[idx].clone());
                                                set_history_idx.set(Some(idx));
                                            }
                                        } else if ev.key() == "ArrowDown" {
                                            ev.prevent_default();
                                            let hist = cmd_history.get_untracked();
                                            if let Some(i) = history_idx.get_untracked() {
                                                if i + 1 < hist.len() {
                                                    set_shell_input.set(hist[i + 1].clone());
                                                    set_history_idx.set(Some(i + 1));
                                                } else {
                                                    set_shell_input.set(String::new());
                                                    set_history_idx.set(None);
                                                }
                                            }
                                        }
                                    }
                                />
                                <button
                                    class="btn btn-primary btn-sm"
                                    style="flex-shrink: 0;"
                                    on:click=move |_| {
                                        let cmd = shell_input.get_untracked();
                                        if !cmd.trim().is_empty() {
                                            if let Some(ws) = ws_handle.get_untracked() {
                                                let msg = serde_json::json!({
                                                    "type": "send_text",
                                                    "data": format!("{}\r\n", cmd)
                                                });
                                                let _ = ws.send_with_str(&msg.to_string());
                                            }
                                            set_cmd_history.update(|h| h.push(cmd));
                                            set_shell_input.set(String::new());
                                            set_history_idx.set(None);
                                        }
                                    }
                                >"Send"</button>
                            </div>
                        </Show>
                        <div class="stream-info" style="margin-top: 0.75rem;">
                            <div class="info-row">
                                <span class="info-label">"Device"</span>
                                <span>{device_name.clone()}</span>
                            </div>
                            <div class="info-row">
                                <span class="info-label">"Status"</span>
                                {move || if connected.get() {
                                    view! { <span class="badge active">"● Connected"</span> }.into_any()
                                } else {
                                    view! { <span class="badge queued">"○ Connecting…"</span> }.into_any()
                                }}
                            </div>
                            <div class="info-row">
                                <span class="info-label">"Lines"</span>
                                <span>{move || frame_count.get()}</span>
                            </div>
                            <div class="info-row">
                                <span class="info-label">"ID"</span>
                                <span class="mono" style="font-size: 0.75rem">{full_id}</span>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
            view! {
                <>
            // ── Phone simulator ──
            <div class="phone-wrapper">
                <div class={if is_android { "phone-frame android" } else { "phone-frame ios" }}>
                    // Top bezel (notch / camera area)
                    <div class="phone-notch">
                        <div class="phone-camera"></div>
                    </div>

                    // Screen area — interactive
                    <div class="phone-screen"
                        on:pointerdown=on_pointer_down
                        on:pointerup=on_pointer_up
                        on:pointercancel=on_pointer_cancel
                        style="touch-action: none; cursor: crosshair;"
                    >
                        <Show
                            when=move || connected.get()
                            fallback=|| view! {
                                <div class="phone-screen-placeholder">
                                    <div class="spinner"></div>
                                    <span>"Connecting to device…"</span>
                                </div>
                            }
                        >
                            <img id="stream-img" node_ref=img_ref
                                 draggable="false"
                                 style="width: 100%; height: 100%; object-fit: cover; user-select: none; -webkit-user-drag: none;"
                                 alt="Device screen"
                            />
                        </Show>
                    </div>

                    // Bottom nav bar (Android) or home indicator (iOS)
                    {if is_android {
                        view! {
                            <div class="phone-navbar android-navbar">
                                <button class="nav-btn" on:click=send_back title="Back">
                                    <svg viewBox="0 0 24 24" width="22" height="22"><path d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z" fill="currentColor"/></svg>
                                </button>
                                <button class="nav-btn nav-home" on:click=send_home title="Home">
                                    <svg viewBox="0 0 24 24" width="24" height="24"><circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="2"/></svg>
                                </button>
                                <button class="nav-btn" on:click=send_recent title="Recent Apps">
                                    <svg viewBox="0 0 24 24" width="20" height="20"><rect x="4" y="4" width="16" height="16" rx="2" fill="none" stroke="currentColor" stroke-width="2"/></svg>
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="phone-navbar ios-navbar">
                                <div class="ios-home-indicator"></div>
                            </div>
                        }.into_any()
                    }}
                </div>

                // Side buttons
                <div class="phone-side-buttons">
                    <button class="side-btn power-btn" on:click=send_power title="Power">"⏻"</button>
                    <button class="side-btn vol-btn" on:click=send_vol_up title="Volume Up">"+"</button>
                    <button class="side-btn vol-btn" on:click=send_vol_down title="Volume Down">"−"</button>
                </div>
            </div>

            // ── Info panel ──
            <div class="viewer-panel">
                <div class="card">
                    <div class="card-header">
                        <h2>{device_name.clone()}</h2>
                        <button class="btn btn-ghost btn-sm" on:click=move |_| {
                            if let Some(ws) = ws_handle.get_untracked() {
                                let _ = ws.close();
                            }
                            on_close();
                        }>"✕ Close"</button>
                    </div>

                    <Show when=move || error.get().is_some()>
                        <div class="alert error">{move || error.get().unwrap_or_default()}</div>
                    </Show>

                    <div class="stream-info">
                        <div class="info-row">
                            <span class="info-label">"Status"</span>
                            {move || if connected.get() {
                                view! { <span class="badge active">"● Connected"</span> }.into_any()
                            } else {
                                view! { <span class="badge queued">"○ Connecting…"</span> }.into_any()
                            }}
                        </div>
                        <div class="info-row">
                            <span class="info-label">"Device"</span>
                            <span>{device_name}</span>
                        </div>
                        <div class="info-row">
                            <span class="info-label">"ID"</span>
                            <span class="mono" style="font-size: 0.75rem">{full_id}</span>
                        </div>
                        <div class="info-row">
                            <span class="info-label">"Resolution"</span>
                            <span>{move || { let r = resolution.get(); format!("{}×{}", r[0], r[1]) }}</span>
                        </div>
                        <div class="info-row">
                            <span class="info-label">"Frames"</span>
                            <span>{move || frame_count.get()}</span>
                        </div>
                        <div class="info-row">
                            <span class="info-label">"FPS"</span>
                            <span>{move || format!("{:.1}", fps.get())}</span>
                        </div>
                    </div>
                </div>

                <div class="card" style="margin-top: 1rem">
                    <div class="card-header"><h2>"Interaction Guide"</h2></div>
                    <div class="interaction-guide">
                        <div class="guide-item">
                            <span class="guide-icon">"👆"</span>
                            <div>
                                <strong>"Tap"</strong>
                                <p>"Click on the screen"</p>
                            </div>
                        </div>
                        <div class="guide-item">
                            <span class="guide-icon">"👆→"</span>
                            <div>
                                <strong>"Swipe"</strong>
                                <p>"Click and drag"</p>
                            </div>
                        </div>
                        <div class="guide-item">
                            <span class="guide-icon">"◀ ● ■"</span>
                            <div>
                                <strong>"Navigation"</strong>
                                <p>"Use the bottom bar"</p>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
                </>
            }.into_any()
            }}
        </div>
    }
}
