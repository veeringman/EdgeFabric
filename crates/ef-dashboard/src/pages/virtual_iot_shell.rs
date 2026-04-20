use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

#[component]
pub fn VirtualIotShellPage() -> impl IntoView {
    let params = leptos_router::hooks::use_query_map();
    let device_id = params.get_untracked().get("device").unwrap_or_default();
    let (output, set_output) = signal(String::new());
    let (input, set_input) = signal(String::new());
    let (connected, set_connected) = signal(false);
    let socket_ref = StoredValue::new(None::<web_sys::WebSocket>);

    Effect::new(move || {
        if device_id.is_empty() || connected.get() {
            return;
        }
        let host = web_sys::window()
            .and_then(|w| w.location().host().ok())
            .unwrap_or_else(|| "localhost:6060".to_string());
        let ws = match web_sys::WebSocket::new(&format!("ws://{host}/api/v1/devices/{}/virtual-iot/shell/ws", device_id)) {
            Ok(ws) => ws,
            Err(_) => return,
        };

        let onmessage = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
            if let Some(text) = event.data().as_string() {
                set_output.update(|buf| buf.push_str(&text));
            }
        });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let onopen = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::Event| {
            set_connected.set(true);
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onclose = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::CloseEvent| {
            set_connected.set(false);
        });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        socket_ref.set_value(Some(ws));
    });

    let send_line = move |_| {
        let line = input.get_untracked();
        if line.is_empty() {
            return;
        }
        if let Some(ws) = socket_ref.get_value() {
            let _ = ws.send_with_str(&(line.clone() + "\r"));
            set_output.update(|buf| {
                buf.push_str(&line);
                buf.push('\n');
            });
        }
        set_input.set(String::new());
    };

    view! {
        <div style="display:flex; flex-direction:column; gap:1rem; height:100%;">
            <div style="display:flex; justify-content:space-between; align-items:center;">
                <div>
                    <h1 style="margin-bottom:0.25rem;">"VeerOS Shell"</h1>
                    <p class="empty-state" style="margin:0; text-align:left;">{format!("Virtual IoT device {}", device_id)}</p>
                </div>
                <a href="/devices" class="btn btn-ghost btn-sm">"Back to Devices"</a>
            </div>

            <div class="card" style="flex:1; display:flex; flex-direction:column;">
                <div class="card-header"><h2>{move || if connected.get() { "Connected" } else { "Connecting…" }}</h2></div>
                <pre style="margin:0; padding:1rem; min-height:24rem; background:#08111f; color:#d7e0ea; overflow:auto; white-space:pre-wrap; font-family:monospace;">{move || output.get()}</pre>
                <div style="display:flex; gap:0.75rem; padding:1rem; border-top:1px solid rgba(255,255,255,0.08);">
                    <input class="form-control" style="flex:1;" prop:value=move || input.get() on:input=move |ev| set_input.set(event_target_value(&ev)) />
                    <button class="btn btn-primary btn-sm" on:click=send_line>"Send"</button>
                </div>
            </div>
        </div>
    }
}