use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

const SHELL_TERM_KEY: &str = "__ef_shell_xterm";
const SHELL_WS_KEY:  &str  = "__ef_shell_ws";

// ── Theme definitions ──────────────────────────────────────────────────────

struct Theme { name: &'static str, bg: &'static str, fg: &'static str, cursor: &'static str, sel: &'static str }

const THEMES: &[Theme] = &[
    Theme { name: "Ocean Dark",    bg: "#08111f", fg: "#d7e0ea", cursor: "#67e8f9", sel: "#264f78" },
    Theme { name: "Dracula",       bg: "#282a36", fg: "#f8f8f2", cursor: "#ff79c6", sel: "#44475a" },
    Theme { name: "Solarized Dark",bg: "#002b36", fg: "#839496", cursor: "#268bd2", sel: "#073642" },
    Theme { name: "Monokai",       bg: "#272822", fg: "#f8f8f2", cursor: "#f8f8f2", sel: "#49483e" },
    Theme { name: "Gruvbox Dark",  bg: "#282828", fg: "#ebdbb2", cursor: "#fe8019", sel: "#3c3836" },
    Theme { name: "Light",         bg: "#fafafa", fg: "#383a42", cursor: "#4078f2", sel: "#c8ccd4" },
    Theme { name: "Nord",          bg: "#2e3440", fg: "#d8dee9", cursor: "#88c0d0", sel: "#3b4252" },
    Theme { name: "Tokyo Night",   bg: "#1a1b26", fg: "#a9b1d6", cursor: "#7aa2f7", sel: "#283457" },
];

const FONTS: &[(&str, &str)] = &[
    ("Mono (system)", "ui-monospace, 'SF Mono', Menlo, Consolas, monospace"),
    ("JetBrains Mono", "'JetBrains Mono', monospace"),
    ("Fira Code",      "'Fira Code', monospace"),
    ("Hack",           "'Hack', monospace"),
    ("IBM Plex Mono",  "'IBM Plex Mono', monospace"),
    ("Cascadia Code",  "'Cascadia Code', monospace"),
];

// ── Page component ─────────────────────────────────────────────────────────

/// Standalone page: opened via `/virtual-iot/shell?device=<id>` (e.g. in a new tab).
#[component]
pub fn VirtualIotShellPage() -> impl IntoView {
    let params = leptos_router::hooks::use_query_map();
    let device_id = params.get_untracked().get("device").unwrap_or_default();

    view! {
        <div style="display:flex; flex-direction:column; gap:1rem; height:100%;">
            <div style="display:flex; justify-content:space-between; align-items:center;">
                <div>
                    <h1 style="margin-bottom:0.25rem;">"VeerOS Shell"</h1>
                    <p class="empty-state" style="margin:0; text-align:left;">
                        {format!("Virtual IoT device {}", device_id)}
                    </p>
                </div>
                <a href="/devices" class="btn btn-ghost btn-sm">"Back to Devices"</a>
            </div>
            <VirtualIotShellTerminal device_id=device_id />
        </div>
    }
}

// ── Terminal component ─────────────────────────────────────────────────────

/// Reusable terminal card (xterm.js + WebSocket to the veer-connect bridge).
/// Embeddable anywhere — e.g. in the Live Device Viewer as a tab.
#[component]
pub fn VirtualIotShellTerminal(device_id: String) -> impl IntoView {
    let device_id_for_effect = device_id.clone();
    let (connected, set_connected) = signal(false);
    let (term_ready, set_term_ready) = signal(false);
    // Reconnect trigger: bumping this re-runs the WebSocket effect.
    let (reconnect_tick, set_reconnect_tick) = signal(0u32);
    let socket_ref    = StoredValue::new(None::<web_sys::WebSocket>);
    let term_ref      = NodeRef::<leptos::html::Div>::new();

    // ── Toolbar state ──────────────────────────────────────────────────────
    let (font_size,   set_font_size)   = signal(13u32);
    let (theme_idx,   set_theme_idx)   = signal(0usize);
    let (font_idx,    set_font_idx)    = signal(0usize);

    // ── Helpers to call xterm API ──────────────────────────────────────────
    let term_write = |text: &str| {
        let Some(win) = web_sys::window() else { return };
        let Ok(term) = js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str(SHELL_TERM_KEY)) else { return };
        if term.is_undefined() || term.is_null() { return }
        if let Ok(f) = js_sys::Reflect::get(&term, &wasm_bindgen::JsValue::from_str("write")) {
            if let Ok(func) = f.dyn_into::<js_sys::Function>() {
                let _ = func.call1(&term, &wasm_bindgen::JsValue::from_str(text));
            }
        }
    };

    let term_write_bytes = |bytes: &js_sys::Uint8Array| {
        let Some(win) = web_sys::window() else { return };
        let Ok(term) = js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str(SHELL_TERM_KEY)) else { return };
        if term.is_undefined() || term.is_null() { return }
        if let Ok(f) = js_sys::Reflect::get(&term, &wasm_bindgen::JsValue::from_str("write")) {
            if let Ok(func) = f.dyn_into::<js_sys::Function>() {
                let _ = func.call1(&term, bytes);
            }
        }
    };

    let term_set_option = |key: &str, val: &str| {
        let js = format!(
            "(function(){{ var t = window.{k}; if(t && t.options) {{ t.options.{key} = {val}; try{{ if(window.__ef_shell_fit) window.__ef_shell_fit.fit(); }}catch(e){{}} }} }})()",
            k = SHELL_TERM_KEY, key = key, val = val
        );
        let _ = js_sys::eval(&js);
    };

    // ── Apply theme reactively ─────────────────────────────────────────────
    Effect::new(move || {
        let idx = theme_idx.get();
        if idx >= THEMES.len() { return }
        let t = &THEMES[idx];
        let js = format!(
            "(function(){{ var t = window.{k}; if(t && t.options) t.options.theme = {{ background:'{bg}', foreground:'{fg}', cursor:'{cu}', selectionBackground:'{sel}' }}; }})()",
            k=SHELL_TERM_KEY, bg=t.bg, fg=t.fg, cu=t.cursor, sel=t.sel
        );
        let _ = js_sys::eval(&js);
        // also update container background so padding area matches
        let _ = js_sys::eval(&format!(
            "var c=document.getElementById('ef-shell-term'); if(c) c.style.background='{bg}';",
            bg=t.bg
        ));
    });

    // ── Apply font size reactively ─────────────────────────────────────────
    Effect::new(move || {
        let sz = font_size.get();
        term_set_option("fontSize", &sz.to_string());
    });

    // ── Apply font family reactively ───────────────────────────────────────
    Effect::new(move || {
        let idx = font_idx.get();
        if idx >= FONTS.len() { return }
        let family = FONTS[idx].1;
        term_set_option("fontFamily", &format!("\"{}\"", family));
    });

    // ── Initialize xterm.js when the container mounts ─────────────────────
    let init_font_size  = font_size.get_untracked();
    let init_theme_idx  = theme_idx.get_untracked();
    let init_font_idx   = font_idx.get_untracked();
    let init_theme      = &THEMES[init_theme_idx];
    let init_font       = FONTS[init_font_idx].1;

    let init_bg  = init_theme.bg;
    let init_fg  = init_theme.fg;
    let init_cu  = init_theme.cursor;
    let init_sel = init_theme.sel;

    Effect::new(move || {
        let Some(_el) = term_ref.get() else { return };
        set_term_ready.set(false);
        let init_js = format!(
            r#"(function() {{
                if (typeof Terminal === 'undefined') return null;
                var existing = window.{term_key};
                if (existing) {{ try {{ existing.dispose(); }} catch(e) {{}} window.{term_key} = null; }}
                var term = new Terminal({{
                    cursorBlink: true,
                    fontSize: {sz},
                    fontFamily: "{font}",
                    theme: {{
                        background: '{bg}',
                        foreground: '{fg}',
                        cursor: '{cu}',
                        selectionBackground: '{sel}',
                    }},
                    scrollback: 10000,
                    convertEol: false,
                    allowProposedApi: true,
                }});
                var container = document.getElementById('ef-shell-term');
                if (!container) return null;
                term.open(container);
                if (typeof FitAddon !== 'undefined') {{
                    var fitAddon = new FitAddon.FitAddon();
                    term.loadAddon(fitAddon);
                    try {{ fitAddon.fit(); }} catch(e) {{}}
                    window.__ef_shell_fit = fitAddon;
                    try {{
                        var ro = new ResizeObserver(function() {{ try {{ fitAddon.fit(); }} catch(e) {{}} }});
                        ro.observe(container);
                        window.__ef_shell_ro = ro;
                    }} catch(e) {{}}
                }}
                if (typeof WebLinksAddon !== 'undefined') {{
                    term.loadAddon(new WebLinksAddon.WebLinksAddon());
                }}
                term.onData(function(data) {{
                    var ws = window.{ws_key};
                    if (ws && ws.readyState === 1) {{ ws.send(data); }}
                }});
                term.focus();
                window.{term_key} = term;
                return term;
            }})()"#,
            term_key = SHELL_TERM_KEY,
            ws_key   = SHELL_WS_KEY,
            sz       = init_font_size,
            font     = init_font,
            bg       = init_bg,
            fg       = init_fg,
            cu       = init_cu,
            sel      = init_sel,
        );
        match js_sys::eval(&init_js) {
            Ok(val) if !val.is_null() && !val.is_undefined() => set_term_ready.set(true),
            _ => set_term_ready.set(false),
        }
    });

    // ── Open the WebSocket (re-runs when reconnect_tick bumps) ────────────
    Effect::new(move || {
        // Subscribe to the reconnect trigger.
        let _tick = reconnect_tick.get();

        if device_id_for_effect.is_empty() {
            return;
        }
        if !term_ready.get() {
            return;
        }
        if term_ref.get().is_none() {
            return;
        }

        // Close any previous socket before opening a new one.
        if let Some(prev) = socket_ref.get_value() {
            let _ = prev.close();
            socket_ref.set_value(None);
        }
        set_connected.set(false);

        let ws_url = format!(
            "{}/devices/{}/virtual-iot/shell/ws",
            crate::api::ws_base_url(),
            device_id_for_effect
        );
        let ws = match web_sys::WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(_)  => return,
        };
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        if let Some(win) = web_sys::window() {
            let _ = js_sys::Reflect::set(&win, &wasm_bindgen::JsValue::from_str(SHELL_WS_KEY), &ws);
        }

        let onmessage = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let trimmed = text.trim_start();
                if trimmed.starts_with('{') && trimmed.contains("\"shell_ready\"") {
                    return; // skip JSON banner; xterm shows veer output right after
                }
                term_write(&text);
                return;
            }
            if let Ok(buf) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buf);
                term_write_bytes(&bytes);
            }
        });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let onopen = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
            set_connected.set(true);
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onclose = Closure::<dyn FnMut(_)>::new(move |event: web_sys::CloseEvent| {
            set_connected.set(false);
            let code   = event.code();
            let reason = event.reason();
            let msg = format!(
                "\r\n\x1b[2m[disconnected · code {code}{}]\x1b[0m\r\n",
                if reason.is_empty() { String::new() } else { format!(" · {reason}") }
            );
            term_write(&msg);

            // Auto-retry for unexpected closes so reconnect is resilient.
            if code != 1000 {
                if let Some(win) = web_sys::window() {
                    let set_reconnect_tick = set_reconnect_tick.clone();
                    let cb = Closure::once_into_js(move || {
                        set_reconnect_tick.update(|t| *t = t.wrapping_add(1));
                    });
                    let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                        cb.unchecked_ref(),
                        700,
                    );
                }
            }
        });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let onerror = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
            term_write("\r\n\x1b[31m[websocket error · check API is reachable]\x1b[0m\r\n");
        });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        socket_ref.set_value(Some(ws));
    });

    // ── Teardown on unmount ────────────────────────────────────────────────
    on_cleanup(move || {
        set_term_ready.set(false);
        if let Some(ws) = socket_ref.get_value() {
            let _ = ws.close();
        }
        let cleanup_js = format!(
            r#"(function() {{
                try {{ if (window.__ef_shell_ro)  {{ window.__ef_shell_ro.disconnect(); }} }} catch(e) {{}}
                try {{ if (window.{term_key})      {{ window.{term_key}.dispose(); }} }} catch(e) {{}}
                window.{term_key} = null;
                window.{ws_key}   = null;
                window.__ef_shell_fit = null;
                window.__ef_shell_ro  = null;
            }})()"#,
            term_key = SHELL_TERM_KEY,
            ws_key   = SHELL_WS_KEY,
        );
        let _ = js_sys::eval(&cleanup_js);
    });

    // ── Toolbar actions ────────────────────────────────────────────────────
    let clear_screen = move |_: web_sys::MouseEvent| {
        // ANSI ED2 (clear) + cursor home
        let _ = js_sys::eval(&format!(
            "var t=window.{k}; if(t) t.write('\\x1b[2J\\x1b[H');",
            k = SHELL_TERM_KEY
        ));
    };

    let reconnect = move |_: web_sys::MouseEvent| {
        // Bump the trigger so the WS effect tears down the old socket and
        // opens a fresh one with all handlers attached.
        set_reconnect_tick.update(|t| *t = t.wrapping_add(1));
    };

    let inc_font = move |_: web_sys::MouseEvent| {
        set_font_size.update(|s| if *s < 28 { *s += 1 });
    };
    let dec_font = move |_: web_sys::MouseEvent| {
        set_font_size.update(|s| if *s > 8 { *s -= 1 });
    };

    let focus_terminal = move |_: web_sys::MouseEvent| {
        let _ = js_sys::eval(&format!(
            "window.{k}.focus && window.{k}.focus()",
            k = SHELL_TERM_KEY
        ));
    };

    let theme_options = THEMES.iter().enumerate().map(|(i, t)| {
        view! { <option value={i.to_string()}>{t.name}</option> }
    }).collect::<Vec<_>>();

    let font_options = FONTS.iter().enumerate().map(|(i, (label, _))| {
        view! { <option value={i.to_string()}>{*label}</option> }
    }).collect::<Vec<_>>();

    // ── View ───────────────────────────────────────────────────────────────
    view! {
        <div class="card" style="flex:1; display:flex; flex-direction:column; min-height:0;">
            // ── Toolbar ──────────────────────────────────────────────────
            <div class="card-header" style="display:flex; flex-wrap:wrap; gap:0.5rem; align-items:center; padding:0.5rem 0.75rem;">

                // Status + title (single concrete view type so it swaps reactively)
                <span style="font-weight:600; margin-right:0.25rem;">
                    {move || {
                        let (color, label) = if connected.get() {
                            ("var(--color-success)", "\u{25cf} Connected")
                        } else {
                            ("var(--color-muted)", "\u{25cb} Connecting\u{2026}")
                        };
                        view! { <span style=format!("color:{color}")>{label}</span> }
                    }}
                </span>

                <div style="flex:1;"></div>

                // Clear screen
                <button
                    class="btn btn-ghost btn-sm"
                    title="Clear screen (Ctrl+L equivalent)"
                    on:click=clear_screen
                >"\u{1f5d1} Clear"</button>

                // Reconnect
                <button
                    class="btn btn-ghost btn-sm"
                    title="Reconnect WebSocket"
                    on:click=reconnect
                >"\u{21ba} Reconnect"</button>

                // Font size
                <div style="display:flex; align-items:center; gap:0.25rem;">
                    <button class="btn btn-ghost btn-sm" title="Decrease font size" on:click=dec_font>"\u{2212}"</button>
                    <span style="min-width:2.5rem; text-align:center; font-size:0.78rem;">
                        {move || format!("{}px", font_size.get())}
                    </span>
                    <button class="btn btn-ghost btn-sm" title="Increase font size" on:click=inc_font>"\u{002b}"</button>
                </div>

                // Theme selector
                <select
                    class="select select-sm"
                    style="font-size:0.78rem; padding:0.2rem 0.4rem;"
                    title="Color theme"
                    on:change=move |e| {
                        use wasm_bindgen::JsCast;
                        if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                            if let Ok(idx) = el.value().parse::<usize>() {
                                set_theme_idx.set(idx);
                            }
                        }
                    }
                >
                    {theme_options}
                </select>

                // Font family selector
                <select
                    class="select select-sm"
                    style="font-size:0.78rem; padding:0.2rem 0.4rem;"
                    title="Font family"
                    on:change=move |e| {
                        use wasm_bindgen::JsCast;
                        if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                            if let Ok(idx) = el.value().parse::<usize>() {
                                set_font_idx.set(idx);
                            }
                        }
                    }
                >
                    {font_options}
                </select>
            </div>

            // ── xterm container ───────────────────────────────────────────
            <div
                node_ref=term_ref
                id="ef-shell-term"
                on:click=focus_terminal
                style="margin:0; padding:0.5rem; flex:1; min-height:26rem; background:#08111f; overflow:hidden;"
            ></div>
        </div>
    }
}


