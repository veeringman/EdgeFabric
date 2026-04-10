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

/// Map iPad ProductType identifiers to specific iPad Pro bezel variants.
/// ProductType is lowercase, e.g. "ipad6,7", "ipad8,1", "ipad16,5".
fn detect_ipad_pro_variant(product_type: &str) -> Option<String> {
    // Parse "ipadN,M" → (N, M)
    let rest = product_type.strip_prefix("ipad")?;
    let (gen_str, _sub_str) = rest.split_once(',')?;
    let gen: u32 = gen_str.parse().ok()?;
    let sub: u32 = _sub_str.parse().ok()?;
    match gen {
        // iPad Pro 12.9" 1st gen (2015): iPad6,7-8 — thick bezels, home button
        6 if sub >= 7 => Some("ipad-pro-classic".into()),
        // iPad Pro 9.7" (2016): iPad6,3-4 — thick bezels, home button, smaller
        6 if sub >= 3 => Some("ipad-pro-10".into()),
        // iPad Pro 12.9" 2nd gen (2017): iPad7,1-2 — thick bezels, home button
        7 if sub <= 2 => Some("ipad-pro-classic".into()),
        // iPad Pro 10.5" (2017): iPad7,3-4 — thick bezels, home button
        7 if sub <= 4 => Some("ipad-pro-10".into()),
        // iPad Pro 11" 1st gen (2018): iPad8,1-4 — thin bezels, Face ID
        8 if sub <= 4 => Some("ipad-pro-11".into()),
        // iPad Pro 12.9" 3rd gen (2018): iPad8,5-8 — thin bezels, Face ID
        8 if sub <= 8 => Some("ipad-pro-12".into()),
        // iPad Pro 11" 2nd gen (2020): iPad8,9-10 — thin bezels
        8 if sub <= 10 => Some("ipad-pro-11".into()),
        // iPad Pro 12.9" 4th gen (2020): iPad8,11-12 — thin bezels
        8 if sub <= 12 => Some("ipad-pro-12".into()),
        // iPad Pro 11" 3rd gen (2021 M1): iPad13,4-7
        13 if sub >= 4 && sub <= 7 => Some("ipad-pro-11".into()),
        // iPad Pro 12.9" 5th gen (2021 M1): iPad13,8-9
        13 if sub >= 8 && sub <= 9 => Some("ipad-pro-12".into()),
        // iPad Pro 11" 4th gen (2022 M2): iPad14,3-4
        14 if sub >= 3 && sub <= 4 => Some("ipad-pro-11".into()),
        // iPad Pro 12.9" 6th gen (2022 M2): iPad14,5-6
        14 if sub >= 5 && sub <= 6 => Some("ipad-pro-12".into()),
        // iPad Pro 11" M4 (2024): iPad16,3-4 — ultra-thin bezels, OLED
        16 if sub >= 3 && sub <= 4 => Some("ipad-pro-m4".into()),
        // iPad Pro 13" M4 (2024): iPad16,5-6 — ultra-thin bezels, OLED
        16 if sub >= 5 && sub <= 6 => Some("ipad-pro-m4-13".into()),
        _ => None,
    }
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
    if os == "ios" || os == "ipados" {
        // Check for iPad via model: or ProductType capability
        let model_lower: String = caps.iter()
            .find(|c| c.starts_with("model:"))
            .map(|c| c[6..].to_lowercase())
            .unwrap_or_default();
        let product_lower: String = caps.iter()
            .find(|c| c.starts_with("ProductType:"))
            .map(|c| c[12..].to_lowercase())
            .unwrap_or_default();
        let is_ipad = os == "ipados" || model_lower.contains("ipad") || product_lower.starts_with("ipad");
        let is_mini = model_lower.contains("mini") || product_lower.contains("mini");
        if is_ipad {
            let ver: f32 = os_version.split('.').next().unwrap_or("0").parse().unwrap_or(0.0);
            if is_mini {
                return if ver >= 15.0 { "ipad-mini-modern".into() } else { "ipad-mini-classic".into() };
            }
            // Auto-detect iPad Pro variant from ProductType
            if let Some(pro_bezel) = detect_ipad_pro_variant(&product_lower) {
                return pro_bezel;
            }
            if model_lower.contains("pro") {
                return "ipad-pro".into();
            }
            if ver >= 15.0 { return "ipad-air".into(); }
            return "ipad-classic".into();
        }
        // iPhone mini detection
        if is_mini {
            return "iphone-mini".into();
        }
        let ver: f32 = os_version.split('.').next().unwrap_or("0").parse().unwrap_or(0.0);
        if ver >= 16.0 { return "iphone-island".into(); }
        if ver >= 14.0 { return "iphone-modern".into(); }
        if ver >= 11.0 { return "iphone-notch".into(); }
        if ver >= 7.0 { return "iphone-classic".into(); }
        if ver >= 6.0 { return "iphone-5".into(); }
        if ver >= 4.0 { return "iphone-4".into(); }
        return "iphone-3g".into();
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
        if name_lower.contains("tab") || name_lower.contains("sm-x") { return "galaxy-tab".into(); }
        if name_lower.contains("pixel") && (name_lower.contains("tablet") || name_lower.contains("tab")) { return "pixel-tablet".into(); }
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
                                            let os_icon = if d.os == "android" { "🤖" } else if d.os == "ios" { "🍎" } else if d.os == "iot" { "🔌" } else if d.os == "ssh" { "🖥️" } else { "📱" };
                                            let status = d.status.clone();
                                            let os_label = if d.os == "android" { "Android".to_string() }
                                                else if d.os == "ios" { "iOS".to_string() }
                                                else if d.os == "iot" { "IoT".to_string() }
                                                else if d.os == "ssh" { "SSH".to_string() }
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
    let term_ref = NodeRef::<leptos::html::Div>::new();
    let log_ref = NodeRef::<leptos::html::Div>::new();
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

    // Watch pairing signals
    let (watch_style, set_watch_style) = signal(String::from("none"));

    // SSH auth signals
    let (ssh_auth_needed, set_ssh_auth_needed) = signal(false);
    let (ssh_username, set_ssh_username) = signal(String::from("root"));
    let (ssh_password, set_ssh_password) = signal(String::new());
    let (ssh_auth_error, set_ssh_auth_error) = signal(Option::<String>::None);

    // xterm.js terminal handle (JS object)
    let (xterm_ready, set_xterm_ready) = signal(false);

    let dev_id = device_id.clone();
    let log_dev_id = device_id.clone();
    let is_android = device_os == "android";
    let is_ios = device_os == "ios";
    let is_iot = device_os == "iot";
    let is_ssh = device_os == "ssh";
    let is_terminal = is_iot || is_ssh;

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
        let img_ref = img_ref;

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

                    if is_terminal {
                        // Terminal device: write raw bytes to xterm.js
                        // Use TextDecoder to convert bytes to string for xterm.write()
                        if let Ok(decoder) = js_sys::Reflect::get(
                            &wasm_bindgen::JsValue::from(web_sys::window().unwrap()),
                            &wasm_bindgen::JsValue::from_str("__ef_xterm"),
                        ) {
                            if !decoder.is_undefined() && !decoder.is_null() {
                                // Call xterm.write(uint8array) — xterm.js accepts Uint8Array directly
                                let _ = js_sys::Reflect::apply(
                                    &js_sys::Function::from(
                                        js_sys::Reflect::get(&decoder, &wasm_bindgen::JsValue::from_str("write")).unwrap_or_default()
                                    ),
                                    &decoder,
                                    &js_sys::Array::of1(&array.into()),
                                );
                            }
                        }
                        set_frame_count.update(|c| *c += 1);
                    } else {
                        // Phone device: JPEG frame
                        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(
                            &js_sys::Array::of1(&array.into()),
                            web_sys::BlobPropertyBag::new().type_("image/jpeg"),
                        ).unwrap();
                        let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();

                        if let Some(img_el) = img_ref.get_untracked() {
                            let old_src = img_el.get_attribute("src").unwrap_or_default();
                            let _ = img_el.set_attribute("src", &url);
                            if !old_src.is_empty() {
                                web_sys::Url::revoke_object_url(&old_src).ok();
                            }
                        }
                        set_frame_count.update(|c| *c += 1);

                        // Calculate FPS
                        let now = js_sys::Date::now();
                        let mut times = fps_counter.borrow_mut();
                        times.push(now);
                        times.retain(|t| now - t < 2000.0);
                        if times.len() > 1 {
                            set_fps.set(times.len() as f64 / ((now - times[0]) / 1000.0));
                        }
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
                            Some("ssh_auth_required") => {
                                set_ssh_auth_needed.set(true);
                            }
                            Some("ssh_connected") => {
                                set_ssh_auth_needed.set(false);
                                set_ssh_auth_error.set(None);
                                set_xterm_ready.set(true);
                            }
                            Some("error") => {
                                if let Some(message) = msg.get("message").and_then(|m| m.as_str()) {
                                    if ssh_auth_needed.get_untracked() {
                                        set_ssh_auth_error.set(Some(message.to_string()));
                                    } else {
                                        set_error.set(Some(message.to_string()));
                                    }
                                }
                            }
                            Some("serial_data") => {
                                // Legacy line-by-line fallback
                                if let Some(data) = msg.get("data").and_then(|d| d.as_str()) {
                                    set_serial_lines.update(|lines| {
                                        lines.push(data.to_string());
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
                            if let Some(el) = log_ref.get_untracked() {
                                let el: &web_sys::Element = &el;
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
        let img_el = img_ref.get_untracked()?;
        let rect = img_el.get_bounding_client_rect();

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
    // Extra closures for iOS home button/indicator (used separately in view)
    let send_ios_home = move |_: web_sys::MouseEvent| send_input("home", None);
    let send_ios_home2 = move |_: web_sys::MouseEvent| send_input("home", None);
    let send_ios_home3 = move |_: web_sys::MouseEvent| send_input("home", None);

    let full_id = device_id.clone();

    view! {
        <div class="viewer-layout">
            // ── Terminal (IoT / SSH) OR Phone simulator ──
            {if is_terminal {
                view! {
                    <div class="card" style="flex: 1; min-width: 0;">
                        <div class="card-header">
                            <h2>{if is_ssh { "🖥️ SSH Terminal" } else { "🔌 Serial Console" }}</h2>
                            <button class="btn btn-ghost btn-sm" on:click=move |_| {
                                // Clean up xterm instance
                                let win = web_sys::window().unwrap();
                                let _ = js_sys::Reflect::delete_property(&win, &wasm_bindgen::JsValue::from_str("__ef_xterm"));
                                if let Some(ws) = ws_handle.get_untracked() {
                                    let _ = ws.close();
                                }
                                on_close();
                            }>"✕ Close"</button>
                        </div>

                        // SSH Login form (shown before terminal)
                        <Show when=move || ssh_auth_needed.get()>
                            <div style="padding: 1rem; background: #161b22; border-radius: 6px; margin: 0.5rem;">
                                <h3 style="color: #58a6ff; margin: 0 0 0.75rem 0; font-size: 0.95rem;">"SSH Authentication"</h3>
                                <Show when=move || ssh_auth_error.get().is_some()>
                                    <div class="alert error" style="margin-bottom: 0.5rem; font-size: 0.82rem;">
                                        {move || ssh_auth_error.get().unwrap_or_default()}
                                    </div>
                                </Show>
                                <div style="display: flex; flex-direction: column; gap: 0.5rem;">
                                    <div style="display: flex; align-items: center; gap: 0.5rem;">
                                        <label style="color: #8b949e; font-size: 0.82rem; width: 70px;">"Username"</label>
                                        <input
                                            type="text"
                                            class="form-control"
                                            placeholder="root"
                                            style="flex: 1; font-family: 'JetBrains Mono', monospace; font-size: 0.82rem; background: #0d1117; color: #c9d1d9; border: 1px solid #30363d; border-radius: 4px; padding: 0.4rem 0.6rem;"
                                            prop:value=move || ssh_username.get()
                                            on:input=move |ev| set_ssh_username.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <div style="display: flex; align-items: center; gap: 0.5rem;">
                                        <label style="color: #8b949e; font-size: 0.82rem; width: 70px;">"Password"</label>
                                        <input
                                            type="password"
                                            class="form-control"
                                            placeholder="Enter password"
                                            style="flex: 1; font-family: 'JetBrains Mono', monospace; font-size: 0.82rem; background: #0d1117; color: #c9d1d9; border: 1px solid #30363d; border-radius: 4px; padding: 0.4rem 0.6rem;"
                                            prop:value=move || ssh_password.get()
                                            on:input=move |ev| set_ssh_password.set(event_target_value(&ev))
                                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                if ev.key() == "Enter" {
                                                    if let Some(ws) = ws_handle.get_untracked() {
                                                        let msg = serde_json::json!({
                                                            "type": "ssh_auth",
                                                            "username": ssh_username.get_untracked(),
                                                            "password": ssh_password.get_untracked(),
                                                        });
                                                        let _ = ws.send_with_str(&msg.to_string());
                                                    }
                                                }
                                            }
                                        />
                                    </div>
                                    <button
                                        class="btn btn-primary btn-sm"
                                        style="align-self: flex-end; margin-top: 0.25rem;"
                                        on:click=move |_| {
                                            if let Some(ws) = ws_handle.get_untracked() {
                                                let msg = serde_json::json!({
                                                    "type": "ssh_auth",
                                                    "username": ssh_username.get_untracked(),
                                                    "password": ssh_password.get_untracked(),
                                                });
                                                let _ = ws.send_with_str(&msg.to_string());
                                            }
                                        }
                                    >"Connect"</button>
                                </div>
                            </div>
                        </Show>

                        <Show
                            when=move || connected.get() && !ssh_auth_needed.get()
                            fallback=move || {
                                if !ssh_auth_needed.get() {
                                    view! {
                                        <div class="loading">{if is_ssh { "Connecting to SSH server…" } else { "Connecting to serial port…" }}</div>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }
                            }
                        >
                            // xterm.js terminal container
                            <div
                                node_ref=term_ref
                                id="ef-terminal-container"
                                style="min-height: 400px; max-height: 600px; background: #000; border-radius: 6px; overflow: hidden;"
                            ></div>

                            // Initialize xterm.js when container is mounted
                            {move || {
                                if let Some(container) = term_ref.get() {
                                    let win = web_sys::window().unwrap();
                                    // Only init once
                                    let existing = js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str("__ef_xterm")).ok();
                                    if existing.map(|v| v.is_undefined() || v.is_null()).unwrap_or(true) {
                                        // Create xterm Terminal instance via JS eval
                                        let init_js = format!(
                                            r#"(function() {{
                                                if (typeof Terminal === 'undefined') return null;
                                                var term = new Terminal({{
                                                    cursorBlink: true,
                                                    fontSize: 14,
                                                    fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
                                                    theme: {{
                                                        background: '#0d1117',
                                                        foreground: '#c9d1d9',
                                                        cursor: '#58a6ff',
                                                        selectionBackground: '#264f78',
                                                    }},
                                                    scrollback: 10000,
                                                    convertEol: true,
                                                }});
                                                var container = document.getElementById('ef-terminal-container');
                                                if (container) {{
                                                    term.open(container);
                                                    if (typeof FitAddon !== 'undefined') {{
                                                        var fitAddon = new FitAddon.FitAddon();
                                                        term.loadAddon(fitAddon);
                                                        fitAddon.fit();
                                                        window.__ef_fit = fitAddon;
                                                        new ResizeObserver(function() {{ try {{ fitAddon.fit(); }} catch(e) {{}} }}).observe(container);
                                                    }}
                                                    if (typeof WebLinksAddon !== 'undefined') {{
                                                        term.loadAddon(new WebLinksAddon.WebLinksAddon());
                                                    }}
                                                    window.__ef_xterm = term;
                                                }}
                                                return term;
                                            }})()"#
                                        );
                                        let _ = js_sys::eval(&init_js);

                                        // Hook up xterm.onData → WS terminal_input
                                        let hook_js = r#"(function() {
                                            if (!window.__ef_xterm) return;
                                            window.__ef_xterm.onData(function(data) {
                                                if (window.__ef_ws && window.__ef_ws.readyState === 1) {
                                                    window.__ef_ws.send(JSON.stringify({type: 'terminal_input', data: data}));
                                                }
                                            });
                                            window.__ef_xterm.onResize(function(size) {
                                                if (window.__ef_ws && window.__ef_ws.readyState === 1) {
                                                    window.__ef_ws.send(JSON.stringify({type: 'terminal_resize', cols: size.cols, rows: size.rows}));
                                                }
                                            });
                                        })()"#;
                                        let _ = js_sys::eval(hook_js);

                                        // Store WS reference for xterm callbacks
                                        if let Some(ws) = ws_handle.get_untracked() {
                                            let _ = js_sys::Reflect::set(
                                                &win,
                                                &wasm_bindgen::JsValue::from_str("__ef_ws"),
                                                &ws,
                                            );
                                        }
                                    }
                                }
                                view! { <span></span> }
                            }}

                            // ── Emergency command input ──
                            <div style="display: flex; gap: 0.5rem; margin-top: 0.5rem; align-items: center;">
                                <span style="color: #f0883e; font-family: monospace; font-weight: 600; flex-shrink: 0;" title="Emergency command (bypass terminal)">"⚡"</span>
                                <input
                                    type="text"
                                    class="form-control"
                                    placeholder="Emergency command (if terminal is unresponsive)…"
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
                                <span class="info-label">"Type"</span>
                                <span>{if is_ssh { "SSH Server" } else { "IoT Device" }}</span>
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
                                <span class="info-label">"Bytes"</span>
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
                                "iphone-3g" | "iphone-4" | "iphone-5" | "iphone-classic" => "ios-classic-top",
                                "iphone-notch" | "iphone-modern" | "iphone-mini" => "ios-notch",
                                "ipad-classic" | "ipad-mini-classic" => "ipad-classic-top",
                                "ipad-pro-classic" => "ipad-pro-classic-top",
                                "ipad-pro-10" => "ipad-pro-10-top",
                                "ipad-pro-11" => "ipad-pro-11-top",
                                "ipad-pro-12" => "ipad-pro-12-top",
                                "ipad-pro-m4" => "ipad-pro-m4-top",
                                "ipad-pro-m4-13" => "ipad-pro-m4-13-top",
                                "ipad-air" | "ipad-pro" | "ipad-mini-modern" => "ipad-modern-top",
                                _ => "ios-notch",
                            })}>
                                {move || match bezel_style.get().as_str() {
                                    "iphone-3g" | "iphone-4" | "iphone-5" | "iphone-classic" => view! { <div class="phone-speaker"></div> }.into_any(),
                                    "iphone-island" => view! { <div class="dynamic-island"></div> }.into_any(),
                                    "iphone-mini" => view! { <div class="ios-notch-cutout mini-notch"></div> }.into_any(),
                                    "ipad-classic" | "ipad-mini-classic" | "ipad-pro-classic" | "ipad-pro-10" => view! { <div class="ipad-camera-dot"></div> }.into_any(),
                                    "ipad-air" | "ipad-pro" | "ipad-pro-11" | "ipad-pro-12" | "ipad-pro-m4" | "ipad-pro-m4-13" | "ipad-mini-modern" => view! { <div class="ipad-camera-dot"></div> }.into_any(),
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
                            <img node_ref=img_ref
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
                                "iphone-3g" | "iphone-4" | "iphone-5" | "iphone-classic" | "ipad-classic" | "ipad-mini-classic" | "ipad-pro-classic" | "ipad-pro-10" => view! {
                                    <div class="phone-navbar ios-navbar classic-navbar">
                                        <div class="ios-home-button" on:click=send_ios_home title="Home"></div>
                                    </div>
                                }.into_any(),
                                "ipad-air" | "ipad-pro" | "ipad-pro-11" | "ipad-pro-12" | "ipad-pro-m4" | "ipad-pro-m4-13" | "ipad-mini-modern" => view! {
                                    <div class="phone-navbar ios-navbar ipad-navbar">
                                        <div class="ios-home-indicator" on:click=send_ios_home2 title="Home"></div>
                                    </div>
                                }.into_any(),
                                _ => view! {
                                    <div class="phone-navbar ios-navbar">
                                        <div class="ios-home-indicator" on:click=send_ios_home3 title="Home"></div>
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

            // ── Paired Watch ──
            <Show when=move || watch_style.get() != "none">
                <div class="watch-wrapper">
                    <div class="watch-band-top">
                        <div class="band-texture"></div>
                    </div>
                    <div class="watch-lugs-top"></div>
                    <div class={move || format!("watch-frame {}", watch_style.get())}>
                        <div class="watch-screen">
                            <div class="watch-placeholder">
                                {move || match watch_style.get().as_str() {
                                    "apple-watch" | "apple-watch-ss" | "apple-watch-ultra" => view! {
                                        <div class="watch-face-time">"10:09"</div>
                                        <div class="watch-face-complication">
                                            <span class="complication-icon">"❤️"</span>
                                            <span class="complication-text">"72 BPM"</span>
                                        </div>
                                        <span class="watch-label">"Apple Watch"</span>
                                    }.into_any(),
                                    "galaxy-watch" => view! {
                                        <div class="watch-face-time">"10:09"</div>
                                        <div class="watch-face-complication">
                                            <span class="complication-icon">"👣"</span>
                                            <span class="complication-text">"4,521"</span>
                                        </div>
                                        <span class="watch-label">"Galaxy Watch"</span>
                                    }.into_any(),
                                    _ => view! {
                                        <div class="watch-face-time">"10:09"</div>
                                        <div class="watch-face-complication">
                                            <span class="complication-icon">"⚡"</span>
                                            <span class="complication-text">"85%"</span>
                                        </div>
                                        <span class="watch-label">"Wear OS"</span>
                                    }.into_any(),
                                }}
                            </div>
                        </div>
                        {move || match watch_style.get().as_str() {
                            "apple-watch" | "apple-watch-ss" | "apple-watch-ultra" => view! {
                                <>
                                <div class="watch-crown"></div>
                                <div class="watch-side-btn"></div>
                                </>
                            }.into_any(),
                            "galaxy-watch" => view! {
                                <>
                                <div class="watch-btn-top"></div>
                                <div class="watch-btn-bottom"></div>
                                </>
                            }.into_any(),
                            "pixel-watch" => view! {
                                <div class="watch-crown pixel-crown"></div>
                            }.into_any(),
                            _ => view! { <span></span> }.into_any(),
                        }}
                    </div>
                    <div class="watch-lugs-bottom"></div>
                    <div class="watch-band-bottom">
                        <div class="band-texture"></div>
                    </div>
                </div>
            </Show>

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
                                            <option value="iphone-3g" selected=move || bezel_style.get() == "iphone-3g">"iPhone 3G/3GS"</option>
                                            <option value="iphone-4" selected=move || bezel_style.get() == "iphone-4">"iPhone 4/4S"</option>
                                            <option value="iphone-5" selected=move || bezel_style.get() == "iphone-5">"iPhone 5/5S/SE"</option>
                                            <option value="iphone-classic" selected=move || bezel_style.get() == "iphone-classic">"iPhone 6/7/8 (Classic)"</option>
                                            <option value="iphone-notch" selected=move || bezel_style.get() == "iphone-notch">"iPhone X/11 (Notch)"</option>
                                            <option value="iphone-modern" selected=move || bezel_style.get() == "iphone-modern">"iPhone 12–14 (Modern)"</option>
                                            <option value="iphone-mini" selected=move || bezel_style.get() == "iphone-mini">"iPhone 12/13 mini"</option>
                                            <option value="iphone-island" selected=move || bezel_style.get() == "iphone-island">"iPhone 15/16 (Dynamic Island)"</option>
                                            <option value="ipad-classic" selected=move || bezel_style.get() == "ipad-classic">"iPad (Home Button)"</option>
                                            <option value="ipad-air" selected=move || bezel_style.get() == "ipad-air">"iPad Air / 10th Gen"</option>
                                            <option value="ipad-pro" selected=move || bezel_style.get() == "ipad-pro">"iPad Pro"</option>
                                            <option value="ipad-pro-classic" selected=move || bezel_style.get() == "ipad-pro-classic">"iPad Pro 12.9\" (1st/2nd Gen)"</option>
                                            <option value="ipad-pro-10" selected=move || bezel_style.get() == "ipad-pro-10">"iPad Pro 9.7\"/10.5\""</option>
                                            <option value="ipad-pro-11" selected=move || bezel_style.get() == "ipad-pro-11">"iPad Pro 11\" (2018-2022)"</option>
                                            <option value="ipad-pro-12" selected=move || bezel_style.get() == "ipad-pro-12">"iPad Pro 12.9\" (2018-2022)"</option>
                                            <option value="ipad-pro-m4" selected=move || bezel_style.get() == "ipad-pro-m4">"iPad Pro 11\" M4"</option>
                                            <option value="ipad-pro-m4-13" selected=move || bezel_style.get() == "ipad-pro-m4-13">"iPad Pro 13\" M4"</option>
                                            <option value="ipad-mini-classic" selected=move || bezel_style.get() == "ipad-mini-classic">"iPad mini (Home Button)"</option>
                                            <option value="ipad-mini-modern" selected=move || bezel_style.get() == "ipad-mini-modern">"iPad mini 6+"</option>
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
                                            <option value="galaxy-tab" selected=move || bezel_style.get() == "galaxy-tab">"Galaxy Tab S"</option>
                                            <option value="pixel-tablet" selected=move || bezel_style.get() == "pixel-tablet">"Pixel Tablet"</option>
                                            <option value="android-tablet" selected=move || bezel_style.get() == "android-tablet">"Android Tablet (Generic)"</option>
                                        </select>
                                    </div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                        {if is_ios {
                            view! {
                                <div class="info-row">
                                    <span class="info-label">"Watch"</span>
                                    <select class="bezel-select"
                                        on:change=move |ev| {
                                            set_watch_style.set(event_target_value(&ev));
                                        }
                                    >
                                        <option value="none" selected=move || watch_style.get() == "none">"None"</option>
                                        <option value="apple-watch" selected=move || watch_style.get() == "apple-watch">"Apple Watch"</option>
                                        <option value="apple-watch-ss" selected=move || watch_style.get() == "apple-watch-ss">"Apple Watch SS"</option>
                                        <option value="apple-watch-ultra" selected=move || watch_style.get() == "apple-watch-ultra">"Apple Watch Ultra"</option>
                                    </select>
                                </div>
                            }.into_any()
                        } else if is_android {
                            view! {
                                <div class="info-row">
                                    <span class="info-label">"Watch"</span>
                                    <select class="bezel-select"
                                        on:change=move |ev| {
                                            set_watch_style.set(event_target_value(&ev));
                                        }
                                    >
                                        <option value="none" selected=move || watch_style.get() == "none">"None"</option>
                                        <option value="galaxy-watch" selected=move || watch_style.get() == "galaxy-watch">"Galaxy Watch"</option>
                                        <option value="pixel-watch" selected=move || watch_style.get() == "pixel-watch">"Pixel Watch"</option>
                                        <option value="wear-os" selected=move || watch_style.get() == "wear-os">"Wear OS (Generic)"</option>
                                    </select>
                                </div>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
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
                        <div node_ref=log_ref style="max-height: 260px; overflow-y: auto; font-family: 'JetBrains Mono', 'Fira Code', monospace; font-size: 0.72rem; background: #0d1117; color: #c9d1d9; padding: 0.4rem; border-radius: 0 0 6px 6px; line-height: 1.45;">
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
