use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, DeviceListResponse};

/// Real-time log viewer — streams logcat/syslog from device via WebSocket.
#[component]
pub fn LogsPage() -> impl IntoView {
    let (device_id, set_device_id) = signal(String::new());
    let (log_level, set_log_level) = signal("all".to_string());
    let (search, set_search) = signal(String::new());
    let (log_lines, set_log_lines) = signal(Vec::<LogLine>::new());
    let (connected, set_connected) = signal(false);
    let (auto_scroll, set_auto_scroll) = signal(true);
    let (ws_handle, set_ws_handle) = signal(Option::<web_sys::WebSocket>::None);
    let (line_count, set_line_count) = signal(0usize);

    let devices = LocalResource::new(|| async {
        api::get::<DeviceListResponse>("/devices").await.unwrap_or_default()
    });

    let start_streaming = move |_| {
        let did = device_id.get_untracked();
        if did.is_empty() {
            return;
        }

        // Close any existing connection
        if let Some(old_ws) = ws_handle.get_untracked() {
            let _ = old_ws.close();
        }

        set_log_lines.set(Vec::new());
        set_line_count.set(0);

        let host = web_sys::window()
            .and_then(|w| w.location().hostname().ok())
            .unwrap_or_else(|| "localhost".to_string());
        let ws_url = format!("ws://{host}:6060/api/v1/devices/{did}/logs");

        let ws = match web_sys::WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(_) => return,
        };

        let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
            set_connected.set(true);
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onmessage = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
            if let Ok(text) = evt.data().dyn_into::<js_sys::JsString>() {
                let s: String = text.into();
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&s) {
                    let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if msg_type == "log" {
                        let line = LogLine {
                            level: msg.get("level").and_then(|v| v.as_str()).unwrap_or("info").to_string(),
                            timestamp: msg.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            tag: msg.get("tag").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            message: msg.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        };
                        set_log_lines.update(|lines| {
                            lines.push(line);
                            // Keep last 2000 lines to prevent memory issues
                            if lines.len() > 2000 {
                                lines.drain(..500);
                            }
                        });
                        set_line_count.update(|c| *c += 1);

                        // Auto-scroll
                        if auto_scroll.get_untracked() {
                            if let Some(el) = web_sys::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.get_element_by_id("log-viewer"))
                            {
                                el.set_scroll_top(el.scroll_height());
                            }
                        }
                    } else if msg_type == "error" {
                        let err_msg = msg.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                        set_log_lines.update(|lines| {
                            lines.push(LogLine {
                                level: "error".into(),
                                timestamp: String::new(),
                                tag: "SYSTEM".into(),
                                message: err_msg.to_string(),
                            });
                        });
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let onclose = Closure::wrap(Box::new(move |_: web_sys::Event| {
            set_connected.set(false);
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        set_ws_handle.set(Some(ws));
    };

    let stop_streaming = move |_| {
        if let Some(ws) = ws_handle.get_untracked() {
            let msg = serde_json::json!({"type":"stop"});
            let _ = ws.send_with_str(&msg.to_string());
            let _ = ws.close();
        }
        set_connected.set(false);
        set_ws_handle.set(None);
    };

    let clear_logs = move |_| {
        if let Some(ws) = ws_handle.get_untracked() {
            let msg = serde_json::json!({"type":"clear"});
            let _ = ws.send_with_str(&msg.to_string());
        }
        set_log_lines.set(Vec::new());
    };

    let filtered_lines = move || {
        let level = log_level.get();
        let query = search.get().to_lowercase();
        log_lines.get()
            .into_iter()
            .filter(|l| {
                level == "all" || l.level == level
                    || (level == "warn" && (l.level == "warn" || l.level == "error" || l.level == "fatal"))
                    || (level == "error" && (l.level == "error" || l.level == "fatal"))
            })
            .filter(|l| {
                query.is_empty()
                    || l.message.to_lowercase().contains(&query)
                    || l.tag.to_lowercase().contains(&query)
            })
            .collect::<Vec<_>>()
    };

    view! {
        <h1 style="margin-bottom: 1rem">"Device Logs"</h1>

        <div class="filters">
            <div class="form-group">
                <label>"Device"</label>
                <Suspense fallback=move || view! { <select><option>"Loading…"</option></select> }>
                    {move || devices.get().as_deref().map(|data| {
                        view! {
                            <select on:change=move |ev| set_device_id.set(event_target_value(&ev))>
                                <option value="">"Select device"</option>
                                {data.devices.iter()
                                    .filter(|d| d.status == "available" || d.status == "busy")
                                    .map(|d| {
                                        let id = d.id.clone();
                                        let label = format!("{} ({})", d.name, &d.id[..8.min(d.id.len())]);
                                        view! { <option value=id>{label}</option> }
                                    }).collect::<Vec<_>>()}
                            </select>
                        }
                    })}
                </Suspense>
            </div>
            <div class="form-group">
                <label>"Min Level"</label>
                <select on:change=move |ev| set_log_level.set(event_target_value(&ev))>
                    <option value="all">"All"</option>
                    <option value="verbose">"Verbose"</option>
                    <option value="debug">"Debug"</option>
                    <option value="info">"Info"</option>
                    <option value="warn">"Warning+"</option>
                    <option value="error">"Error+"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Search"</label>
                <input type="text" placeholder="Filter by message or tag…" prop:value=search
                    on:input=move |ev| set_search.set(event_target_value(&ev))/>
            </div>
            <Show
                when=move || !connected.get()
                fallback=move || view! {
                    <div style="display: flex; gap: 0.5rem; align-self: flex-end">
                        <button class="btn btn-ghost btn-sm" on:click=clear_logs>"🗑 Clear"</button>
                        <button class="btn btn-danger btn-sm" on:click=stop_streaming>"⏹ Stop"</button>
                    </div>
                }
            >
                <button class="btn btn-primary btn-sm" style="align-self: flex-end" on:click=start_streaming
                    disabled=move || device_id.get().is_empty()>
                    "▶ Start Streaming"
                </button>
            </Show>
            <label style="align-self: flex-end; display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; color: var(--text-muted)">
                <input type="checkbox" prop:checked=auto_scroll
                    on:change=move |ev| set_auto_scroll.set(event_target_checked(&ev))/>
                "Auto-scroll"
            </label>
        </div>

        <div class="card">
            <div class="card-header">
                <h2>"Log Output"</h2>
                <div style="display: flex; align-items: center; gap: 1rem; font-size: 0.8rem; color: var(--text-muted)">
                    {move || if connected.get() {
                        view! { <span class="badge active">"● Live"</span> }.into_any()
                    } else {
                        view! { <span class="badge queued">"○ Disconnected"</span> }.into_any()
                    }}
                    <span>{move || format!("{} lines", line_count.get())}</span>
                </div>
            </div>
            <div id="log-viewer" class="log-viewer">
                {move || {
                    let lines = filtered_lines();
                    if lines.is_empty() {
                        view! { <div class="log-line" style="color: var(--text-muted)">"No log output. Select a device and start streaming."</div> }.into_any()
                    } else {
                        lines.into_iter().map(|l| {
                            let class = format!("log-line {}", l.level);
                            let tag_display = if l.tag.is_empty() { String::new() } else { format!("[{}] ", l.tag) };
                            view! {
                                <div class=class>
                                    <span class="log-ts">{l.timestamp}</span>
                                    <span class="log-level">{l.level.chars().next().unwrap_or('I').to_uppercase().to_string()}</span>
                                    <span class="log-tag">{tag_display}</span>
                                    <span>{l.message}</span>
                                </div>
                            }
                        }).collect_view().into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[derive(Clone, Debug)]
struct LogLine {
    level: String,
    timestamp: String,
    tag: String,
    message: String,
}

fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
