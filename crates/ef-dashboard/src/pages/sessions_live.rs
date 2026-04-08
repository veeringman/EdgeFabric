use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, DeviceListResponse};
use crate::components::status_badge::StatusBadge;

/// Device info passed from card selection to the viewer.
#[derive(Clone, Debug)]
struct SelectedDevice {
    id: String,
    name: String,
    os: String,
    os_version: String,
    serial: Option<String>,
    kind: String,
    capabilities: Vec<String>,
}

/// Extract the bezel style from capabilities (e.g. "bezel:iphone-modern" → "iphone-modern").
/// Returns a sensible default based on OS version if not set.
fn bezel_from_caps(caps: &[String], os: &str, os_version: &str) -> String {
    for cap in caps {
        if let Some(val) = cap.strip_prefix("bezel:") {
            return val.to_string();
        }
    }
    // Auto-detect from OS version for iOS
    if os == "ios" {
        let ver: f32 = os_version.split('.').next().unwrap_or("0").parse().unwrap_or(0.0);
        if ver >= 16.0 { return "iphone-island".into(); }
        if ver >= 14.0 { return "iphone-modern".into(); }
        if ver >= 11.0 { return "iphone-notch".into(); }
        return "iphone-classic".into();
    }
    // Auto-detect from device name / manufacturer for Android
    if os == "android" {
        let name_lower: String = caps.iter()
            .find(|c| c.starts_with("model:"))
            .map(|c| c[6..].to_lowercase())
            .unwrap_or_default();
        if name_lower.contains("pixel") { return "pixel".into(); }
        if name_lower.contains("oneplus") { return "oneplus".into(); }
        if name_lower.contains("xiaomi") || name_lower.contains("redmi") { return "xiaomi".into(); }
        if name_lower.contains("nothing") { return "nothing".into(); }
        if name_lower.contains("moto") || name_lower.contains("razr") { return "motorola".into(); }
        if name_lower.contains("xperia") { return "xperia".into(); }
        if name_lower.contains("huawei") || name_lower.contains("mate") { return "huawei".into(); }
        if name_lower.contains("oppo") || name_lower.contains("realme") || name_lower.contains("find") { return "oppo".into(); }
        if name_lower.contains("rog") || name_lower.contains("asus") { return "rog".into(); }
        if name_lower.contains("fold") || name_lower.contains("sm-f9") { return "galaxy-fold".into(); }
        if name_lower.contains("flip") || name_lower.contains("sm-f7") { return "galaxy-flip".into(); }
        if name_lower.contains("sm-a") { return "galaxy-a".into(); }
        return "galaxy-s".into(); // default Android
    }
    "galaxy-s".into()
}

/// Live session viewer — screenshot streaming via WebSocket + input relay.
#[component]
pub fn LiveSessionsPage() -> impl IntoView {
    let (selected_device, set_selected_device) = signal(Option::<SelectedDevice>::None);
    let (refresh, set_refresh) = signal(0u32);

    // Auto-refresh device list every 5 seconds
    Effect::new(move || {
        let handle = gloo_timers::callback::Interval::new(5_000, move || {
            set_refresh.update(|n| *n = n.wrapping_add(1));
        });
        std::mem::forget(handle);
    });

    let devices = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::get::<DeviceListResponse>("/devices")
                .await
                .unwrap_or_default()
        }
    });

    view! {
        <h1 style="margin-bottom: 1rem">"Live Device Viewer"</h1>

        {move || {
            if let Some(sel) = selected_device.get() {
                view! {
                    <DeviceViewer
                        device_id=sel.id.clone()
                        device_name=sel.name.clone()
                        device_os=sel.os.clone()
                        device_os_version=sel.os_version.clone()
                        device_serial=sel.serial.clone()
                        device_kind=sel.kind.clone()
                        device_capabilities=sel.capabilities.clone()
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
                        <Transition fallback=move || view! { <div class="loading">"Loading…"</div> }>
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
                                            let caps: Vec<String> = d.capabilities.as_array()
                                                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                                .unwrap_or_default();
                                            let sel = SelectedDevice {
                                                id: d.id.clone(),
                                                name: d.name.clone(),
                                                os: d.os.clone(),
                                                os_version: d.os_version.clone(),
                                                serial: d.serial.clone(),
                                                kind: d.kind.clone(),
                                                capabilities: caps,
                                            };
                                            let os_icon = if d.os == "android" { "🤖" } else if d.os == "ios" { "🍎" } else if d.os == "iot" { "🔌" } else { "📱" };
                                            let status = d.status.clone();
                                            let os_label = if d.os == "android" { "Android".to_string() }
                                                else if d.os == "ios" { "iOS".to_string() }
                                                else if d.os == "iot" { "IoT".to_string() }
                                                else { d.os.clone() };
                                            let status = d.status.clone();
                                            view! {
                                                <div class="device-card" on:click=move |_| {
                                                    set_selected_device.set(Some(sel.clone()));
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
                        </Transition>
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
    device_os_version: String,
    device_serial: Option<String>,
    device_kind: String,
    device_capabilities: Vec<String>,
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

    // Log panel signals
    let (log_lines, set_log_lines) = signal(Vec::<(String, String, String)>::new()); // (level, tag, message)
    let (log_open, set_log_open) = signal(false);
    let (log_connected, set_log_connected) = signal(false);
    let (log_filter, set_log_filter) = signal(String::from("all"));
    let (log_ws_handle, set_log_ws_handle) = signal(Option::<web_sys::WebSocket>::None);

    let initial_bezel = bezel_from_caps(&device_capabilities, &device_os, &device_os_version);
    let (bezel_style, set_bezel_style) = signal(initial_bezel);

    let dev_id = device_id.clone();
    let log_dev_id = device_id.clone();
    let is_android = device_os == "android";
    let is_ios = device_os == "ios";
    let is_iot = device_os == "iot";

    // Pre-clone for branches that need them
    let iot_os_label = format!("{} {}", device_os, device_os_version);
    let phone_os_label = iot_os_label.clone();
    let iot_serial_display = device_serial.clone().unwrap_or_else(|| "—".into());
    let phone_serial_display = device_serial.unwrap_or_else(|| "—".into());
    let phone_kind = device_kind.clone();
    let iot_name = device_name.clone();
    let phone_name = device_name.clone();
    let phone_name2 = device_name;
    let bezel_dev_id = device_id.clone();

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

    // ── Log WebSocket (connects when log_open toggles on) ─────────

    Effect::new(move || {
        let open = log_open.get();
        if !open {
            // Closing: disconnect
            if let Some(ws) = log_ws_handle.get_untracked() {
                let _ = ws.close();
            }
            set_log_ws_handle.set(None);
            set_log_connected.set(false);
            return;
        }

        let log_id = log_dev_id.clone();
        let set_log_connected = set_log_connected.clone();
        let set_log_lines = set_log_lines.clone();
        let set_log_ws_handle = set_log_ws_handle.clone();

        leptos::task::spawn_local(async move {
            let host = web_sys::window()
                .and_then(|w| w.location().hostname().ok())
                .unwrap_or_else(|| "localhost".to_string());
            let ws_url = format!("ws://{host}:6060/api/v1/devices/{log_id}/logs");

            let ws = match web_sys::WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(_) => return,
            };

            let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
                set_log_connected.set(true);
            }) as Box<dyn FnMut(_)>);
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            onopen.forget();

            let onmessage = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                if let Ok(text) = evt.data().dyn_into::<js_sys::JsString>() {
                    let s: String = text.into();
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&s) {
                        if msg.get("type").and_then(|t| t.as_str()) == Some("log") {
                            let level = msg.get("level").and_then(|v| v.as_str()).unwrap_or("info").to_string();
                            let tag = msg.get("tag").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let message = msg.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            set_log_lines.update(|lines| {
                                lines.push((level, tag, message));
                                if lines.len() > 1000 {
                                    lines.drain(..300);
                                }
                            });
                            // Auto-scroll
                            if let Some(el) = web_sys::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.get_element_by_id("inline-log-viewer"))
                            {
                                el.set_scroll_top(el.scroll_height());
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();

            set_log_ws_handle.set(Some(ws));
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
                                <span>{iot_name}</span>
                            </div>
                            <div class="info-row">
                                <span class="info-label">"OS"</span>
                                <span>{iot_os_label}</span>
                            </div>
                            <div class="info-row">
                                <span class="info-label">"Serial"</span>
                                <span class="mono" style="font-size: 0.75rem">{iot_serial_display}</span>
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
                <div class={move || if is_android { format!("phone-frame android {}", bezel_style.get()) } else { format!("phone-frame ios {}", bezel_style.get()) }}>
                    // Top bezel
                    {if is_android {
                        view! {
                            <div class={move || format!("phone-notch {}", match bezel_style.get().as_str() {
                                "pixel" => "android-punch-hole pixel-bar",
                                "galaxy-fold" => "android-punch-hole",
                                "rog" => "android-punch-hole",
                                _ => "android-punch-hole",
                            })}>
                                {move || match bezel_style.get().as_str() {
                                    "pixel" => view! { <div class="pixel-camera-dot"></div> }.into_any(),
                                    _ => view! { <div class="android-hole-punch"></div> }.into_any(),
                                }}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class={move || format!("phone-notch {}", match bezel_style.get().as_str() {
                                "iphone-classic" => "ios-classic-top",
                                "iphone-notch" | "iphone-modern" => "ios-notch",
                                _ => "ios-notch",
                            })}>
                                {move || match bezel_style.get().as_str() {
                                    "iphone-classic" => view! { <div class="phone-speaker"></div> }.into_any(),
                                    "iphone-island" => view! { <div class="dynamic-island"></div> }.into_any(),
                                    _ => view! { <div class="ios-notch-cutout"></div> }.into_any(),
                                }}
                            </div>
                        }.into_any()
                    }}

                    // Screen area — interactive
                    <div class="phone-screen"
                        on:pointerdown=on_pointer_down
                        on:pointerup=on_pointer_up
                        on:pointercancel=on_pointer_cancel
                        style="touch-action: none; cursor: crosshair;"
                    >
                        <Show
                            when=move || connected.get()
                            fallback=move || {
                                if is_ios {
                                    view! {
                                        <div class="phone-screen-placeholder ios-boot">
                                            <span class="apple-logo">"" </span>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="phone-screen-placeholder">
                                            <div class="spinner"></div>
                                            <span>"Connecting to device…"</span>
                                        </div>
                                    }.into_any()
                                }
                            }
                        >
                            <img id="stream-img" node_ref=img_ref
                                 draggable="false"
                                 style="width: 100%; height: 100%; object-fit: cover; user-select: none; -webkit-user-drag: none; color: transparent;"
                                 alt=""
                            />
                            // Overlay until first frame arrives
                            <Show when=move || frame_count.get() == 0>
                                {if is_ios {
                                    view! {
                                        <div class="phone-screen-placeholder ios-boot" style="position:absolute;inset:0;z-index:2;">
                                            <span class="apple-logo">"" </span>
                                            <div class="ios-progress-bar">
                                                <div class="ios-progress-fill"></div>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="phone-screen-placeholder" style="position:absolute;inset:0;z-index:2;">
                                            <div class="spinner"></div>
                                            <span>"Waiting for stream…"</span>
                                        </div>
                                    }.into_any()
                                }}
                            </Show>
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
                            {move || match bezel_style.get().as_str() {
                                "iphone-classic" => view! {
                                    <div class="phone-navbar ios-navbar classic-navbar">
                                        <div class="ios-home-button"></div>
                                    </div>
                                }.into_any(),
                                _ => view! {
                                    <div class="phone-navbar ios-navbar">
                                        <div class="ios-home-indicator"></div>
                                    </div>
                                }.into_any(),
                            }}
                        }.into_any()
                    }}
                </div>

                // Side buttons
                {if is_android {
                    view! {
                        <div class="phone-side-buttons side-right">
                            <button class="side-btn power-btn" on:click=send_power title="Power">"⏻"</button>
                            <button class="side-btn vol-btn" on:click=send_vol_up title="Volume Up">"+"</button>
                            <button class="side-btn vol-btn" on:click=send_vol_down title="Volume Down">"−"</button>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <>
                        // iOS: power button on the right
                        <div class="phone-side-buttons side-right">
                            <button class="side-btn power-btn ios-power" on:click=send_power title="Side Button">"⏻"</button>
                        </div>
                        // iOS: volume + action button on the left
                        <div class="phone-side-buttons side-left">
                            <button class="side-btn ios-action-btn" title="Action Button">"⊚"</button>
                            <button class="side-btn vol-btn" on:click=send_vol_up title="Volume Up">"+"</button>
                            <button class="side-btn vol-btn" on:click=send_vol_down title="Volume Down">"−"</button>
                        </div>
                        </>
                    }.into_any()
                }}
            </div>

            // ── Info panel ──
            <div class="viewer-panel">
                <div class="card">
                    <div class="card-header">
                        <h2>{phone_name2}</h2>
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
                            <span>{phone_name}</span>
                        </div>
                        <div class="info-row">
                            <span class="info-label">"OS"</span>
                            <span>{phone_os_label}</span>
                        </div>
                        <div class="info-row">
                            <span class="info-label">"Kind"</span>
                            <span>{phone_kind}</span>
                        </div>
                        {{
                            let dev_id_for_bezel = bezel_dev_id.clone();
                            if is_ios {
                                view! {
                                    <div class="info-row">
                                        <span class="info-label">"Bezel"</span>
                                        <select class="bezel-select"
                                            on:change=move |ev| {
                                                let val = event_target_value(&ev);
                                                set_bezel_style.set(val.clone());
                                                let dev_id = dev_id_for_bezel.clone();
                                                leptos::task::spawn_local(async move {
                                                    let body = serde_json::json!({"key": "bezel", "value": val});
                                                    let _ = api::put::<_, serde_json::Value>(
                                                        &format!("/devices/{dev_id}/preference"),
                                                        &body,
                                                    ).await;
                                                });
                                            }
                                        >
                                            <option value="iphone-classic" selected=move || bezel_style.get() == "iphone-classic">"iPhone 6/7/8 (Classic)"</option>
                                            <option value="iphone-notch" selected=move || bezel_style.get() == "iphone-notch">"iPhone X/11 (Notch)"</option>
                                            <option value="iphone-modern" selected=move || bezel_style.get() == "iphone-modern">"iPhone 12–14 (Modern)"</option>
                                            <option value="iphone-island" selected=move || bezel_style.get() == "iphone-island">"iPhone 15/16 (Dynamic Island)"</option>
                                        </select>
                                    </div>
                                }.into_any()
                            } else if is_android {
                                view! {
                                    <div class="info-row">
                                        <span class="info-label">"Bezel"</span>
                                        <select class="bezel-select"
                                            on:change=move |ev| {
                                                let val = event_target_value(&ev);
                                                set_bezel_style.set(val.clone());
                                                let dev_id = dev_id_for_bezel.clone();
                                                leptos::task::spawn_local(async move {
                                                    let body = serde_json::json!({"key": "bezel", "value": val});
                                                    let _ = api::put::<_, serde_json::Value>(
                                                        &format!("/devices/{dev_id}/preference"),
                                                        &body,
                                                    ).await;
                                                });
                                            }
                                        >
                                            <option value="galaxy-s" selected=move || bezel_style.get() == "galaxy-s">"Galaxy S (S23–S25)"</option>
                                            <option value="galaxy-a" selected=move || bezel_style.get() == "galaxy-a">"Galaxy A (A54/A55)"</option>
                                            <option value="galaxy-fold" selected=move || bezel_style.get() == "galaxy-fold">"Galaxy Z Fold"</option>
                                            <option value="galaxy-flip" selected=move || bezel_style.get() == "galaxy-flip">"Galaxy Z Flip"</option>
                                            <option value="pixel" selected=move || bezel_style.get() == "pixel">"Google Pixel (8/9)"</option>
                                            <option value="oneplus" selected=move || bezel_style.get() == "oneplus">"OnePlus (12/13)"</option>
                                            <option value="xiaomi" selected=move || bezel_style.get() == "xiaomi">"Xiaomi / Redmi"</option>
                                            <option value="nothing" selected=move || bezel_style.get() == "nothing">"Nothing Phone"</option>
                                            <option value="motorola" selected=move || bezel_style.get() == "motorola">"Motorola Edge/Razr"</option>
                                            <option value="xperia" selected=move || bezel_style.get() == "xperia">"Sony Xperia"</option>
                                            <option value="huawei" selected=move || bezel_style.get() == "huawei">"Huawei Mate/P"</option>
                                            <option value="oppo" selected=move || bezel_style.get() == "oppo">"Oppo / Realme"</option>
                                            <option value="rog" selected=move || bezel_style.get() == "rog">"Asus ROG Phone"</option>
                                        </select>
                                    </div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                        <div class="info-row">
                            <span class="info-label">"Serial"</span>
                            <span class="mono" style="font-size: 0.75rem">{phone_serial_display}</span>
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

                <div class="card">
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

                // ── Inline Device Logs ──
                <div class="card">
                    <div class="card-header" style="cursor: pointer; user-select: none;"
                        on:click=move |_| set_log_open.update(|v| *v = !*v)
                    >
                        <h2>{move || if log_open.get() { "▾ Device Logs" } else { "▸ Device Logs" }}</h2>
                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                            {move || if log_connected.get() {
                                view! { <span class="badge active" style="font-size: 0.7rem">"● Live"</span> }.into_any()
                            } else if log_open.get() {
                                view! { <span class="badge queued" style="font-size: 0.7rem">"○ Connecting…"</span> }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}
                        </div>
                    </div>
                    <Show when=move || log_open.get()>
                        <div style="display: flex; gap: 0.4rem; padding: 0.4rem 0.75rem; align-items: center;">
                            <select class="bezel-select" style="flex: 0 0 auto; font-size: 0.75rem; padding: 0.2rem 0.4rem;"
                                on:change=move |ev| set_log_filter.set(event_target_value(&ev))
                            >
                                <option value="all">"All"</option>
                                <option value="error">"Error"</option>
                                <option value="warn">"Warn+"</option>
                                <option value="info">"Info+"</option>
                                <option value="debug">"Debug+"</option>
                            </select>
                            <button class="btn btn-ghost btn-sm" style="font-size: 0.7rem; padding: 0.15rem 0.4rem;"
                                on:click=move |_| {
                                    set_log_lines.set(Vec::new());
                                    if let Some(ws) = log_ws_handle.get_untracked() {
                                        let msg = serde_json::json!({"type":"clear"});
                                        let _ = ws.send_with_str(&msg.to_string());
                                    }
                                }
                            >"Clear"</button>
                            <span style="margin-left: auto; font-size: 0.7rem; color: var(--text-muted);">
                                {move || format!("{} lines", log_lines.get().len())}
                            </span>
                        </div>
                        <div id="inline-log-viewer" style="max-height: 260px; overflow-y: auto; font-family: 'JetBrains Mono', 'Fira Code', monospace; font-size: 0.72rem; background: #0d1117; color: #c9d1d9; padding: 0.4rem; border-radius: 0 0 6px 6px; line-height: 1.45;">
                            {move || {
                                let filter = log_filter.get();
                                let lines = log_lines.get();
                                let filtered: Vec<_> = lines.iter().filter(|(level, _, _)| {
                                    match filter.as_str() {
                                        "error" => level == "error" || level == "fatal",
                                        "warn" => level == "warn" || level == "error" || level == "fatal",
                                        "info" => level != "debug" && level != "verbose",
                                        "debug" => level != "verbose",
                                        _ => true,
                                    }
                                }).collect();
                                if filtered.is_empty() {
                                    view! { <div style="color: #484f58; padding: 0.5rem;">"Waiting for logs…"</div> }.into_any()
                                } else {
                                    filtered.into_iter().map(|(level, tag, message)| {
                                        let color = match level.as_str() {
                                            "error" | "fatal" => "#f85149",
                                            "warn" => "#d29922",
                                            "info" => "#58a6ff",
                                            "debug" => "#8b949e",
                                            _ => "#6e7681",
                                        };
                                        let level_char = level.chars().next().unwrap_or('I').to_uppercase().to_string();
                                        let tag_str = if tag.is_empty() { String::new() } else { format!("[{}] ", tag) };
                                        view! {
                                            <div style="white-space: pre-wrap; word-break: break-all; padding: 0.5px 0;">
                                                <span style=format!("color: {}; font-weight: 600; margin-right: 0.3em;", color)>{level_char}</span>
                                                <span style="color: #79c0ff;">{tag_str}</span>
                                                <span>{message.clone()}</span>
                                            </div>
                                        }
                                    }).collect_view().into_any()
                                }
                            }}
                        </div>
                    </Show>
                </div>
            </div>
                </>
            }.into_any()
            }}
        </div>
    }
}
