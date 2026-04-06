use leptos::prelude::*;

use crate::api::{self, DeviceListResponse};

/// Real-time log viewer (streaming logs from device logcat / syslog).
#[component]
pub fn LogsPage() -> impl IntoView {
    let (device_id, set_device_id) = signal(String::new());
    let (log_level, set_log_level) = signal("all".to_string());
    let (search, set_search) = signal(String::new());
    let (log_lines, set_log_lines) = signal(Vec::<LogLine>::new());
    let (connected, set_connected) = signal(false);
    let (auto_scroll, set_auto_scroll) = signal(true);

    let devices = LocalResource::new(|| async {
        api::get::<DeviceListResponse>("/devices").await.unwrap_or_default()
    });

    // Simulated log streaming (in production would use WebSocket to /stream/{session_id})
    let start_streaming = move |_| {
        let did = device_id.get_untracked();
        if did.is_empty() {
            return;
        }
        set_connected.set(true);
        set_log_lines.set(vec![
            LogLine { level: "info".into(), timestamp: "2026-04-07T10:00:01Z".into(), message: format!("Connected to device {}", &did[..8.min(did.len())]) },
            LogLine { level: "info".into(), timestamp: "2026-04-07T10:00:02Z".into(), message: "Log streaming started…".into() },
            LogLine { level: "info".into(), timestamp: "2026-04-07T10:00:03Z".into(), message: "Waiting for log events from device agent".into() },
        ]);
    };

    let stop_streaming = move |_| {
        set_connected.set(false);
    };

    let filtered_lines = move || {
        let level = log_level.get();
        let query = search.get().to_lowercase();
        log_lines.get()
            .into_iter()
            .filter(|l| level == "all" || l.level == level)
            .filter(|l| query.is_empty() || l.message.to_lowercase().contains(&query))
            .collect::<Vec<_>>()
    };

    view! {
        <h1 style="margin-bottom: 1rem">"Logs"</h1>

        <div class="filters">
            <div class="form-group">
                <label>"Device"</label>
                <Suspense fallback=move || view! { <select><option>"Loading…"</option></select> }>
                    {move || devices.get().as_deref().map(|data| {
                        view! {
                            <select on:change=move |ev| set_device_id.set(event_target_value(&ev))>
                                <option value="">"Select device"</option>
                                {data.devices.iter().map(|d| {
                                    let id = d.id.clone();
                                    let label = format!("{} ({})", d.name, &d.id[..8]);
                                    view! { <option value=id>{label}</option> }
                                }).collect::<Vec<_>>()}
                            </select>
                        }
                    })}
                </Suspense>
            </div>
            <div class="form-group">
                <label>"Level"</label>
                <select on:change=move |ev| set_log_level.set(event_target_value(&ev))>
                    <option value="all">"All"</option>
                    <option value="error">"Error"</option>
                    <option value="warn">"Warning"</option>
                    <option value="info">"Info"</option>
                    <option value="debug">"Debug"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Search"</label>
                <input type="text" placeholder="Filter logs…" prop:value=search
                    on:input=move |ev| set_search.set(event_target_value(&ev))/>
            </div>
            <Show
                when=move || !connected.get()
                fallback=move || view! {
                    <button class="btn btn-danger btn-sm" style="align-self: flex-end" on:click=stop_streaming>"⏹ Stop"</button>
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
                <span style="font-size: 0.8rem; color: var(--text-muted)">
                    {move || format!("{} lines", filtered_lines().len())}
                </span>
            </div>
            <div class="log-viewer">
                {move || {
                    let lines = filtered_lines();
                    if lines.is_empty() {
                        view! { <div class="log-line" style="color: var(--text-muted)">"No log output. Select a device and start streaming."</div> }.into_any()
                    } else {
                        lines.into_iter().map(|l| {
                            let class = format!("log-line {}", l.level);
                            view! {
                                <div class=class>
                                    <span style="color: var(--text-muted); margin-right: 0.5rem">{l.timestamp}</span>
                                    <span style="margin-right: 0.5rem; text-transform: uppercase; font-weight: 600; width: 4ch; display: inline-block">{l.level.clone()}</span>
                                    {l.message}
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
    message: String,
}

fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
