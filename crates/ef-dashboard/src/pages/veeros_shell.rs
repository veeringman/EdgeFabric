use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

const SHELL_TERM_KEY: &str = "__ef_veeros_shell_xterm";
const SHELL_WS_KEY: &str = "__ef_veeros_shell_ws";

#[component]
pub fn VeerosShellPage() -> impl IntoView {
    let params = leptos_router::hooks::use_query_map();
    let map = params.get_untracked();

    let host = map.get("host").unwrap_or_else(|| "127.0.0.1".to_string());
    let port = map.get("port").unwrap_or_else(|| "22".to_string());
    let agent_id = map.get("agent_id").unwrap_or_default();

    let subtitle = if agent_id.is_empty() {
        format!("{host}:{port}")
    } else {
        format!("{host}:{port} via agent {agent_id}")
    };

    let host_q = host.clone();
    let port_q = port.clone();
    let agent_q = agent_id.clone();

    let ws_path = if agent_q.is_empty() {
        format!("/api/v1/veeros/shell/ws?host={host_q}&port={port_q}")
    } else {
        format!("/api/v1/veeros/shell/ws?host={host_q}&port={port_q}&agent_id={agent_q}")
    };

    view! {
        <div style="display:flex; flex-direction:column; gap:1rem; height:100%;">
            <div style="display:flex; justify-content:space-between; align-items:center;">
                <div>
                    <h1 style="margin-bottom:0.25rem;">"VeerOS Shell"</h1>
                    <p class="empty-state" style="margin:0; text-align:left;">{subtitle}</p>
                </div>
                <a href="/veeros/hosts" class="btn btn-ghost btn-sm">"Back to Hosts"</a>
            </div>
            <VeerosShellTerminal ws_path=ws_path />
        </div>
    }
}

#[component]
fn VeerosShellTerminal(ws_path: String) -> impl IntoView {
    let (connected, set_connected) = signal(false);
    let socket_ref = StoredValue::new(None::<web_sys::WebSocket>);
    let attempted = StoredValue::new(false);
    let term_ref = NodeRef::<leptos::html::Div>::new();

    let term_write = |text: &str| {
        let Some(win) = web_sys::window() else { return };
        let term = match js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str(SHELL_TERM_KEY)) {
            Ok(t) if !t.is_undefined() && !t.is_null() => t,
            _ => return,
        };
        if let Ok(write_fn) = js_sys::Reflect::get(&term, &wasm_bindgen::JsValue::from_str("write")) {
            if let Ok(func) = write_fn.dyn_into::<js_sys::Function>() {
                let _ = func.call1(&term, &wasm_bindgen::JsValue::from_str(text));
            }
        }
    };

    Effect::new(move || {
        let Some(_el) = term_ref.get() else { return };
        let init_js = format!(
            r#"(function() {{
                if (typeof Terminal === 'undefined') return null;
                var existing = window.{term_key};
                if (existing) {{ try {{ existing.dispose(); }} catch(e) {{}} window.{term_key} = null; }}
                var term = new Terminal({{
                    cursorBlink: true,
                    fontSize: 13,
                    fontFamily: "ui-monospace, 'JetBrains Mono', 'Fira Code', 'SF Mono', Menlo, Consolas, monospace",
                    theme: {{
                        background: '#08111f',
                        foreground: '#d7e0ea',
                        cursor: '#67e8f9',
                        selectionBackground: '#264f78',
                    }},
                    scrollback: 10000,
                    convertEol: false,
                    allowProposedApi: true,
                }});
                var container = document.getElementById('ef-veeros-shell-term');
                if (!container) return null;
                term.open(container);
                if (typeof FitAddon !== 'undefined') {{
                    var fitAddon = new FitAddon.FitAddon();
                    term.loadAddon(fitAddon);
                    try {{ fitAddon.fit(); }} catch(e) {{}}
                    window.__ef_veeros_shell_fit = fitAddon;
                    try {{
                        var ro = new ResizeObserver(function() {{ try {{ fitAddon.fit(); }} catch(e) {{}} }});
                        ro.observe(container);
                        window.__ef_veeros_shell_ro = ro;
                    }} catch(e) {{}}
                }}
                if (typeof WebLinksAddon !== 'undefined') {{
                    term.loadAddon(new WebLinksAddon.WebLinksAddon());
                }}
                term.onData(function(data) {{
                    var ws = window.{ws_key};
                    if (ws && ws.readyState === 1) {{
                        ws.send(data);
                    }}
                }});
                term.focus();
                window.{term_key} = term;
                return term;
            }})()"#,
            term_key = SHELL_TERM_KEY,
            ws_key = SHELL_WS_KEY,
        );
        let _ = js_sys::eval(&init_js);
    });

    Effect::new(move || {
        if attempted.get_value() {
            return;
        }
        if term_ref.get().is_none() {
            return;
        }
        attempted.set_value(true);

        let host = web_sys::window()
            .and_then(|w| w.location().hostname().ok())
            .unwrap_or_else(|| "localhost".to_string());
        let ws_url = format!("ws://{host}:6060{}", ws_path);

        let ws = match web_sys::WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(_) => return,
        };
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        if let Some(win) = web_sys::window() {
            let _ = js_sys::Reflect::set(&win, &wasm_bindgen::JsValue::from_str(SHELL_WS_KEY), &ws);
        }

        let onmessage = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let trimmed = text.trim_start();
                if trimmed.starts_with('{') && trimmed.contains("\"shell_ready\"") {
                    return;
                }
                term_write(&text);
                return;
            }

            if let Ok(buf) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buf);
                let mut out = vec![0u8; bytes.length() as usize];
                bytes.copy_to(&mut out);
                term_write(&String::from_utf8_lossy(&out));
            }
        });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let onopen = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::Event| {
            set_connected.set(true);
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onclose = Closure::<dyn FnMut(_)>::new(move |event: web_sys::CloseEvent| {
            set_connected.set(false);
            let code = event.code();
            let reason = event.reason();
            let msg = format!(
                "\r\n\x1b[2m[disconnected · code {code}{}]\x1b[0m\r\n",
                if reason.is_empty() { String::new() } else { format!(" · {reason}") }
            );
            term_write(&msg);
        });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let onerror = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::Event| {
            term_write("\r\n\x1b[31m[websocket error · check API is reachable]\x1b[0m\r\n");
        });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        socket_ref.set_value(Some(ws));
    });

    on_cleanup(move || {
        if let Some(ws) = socket_ref.get_value() {
            let _ = ws.close();
        }
        let cleanup_js = format!(
            r#"(function() {{
                try {{ if (window.__ef_veeros_shell_ro) {{ window.__ef_veeros_shell_ro.disconnect(); }} }} catch(e) {{}}
                try {{ if (window.{term_key}) {{ window.{term_key}.dispose(); }} }} catch(e) {{}}
                window.{term_key} = null;
                window.{ws_key} = null;
                window.__ef_veeros_shell_fit = null;
                window.__ef_veeros_shell_ro = null;
            }})()"#,
            term_key = SHELL_TERM_KEY,
            ws_key = SHELL_WS_KEY,
        );
        let _ = js_sys::eval(&cleanup_js);
    });

    let focus_terminal = move |_: web_sys::MouseEvent| {
        let _ = js_sys::eval(&format!(
            "window.{}.focus && window.{}.focus()",
            SHELL_TERM_KEY, SHELL_TERM_KEY
        ));
    };

    view! {
        <div class="card" style="flex:1; display:flex; flex-direction:column; min-height:0;">
            <div class="card-header" style="display:flex; justify-content:space-between; align-items:center;">
                <h2>{move || if connected.get() { "Connected" } else { "Connecting…" }}</h2>
                <span class="empty-state" style="margin:0;">
                    "Full xterm · vi / nano / clear / colors · Ctrl+C to interrupt"
                </span>
            </div>
            <div
                node_ref=term_ref
                id="ef-veeros-shell-term"
                on:click=focus_terminal
                style="margin:0; padding:0.5rem; flex:1; min-height:26rem; background:#08111f; overflow:hidden;"
            ></div>
        </div>
    }
}
