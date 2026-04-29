use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{
    self, DeviceListResponse, SensorInjectRequest, VirtualIotConfigResponse,
    VirtualIotEventDefinition, VirtualIotSensorLogEntry,
};
use crate::board_config;
use crate::components::board_layout::render_catalog_board_layout;
use crate::components::device_icon::DeviceOsIcon;
use crate::components::status_badge::StatusBadge;
use crate::pages::virtual_iot_shell::VirtualIotShellTerminal;

struct TerminalTheme {
    name: &'static str,
    bg: &'static str,
    fg: &'static str,
    cursor: &'static str,
    sel: &'static str,
}

const TERMINAL_THEMES: &[TerminalTheme] = &[
    TerminalTheme { name: "Ocean Dark", bg: "#08111f", fg: "#d7e0ea", cursor: "#67e8f9", sel: "#264f78" },
    TerminalTheme { name: "Dracula", bg: "#282a36", fg: "#f8f8f2", cursor: "#ff79c6", sel: "#44475a" },
    TerminalTheme { name: "Solarized Dark", bg: "#002b36", fg: "#839496", cursor: "#268bd2", sel: "#073642" },
    TerminalTheme { name: "Monokai", bg: "#272822", fg: "#f8f8f2", cursor: "#f8f8f2", sel: "#49483e" },
    TerminalTheme { name: "Nord", bg: "#2e3440", fg: "#d8dee9", cursor: "#88c0d0", sel: "#3b4252" },
    TerminalTheme { name: "Light", bg: "#fafafa", fg: "#383a42", cursor: "#4078f2", sel: "#c8ccd4" },
];

const TERMINAL_FONTS: &[(&str, &str)] = &[
    ("Mono (system)", "ui-monospace, 'SF Mono', Menlo, Consolas, monospace"),
    ("JetBrains Mono", "'JetBrains Mono', monospace"),
    ("Fira Code", "'Fira Code', monospace"),
    ("Hack", "'Hack', monospace"),
    ("Cascadia Code", "'Cascadia Code', monospace"),
];

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

fn is_tablet_bezel_style(style: &str) -> bool {
    style.starts_with("ipad") || matches!(style, "galaxy-tab" | "pixel-tablet" | "android-tablet")
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
                                            let os_label = if d.os == "android" { "Android".to_string() }
                                                else if d.os == "ios" { "iOS".to_string() }
                                                else if d.os == "iot" { "IoT".to_string() }
                                                else if d.os == "ssh" { "SSH".to_string() }
                                                else if d.os == "virtual_iot" { "Virtual IoT".to_string() }
                                                else { d.os.clone() };
                                            let status = d.status.clone();
                                            view! {
                                                <div class="device-card" on:click=move |_| {
                                                    set_selected_device.set(Some(sel.clone()));
                                                }>
                                                    <div class="device-card-icon"><DeviceOsIcon os=d.os.clone() class="os-icon-lg"/></div>
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
    let (term_maximized, set_term_maximized) = signal(false);
    let (reconnect_tick, set_reconnect_tick) = signal(0u32);
    let (term_font_size, set_term_font_size) = signal(14u32);
    let (term_theme_idx, set_term_theme_idx) = signal(0usize);
    let (term_font_idx, set_term_font_idx) = signal(1usize);

    // Log panel signals
    let (log_lines, set_log_lines) = signal(Vec::<(String, String, String)>::new()); // (level, tag, message)
    let (log_open, set_log_open) = signal(false);
    let (log_connected, set_log_connected) = signal(false);
    let (log_filter, set_log_filter) = signal(String::from("all"));
    let (log_ws_handle, set_log_ws_handle) = signal(Option::<web_sys::WebSocket>::None);

    let initial_bezel = bezel_from_caps(&device_capabilities, &device_os, &device_os_version);
    let default_landscape = initial_bezel.starts_with("ipad")
        || matches!(
            initial_bezel.as_str(),
            "galaxy-tab" | "pixel-tablet" | "android-tablet"
        );
    let (bezel_style, set_bezel_style) = signal(initial_bezel);

    // Orientation toggle (portrait / landscape) — iOS & Android only.
    // Tablets default to landscape; phones default to portrait.
    let (landscape, set_landscape) = signal(default_landscape);

    // Watch pairing signals
    let (watch_style, set_watch_style) = signal(String::from("none"));

    // Drag state for moving phone/watch previews around the live viewer.
    let (phone_pos, set_phone_pos) = signal((0.0f64, 0.0f64));
    let (phone_dragging, set_phone_dragging) = signal(false);
    let (phone_drag_origin, set_phone_drag_origin) = signal((0.0f64, 0.0f64));
    let (phone_drag_start_pos, set_phone_drag_start_pos) = signal((0.0f64, 0.0f64));
    let (phone_drag_start_rect, set_phone_drag_start_rect) = signal((0.0f64, 0.0f64, 0.0f64, 0.0f64));

    let (watch_pos, set_watch_pos) = signal((0.0f64, 0.0f64));
    let (watch_dragging, set_watch_dragging) = signal(false);
    let (watch_drag_origin, set_watch_drag_origin) = signal((0.0f64, 0.0f64));
    let (watch_drag_start_pos, set_watch_drag_start_pos) = signal((0.0f64, 0.0f64));
    let (watch_drag_start_rect, set_watch_drag_start_rect) = signal((0.0f64, 0.0f64, 0.0f64, 0.0f64));

    let (details_pos, set_details_pos) = signal((0.0f64, 0.0f64));
    let (details_dragging, set_details_dragging) = signal(false);
    let (details_drag_origin, set_details_drag_origin) = signal((0.0f64, 0.0f64));
    let (details_drag_start_pos, set_details_drag_start_pos) = signal((0.0f64, 0.0f64));
    let (details_drag_start_rect, set_details_drag_start_rect) = signal((0.0f64, 0.0f64, 0.0f64, 0.0f64));

    let (guide_pos, set_guide_pos) = signal((0.0f64, 0.0f64));
    let (guide_dragging, set_guide_dragging) = signal(false);
    let (guide_drag_origin, set_guide_drag_origin) = signal((0.0f64, 0.0f64));
    let (guide_drag_start_pos, set_guide_drag_start_pos) = signal((0.0f64, 0.0f64));
    let (guide_drag_start_rect, set_guide_drag_start_rect) = signal((0.0f64, 0.0f64, 0.0f64, 0.0f64));

    let (logs_pos, set_logs_pos) = signal((0.0f64, 0.0f64));
    let (logs_dragging, set_logs_dragging) = signal(false);
    let (logs_drag_origin, set_logs_drag_origin) = signal((0.0f64, 0.0f64));
    let (logs_drag_start_pos, set_logs_drag_start_pos) = signal((0.0f64, 0.0f64));
    let (logs_drag_start_rect, set_logs_drag_start_rect) = signal((0.0f64, 0.0f64, 0.0f64, 0.0f64));

    let dev_id = device_id.clone();
    let log_dev_id = device_id.clone();
    let is_android = device_os == "android";
    let is_ios = device_os == "ios";
    let is_iot = device_os == "iot";
    let is_ssh = device_os == "ssh";
    let is_virtual_iot = device_os == "virtual_iot";
    let is_terminal = is_iot || is_ssh;

    // Pre-clone for branches that need them
    let iot_os_label = format!("{} {}", device_os, device_os_version);
    let phone_os_label = iot_os_label.clone();
    let iot_serial_display = device_serial.clone().unwrap_or_else(|| "—".into());
    let phone_serial_display = device_serial.unwrap_or_else(|| "—".into());
    let phone_kind = device_kind.clone();
    let iot_name = device_name.clone();
    let phone_name = device_name.clone();
    let phone_name2 = device_name.clone();
    let bezel_dev_id = device_id.clone();

    // FPS counter
    let last_frame_time = std::cell::Cell::new(0.0f64);
    let fps_counter = std::rc::Rc::new(std::cell::RefCell::new(Vec::<f64>::new()));

    let term_set_option = |key: &str, val: &str| {
        let js = format!(
            "(function(){{ var t = window.__ef_xterm; if(t && t.options) {{ t.options.{key} = {val}; try{{ if(window.__ef_fit) window.__ef_fit.fit(); }}catch(e){{}} }} }})()",
        );
        let _ = js_sys::eval(&js);
    };

    Effect::new(move || {
        let size = term_font_size.get();
        term_set_option("fontSize", &size.to_string());
    });

    Effect::new(move || {
        let idx = term_font_idx.get();
        if idx >= TERMINAL_FONTS.len() {
            return;
        }
        let family = TERMINAL_FONTS[idx].1;
        term_set_option("fontFamily", &format!("\"{}\"", family));
    });

    Effect::new(move || {
        let idx = term_theme_idx.get();
        if idx >= TERMINAL_THEMES.len() {
            return;
        }
        let t = &TERMINAL_THEMES[idx];
        let js = format!(
            "(function(){{ var term=window.__ef_xterm; if(term && term.options) term.options.theme = {{ background:'{bg}', foreground:'{fg}', cursor:'{cu}', selectionBackground:'{sel}' }}; var c=document.getElementById('ef-terminal-container'); if(c) c.style.background='{bg}'; }})()",
            bg = t.bg,
            fg = t.fg,
            cu = t.cursor,
            sel = t.sel
        );
        let _ = js_sys::eval(&js);
    });

    Effect::new(move || {
        let _reconnect = reconnect_tick.get();

        if is_terminal {
            if let Some(ws) = ws_handle.get_untracked() {
                let _ = ws.close();
            }
            set_ws_handle.set(None);
            set_connected.set(false);
        }

        if is_terminal && term_ref.get().is_none() {
            return;
        }

        let dev_id = dev_id.clone();
        let set_connected = set_connected.clone();
        let set_error = set_error.clone();
        let set_frame_count = set_frame_count.clone();
        let set_ws_handle = set_ws_handle.clone();
        let set_resolution = set_resolution.clone();
        let set_fps = set_fps.clone();
        let fps_counter = fps_counter.clone();
        let img_ref = img_ref;
        let term_ref = term_ref;

        leptos::task::spawn_local(async move {
            if is_terminal {
                let font_size = term_font_size.get_untracked();
                let theme_idx = term_theme_idx.get_untracked();
                let font_idx = term_font_idx.get_untracked();
                let theme = &TERMINAL_THEMES[theme_idx.min(TERMINAL_THEMES.len().saturating_sub(1))];
                let font = TERMINAL_FONTS[font_idx.min(TERMINAL_FONTS.len().saturating_sub(1))].1;
                let init_js = format!(r#"(function() {{
                    if (typeof Terminal === 'undefined') return null;
                    var container = document.getElementById('ef-terminal-container');
                    if (!container) return null;
                    try {{ if (window.__ef_ro) window.__ef_ro.disconnect(); }} catch (e) {{}}
                    try {{ if (window.__ef_xterm) window.__ef_xterm.dispose(); }} catch (e) {{}}
                    window.__ef_ro = null;
                    window.__ef_fit = null;
                    window.__ef_xterm = null;
                    window.__ef_xterm_hooked = false;

                    var term = new Terminal({{
                        cursorBlink: true,
                        fontSize: {font_size},
                        fontFamily: "{font}",
                        theme: {{
                            background: '{bg}',
                            foreground: '{fg}',
                            cursor: '{cu}',
                            selectionBackground: '{sel}',
                        }},
                        scrollback: 10000,
                        convertEol: false,
                    }});
                    term.open(container);
                    if (typeof FitAddon !== 'undefined') {{
                        var fitAddon = new FitAddon.FitAddon();
                        term.loadAddon(fitAddon);
                        try {{ fitAddon.fit(); }} catch (e) {{}}
                        window.__ef_fit = fitAddon;
                        try {{
                            var ro = new ResizeObserver(function() {{ try {{ fitAddon.fit(); }} catch (e) {{}} }});
                            ro.observe(container);
                            window.__ef_ro = ro;
                        }} catch (e) {{}}
                    }}
                    if (typeof WebLinksAddon !== 'undefined') {{
                        term.loadAddon(new WebLinksAddon.WebLinksAddon());
                    }}
                    try {{ term.focus(); }} catch (e) {{}}
                    window.__ef_xterm = term;
                    return window.__ef_xterm;
                }})()"#,
                    font_size = font_size,
                    font = font,
                    bg = theme.bg,
                    fg = theme.fg,
                    cu = theme.cursor,
                    sel = theme.sel,
                );
                let _ = js_sys::eval(&init_js);
                let _ = term_ref.get_untracked();
            }

            let ws_url = format!("{}/devices/{dev_id}/stream", crate::api::ws_base_url());

            let ws = match web_sys::WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(_) => {
                    set_error.set(Some("Failed to create WebSocket".into()));
                    return;
                }
            };
            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

            if is_terminal {
                let win = web_sys::window().unwrap();
                let _ = js_sys::Reflect::set(
                    &win,
                    &wasm_bindgen::JsValue::from_str("__ef_ws"),
                    &ws,
                );
                let hook_js = r#"(function() {
                    if (!window.__ef_xterm || window.__ef_xterm_hooked) return;
                    window.__ef_xterm_hooked = true;
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
            }

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
                            Some("error") => {
                                if let Some(message) = msg.get("message").and_then(|m| m.as_str()) {
                                    set_error.set(Some(message.to_string()));
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

            let onclose = Closure::wrap(Box::new(move |_: web_sys::Event| {
                set_connected.set(false);
            }) as Box<dyn FnMut(_)>);
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            onclose.forget();

            set_ws_handle.set(Some(ws));
        });
    });

    on_cleanup(move || {
        if let Some(ws) = ws_handle.get_untracked() {
            let _ = ws.close();
        }
        let _ = js_sys::eval(
            "(function(){ try{ if(window.__ef_ro) window.__ef_ro.disconnect(); }catch(e){} try{ if(window.__ef_xterm) window.__ef_xterm.dispose(); }catch(e){} window.__ef_ro=null; window.__ef_fit=null; window.__ef_ws=null; window.__ef_xterm=null; window.__ef_xterm_hooked=false; })()"
        );
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
            let ws_url = format!("{}/devices/{log_id}/logs", crate::api::ws_base_url());

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
        let scale_w = dev_w as f64;
        let scale_h = dev_h as f64;

        // When visually rotated 90° the img bounding rect is already in rotated-space.
        // Tablets are landscape by default in bezel style; phones are portrait by default.
        // So "effective rotation" differs by bezel type.
        let bezel_now = bezel_style.get_untracked();
        let rotated = if is_tablet_bezel_style(&bezel_now) {
            !landscape.get_untracked()
        } else {
            landscape.get_untracked()
        };

        if rotated {
            Some(((rel_y * scale_w) as i32, ((1.0 - rel_x) * scale_h) as i32))
        } else {
            Some(((rel_x * scale_w) as i32, (rel_y * scale_h) as i32))
        }
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

    let clamp_drag_delta = |start_left: f64, start_top: f64, width: f64, height: f64, dx: f64, dy: f64| -> (f64, f64) {
        let (vw, vh) = web_sys::window()
            .and_then(|w| {
                let vw = w.inner_width().ok()?.as_f64()?;
                let vh = w.inner_height().ok()?.as_f64()?;
                Some((vw, vh))
            })
            .unwrap_or((1920.0, 1080.0));

        let padding = 8.0;
        let min_left = padding;
        let min_top = padding;
        let max_left = (vw - width - padding).max(min_left);
        let max_top = (vh - height - padding).max(min_top);

        let proposed_left = start_left + dx;
        let proposed_top = start_top + dy;

        let clamped_left = proposed_left.max(min_left).min(max_left);
        let clamped_top = proposed_top.max(min_top).min(max_top);

        (clamped_left - start_left, clamped_top - start_top)
    };

    // Drag handlers (separate from device input pointer handlers)
    let on_phone_drag_start = move |evt: web_sys::PointerEvent| {
        if let Some(target) = evt.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            // Keep taps/swipes on the live screen for input events, not dragging.
            if target.closest(".phone-screen").ok().flatten().is_some() {
                return;
            }
        }
        evt.prevent_default();
        evt.stop_propagation();
        set_phone_dragging.set(true);
        set_phone_drag_origin.set((evt.client_x() as f64, evt.client_y() as f64));
        set_phone_drag_start_pos.set(phone_pos.get_untracked());
        if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            if let Some(shell) = target.closest(".draggable-shell").ok().flatten() {
                let rect = shell.get_bounding_client_rect();
                set_phone_drag_start_rect.set((rect.left(), rect.top(), rect.width(), rect.height()));
            }
        }
        if let Some(target) = evt.current_target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(evt.pointer_id());
            }
        }
    };

    let on_phone_drag_move = move |evt: web_sys::PointerEvent| {
        if !phone_dragging.get_untracked() {
            return;
        }
        evt.prevent_default();
        let (sx, sy) = phone_drag_origin.get_untracked();
        let (px, py) = phone_drag_start_pos.get_untracked();
        let dx = evt.client_x() as f64 - sx;
        let dy = evt.client_y() as f64 - sy;
        let (left, top, w, h) = phone_drag_start_rect.get_untracked();
        let (cdx, cdy) = clamp_drag_delta(left, top, w, h, dx, dy);
        set_phone_pos.set((px + cdx, py + cdy));
    };

    let on_phone_drag_end = move |evt: web_sys::PointerEvent| {
        if phone_dragging.get_untracked() {
            set_phone_dragging.set(false);
            if let Some(target) = evt.current_target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    let _ = el.release_pointer_capture(evt.pointer_id());
                }
            }
        }
    };

    let on_watch_drag_start = move |evt: web_sys::PointerEvent| {
        let mut allow_drag = false;
        if let Some(target) = evt.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            allow_drag = target
                .closest(".watch-band-top, .watch-band-bottom, .band-texture")
                .ok()
                .flatten()
                .is_some();
        }
        if !allow_drag {
            return;
        }
        evt.prevent_default();
        evt.stop_propagation();
        set_watch_dragging.set(true);
        set_watch_drag_origin.set((evt.client_x() as f64, evt.client_y() as f64));
        set_watch_drag_start_pos.set(watch_pos.get_untracked());
        if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            if let Some(shell) = target.closest(".draggable-shell").ok().flatten() {
                let rect = shell.get_bounding_client_rect();
                set_watch_drag_start_rect.set((rect.left(), rect.top(), rect.width(), rect.height()));
            }
        }
        if let Some(target) = evt.current_target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(evt.pointer_id());
            }
        }
    };

    let on_watch_drag_move = move |evt: web_sys::PointerEvent| {
        if !watch_dragging.get_untracked() {
            return;
        }
        evt.prevent_default();
        let (sx, sy) = watch_drag_origin.get_untracked();
        let (wx, wy) = watch_drag_start_pos.get_untracked();
        let dx = evt.client_x() as f64 - sx;
        let dy = evt.client_y() as f64 - sy;
        let (left, top, w, h) = watch_drag_start_rect.get_untracked();
        let (cdx, cdy) = clamp_drag_delta(left, top, w, h, dx, dy);
        set_watch_pos.set((wx + cdx, wy + cdy));
    };

    let on_watch_drag_end = move |evt: web_sys::PointerEvent| {
        if watch_dragging.get_untracked() {
            set_watch_dragging.set(false);
            if let Some(target) = evt.current_target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    let _ = el.release_pointer_capture(evt.pointer_id());
                }
            }
        }
    };

    let on_details_drag_start = move |evt: web_sys::PointerEvent| {
        evt.prevent_default();
        evt.stop_propagation();
        set_details_dragging.set(true);
        set_details_drag_origin.set((evt.client_x() as f64, evt.client_y() as f64));
        set_details_drag_start_pos.set(details_pos.get_untracked());
        if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            if let Some(panel) = target.closest(".draggable-panel").ok().flatten() {
                let rect = panel.get_bounding_client_rect();
                set_details_drag_start_rect.set((rect.left(), rect.top(), rect.width(), rect.height()));
            }
        }
        if let Some(target) = evt.current_target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(evt.pointer_id());
            }
        }
    };

    let on_details_drag_move = move |evt: web_sys::PointerEvent| {
        if !details_dragging.get_untracked() {
            return;
        }
        evt.prevent_default();
        let (sx, sy) = details_drag_origin.get_untracked();
        let (px, py) = details_drag_start_pos.get_untracked();
        let dx = evt.client_x() as f64 - sx;
        let dy = evt.client_y() as f64 - sy;
        let (left, top, w, h) = details_drag_start_rect.get_untracked();
        let (cdx, cdy) = clamp_drag_delta(left, top, w, h, dx, dy);
        set_details_pos.set((px + cdx, py + cdy));
    };

    let on_details_drag_end = move |evt: web_sys::PointerEvent| {
        if details_dragging.get_untracked() {
            set_details_dragging.set(false);
            if let Some(target) = evt.current_target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    let _ = el.release_pointer_capture(evt.pointer_id());
                }
            }
        }
    };

    let on_guide_drag_start = move |evt: web_sys::PointerEvent| {
        evt.prevent_default();
        evt.stop_propagation();
        set_guide_dragging.set(true);
        set_guide_drag_origin.set((evt.client_x() as f64, evt.client_y() as f64));
        set_guide_drag_start_pos.set(guide_pos.get_untracked());
        if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            if let Some(panel) = target.closest(".draggable-panel").ok().flatten() {
                let rect = panel.get_bounding_client_rect();
                set_guide_drag_start_rect.set((rect.left(), rect.top(), rect.width(), rect.height()));
            }
        }
        if let Some(target) = evt.current_target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(evt.pointer_id());
            }
        }
    };

    let on_guide_drag_move = move |evt: web_sys::PointerEvent| {
        if !guide_dragging.get_untracked() {
            return;
        }
        evt.prevent_default();
        let (sx, sy) = guide_drag_origin.get_untracked();
        let (px, py) = guide_drag_start_pos.get_untracked();
        let dx = evt.client_x() as f64 - sx;
        let dy = evt.client_y() as f64 - sy;
        let (left, top, w, h) = guide_drag_start_rect.get_untracked();
        let (cdx, cdy) = clamp_drag_delta(left, top, w, h, dx, dy);
        set_guide_pos.set((px + cdx, py + cdy));
    };

    let on_guide_drag_end = move |evt: web_sys::PointerEvent| {
        if guide_dragging.get_untracked() {
            set_guide_dragging.set(false);
            if let Some(target) = evt.current_target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    let _ = el.release_pointer_capture(evt.pointer_id());
                }
            }
        }
    };

    let on_logs_drag_start = move |evt: web_sys::PointerEvent| {
        evt.prevent_default();
        evt.stop_propagation();
        set_logs_dragging.set(true);
        set_logs_drag_origin.set((evt.client_x() as f64, evt.client_y() as f64));
        set_logs_drag_start_pos.set(logs_pos.get_untracked());
        if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            if let Some(panel) = target.closest(".draggable-panel").ok().flatten() {
                let rect = panel.get_bounding_client_rect();
                set_logs_drag_start_rect.set((rect.left(), rect.top(), rect.width(), rect.height()));
            }
        }
        if let Some(target) = evt.current_target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(evt.pointer_id());
            }
        }
    };

    let on_logs_drag_move = move |evt: web_sys::PointerEvent| {
        if !logs_dragging.get_untracked() {
            return;
        }
        evt.prevent_default();
        let (sx, sy) = logs_drag_origin.get_untracked();
        let (px, py) = logs_drag_start_pos.get_untracked();
        let dx = evt.client_x() as f64 - sx;
        let dy = evt.client_y() as f64 - sy;
        let (left, top, w, h) = logs_drag_start_rect.get_untracked();
        let (cdx, cdy) = clamp_drag_delta(left, top, w, h, dx, dy);
        set_logs_pos.set((px + cdx, py + cdy));
    };

    let on_logs_drag_end = move |evt: web_sys::PointerEvent| {
        if logs_dragging.get_untracked() {
            set_logs_dragging.set(false);
            if let Some(target) = evt.current_target() {
                if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                    let _ = el.release_pointer_capture(evt.pointer_id());
                }
            }
        }
    };

    let reset_layout_positions = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        set_phone_pos.set((0.0, 0.0));
        set_watch_pos.set((0.0, 0.0));
        set_details_pos.set((0.0, 0.0));
        set_guide_pos.set((0.0, 0.0));
        set_logs_pos.set((0.0, 0.0));
    };

    let snap_layout_positions = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        const GRID: f64 = 24.0;
        let snap = |v: f64| (v / GRID).round() * GRID;
        let (x1, y1) = phone_pos.get_untracked();
        let (x2, y2) = watch_pos.get_untracked();
        let (x3, y3) = details_pos.get_untracked();
        let (x4, y4) = guide_pos.get_untracked();
        let (x5, y5) = logs_pos.get_untracked();
        set_phone_pos.set((snap(x1), snap(y1)));
        set_watch_pos.set((snap(x2), snap(y2)));
        set_details_pos.set((snap(x3), snap(y3)));
        set_guide_pos.set((snap(x4), snap(y4)));
        set_logs_pos.set((snap(x5), snap(y5)));
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
            {if is_virtual_iot {
                let viot_id = device_id.clone();
                let viot_name = device_name.clone();
                let close_cb = Callback::new(move |_| on_close());
                view! {
                    <VirtualIotViewer
                        device_id=viot_id
                        device_name=viot_name
                        on_close=close_cb
                    />
                }.into_any()
            } else if is_terminal {
                view! {
                    <div
                        class="card ef-term-card"
                        style=move || if term_maximized.get() {
                            "position:fixed; top:0; left:0; right:0; bottom:0; width:100vw; height:100vh; z-index:2147483647; margin:0; border-radius:0; display:flex; flex-direction:column; min-width:0; background:var(--bg-primary,#0d1117);".to_string()
                        } else {
                            "flex: 1; min-width: 0;".to_string()
                        }
                    >
                        <div class="card-header">
                            <h2>
                                <DeviceOsIcon os=if is_ssh { "ssh".to_string() } else { "iot".to_string() } class="os-icon-title"/>
                                " "
                                {if is_ssh { "SSH Terminal" } else { "Serial Console" }}
                            </h2>
                            <div style="display:flex; gap:0.5rem;">
                                <button
                                    class="btn btn-ghost btn-sm"
                                    title=move || if term_maximized.get() { "Exit fullscreen (Esc)" } else { "Maximize" }
                                    on:click=move |_| {
                                        let next = !term_maximized.get_untracked();
                                        set_term_maximized.set(next);
                                        // Use Fullscreen API for true topmost + whole-screen; fall back to CSS overlay.
                                        let js = if next {
                                            r#"
                                            (function(){
                                              try {
                                                var el = document.querySelector('.ef-term-card') || document.getElementById('ef-terminal-container');
                                                if (el) {
                                                  var req = el.requestFullscreen || el.webkitRequestFullscreen || el.msRequestFullscreen;
                                                  if (req) req.call(el).catch(function(){});
                                                }
                                              } catch(e){}
                                              setTimeout(function(){
                                                try { if (window.__ef_fit) window.__ef_fit.fit(); } catch(e){}
                                                try {
                                                  if (window.__ef_xterm && window.__ef_ws && window.__ef_ws.readyState === 1) {
                                                    window.__ef_ws.send(JSON.stringify({
                                                      type:'terminal_resize',
                                                      cols: window.__ef_xterm.cols,
                                                      rows: window.__ef_xterm.rows
                                                    }));
                                                  }
                                                } catch(e){}
                                              }, 120);
                                            })();
                                            "#
                                        } else {
                                            r#"
                                            (function(){
                                              try {
                                                if (document.fullscreenElement || document.webkitFullscreenElement) {
                                                  var ex = document.exitFullscreen || document.webkitExitFullscreen || document.msExitFullscreen;
                                                  if (ex) ex.call(document).catch(function(){});
                                                }
                                              } catch(e){}
                                              setTimeout(function(){
                                                try { if (window.__ef_fit) window.__ef_fit.fit(); } catch(e){}
                                                try {
                                                  if (window.__ef_xterm && window.__ef_ws && window.__ef_ws.readyState === 1) {
                                                    window.__ef_ws.send(JSON.stringify({
                                                      type:'terminal_resize',
                                                      cols: window.__ef_xterm.cols,
                                                      rows: window.__ef_xterm.rows
                                                    }));
                                                  }
                                                } catch(e){}
                                              }, 120);
                                            })();
                                            "#
                                        };
                                        let _ = js_sys::eval(js);
                                    }
                                >
                                    {move || if term_maximized.get() { "🗗 Restore" } else { "⛶ Maximize" }}
                                </button>
                                <button class="btn btn-ghost btn-sm" on:click=move |_| {
                                    // Clean up xterm instance
                                    let win = web_sys::window().unwrap();
                                    let _ = js_sys::Reflect::delete_property(&win, &wasm_bindgen::JsValue::from_str("__ef_ws"));
                                    let _ = js_sys::Reflect::delete_property(&win, &wasm_bindgen::JsValue::from_str("__ef_xterm"));
                                    let _ = js_sys::Reflect::delete_property(&win, &wasm_bindgen::JsValue::from_str("__ef_xterm_hooked"));
                                    let _ = js_sys::Reflect::delete_property(&win, &wasm_bindgen::JsValue::from_str("__ef_fit"));
                                    let _ = js_sys::Reflect::delete_property(&win, &wasm_bindgen::JsValue::from_str("__ef_ro"));
                                    if let Some(ws) = ws_handle.get_untracked() {
                                        let _ = ws.close();
                                    }
                                    set_term_maximized.set(false);
                                    on_close();
                                }>"✕ Close"</button>
                            </div>
                        </div>

                        <div style="display:flex; flex-wrap:wrap; align-items:center; gap:0.5rem; margin-bottom:0.6rem; padding:0.55rem 0.6rem; border:1px solid rgba(148,163,184,0.25); border-radius:10px; background:linear-gradient(135deg, rgba(15,23,42,0.82), rgba(30,41,59,0.72));">
                            <span style="font-weight:600; margin-right:0.4rem; color: #dbeafe;">
                                {move || if connected.get() { "● Connected" } else { if is_ssh { "○ Connecting SSH…" } else { "○ Connecting Serial…" } }}
                            </span>
                            <button class="btn btn-ghost btn-sm" title="Clear terminal" on:click=move |_| {
                                let _ = js_sys::eval("var t=window.__ef_xterm; if(t) t.write('\\x1b[2J\\x1b[H');");
                            }>
                                "Clear"
                            </button>
                            <button class="btn btn-ghost btn-sm" title="Reconnect stream" on:click=move |_| {
                                set_reconnect_tick.update(|t| *t = t.wrapping_add(1));
                            }>
                                "Reconnect"
                            </button>
                            <div style="display:flex; align-items:center; gap:0.25rem;">
                                <button class="btn btn-ghost btn-sm" title="Decrease font" on:click=move |_| {
                                    set_term_font_size.update(|s| if *s > 10 { *s -= 1 });
                                }>
                                    "−"
                                </button>
                                <span style="font-size:0.76rem; min-width:2.6rem; text-align:center; color:#cbd5e1;">{move || format!("{}px", term_font_size.get())}</span>
                                <button class="btn btn-ghost btn-sm" title="Increase font" on:click=move |_| {
                                    set_term_font_size.update(|s| if *s < 28 { *s += 1 });
                                }>
                                    "+"
                                </button>
                            </div>
                            <select class="select select-sm" style="font-size:0.76rem; padding:0.2rem 0.35rem;" title="Theme" on:change=move |ev| {
                                if let Ok(idx) = event_target_value(&ev).parse::<usize>() {
                                    set_term_theme_idx.set(idx);
                                }
                            }>
                                {TERMINAL_THEMES.iter().enumerate().map(|(i, t)| view!{
                                    <option value={i.to_string()} selected={move || term_theme_idx.get() == i}>{t.name}</option>
                                }).collect::<Vec<_>>()}
                            </select>
                            <select class="select select-sm" style="font-size:0.76rem; padding:0.2rem 0.35rem;" title="Font" on:change=move |ev| {
                                if let Ok(idx) = event_target_value(&ev).parse::<usize>() {
                                    set_term_font_idx.set(idx);
                                }
                            }>
                                {TERMINAL_FONTS.iter().enumerate().map(|(i, f)| view!{
                                    <option value={i.to_string()} selected={move || term_font_idx.get() == i}>{f.0}</option>
                                }).collect::<Vec<_>>()}
                            </select>
                            {move || error.get().map(|msg| view! { <span style="color:#fca5a5; font-size:0.78rem;">{msg}</span> })}
                        </div>

                        <div
                            node_ref=term_ref
                            id="ef-terminal-container"
                            style=move || if term_maximized.get() {
                                "flex:1; min-height:0; background:#08111f; border-radius:12px; overflow:hidden; border:1px solid rgba(103,232,249,0.18); box-shadow: 0 12px 30px rgba(2,6,23,0.55), inset 0 1px 0 rgba(148,163,184,0.12);".to_string()
                            } else {
                                "min-height: 400px; max-height: 600px; background: #08111f; border-radius: 12px; overflow: hidden; border:1px solid rgba(103,232,249,0.18); box-shadow: 0 12px 30px rgba(2,6,23,0.45), inset 0 1px 0 rgba(148,163,184,0.12);".to_string()
                            }
                        ></div>

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
            <div
                class={move || if phone_dragging.get() { "draggable-shell phone-shell dragging" } else { "draggable-shell phone-shell" }}
                style=move || {
                    let (x, y) = phone_pos.get();
                    format!("transform: translate({x:.1}px, {y:.1}px);")
                }
            >
                <div class={move || {
                    let bezel = bezel_style.get();
                    if is_tablet_bezel_style(&bezel) {
                        if landscape.get() { "phone-wrapper" } else { "phone-wrapper portrait-for-tablet" }
                    } else if landscape.get() {
                        "phone-wrapper landscape-orientation"
                    } else {
                        "phone-wrapper"
                    }
                }}>
                <div
                    class={move || if is_android { format!("phone-frame android {}", bezel_style.get()) } else { format!("phone-frame ios {}", bezel_style.get()) }}
                    on:pointerdown=on_phone_drag_start
                    on:pointermove=on_phone_drag_move
                    on:pointerup=on_phone_drag_end
                    on:pointercancel=on_phone_drag_end
                >
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
            </div>

            // ── Paired Watch ──
            <Show when=move || watch_style.get() != "none">
                <div
                    class={move || if watch_dragging.get() { "draggable-shell watch-shell dragging" } else { "draggable-shell watch-shell" }}
                    style=move || {
                        let (x, y) = watch_pos.get();
                        format!("transform: translate({x:.1}px, {y:.1}px);")
                    }
                >
                <div class="watch-wrapper"
                    on:pointerdown=on_watch_drag_start
                    on:pointermove=on_watch_drag_move
                    on:pointerup=on_watch_drag_end
                    on:pointercancel=on_watch_drag_end
                >
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
                </div>
            </Show>

            // ── Info panel ──
            <div class="viewer-panel">
                <div
                    class={move || if details_dragging.get() { "card draggable-panel dragging" } else { "card draggable-panel" }}
                    style=move || {
                        let (x, y) = details_pos.get();
                        format!("transform: translate({x:.1}px, {y:.1}px);")
                    }
                >
                    <div class="card-header">
                        <h2>{phone_name2}</h2>
                        <div style="display:flex; align-items:center; gap:0.35rem;">
                            <button class="btn btn-ghost btn-sm drag-icon-btn"
                                title="Drag panel"
                                on:pointerdown=on_details_drag_start
                                on:pointermove=on_details_drag_move
                                on:pointerup=on_details_drag_end
                                on:pointercancel=on_details_drag_end
                            >
                                <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true"><circle cx="7" cy="5" r="1.2" fill="currentColor"/><circle cx="13" cy="5" r="1.2" fill="currentColor"/><circle cx="7" cy="10" r="1.2" fill="currentColor"/><circle cx="13" cy="10" r="1.2" fill="currentColor"/><circle cx="7" cy="15" r="1.2" fill="currentColor"/><circle cx="13" cy="15" r="1.2" fill="currentColor"/></svg>
                            </button>
                            <button class="btn btn-ghost btn-sm" on:click=move |_| {
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

                    <div style="display:flex; gap:0.35rem; margin-bottom:0.4rem; justify-content:flex-end;">
                        <button class="btn btn-ghost btn-sm" title="Snap dragged items to grid" on:click=snap_layout_positions>
                            <svg viewBox="0 0 20 20" width="12" height="12" aria-hidden="true"><path d="M2 2h6v6H2V2zm10 0h6v6h-6V2zM2 12h6v6H2v-6zm10 0h6v6h-6v-6z" fill="currentColor"/></svg>
                        </button>
                        <button class="btn btn-ghost btn-sm" title="Reset dragged items" on:click=reset_layout_positions>
                            <svg viewBox="0 0 20 20" width="12" height="12" aria-hidden="true"><path d="M10 3a7 7 0 1 1-6.8 8.7h2.1A5 5 0 1 0 10 5c-1.4 0-2.7.6-3.6 1.5L8 8H2V2l2.1 2.1A7 7 0 0 1 10 3z" fill="currentColor"/></svg>
                        </button>
                    </div>

                    <div class="stream-info">
                        // ── Orientation toggle (iOS / Android only) ──
                        {if is_ios || is_android {
                            view! {
                                <div class="info-row" style="align-items: center;">
                                    <span class="info-label">"Orientation"</span>
                                    <div style="display: flex; gap: 0.35rem;">
                                        <button
                                            class={move || if !landscape.get() { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" }}
                                            on:click=move |ev: web_sys::MouseEvent| { ev.prevent_default(); ev.stop_propagation(); set_landscape.set(false); }
                                            title="Portrait"
                                        >
                                            <svg viewBox="0 0 20 20" width="13" height="13" style="vertical-align: middle; margin-right: 3px;"><rect x="5" y="1" width="10" height="18" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>
                                            "Portrait"
                                        </button>
                                        <button
                                            class={move || if landscape.get() { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" }}
                                            on:click=move |ev: web_sys::MouseEvent| { ev.prevent_default(); ev.stop_propagation(); set_landscape.set(true); }
                                            title="Landscape"
                                        >
                                            <svg viewBox="0 0 20 20" width="13" height="13" style="vertical-align: middle; margin-right: 3px;"><rect x="1" y="5" width="18" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>
                                            "Landscape"
                                        </button>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
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
                                                set_landscape.set(val.starts_with("ipad"));
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
                                                set_landscape.set(matches!(val.as_str(), "galaxy-tab" | "pixel-tablet" | "android-tablet"));
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

                <div
                    class={move || if guide_dragging.get() { "card draggable-panel dragging" } else { "card draggable-panel" }}
                    style=move || {
                        let (x, y) = guide_pos.get();
                        format!("transform: translate({x:.1}px, {y:.1}px);")
                    }
                >
                    <div class="card-header">
                        <h2>"Interaction Guide"</h2>
                        <button class="btn btn-ghost btn-sm drag-icon-btn"
                            title="Drag panel"
                            on:pointerdown=on_guide_drag_start
                            on:pointermove=on_guide_drag_move
                            on:pointerup=on_guide_drag_end
                            on:pointercancel=on_guide_drag_end
                        >
                            <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true"><circle cx="7" cy="5" r="1.2" fill="currentColor"/><circle cx="13" cy="5" r="1.2" fill="currentColor"/><circle cx="7" cy="10" r="1.2" fill="currentColor"/><circle cx="13" cy="10" r="1.2" fill="currentColor"/><circle cx="7" cy="15" r="1.2" fill="currentColor"/><circle cx="13" cy="15" r="1.2" fill="currentColor"/></svg>
                        </button>
                    </div>
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
                <div
                    class={move || if logs_dragging.get() { "card draggable-panel dragging" } else { "card draggable-panel" }}
                    style=move || {
                        let (x, y) = logs_pos.get();
                        format!("transform: translate({x:.1}px, {y:.1}px);")
                    }
                >
                    <div class="card-header" style="cursor: pointer; user-select: none;"
                        on:click=move |_| set_log_open.update(|v| *v = !*v)
                    >
                        <h2>{move || if log_open.get() { "▾ Device Logs" } else { "▸ Device Logs" }}</h2>
                        <div style="display: flex; align-items: center; gap: 0.5rem;">
                            <button class="btn btn-ghost btn-sm drag-icon-btn"
                                title="Drag panel"
                                on:pointerdown=on_logs_drag_start
                                on:pointermove=on_logs_drag_move
                                on:pointerup=on_logs_drag_end
                                on:pointercancel=on_logs_drag_end
                            >
                                <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true"><circle cx="7" cy="5" r="1.2" fill="currentColor"/><circle cx="13" cy="5" r="1.2" fill="currentColor"/><circle cx="7" cy="10" r="1.2" fill="currentColor"/><circle cx="13" cy="10" r="1.2" fill="currentColor"/><circle cx="7" cy="15" r="1.2" fill="currentColor"/><circle cx="13" cy="15" r="1.2" fill="currentColor"/></svg>
                            </button>
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

fn virtual_iot_sensor_label(sensor: &str) -> &'static str {
    board_config::sensor_label(sensor)
}

fn virtual_iot_sensor_preview(sensor: &str) -> &'static str {
    board_config::sensor_profile(sensor).preview.as_str()
}

fn virtual_iot_sensor_realism(sensor: &str) -> (&'static str, &'static str) {
    let profile = board_config::sensor_profile(sensor);
    (profile.module_profile.as_str(), profile.expected_range.as_str())
}

fn virtual_iot_controller_name(controller_type: &str) -> &'static str {
    board_config::controller_name(controller_type)
}

#[derive(Clone)]
struct WiringMapEntry {
    sensor_key: String,
    sensor_label: String,
    pin: String,
    bus: String,
    note: String,
}

fn wire_color_for_bus(pin: &str, bus: &str) -> &'static str {
    board_config::bus_wire_color(pin, bus).unwrap_or("#94a3b8")
}

fn virtual_iot_default_wiring(controller_type: &str, enabled_sensors: &[String]) -> Vec<WiringMapEntry> {
    board_config::default_wiring(controller_type, enabled_sensors)
        .into_iter()
        .map(|entry| WiringMapEntry {
            sensor_key: entry.sensor_key,
            sensor_label: entry.sensor_label,
            pin: entry.pin,
            bus: entry.bus,
            note: entry.note,
        })
        .collect()
}

fn virtual_iot_primary_pin(pin: &str) -> String {
    pin.split('/').next().unwrap_or(pin).trim().to_string()
}

fn virtual_iot_pin_anchor_y(controller_type: &str, pin: &str) -> f32 {
    let wiring_family = board_config::find_controller(controller_type).wiring_family.as_str();
    if let Some(y) = board_config::pin_anchor_for_family(wiring_family, pin) {
        return y;
    }
    match pin {
        "GPIO2" => 120.0,
        "GPIO3" => 145.0,
        "GPIO21" | "A4" | "GP4" | "PB9" | "PIN_1" => 120.0,
        "GPIO22" | "A5" | "GP5" | "PB8" | "PIN_2" => 145.0,
        "GPIO8" => 170.0,
        "GPIO34" | "A0" | "GP26" | "PA0" | "PIN_5" => 170.0,
        "GPIO14" => 195.0,
        "GPIO16" | "D2" | "GP8" | "PA2" | "PIN_3" => 195.0,
        "GPIO15" => 220.0,
        "GPIO17" | "D3" | "GP9" | "PA3" | "PIN_4" => 220.0,
        "GPIO27" => 245.0,
        "GPIO35" | "A1" | "GP27" | "PA1" => 245.0,
        "GPIO32" | "A2" | "GP28" | "PA4" => 270.0,
        "GPIO22 (pin 15)" => 270.0,
        "GPIO4" | "D4" | "GP10" | "PB0" | "PIN_6" => 295.0,
        "GPIO5" | "D5" | "GP11" | "PB1" | "PIN_7" => 320.0,
        "GPIO12" => 320.0,
        "GPIO13" => 345.0,
        "GPIO18" | "D6" | "GP12" | "PB2" => 345.0,
        "GPIO36" | "A3" | "PA5" => 370.0,
        "GPIO39" | "PA6" => 395.0,
        _ => 255.0,
    }
}

#[component]
fn VirtualIotBoard(
    controller_type: String,
    /// Each entry: (pin_name, wire_color_hex, is_left_rail).
    #[prop(optional)] connected_pins: Vec<(String, String, bool)>,
) -> impl IntoView {
    let profile = board_config::find_controller(&controller_type);
    let chip_label = profile.chip_label.as_str();
    let has_wifi = profile.has_wifi;
    let pin_count = profile.pin_count;
    let board_color = profile.board_color.as_str();
    let board_title = profile.name.as_str();
    let wiring_family = profile.wiring_family.clone();

    let inner = if let Some(layout_view) = render_catalog_board_layout(profile, chip_label) {
        layout_view
    } else {
        view! {
            <>
                <rect x="22" y="24" width="216" height="312" rx="12" fill={board_color} stroke="#1f2937" stroke-width="2"/>
                <rect x="70" y="90" width="120" height="82" rx="5" fill="#111827" stroke="#5f6872" stroke-width="1.2" filter="url(#chip-shadow)"/>
                <text x="130" y="134" text-anchor="middle" fill="#d1dae2" font-size="9" font-family="monospace">{chip_label}</text>
                {(0..pin_count).map(|i| {
                    let y = 50 + i * 16;
                    view! {
                        <>
                            <rect x="28" y={y} width="12" height="8" rx="1" fill="#d4af37"/>
                            <rect x="220" y={y} width="12" height="8" rx="1" fill="#d4af37"/>
                        </>
                    }
                }).collect::<Vec<_>>()}
            </>
        }.into_any()
    };

    view! {
        <svg viewBox="0 0 260 360" class="virtual-iot-board-svg" xmlns="http://www.w3.org/2000/svg">
            <defs>
                <linearGradient id="pcb-sheen" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0%" stop-color="rgba(255,255,255,0.18)"/>
                    <stop offset="28%" stop-color="rgba(255,255,255,0.02)"/>
                    <stop offset="100%" stop-color="rgba(0,0,0,0.18)"/>
                </linearGradient>
                <pattern id="pcb-grid" width="18" height="18" patternUnits="userSpaceOnUse">
                    <path d="M18 0H0V18" fill="none" stroke="rgba(255,255,255,0.03)" stroke-width="1"/>
                </pattern>
                <filter id="chip-shadow" x="-20%" y="-20%" width="140%" height="140%">
                    <feDropShadow dx="0" dy="3" stdDeviation="3" flood-color="rgba(0,0,0,0.45)"/>
                </filter>
                // Solder pad — gold ring with subtle highlight
                <radialGradient id="pcb-pad-gold" cx="35%" cy="32%" r="65%">
                    <stop offset="0%" stop-color="#fde68a"/>
                    <stop offset="55%" stop-color="#d4a83a"/>
                    <stop offset="100%" stop-color="#7a5510"/>
                </radialGradient>
                // IC plastic body — dark molded with top sheen
                <linearGradient id="pcb-chip-body" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="#2a2f3a"/>
                    <stop offset="35%" stop-color="#15181f"/>
                    <stop offset="100%" stop-color="#080a0f"/>
                </linearGradient>
                // USB-C metallic shell
                <linearGradient id="pcb-usbc-shell" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%"  stop-color="#dde3eb"/>
                    <stop offset="40%" stop-color="#8d97a6"/>
                    <stop offset="60%" stop-color="#5b6472"/>
                    <stop offset="100%" stop-color="#1c2230"/>
                </linearGradient>
                // LED specular highlight (small white blob, reused)
                <radialGradient id="pcb-led-spec" cx="35%" cy="30%" r="55%">
                    <stop offset="0%"  stop-color="rgba(255,255,255,0.95)"/>
                    <stop offset="45%" stop-color="rgba(255,255,255,0.25)"/>
                    <stop offset="100%" stop-color="rgba(255,255,255,0)"/>
                </radialGradient>
                // LED dome shading — dark on bottom right for depth
                <radialGradient id="pcb-led-dome" cx="35%" cy="30%" r="80%">
                    <stop offset="0%"  stop-color="rgba(255,255,255,0.55)"/>
                    <stop offset="55%" stop-color="rgba(255,255,255,0)"/>
                    <stop offset="100%" stop-color="rgba(0,0,0,0.6)"/>
                </radialGradient>
                // Board edge bevel
                <linearGradient id="pcb-edge" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%"  stop-color="rgba(255,255,255,0.22)"/>
                    <stop offset="50%" stop-color="rgba(255,255,255,0)"/>
                    <stop offset="100%" stop-color="rgba(0,0,0,0.45)"/>
                </linearGradient>
                // Soldermask gloss — strong top-down highlight for 3D feel
                <linearGradient id="pcb-mask-gloss" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%"  stop-color="rgba(255,255,255,0.18)"/>
                    <stop offset="35%" stop-color="rgba(255,255,255,0.04)"/>
                    <stop offset="100%" stop-color="rgba(0,0,0,0.35)"/>
                </linearGradient>
                // Board drop-shadow (for the whole PCB to lift off the chassis)
                <filter id="pcb-lift" x="-15%" y="-15%" width="130%" height="130%">
                    <feDropShadow dx="0" dy="6" stdDeviation="5" flood-color="rgba(0,0,0,0.55)"/>
                </filter>
                // Silk-screen trace pattern
                <pattern id="pcb-traces" width="36" height="36" patternUnits="userSpaceOnUse">
                    <path d="M0 12H14M22 12H36M0 24H8M16 24H36" fill="none" stroke="rgba(96,165,250,0.06)" stroke-width="0.6"/>
                </pattern>
                // Copper-trace pattern — visible thin gold lines that look like routed traces
                <pattern id="pcb-copper" width="48" height="48" patternUnits="userSpaceOnUse">
                    <path d="M0 8 H18 L24 14 H48 M0 22 H10 L16 28 H34 L40 22 H48 M0 36 H22 L28 42 H48"
                          fill="none" stroke="rgba(212,175,55,0.18)" stroke-width="0.7" stroke-linecap="round" stroke-linejoin="round"/>
                </pattern>
            </defs>

            // Board substrate with sheen + traces + bevel
            <g filter="url(#pcb-lift)">
                <rect x="10" y="10" width="240" height="340" rx="18" fill={board_color} stroke="#0a1018" stroke-width="2"/>
                // Inner soldermask — slightly inset and a touch lighter so the green pops
                <rect x="13" y="13" width="234" height="334" rx="16" fill="rgba(255,255,255,0.05)"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-copper)"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-traces)"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-grid)"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-mask-gloss)"/>
                // Hard inner edge — subtle dark border to give the board a milled bevel
                <rect x="10.5" y="10.5" width="239" height="339" rx="17.5" fill="none" stroke="rgba(0,0,0,0.55)" stroke-width="1"/>
                // Outer top bevel highlight — bright thin line on the top edge
                <rect x="11" y="11" width="238" height="338" rx="17" fill="none" stroke="rgba(255,255,255,0.18)" stroke-width="0.6"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-edge)" opacity="0.55"/>
            </g>

            // Mounting holes — silver outer ring + dark drilled center
            <g>
                <circle cx="28" cy="28" r="6" fill="#0c111a" stroke="#a9b4c2" stroke-width="1.4"/>
                <circle cx="28" cy="28" r="2.4" fill="#04070c"/>
                <circle cx="232" cy="28" r="6" fill="#0c111a" stroke="#a9b4c2" stroke-width="1.4"/>
                <circle cx="232" cy="28" r="2.4" fill="#04070c"/>
                <circle cx="28" cy="332" r="6" fill="#0c111a" stroke="#a9b4c2" stroke-width="1.4"/>
                <circle cx="28" cy="332" r="2.4" fill="#04070c"/>
                <circle cx="232" cy="332" r="6" fill="#0c111a" stroke="#a9b4c2" stroke-width="1.4"/>
                <circle cx="232" cy="332" r="2.4" fill="#04070c"/>
            </g>

            <text x="130" y="30" text-anchor="middle" fill="rgba(255,255,255,0.85)" font-size="8" font-family="sans-serif">{board_title}</text>

            {inner}

            {if has_wifi {
                view! {
                    <g transform="translate(176, 320)">
                        <rect x="0" y="0" width="54" height="18" rx="4" fill="#0b315a" stroke="#2b6cb0" stroke-width="0.8"/>
                        <text x="27" y="12.5" text-anchor="middle" fill="#a5d8ff" font-size="7" font-family="sans-serif">"Wireless"</text>
                    </g>
                }.into_any()
            } else {
                view! { <g/> }.into_any()
            }}

            // ── Connected GPIO knobs (drawn LAST so they sit on top of pads) ──
            <defs>
                <radialGradient id="knob-spec" cx="35%" cy="30%" r="60%">
                    <stop offset="0%" stop-color="rgba(255,255,255,0.95)"/>
                    <stop offset="55%" stop-color="rgba(255,255,255,0.15)"/>
                    <stop offset="100%" stop-color="rgba(255,255,255,0)"/>
                </radialGradient>
                <radialGradient id="knob-shade" cx="35%" cy="30%" r="80%">
                    <stop offset="0%" stop-color="rgba(255,255,255,0.45)"/>
                    <stop offset="55%" stop-color="rgba(255,255,255,0)"/>
                    <stop offset="100%" stop-color="rgba(0,0,0,0.6)"/>
                </radialGradient>
            </defs>
            {{
                let mut knobs: Vec<(f32, f32, String)> = Vec::new();
                for (pin_name, color, is_left) in &connected_pins {
                    if let Some(y) = board_config::pin_anchor_for_family(&wiring_family, pin_name) {
                        let kx: f32 = if *is_left { 40.0 } else { 220.0 };
                        knobs.push((kx, y, color.clone()));
                    }
                }
                view! {
                    <g>
                        {knobs.into_iter().map(|(kx, ky, color)| view! {
                            <g>
                                // Drop shadow under the knob
                                <ellipse cx={kx + 0.6} cy={ky + 1.4} rx="5.4" ry="1.7" fill="rgba(0,0,0,0.6)"/>
                                // Colored body
                                <circle cx={kx} cy={ky} r="5.0" fill={color.clone()} stroke="#04070c" stroke-width="0.6"/>
                                // Domed shading
                                <circle cx={kx} cy={ky} r="5.0" fill="url(#knob-shade)"/>
                                // Specular highlight
                                <circle cx={kx - 1.6} cy={ky - 1.7} r="2.0" fill="url(#knob-spec)"/>
                                // Drilled center where the wire enters
                                <circle cx={kx} cy={ky} r="1.1" fill="#04070c"/>
                            </g>
                        }).collect::<Vec<_>>()}
                    </g>
                }
            }}
        </svg>
    }
}

#[component]
fn VirtualIotViewer(
    device_id: String,
    device_name: String,
    on_close: Callback<()>,
) -> impl IntoView {
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);
    let (controller_type, set_controller_type) = signal(String::from("esp32-devkit"));
    let (sim_rate_ms, set_sim_rate_ms) = signal(500i32);
    let (enabled_sensors, set_enabled_sensors) = signal(Vec::<String>::new());
    let (sensor_values, set_sensor_values) = signal(HashMap::<String, String>::new());
    let (event_definitions, set_event_definitions) = signal(Vec::<VirtualIotEventDefinition>::new());
    let (sensor_log, set_sensor_log) = signal(Vec::<VirtualIotSensorLogEntry>::new());
    let (backend_attached, set_backend_attached) = signal(false);
    let (injecting_sensor, set_injecting_sensor) = signal(Option::<String>::None);
    let (inject_error, set_inject_error) = signal(Option::<String>::None);
    let (script_source, set_script_source) = signal(String::from(
        "# VeerOS shell script\n# Available: gpio write|read|toggle, sensor set|list, let, print, if, while, for\ngpio write 15 1\nsleep 200\ngpio write 15 0\nsleep 200\ngpio write 15 1\n",
    ));
    let (script_busy, set_script_busy) = signal(false);
    let (script_status, set_script_status) = signal(Option::<String>::None);
    let (active_tab, set_active_tab) = signal("detail".to_string());
    // Once user opens the shell tab, keep the terminal mounted so switching
    // back to "Detail" and returning doesn't kill the WS / xterm session.
    let (shell_opened, set_shell_opened) = signal(false);
    let device_id_value = StoredValue::new(device_id.clone());

    // Free-form scene layout: each item (controller + each sensor module) stores [x, y, w, h]
    // in container-relative px. Seeded on first render per enabled-sensors list.
    let scene_rects = RwSignal::new(HashMap::<String, [f64; 4]>::new());
    let scene_ref = NodeRef::<leptos::html::Div>::new();

    let fetch_device_id = device_id.clone();
    Effect::new(move || {
        let device_id = fetch_device_id.clone();
        spawn_local(async move {
            match api::get::<VirtualIotConfigResponse>(&format!("/devices/{device_id}/virtual-iot/config")).await {
                Ok(resp) => {
                    set_controller_type.set(resp.config.controller_type);
                    set_sim_rate_ms.set(resp.config.sim_rate_ms);
                    set_event_definitions.set(resp.config.event_definitions);
                    if let Ok(sensors) = serde_json::from_value::<Vec<String>>(resp.config.enabled_sensors) {
                        let defaults = virtual_iot_default_sensor_values(&sensors);
                        let persisted = virtual_iot_sensor_state_map(&resp.config.sensor_state);
                        let merged = defaults
                            .into_iter()
                            .map(|(key, default_value)| {
                                let value = persisted.get(&key).cloned().unwrap_or(default_value);
                                (key, value)
                            })
                            .collect::<HashMap<_, _>>();
                        set_sensor_values.set(merged);
                        set_enabled_sensors.set(sensors);
                    }
                    set_sensor_log.set(resp.config.sensor_log);
                    set_backend_attached.set(resp.config.backend_id.is_some());
                    set_loading.set(false);
                }
                Err(err) => {
                    set_error.set(Some(err));
                    set_loading.set(false);
                }
            }
        });
    });

    // ── Sensor Stream WebSocket ────────────────────────────────────
    // Connects to /api/v1/devices/{id}/virtual-iot/sensors/stream and
    // pushes live sensor_frame events into the sensor_values signal.
    // Works for both simulated (software noise) and UI-inject sources.
    {
        let stream_device_id = device_id.clone();
        Effect::new(move || {
            let device_id = stream_device_id.clone();
            spawn_local(async move {
                let ws_url = format!(
                    "{}/devices/{device_id}/virtual-iot/sensors/stream",
                    crate::api::ws_base_url()
                );

                let ws = match web_sys::WebSocket::new(&ws_url) {
                    Ok(ws) => ws,
                    Err(_) => return,
                };

                let onmessage_set_values = set_sensor_values.clone();
                let onmessage_set_log = set_sensor_log.clone();
                let onmessage = Closure::wrap(Box::new(move |evt: web_sys::MessageEvent| {
                    if let Ok(text) = evt.data().dyn_into::<js_sys::JsString>() {
                        let s: String = text.into();
                        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&s) {
                            if msg.get("type").and_then(|t| t.as_str()) == Some("sensor_frame") {
                                let sensor = msg.get("sensor").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let value  = msg.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let source = msg.get("source").and_then(|v| v.as_str()).unwrap_or("stream").to_string();
                                let ts     = msg.get("ts").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                if !sensor.is_empty() {
                                    onmessage_set_values.update(|values| {
                                        values.insert(sensor.clone(), value.clone());
                                    });
                                    onmessage_set_log.update(|log| {
                                        log.push(VirtualIotSensorLogEntry {
                                            ts,
                                            sensor: sensor.clone(),
                                            value: value.clone(),
                                            source,
                                        });
                                        if log.len() > 500 {
                                            log.drain(..100);
                                        }
                                    });
                                }
                            }
                        }
                    }
                }) as Box<dyn FnMut(_)>);
                ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
                onmessage.forget();

                // Keep the WebSocket handle alive for the component lifetime.
                // A closure that captures `ws` and does nothing is enough.
                let _ws_keep = ws;
            });
        });
    }

    let controller_name = move || virtual_iot_controller_name(&controller_type.get()).to_string();
    let sensor_count = move || enabled_sensors.get().len();
    let scene_modules = move || {
        virtual_iot_scene_modules(
            &controller_type.get(),
            &enabled_sensors.get(),
            &sensor_values.get(),
        )
    };
    let board_alert = move || virtual_iot_controller_has_alert(&sensor_values.get());

    // Seed scene positions for any new items (controller + each module) as sensors load/change.
    Effect::new(move || {
        let modules = scene_modules();
        scene_rects.update(|rects| {
            rects.entry(String::from("__controller")).or_insert([340.0, 60.0, 380.0, 640.0]);
            for m in &modules {
                let idx_on_side = modules
                    .iter()
                    .filter(|x| x.side == m.side)
                    .position(|x| x.sensor_key == m.sensor_key)
                    .unwrap_or(0) as f64;
                let (x, y) = match m.side {
                    VirtualIotSceneSide::Left  => (20.0,  40.0 + idx_on_side * 190.0),
                    VirtualIotSceneSide::Right => (720.0, 40.0 + idx_on_side * 190.0),
                };
                rects.entry(m.sensor_key.clone()).or_insert([x, y, 260.0, 180.0]);
            }
        });
    });

    // Reset all scene positions to seeded defaults.
    let reset_layout = move || {
        scene_rects.set(HashMap::new());
    };

    let inject_device_id = device_id.clone();
    let inject_sensor = Callback::new(move |(sensor, value, source): (String, String, Option<String>)| {
        set_inject_error.set(None);
        set_injecting_sensor.set(Some(sensor.clone()));
        set_sensor_values.update(|values| {
            values.insert(sensor.clone(), value.clone());
        });

        let device_id = inject_device_id.clone();
        spawn_local(async move {
            let request = SensorInjectRequest {
                sensor: sensor.clone(),
                value: value.clone(),
                source: source.clone(),
            };
            let result = api::post::<SensorInjectRequest, serde_json::Value>(
                &format!("/devices/{device_id}/virtual-iot/sensors"),
                &request,
            )
            .await;

            set_injecting_sensor.set(None);
            match result {
                Ok(payload) => {
                    if let Some(log_value) = payload.get("sensor_log") {
                        if let Ok(next_log) = serde_json::from_value::<Vec<VirtualIotSensorLogEntry>>(log_value.clone()) {
                            set_sensor_log.set(next_log);
                        }
                    }
                    if let Some(state_value) = payload.get("sensor_state") {
                        let next_state = virtual_iot_sensor_state_map(state_value);
                        set_sensor_values.update(|values| {
                            for (key, value) in next_state {
                                values.insert(key, value);
                            }
                        });
                    }
                }
                Err(err) => {
                    set_inject_error.set(Some(err));
                }
            }
        });
    });

    // Fire-and-forget actuator write. POSTs to /virtual-iot/actuators which
    // writes `gpio write <pin> <level>\n` to the sensor FIFO. The live stream
    // WS then echoes the resulting `[actuator] gpio.<pin>=<level>` back as a
    // `sensor_frame` under sensor key `gpio_<pin>`, which drives the UI.
    let actuator_device_id = device_id.clone();
    let write_actuator = Callback::new(move |(pin, level): (u32, u32)| {
        set_inject_error.set(None);
        // Optimistic UI: update the gpio_<pin> cell immediately.
        let sensor_key = format!("gpio_{pin}");
        let level_text = level.to_string();
        set_sensor_values.update(|values| {
            values.insert(sensor_key.clone(), level_text.clone());
        });
        let device_id = actuator_device_id.clone();
        spawn_local(async move {
            let request = crate::api::ActuatorWriteRequest { pin, level };
            let result = api::post::<crate::api::ActuatorWriteRequest, serde_json::Value>(
                &format!("/devices/{device_id}/virtual-iot/actuators"),
                &request,
            )
            .await;
            if let Err(err) = result {
                set_inject_error.set(Some(err));
            }
        });
    });

    // Fire-and-forget script run. POSTs the full source to
    // /virtual-iot/scripts/run; output streams back via the shell WS and any
    // `[actuator]` lines are picked up by the sensor stream.
    let script_device_id = device_id.clone();
    let run_script = Callback::new(move |_: ()| {
        if script_busy.get_untracked() {
            return;
        }
        let source = script_source.get_untracked();
        if source.trim().is_empty() {
            set_script_status.set(Some("Script is empty".into()));
            return;
        }
        set_script_busy.set(true);
        set_script_status.set(Some("Running…".into()));
        set_inject_error.set(None);
        let device_id = script_device_id.clone();
        spawn_local(async move {
            let request = crate::api::ScriptRunRequest { source };
            let result = api::post::<crate::api::ScriptRunRequest, serde_json::Value>(
                &format!("/devices/{device_id}/virtual-iot/scripts/run"),
                &request,
            )
            .await;
            set_script_busy.set(false);
            match result {
                Ok(payload) => {
                    let lines = payload.get("lines").and_then(|v| v.as_u64()).unwrap_or(0);
                    let bytes = payload.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                    set_script_status.set(Some(format!(
                        "Sent {lines} line(s), {bytes} byte(s) — watch shell pane for output."
                    )));
                }
                Err(err) => {
                    set_script_status.set(Some(format!("Error: {err}")));
                    set_inject_error.set(Some(err));
                }
            }
        });
    });

    let trigger_event = {
        let inject_sensor = inject_sensor;
        Callback::new(move |event: VirtualIotEventDefinition| {
            let source_tag = format!("event:{}", event.name);
            for step in event.steps {
                if step.sensor.trim().is_empty() || step.value.trim().is_empty() {
                    continue;
                }

                // Step label = "event:<event name> · <action>" if action present, else just the event.
                let step_source = match step.action.as_deref() {
                    Some(a) if !a.is_empty() => format!("{source_tag} · {a}"),
                    _ => source_tag.clone(),
                };

                inject_sensor.run((step.sensor.clone(), step.value.clone(), Some(step_source.clone())));

                if let (Some(end_value), Some(duration_ms)) = (step.end_value.clone(), step.duration_ms) {
                    if duration_ms > 0 {
                        queue_virtual_iot_event_follow_up(
                            inject_sensor,
                            step.sensor,
                            end_value,
                            duration_ms,
                            Some(format!("{step_source} → end")),
                        );
                    }
                }
            }
        })
    };

    view! {
        <div class="card virtual-iot-viewer">
            <div class="card-header" style="display:flex; justify-content:space-between; align-items:center; gap:1rem;">
                <div style="display:flex; align-items:center; gap:0.5rem; flex-wrap:wrap;">
                    <DeviceOsIcon os="virtual_iot".to_string() class="os-icon-title"/>
                    <div role="tablist" aria-label="Virtual IoT viewer tabs" style="display:flex; gap:0.25rem;">
                        <button
                            type="button"
                            role="tab"
                            class=move || {
                                if active_tab.get() == "detail" { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" }
                            }
                            aria-selected=move || (active_tab.get() == "detail").to_string()
                            on:click=move |_| set_active_tab.set("detail".to_string())
                        >"Virtual IoT Detail"</button>
                        <button
                            type="button"
                            role="tab"
                            class=move || {
                                let base = if active_tab.get() == "shell" { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" };
                                base.to_string()
                            }
                            aria-selected=move || (active_tab.get() == "shell").to_string()
                            disabled=move || !backend_attached.get()
                            title=move || {
                                if backend_attached.get() {
                                    "Open the VeerOS runtime shell for the attached backend".to_string()
                                } else {
                                    "Attach a VeerOS backend to enable the runtime shell".to_string()
                                }
                            }
                            on:click=move |_| {
                                if backend_attached.get_untracked() {
                                    set_active_tab.set("shell".to_string());
                                    set_shell_opened.set(true);
                                }
                            }
                        >"IoT Runtime Shell"</button>
                        <button
                            type="button"
                            role="tab"
                            class=move || {
                                if active_tab.get() == "scripts" { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" }
                            }
                            aria-selected=move || (active_tab.get() == "scripts").to_string()
                            disabled=move || !backend_attached.get()
                            title=move || {
                                if backend_attached.get() {
                                    "Write and run VeerOS shell scripts on the attached VM".to_string()
                                } else {
                                    "Attach a VeerOS backend to enable scripting".to_string()
                                }
                            }
                            on:click=move |_| {
                                if backend_attached.get_untracked() {
                                    set_active_tab.set("scripts".to_string());
                                }
                            }
                        >"Scripts"</button>
                    </div>
                </div>
                <div style="display:flex; gap:0.5rem; align-items:center;">
                    <Show when=move || backend_attached.get() && active_tab.get() == "shell">
                        <a
                            class="btn btn-ghost btn-sm"
                            href={move || format!("/virtual-iot/shell?device={}", device_id_value.get_value())}
                            target="_blank"
                            rel="noopener"
                            title="Pop the shell out to a new browser tab"
                        >"⇗ Pop out"</a>
                    </Show>
                    <button class="btn btn-ghost btn-sm" on:click=move |_| on_close.run(())>"✕ Close"</button>
                </div>
            </div>

            <Show when=move || loading.get()>
                <div class="loading">"Loading Virtual IoT configuration…"</div>
            </Show>

            <Show when=move || error.get().is_some()>
                <div class="alert error" style="margin: 1rem;">
                    {move || error.get().unwrap_or_default()}
                </div>
            </Show>

            <Show when=move || !loading.get() && error.get().is_none() && shell_opened.get() && backend_attached.get()>
                <div
                    style=move || {
                        let visible = active_tab.get() == "shell";
                        format!(
                            "padding:1rem; flex-direction:column; flex:1; min-height:30rem; display:{};",
                            if visible { "flex" } else { "none" }
                        )
                    }
                >
                    <VirtualIotShellTerminal device_id=device_id_value.get_value() />
                </div>
            </Show>

            <Show when=move || !loading.get() && error.get().is_none() && active_tab.get() == "scripts" && backend_attached.get()>
                <div class="virtual-iot-workbench" style="padding: 1rem; display: flex; flex-direction: column; gap: 0.75rem;">
                    <div>
                        <div class="virtual-iot-section-title">"Program the board"</div>
                        <div class="virtual-iot-section-caption">"Write a VeerOS shell script and run it on the attached VM. Output streams into the IoT Runtime Shell pane; any gpio writes flash the onboard LED tile live."</div>
                    </div>
                    <textarea
                        class="virtual-iot-script-editor"
                        style="width: 100%; min-height: 260px; font-family: 'JetBrains Mono', 'Fira Code', monospace; font-size: 0.85rem; background: #0f172a; color: #e2e8f0; border: 1px solid #334155; border-radius: 6px; padding: 0.75rem; resize: vertical; box-sizing: border-box;"
                        prop:value=move || script_source.get()
                        on:input=move |ev| set_script_source.set(event_target_value(&ev))
                    />
                    <div style="display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap;">
                        <button
                            class="btn btn-primary btn-sm"
                            disabled=move || script_busy.get()
                            on:click=move |_| run_script.run(())
                        >
                            {move || if script_busy.get() { "Running…" } else { "▶ Run on VM" }}
                        </button>
                        <button
                            class="btn btn-ghost btn-sm"
                            on:click=move |_| set_script_source.set(String::from(
                                "# Blink the onboard LED 3 times\nfor i in 1 2 3\n  gpio write 15 1\n  sleep 150\n  gpio write 15 0\n  sleep 150\nend\n"
                            ))
                        >"Load: blink LED"</button>
                        <button
                            class="btn btn-ghost btn-sm"
                            on:click=move |_| set_script_source.set(String::from(
                                "# Ramp the temperature sensor\nfor t in 2200 2400 2600 2800 3000 3200 3400\n  sensor set temperature $t\n  sleep 250\nend\n"
                            ))
                        >"Load: temp ramp"</button>
                        <button
                            class="btn btn-ghost btn-sm"
                            on:click=move |_| set_script_source.set(String::from(
                                "# Read all GPIO pins and list sensors\ngpio list\nsensor list\n"
                            ))
                        >"Load: inspect"</button>
                        {move || script_status.get().map(|msg| view! {
                            <span style="color:#94a3b8; font-size: 0.82rem;">{msg}</span>
                        })}
                    </div>
                </div>
            </Show>

            <Show when=move || !loading.get() && error.get().is_none() && active_tab.get() == "detail">
                <div class="virtual-iot-workbench">
                    <div class="virtual-iot-workbench-meta">
                        <div class="virtual-iot-meta-chip">
                            <span class="virtual-iot-meta-label">"Device"</span>
                            <strong>{device_name.clone()}</strong>
                        </div>
                        <div class="virtual-iot-meta-chip">
                            <span class="virtual-iot-meta-label">"Controller"</span>
                            <strong>{controller_name}</strong>
                        </div>
                        <div class="virtual-iot-meta-chip">
                            <span class="virtual-iot-meta-label">"Simulation"</span>
                            <strong>{move || format!("{} ms loop", sim_rate_ms.get())}</strong>
                        </div>
                        <div class="virtual-iot-meta-chip">
                            <span class="virtual-iot-meta-label">"Sensors"</span>
                            <strong>{sensor_count}</strong>
                        </div>
                        <div class="virtual-iot-meta-chip">
                            <span class="virtual-iot-meta-label">"Link"</span>
                            <strong>{move || if backend_attached.get() { "Attached backend" } else { "Controller-only" }}</strong>
                        </div>
                    </div>

                    <Show when=move || inject_error.get().is_some()>
                        <div class="alert error" style="margin: 0 1rem;">
                            {move || inject_error.get().unwrap_or_default()}
                        </div>
                    </Show>

                    <Show when=move || !event_definitions.get().is_empty()>
                        <div class="virtual-iot-events-rack">
                            <div>
                                <div class="virtual-iot-section-title">"Scenario triggers"</div>
                                <div class="virtual-iot-section-caption">"Replay saved Virtual IoT events directly from the live dashboard. End values are applied after the configured interval."</div>
                            </div>
                            <div class="virtual-iot-event-trigger-list">
                                {move || event_definitions.get().into_iter().map(|event| {
                                    let button_label = event.name.clone();
                                    let event_summary = if event.description.trim().is_empty() {
                                        format!("{} step(s)", event.steps.len())
                                    } else {
                                        event.description.clone()
                                    };
                                    let event_to_run = event.clone();
                                    view! {
                                        <button class="virtual-iot-event-trigger" on:click=move |_| trigger_event.run(event_to_run.clone())>
                                            <strong>{button_label}</strong>
                                            <span>{event_summary}</span>
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    </Show>

                    <Show
                        when=move || !enabled_sensors.get().is_empty()
                        fallback=move || view! { <p class="empty-state" style="padding: 1rem;">"No sensors enabled for this Virtual IoT device."</p> }
                    >
                        <div class="virtual-iot-scene-toolbar">
                            <span class="virtual-iot-section-caption">"Drag from the top bar to move. Drag the bottom-right corner to resize. Layout is clamped to the scene."</span>
                            <button class="btn btn-ghost btn-sm" on:click=move |_| reset_layout()>"Reset layout"</button>
                        </div>
                        <div class="virtual-iot-physical-scene virtual-iot-physical-scene-canvas" node_ref=scene_ref>
                            <VirtualIotWorkbenchWires
                                modules=scene_modules()
                                scene_rects=scene_rects
                            />

                            {move || {
                                let modules = scene_modules();
                                modules.into_iter().map(|module| {
                                    let inject_sensor = inject_sensor;
                                    let key = module.sensor_key.clone();
                                    view! {
                                        <VirtualIotSceneItem
                                            item_key=key
                                            scene_rects=scene_rects
                                            scene_ref=scene_ref
                                            min_w=210.0
                                            min_h=210.0
                                        >
                                            <VirtualIotSensorWorkbenchModule
                                                module=module
                                                on_inject=inject_sensor
                                                injecting_sensor
                                            />
                                        </VirtualIotSceneItem>
                                    }
                                }).collect::<Vec<_>>()
                            }}

                            <VirtualIotSceneItem
                                item_key="__controller".to_string()
                                scene_rects=scene_rects
                                scene_ref=scene_ref
                                min_w=300.0
                                min_h=360.0
                            >
                                <div class="virtual-iot-controller-shell">
                                    <div class="virtual-iot-controller-led-strip">
                                        <span class="virtual-iot-led virtual-iot-led-power is-on"></span>
                                        <span class="virtual-iot-led virtual-iot-led-activity" class:is-on=move || injecting_sensor.get().is_some()></span>
                                        <span class="virtual-iot-led virtual-iot-led-alert" class:is-on=board_alert></span>
                                        <span class="virtual-iot-led virtual-iot-led-link" class:is-on=move || backend_attached.get()></span>
                                    </div>
                                    <div class="virtual-iot-controller-board-wrap">
                                        <div class="virtual-iot-board-holder">
                                            {move || {
                                                let current = controller_type.get();
                                                let modules = scene_modules();
                                                let rects = scene_rects.get();
                                                let default = [0.0_f64, 0.0, 260.0, 180.0];
                                                let ctrl = rects.get("__controller").copied().unwrap_or(default);
                                                let ctrl_cx = ctrl[0] + ctrl[2] / 2.0;
                                                let mut connected: Vec<(String, String, bool)> = Vec::new();
                                                for m in &modules {
                                                    if m.pin == "INTERNAL" || m.pin == "EXTERNAL" || m.pin == "N/A" {
                                                        continue;
                                                    }
                                                    let mr = rects.get(&m.sensor_key).copied().unwrap_or(default);
                                                    let m_cx = mr[0] + mr[2] / 2.0;
                                                    let is_left = m_cx < ctrl_cx;
                                                    let color = wire_color_for_bus(&m.pin, &m.bus).to_string();
                                                    for raw in m.pin.split('/') {
                                                        let pin_name = raw.trim().to_string();
                                                        if !pin_name.is_empty() {
                                                            connected.push((pin_name, color.clone(), is_left));
                                                        }
                                                    }
                                                }
                                                view! { <VirtualIotBoard controller_type=current connected_pins=connected/> }
                                            }}
                                            {move || {
                                                // Only XIAO ESP32C6 has the pin-15 onboard LED mapping.
                                                let ct = controller_type.get();
                                                if ct != "xiao-esp32c6" { return view! { <svg class="virtual-iot-board-live-overlay"/> }.into_any(); }
                                                let led_on_signal = move || {
                                                    sensor_values.get().get("gpio_15").cloned().unwrap_or_else(|| "0".into()) == "1"
                                                };
                                                let toggle = move |_: web_sys::MouseEvent| {
                                                    let next: u32 = if sensor_values.get_untracked().get("gpio_15").cloned().unwrap_or_else(|| "0".into()) == "1" { 0 } else { 1 };
                                                    write_actuator.run((15, next));
                                                };
                                                view! {
                                                    <svg viewBox="0 0 260 360" class="virtual-iot-board-live-overlay" xmlns="http://www.w3.org/2000/svg">
                                                        // Outer glow halo (visible only when LED on)
                                                        <circle
                                                            class="live-led-glow"
                                                            cx="152" cy="232" r="18"
                                                            fill="#fb923c"
                                                            opacity=move || if led_on_signal() { "0.35" } else { "0.0" }
                                                        />
                                                        // Bright center dot
                                                        <circle
                                                            class="live-led"
                                                            cx="152" cy="232" r="5"
                                                            fill=move || if led_on_signal() { "#fde68a" } else { "#3a2207" }
                                                            stroke=move || if led_on_signal() { "#fef3c7" } else { "#fb923c" }
                                                            stroke-width="1.2"
                                                            on:click=toggle
                                                        >
                                                            <title>"Onboard LED · GPIO 15 · click to toggle"</title>
                                                        </circle>
                                                        <text
                                                            x="152" y="234"
                                                            text-anchor="middle"
                                                            fill=move || if led_on_signal() { "#7c2d12" } else { "#ffedd5" }
                                                            font-size="4.8"
                                                            font-family="monospace"
                                                            pointer-events="none"
                                                        >"LED"</text>
                                                    </svg>
                                                }.into_any()
                                            }}
                                        </div>
                                    </div>
                                    <div class="virtual-iot-controller-readout">
                                        <div class="virtual-iot-controller-title">{move || format!("{} bench harness", virtual_iot_controller_name(&controller_type.get()))}</div>
                                        <div class="virtual-iot-controller-status-row">
                                            <span class="virtual-iot-status-pill">"PWR"</span>
                                            <span class="virtual-iot-status-pill" class:is-hot=move || injecting_sensor.get().is_some()>"ACT"</span>
                                            <span class="virtual-iot-status-pill" class:is-alert=board_alert>"ALERT"</span>
                                            <span class="virtual-iot-status-pill" class:is-linked=move || backend_attached.get()>"LINK"</span>
                                        </div>
                                        <div class="virtual-iot-controller-caption mono">{device_id_value.get_value()}</div>
                                    </div>
                                </div>
                            </VirtualIotSceneItem>
                        </div>
                    </Show>

                    <div class="virtual-iot-runtime-log card-section">
                        <div class="virtual-iot-section-title">"Recent sensor history"</div>
                        <div class="virtual-iot-runtime-log-table">
                            <Show
                                when=move || !sensor_log.get().is_empty()
                                fallback=move || view! { <div class="virtual-iot-runtime-log-empty">"No readings stored yet. Use a preset, slider, or scenario trigger to generate simulated telemetry."</div> }
                            >
                                {move || sensor_log.get().into_iter().rev().take(20).map(|entry| {
                                    let is_event = entry.source.starts_with("event:");
                                    let source_text = entry.source.clone();
                                    view! {
                                        <div class="virtual-iot-runtime-log-row" class:is-event=is_event>
                                            <span class="virtual-iot-runtime-log-ts mono">{virtual_iot_short_ts(&entry.ts)}</span>
                                            <span class="virtual-iot-runtime-log-sensor">{virtual_iot_sensor_label(&entry.sensor)}</span>
                                            <span class="virtual-iot-runtime-log-value mono">{entry.value}</span>
                                            <span
                                                class="virtual-iot-runtime-log-source"
                                                class:is-event=is_event
                                            >{source_text}</span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </Show>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VirtualIotSceneSide {
    Left,
    Right,
}

#[derive(Clone)]
struct VirtualIotSceneModule {
    sensor_key: String,
    sensor_label: String,
    reading: String,
    module_profile: String,
    expected_range: String,
    pin: String,
    bus: String,
    note: String,
    side: VirtualIotSceneSide,
    row: usize,
}

fn virtual_iot_default_sensor_values(enabled_sensors: &[String]) -> HashMap<String, String> {
    enabled_sensors
        .iter()
        .map(|sensor| (sensor.clone(), virtual_iot_sensor_preview(sensor).to_string()))
        .collect()
}

fn virtual_iot_sensor_state_map(value: &serde_json::Value) -> HashMap<String, String> {
    value
        .as_object()
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| value.as_str().map(|text| (key.clone(), text.to_string())))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default()
}

fn queue_virtual_iot_event_follow_up(
    inject_sensor: Callback<(String, String, Option<String>)>,
    sensor: String,
    value: String,
    delay_ms: i32,
    source: Option<String>,
) {
    if let Some(window) = web_sys::window() {
        let follow_up = Closure::once_into_js(move || {
            inject_sensor.run((sensor.clone(), value.clone(), source.clone()));
        });
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            follow_up.unchecked_ref(),
            delay_ms,
        );
    }
}

fn virtual_iot_short_ts(input: &str) -> String {
    input
        .split('T')
        .nth(1)
        .and_then(|value| value.split('.').next())
        .unwrap_or(input)
        .to_string()
}

fn virtual_iot_scene_modules(
    controller_type: &str,
    enabled_sensors: &[String],
    sensor_values: &HashMap<String, String>,
) -> Vec<VirtualIotSceneModule> {
    let wiring = virtual_iot_default_wiring(controller_type, enabled_sensors);

    enabled_sensors
        .iter()
        .enumerate()
        .map(|(idx, sensor)| {
            let (module_profile, expected_range) = virtual_iot_sensor_realism(sensor);
            let default_wiring = wiring
                .iter()
                .find(|entry| entry.sensor_key == *sensor)
                .cloned()
                .unwrap_or_else(|| WiringMapEntry {
                    sensor_key: sensor.clone(),
                    sensor_label: virtual_iot_sensor_label(sensor).to_string(),
                    pin: String::from("PIN_1"),
                    bus: String::from("I2C"),
                    note: String::from("Default mapping"),
                });

            VirtualIotSceneModule {
                sensor_key: sensor.clone(),
                sensor_label: virtual_iot_sensor_label(sensor).to_string(),
                reading: sensor_values
                    .get(sensor)
                    .cloned()
                    .unwrap_or_else(|| virtual_iot_sensor_preview(sensor).to_string()),
                module_profile: module_profile.to_string(),
                expected_range: expected_range.to_string(),
                pin: default_wiring.pin,
                bus: default_wiring.bus,
                note: default_wiring.note,
                side: if idx % 2 == 0 { VirtualIotSceneSide::Left } else { VirtualIotSceneSide::Right },
                row: idx / 2,
            }
        })
        .collect()
}

fn virtual_iot_parse_numeric(sensor: &str, reading: &str) -> Option<f64> {
    let trimmed = reading.trim();
    if sensor == "wifi_rssi" {
        return trimmed.split_whitespace().next()?.parse::<f64>().ok();
    }
    trimmed
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-'))
        .find(|chunk| !chunk.is_empty())?
        .parse::<f64>()
        .ok()
}

fn virtual_iot_slider_spec(sensor: &str) -> Option<(f64, f64, f64, &'static str)> {
    match sensor {
        "temperature" => Some((10.0, 45.0, 0.1, "C")),
        "humidity" => Some((10.0, 100.0, 1.0, "%")),
        "pressure" => Some((960.0, 1045.0, 1.0, "hPa")),
        "air_quality_co2" => Some((350.0, 2400.0, 10.0, "ppm")),
        "air_quality_tvoc" => Some((0.0, 2.0, 0.01, "ppm")),
        "ambient_light" => Some((0.0, 1600.0, 5.0, "lux")),
        "uv_index" => Some((0.0, 12.0, 0.1, "uv")),
        "proximity" => Some((2.0, 140.0, 1.0, "cm")),
        "battery_voltage" => Some((3.2, 4.25, 0.01, "V")),
        "current" => Some((0.0, 600.0, 5.0, "mA")),
        "adc_raw" => Some((0.0, 4095.0, 1.0, "raw")),
        "sound_level" => Some((20.0, 100.0, 1.0, "dB")),
        "wifi_rssi" => Some((-90.0, -30.0, 1.0, "dBm")),
        _ => None,
    }
}

fn virtual_iot_format_sensor_value(sensor: &str, raw: f64) -> String {
    match sensor {
        "temperature" => format!("{raw:.1} C"),
        "humidity" => format!("{raw:.0} %"),
        "pressure" => format!("{raw:.0} hPa"),
        "air_quality_co2" => format!("{raw:.0} ppm"),
        "air_quality_tvoc" => format!("{raw:.2} ppm"),
        "ambient_light" => format!("{raw:.0} lux"),
        "uv_index" => format!("{raw:.1}"),
        "proximity" => format!("{raw:.0} cm"),
        "battery_voltage" => format!("{raw:.2} V"),
        "current" => format!("{raw:.0} mA"),
        "adc_raw" => format!("{raw:.0}"),
        "sound_level" => format!("{raw:.0} dB"),
        "wifi_rssi" => format!("{raw:.0} dBm"),
        _ => format!("{raw:.1}"),
    }
}

fn virtual_iot_sensor_presets(sensor: &str) -> Vec<(&'static str, &'static str)> {
    match sensor {
        "temperature" => vec![("Cool", "18.4 C"), ("Nominal", "24.6 C"), ("Hot", "33.8 C")],
        "humidity" => vec![("Dry", "34 %"), ("Normal", "58 %"), ("Damp", "86 %")],
        "pressure" => vec![("Low", "990 hPa"), ("Sea", "1012 hPa"), ("Storm", "1030 hPa")],
        "air_quality_co2" => vec![("Fresh", "420 ppm"), ("Office", "830 ppm"), ("Alert", "1650 ppm")],
        "air_quality_tvoc" => vec![("Clean", "0.08 ppm"), ("Normal", "0.41 ppm"), ("Poor", "0.92 ppm")],
        "ambient_light" => vec![("Night", "8 lux"), ("Desk", "326 lux"), ("Sun", "1290 lux")],
        "uv_index" => vec![("Shade", "0.6"), ("Outdoor", "2.8"), ("Harsh", "7.1")],
        "proximity" => vec![("Far", "120 cm"), ("Near", "18 cm"), ("Touch", "4 cm")],
        "battery_voltage" => vec![("Low", "3.42 V"), ("Run", "3.92 V"), ("Full", "4.18 V")],
        "current" => vec![("Idle", "72 mA"), ("Run", "118 mA"), ("Peak", "310 mA")],
        "sound_level" => vec![("Quiet", "31 dB"), ("Speech", "52 dB"), ("Alarm", "88 dB")],
        "accel_xyz" => vec![("Level", "0.04, -0.02, 0.98 g"), ("Tilt", "0.42, 0.12, 0.86 g"), ("Shake", "1.44, -0.65, 0.32 g")],
        "gyro_xyz" => vec![("Idle", "0.1, 0.0, -0.1 dps"), ("Turn", "8.2, 1.4, 17.0 dps"), ("Spin", "42.0, -11.0, 86.5 dps")],
        "magnetometer" => vec![("North", "12, -8, 31 uT"), ("East", "30, 5, 10 uT"), ("Disturbed", "84, -66, 54 uT")],
        "gps" => vec![("HQ", "12.9716, 77.5946"), ("Warehouse", "12.9352, 77.6245"), ("Transit", "12.9981, 77.7004")],
        "gpio_states" => vec![("Idle", "GPIO4=0 GPIO5=0"), ("Latch", "GPIO4=1 GPIO5=0"), ("Both", "GPIO4=1 GPIO5=1")],
        "adc_raw" => vec![("Low", "412"), ("Mid", "1842"), ("High", "3510")],
        "ble_scan" => vec![("Quiet", "[]"), ("One tag", "[beacon-a]"), ("Crowded", "[beacon-a, tag-b, watch-c]")],
        "wifi_rssi" => vec![("Weak", "-84 dBm"), ("Good", "-58 dBm"), ("Strong", "-39 dBm")],
        _ => Vec::new(),
    }
}

fn virtual_iot_sensor_level(sensor: &str, reading: &str) -> f64 {
    if let Some((min, max, _, _)) = virtual_iot_slider_spec(sensor) {
        if let Some(value) = virtual_iot_parse_numeric(sensor, reading) {
            return ((value - min) / (max - min)).clamp(0.0, 1.0);
        }
    }
    0.45
}

fn virtual_iot_sensor_visual_kind(sensor: &str) -> &'static str {
    match sensor {
        "temperature" | "humidity" | "pressure" => "env",
        "air_quality_co2" | "air_quality_tvoc" => "air",
        "ambient_light" | "uv_index" => "light",
        "proximity" => "proximity",
        "battery_voltage" | "current" => "power",
        "sound_level" => "audio",
        "gps" => "location",
        "gpio_states" | "adc_raw" => "digital",
        "wifi_rssi" | "ble_scan" => "network",
        _ => "motion",
    }
}

fn virtual_iot_controller_has_alert(sensor_values: &HashMap<String, String>) -> bool {
    let reading = |key: &str| sensor_values.get(key).and_then(|value| virtual_iot_parse_numeric(key, value));
    reading("temperature").is_some_and(|value| value >= 32.0)
        || reading("humidity").is_some_and(|value| value >= 82.0)
        || reading("air_quality_co2").is_some_and(|value| value >= 1200.0)
        || reading("sound_level").is_some_and(|value| value >= 80.0)
        || reading("battery_voltage").is_some_and(|value| value <= 3.5)
        || reading("proximity").is_some_and(|value| value <= 6.0)
}

#[component]
fn VirtualIotSceneItem(
    item_key: String,
    scene_rects: RwSignal<HashMap<String, [f64; 4]>>,
    scene_ref: NodeRef<leptos::html::Div>,
    #[prop(default = 180.0)] min_w: f64,
    #[prop(default = 140.0)] min_h: f64,
    children: Children,
) -> impl IntoView {
    let key = StoredValue::new(item_key);
    let (dragging, set_dragging) = signal(false);
    let (resizing, set_resizing) = signal(false);
    let drag_origin = StoredValue::new((0.0f64, 0.0f64));
    let start_rect = StoredValue::new([0.0f64, 0.0f64, 260.0f64, 180.0f64]);
    let container_size = StoredValue::new((1000.0f64, 760.0f64));

    let refresh_container = move || {
        if let Some(el) = scene_ref.get_untracked() {
            let r = el.get_bounding_client_rect();
            container_size.set_value((r.width(), r.height()));
        }
    };

    let style = move || {
        let rects = scene_rects.get();
        let k = key.get_value();
        let r = rects.get(&k).copied().unwrap_or([20.0, 20.0, 260.0, 180.0]);
        format!("left:{:.1}px;top:{:.1}px;width:{:.1}px;height:{:.1}px;", r[0], r[1], r[2], r[3])
    };

    let on_drag_start = move |evt: web_sys::PointerEvent| {
        evt.prevent_default();
        evt.stop_propagation();
        set_dragging.set(true);
        drag_origin.set_value((evt.client_x() as f64, evt.client_y() as f64));
        let k = key.get_value();
        let r = scene_rects.get_untracked().get(&k).copied().unwrap_or([0.0, 0.0, 260.0, 180.0]);
        start_rect.set_value(r);
        refresh_container();
        if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let _ = target.set_pointer_capture(evt.pointer_id());
        }
    };

    let on_drag_move = move |evt: web_sys::PointerEvent| {
        if !dragging.get_untracked() { return; }
        evt.prevent_default();
        let (ox, oy) = drag_origin.get_value();
        let sr = start_rect.get_value();
        let (cw, ch) = container_size.get_value();
        let dx = evt.client_x() as f64 - ox;
        let dy = evt.client_y() as f64 - oy;
        let max_x = (cw - sr[2]).max(0.0);
        let max_y = (ch - sr[3]).max(0.0);
        let new_x = (sr[0] + dx).clamp(0.0, max_x);
        let new_y = (sr[1] + dy).clamp(0.0, max_y);
        let k = key.get_value();
        scene_rects.update(|m| {
            let r = m.entry(k).or_insert(sr);
            r[0] = new_x;
            r[1] = new_y;
        });
    };

    let on_drag_end = move |evt: web_sys::PointerEvent| {
        if dragging.get_untracked() {
            set_dragging.set(false);
            if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                let _ = target.release_pointer_capture(evt.pointer_id());
            }
        }
    };

    let on_resize_start = move |evt: web_sys::PointerEvent| {
        evt.prevent_default();
        evt.stop_propagation();
        set_resizing.set(true);
        drag_origin.set_value((evt.client_x() as f64, evt.client_y() as f64));
        let k = key.get_value();
        let r = scene_rects.get_untracked().get(&k).copied().unwrap_or([0.0, 0.0, 260.0, 180.0]);
        start_rect.set_value(r);
        refresh_container();
        if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let _ = target.set_pointer_capture(evt.pointer_id());
        }
    };

    let on_resize_move = move |evt: web_sys::PointerEvent| {
        if !resizing.get_untracked() { return; }
        evt.prevent_default();
        let (ox, oy) = drag_origin.get_value();
        let sr = start_rect.get_value();
        let (cw, ch) = container_size.get_value();
        let dx = evt.client_x() as f64 - ox;
        let dy = evt.client_y() as f64 - oy;
        let max_w = (cw - sr[0]).max(min_w);
        let max_h = (ch - sr[1]).max(min_h);
        let new_w = (sr[2] + dx).clamp(min_w, max_w);
        let new_h = (sr[3] + dy).clamp(min_h, max_h);
        let k = key.get_value();
        scene_rects.update(|m| {
            let r = m.entry(k).or_insert(sr);
            r[2] = new_w;
            r[3] = new_h;
        });
    };

    let on_resize_end = move |evt: web_sys::PointerEvent| {
        if resizing.get_untracked() {
            set_resizing.set(false);
            if let Some(target) = evt.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                let _ = target.release_pointer_capture(evt.pointer_id());
            }
        }
    };

    view! {
        <div
            class="virtual-iot-scene-item"
            class:is-dragging=move || dragging.get()
            class:is-resizing=move || resizing.get()
            style=style
        >
            <div
                class="virtual-iot-scene-drag-handle"
                on:pointerdown=on_drag_start
                on:pointermove=on_drag_move
                on:pointerup=on_drag_end
                on:pointercancel=on_drag_end
                title="Drag to move"
            >
                <span class="virtual-iot-scene-drag-grip"></span>
            </div>
            <div class="virtual-iot-scene-body">
                {children()}
            </div>
            <div
                class="virtual-iot-scene-resize-handle"
                on:pointerdown=on_resize_start
                on:pointermove=on_resize_move
                on:pointerup=on_resize_end
                on:pointercancel=on_resize_end
                title="Drag to resize"
            ></div>
        </div>
    }
}

#[component]
fn VirtualIotWorkbenchWires(
    modules: Vec<VirtualIotSceneModule>,
    scene_rects: RwSignal<HashMap<String, [f64; 4]>>,
) -> impl IntoView {
    view! {
        <svg class="virtual-iot-physical-wires" xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="none">
            <defs>
                <filter id="viot-wire-shadow" x="-30%" y="-30%" width="160%" height="160%">
                    <feDropShadow dx="0" dy="2" stdDeviation="2.5" flood-color="rgba(0,0,0,0.45)"/>
                </filter>
                <radialGradient id="viot-connector-chrome" cx="35%" cy="30%" r="75%">
                    <stop offset="0%" stop-color="#f4f7fb"/>
                    <stop offset="35%" stop-color="#a9b4c2"/>
                    <stop offset="70%" stop-color="#3b4554"/>
                    <stop offset="100%" stop-color="#0a0f17"/>
                </radialGradient>
                <radialGradient id="viot-connector-inner" cx="35%" cy="30%" r="75%">
                    <stop offset="0%" stop-color="rgba(255,255,255,0.55)"/>
                    <stop offset="50%" stop-color="rgba(255,255,255,0.0)"/>
                    <stop offset="100%" stop-color="rgba(0,0,0,0.55)"/>
                </radialGradient>
            </defs>
            {modules.into_iter().map(|module| {
                let key = module.sensor_key.clone();
                let pin = module.pin.clone();
                let bus = module.bus.clone();
                let shape = virtual_iot_sensor_shape_kind(&key).to_string();
                view! {
                    <g>
                        {move || {
                            let rects = scene_rects.get();
                            let default = [0.0f64, 0.0, 260.0, 180.0];
                            let ctrl = rects.get("__controller").copied().unwrap_or(default);
                            let m = rects.get(&key).copied().unwrap_or(default);
                            let ctrl_cx = ctrl[0] + ctrl[2] / 2.0;
                            let m_cx = m[0] + m[2] / 2.0;
                            let is_left = m_cx < ctrl_cx;
                            let (start_x, start_y) = if is_left {
                                (ctrl[0], ctrl[1] + ctrl[3] / 2.0)
                            } else {
                                (ctrl[0] + ctrl[2], ctrl[1] + ctrl[3] / 2.0)
                            };
                            // Compute the actual metallic body bounds inside the card:
                            //   round/square/hex shells: stage is centered, width = min(180, card_w),
                            //   square aspect (height == width). For rect shells: full card width,
                            //   aspect 5:4 (height ≈ 0.8 × width).
                            //   The topline (instrument name) reserves ~26 px above the stage and the
                            //   readout reserves ~40 px below it inside the card.
                            let topline_h = 26.0_f64;
                            let readout_h = 40.0_f64;
                            let avail_h = (m[3] - topline_h - readout_h).max(60.0);
                            let (stage_w, stage_h) = match shape.as_str() {
                                "rect" => {
                                    let w = m[2].max(40.0);
                                    let h = (w * 0.8).min(avail_h);
                                    (w, h)
                                }
                                _ => {
                                    let target = m[2].min(180.0).min(avail_h);
                                    (target, target)
                                }
                            };
                            let stage_cx = m[0] + m[2] / 2.0;
                            let stage_cy = m[1] + topline_h + stage_h / 2.0;
                            // Wire arrives at the metallic body's edge facing the controller.
                            let end_x = if is_left {
                                stage_cx + stage_w / 2.0  // right edge of left-side instrument
                            } else {
                                stage_cx - stage_w / 2.0  // left edge of right-side instrument
                            };
                            let end_y = stage_cy;
                            // Inset wire endpoints so the path emerges from the back of each connector barrel.
                            let barrel_len = 14.0_f64;
                            let dir = if is_left { -1.0 } else { 1.0 };
                            let wire_start_x = start_x + dir * barrel_len;
                            let wire_end_x = end_x - dir * barrel_len;
                            let mid_x = (wire_start_x + wire_end_x) / 2.0;
                            let d = format!(
                                "M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
                                wire_start_x, start_y, mid_x, start_y, mid_x, end_y, wire_end_x, end_y,
                            );
                            let stroke = wire_color_for_bus(&pin, &bus);
                            let dash = if pin == "INTERNAL" || pin == "EXTERNAL" { "6 6" } else { "0" };
                            // Connector barrel geometry: a small rounded rect that "plugs into" the body.
                            let barrel_w = 16.0_f64;
                            let barrel_h = 12.0_f64;
                            let (s_bx, s_by) = (start_x + dir * 0.0 - if is_left { 0.0 } else { 0.0 }, start_y - barrel_h / 2.0);
                            let s_bx = if is_left { s_bx - barrel_w + 2.0 } else { s_bx - 2.0 };
                            let (e_bx, e_by) = (end_x, end_y - barrel_h / 2.0);
                            let e_bx = if is_left { e_bx - 2.0 } else { e_bx - barrel_w + 2.0 };
                            // Colored band where wire enters the barrel.
                            let band_w = 3.0_f64;
                            let s_band_x = if is_left { s_bx } else { s_bx + barrel_w - band_w };
                            let e_band_x = if is_left { e_bx + barrel_w - band_w } else { e_bx };
                            view! {
                                <>
                                    // Wire halo + main wire
                                    <path d={d.clone()} stroke={stroke.clone()} stroke-opacity="0.22" stroke-width="9" fill="none" stroke-linecap="round"/>
                                    <path d={d.clone()} stroke={stroke.clone()} stroke-width="3.8" stroke-dasharray={dash.to_string()} fill="none" stroke-linecap="round" filter="url(#viot-wire-shadow)"/>
                                    // Subtle chrome highlight along the wire
                                    <path d={d.clone()} stroke="rgba(255,255,255,0.18)" stroke-width="1" fill="none" stroke-linecap="round"/>

                                    // ── Controller-side connector (metallic barrel) ──
                                    <rect x={format!("{s_bx:.1}")} y={format!("{s_by:.1}")} width={barrel_w} height={barrel_h} rx="2.5" fill="url(#viot-connector-chrome)" stroke="#04080e" stroke-width="0.8" filter="url(#viot-wire-shadow)"/>
                                    <rect x={format!("{:.1}", s_bx + 1.2)} y={format!("{:.1}", s_by + 1.2)} width={format!("{:.1}", barrel_w - 2.4)} height={format!("{:.1}", barrel_h - 2.4)} rx="1.6" fill="url(#viot-connector-inner)" opacity="0.6"/>
                                    <rect x={format!("{s_band_x:.1}")} y={format!("{:.1}", s_by + 0.8)} width={band_w} height={format!("{:.1}", barrel_h - 1.6)} rx="0.8" fill={stroke.clone()} opacity="0.95"/>
                                    // Tiny screw on the barrel
                                    <circle cx={format!("{:.1}", s_bx + barrel_w / 2.0)} cy={format!("{:.1}", s_by + 2.4)} r="0.9" fill="#1b2230" stroke="rgba(255,255,255,0.35)" stroke-width="0.3"/>

                                    // ── Instrument-side connector (metallic barrel) ──
                                    <rect x={format!("{e_bx:.1}")} y={format!("{e_by:.1}")} width={barrel_w} height={barrel_h} rx="2.5" fill="url(#viot-connector-chrome)" stroke="#04080e" stroke-width="0.8" filter="url(#viot-wire-shadow)"/>
                                    <rect x={format!("{:.1}", e_bx + 1.2)} y={format!("{:.1}", e_by + 1.2)} width={format!("{:.1}", barrel_w - 2.4)} height={format!("{:.1}", barrel_h - 2.4)} rx="1.6" fill="url(#viot-connector-inner)" opacity="0.6"/>
                                    <rect x={format!("{e_band_x:.1}")} y={format!("{:.1}", e_by + 0.8)} width={band_w} height={format!("{:.1}", barrel_h - 1.6)} rx="0.8" fill={stroke.clone()} opacity="0.95"/>
                                    <circle cx={format!("{:.1}", e_bx + barrel_w / 2.0)} cy={format!("{:.1}", e_by + 2.4)} r="0.9" fill="#1b2230" stroke="rgba(255,255,255,0.35)" stroke-width="0.3"/>
                                </>
                            }
                        }}
                    </g>
                }
            }).collect::<Vec<_>>()}
        </svg>
    }
}

#[component]
fn VirtualIotSensorWorkbenchModule(
    module: VirtualIotSceneModule,
    on_inject: Callback<(String, String, Option<String>)>,
    injecting_sensor: ReadSignal<Option<String>>,
) -> impl IntoView {
    let (expanded, set_expanded) = signal(false);
    let sensor_key = module.sensor_key.clone();
    let reading = module.reading.clone();
    let presets = virtual_iot_sensor_presets(&sensor_key);
    let slider_spec = virtual_iot_slider_spec(&sensor_key);
    let level = virtual_iot_sensor_level(&sensor_key, &reading);
    let visual_kind = virtual_iot_sensor_visual_kind(&sensor_key).to_string();
    let shape_kind = virtual_iot_sensor_shape_kind(&sensor_key).to_string();
    let chip_name = virtual_iot_sensor_chip_name(&sensor_key);
    let busy_sensor_key = StoredValue::new(sensor_key.clone());
    let active_sensor_key = sensor_key.clone();
    let dock_sensor_key = module.sensor_key.clone();
    let dock_label = module.sensor_label.clone();
    let dock_pin = module.pin.clone();
    let dock_bus = module.bus.clone();
    let needle_deg = -120.0 + (level * 240.0);
    let is_digital_widget = matches!(visual_kind.as_str(), "digital" | "location");
    let is_bar_widget = matches!(visual_kind.as_str(), "audio" | "network");
    let is_wave_widget = visual_kind == "motion";

    view! {
        <div
            class="virtual-iot-module-card"
            class:is-active=move || injecting_sensor.get().as_deref() == Some(active_sensor_key.as_str())
            class:is-expanded=move || expanded.get()
            data-kind={visual_kind.clone()}
        >
            <div class={format!("virtual-iot-module-shell shape-{}", shape_kind)}>
                <div class="virtual-iot-module-topline">
                    <div class="virtual-iot-module-title">{dock_label}</div>
                    <div class="virtual-iot-module-io mono">{dock_pin}</div>
                </div>

                <button class="virtual-iot-module-visual-stage" on:click=move |_| set_expanded.update(|v| *v = !*v)>
                    {if is_digital_widget {
                        view! {
                            <div class="virtual-iot-digital-screen">
                                <div class="virtual-iot-digital-chip mono">{chip_name}</div>
                                <div class="virtual-iot-digital-reading mono">{reading.clone()}</div>
                                <div class="virtual-iot-digital-bus mono">{dock_bus.clone()}</div>
                            </div>
                        }.into_any()
                    } else if is_bar_widget {
                        let fill_pct = (level * 100.0).clamp(0.0, 100.0);
                        view! {
                            <div class="virtual-iot-bar-screen">
                                <div class="virtual-iot-bar-rail">
                                    <div class="virtual-iot-bar-fill" style={format!("height: {fill_pct:.1}%;")}></div>
                                </div>
                                <div class="virtual-iot-bar-peaks">
                                    <span style={format!("height:{:.0}%;", (30.0 + level * 55.0).clamp(8.0, 100.0))}></span>
                                    <span style={format!("height:{:.0}%;", (16.0 + level * 70.0).clamp(8.0, 100.0))}></span>
                                    <span style={format!("height:{:.0}%;", (24.0 + level * 60.0).clamp(8.0, 100.0))}></span>
                                    <span style={format!("height:{:.0}%;", (12.0 + level * 75.0).clamp(8.0, 100.0))}></span>
                                </div>
                                <div class="virtual-iot-bar-reading mono">{reading.clone()}</div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="virtual-iot-meter-dial" style={format!("--needle-deg:{needle_deg:.2}deg;")}>
                                <div class="virtual-iot-meter-ring"></div>
                                <div class="virtual-iot-meter-ticks">
                                    <span></span><span></span><span></span><span></span><span></span>
                                </div>
                                <div class="virtual-iot-meter-needle"></div>
                                <div class="virtual-iot-meter-core"></div>
                                {if is_wave_widget {
                                    view! {
                                        <div class="virtual-iot-wave-overlay">
                                            <span></span><span></span><span></span>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }}
                                <div class="virtual-iot-meter-icon mono">{virtual_iot_sensor_visual_icon(&dock_sensor_key)}</div>
                            </div>
                        }.into_any()
                    }}
                </button>

                <div class="virtual-iot-module-readout">
                    <div class="virtual-iot-module-subtitle mono">{chip_name}</div>
                    <div class="virtual-iot-module-reading mono">{reading.clone()}</div>
                    <div class="virtual-iot-module-range mono">{dock_bus}</div>
                </div>

                <div class="virtual-iot-module-tools">
                    <button class="btn btn-ghost btn-sm" on:click=move |_| set_expanded.update(|v| *v = !*v)>
                        {move || if expanded.get() { "Minimize" } else { "Expand" }}
                    </button>
                    <span class="virtual-iot-module-drag-tip mono">"drag via top handle"</span>
                </div>

                <Show when=move || expanded.get()>
                    <div class="virtual-iot-module-expand-panel">
                        {if let Some((min, max, step, unit)) = slider_spec {
                            let sensor_for_slider = module.sensor_key.clone();
                            let on_inject_slider = on_inject;
                            let slider_value = virtual_iot_parse_numeric(&sensor_for_slider, &reading).unwrap_or((min + max) / 2.0);
                            view! {
                                <div class="virtual-iot-module-control-row">
                                    <input
                                        class="virtual-iot-module-slider"
                                        type="range"
                                        min={min.to_string()}
                                        max={max.to_string()}
                                        step={step.to_string()}
                                        prop:value={slider_value.to_string()}
                                        on:change=move |ev| {
                                            if let Ok(next_value) = event_target_value(&ev).parse::<f64>() {
                                                on_inject_slider.run((sensor_for_slider.clone(), virtual_iot_format_sensor_value(&sensor_for_slider, next_value), None));
                                            }
                                        }
                                    />
                                    <div class="virtual-iot-module-control-meta mono">{format!("{min:.0} → {max:.0} {unit}")}</div>
                                </div>
                            }.into_any()
                        } else {
                            view! { <div class="virtual-iot-module-control-meta mono">"Preset-driven simulation module"</div> }.into_any()
                        }}

                        <div class="virtual-iot-module-actions">
                            {presets.iter().map(|(label, value)| {
                                let sensor_key = module.sensor_key.clone();
                                let next_value = (*value).to_string();
                                let on_inject = on_inject;
                                view! {
                                    <button class="btn btn-secondary btn-sm" on:click=move |_| on_inject.run((sensor_key.clone(), next_value.clone(), None))>{*label}</button>
                                }
                            }).collect::<Vec<_>>()}
                            <span class="virtual-iot-module-busy" class:is-visible=move || busy_sensor_key.with_value(|key| injecting_sensor.get().as_deref() == Some(key.as_str()))>
                                {move || {
                                    if busy_sensor_key.with_value(|key| injecting_sensor.get().as_deref() == Some(key.as_str())) {
                                        "Applying…"
                                    } else {
                                        "Ready"
                                    }
                                }}
                            </span>
                        </div>
                    </div>
                </Show>
            </div>
        </div>
    }
}

fn virtual_iot_sensor_shape_kind(sensor: &str) -> &'static str {
    match virtual_iot_sensor_visual_kind(sensor) {
        "env" | "light" | "power" => "round",
        "audio" | "network" => "rect",
        "digital" | "location" => "square",
        "motion" => "hex",
        _ => "round",
    }
}

fn virtual_iot_sensor_visual_icon(sensor: &str) -> &'static str {
    match sensor {
        "temperature" => "T",
        "humidity" => "H",
        "pressure" => "P",
        "air_quality_co2" | "air_quality_tvoc" => "AQ",
        "ambient_light" | "uv_index" => "LX",
        "proximity" => "PX",
        "battery_voltage" | "current" => "V",
        "sound_level" => "DB",
        "gps" => "GPS",
        "gpio_states" | "adc_raw" => "IO",
        "wifi_rssi" | "ble_scan" => "RF",
        _ => "IMU",
    }
}

/// Short module/chip name used as silkscreen text on each sensor card.
/// Chosen to feel like real breakout boards (adafruit/sparkfun style).
fn virtual_iot_sensor_chip_name(sensor: &str) -> &'static str {
    match sensor {
        "temperature" | "humidity" | "pressure" => "BME280",
        "air_quality_co2" => "SCD41",
        "air_quality_tvoc" => "SGP40",
        "ambient_light" => "TSL2591",
        "uv_index" => "VEML6075",
        "proximity" => "VL53L1X",
        "battery_voltage" => "MAX17048",
        "current" => "INA219",
        "sound_level" => "MAX4466",
        "gps" => "MAX-M10",
        "gpio_states" => "MCP23017",
        "adc_raw" => "ADS1115",
        "wifi_rssi" => "ESP-WROOM",
        "ble_scan" => "NRF52840",
        "accel_xyz" => "LSM6DSOX",
        "gyro_xyz" => "BMI270",
        "magnetometer" => "LIS3MDL",
        _ => "MCU-MOD",
    }
}

#[component]
fn VirtualIotWiringDiagram(
    controller_type: String,
    wiring_map: Vec<WiringMapEntry>,
) -> impl IntoView {
    let height = (wiring_map.len().max(4) as f32 * 56.0 + 100.0).min(760.0);
    let board_name = virtual_iot_controller_name(&controller_type).to_string();

    let initial_positions = wiring_map
        .iter()
        .enumerate()
        .map(|(idx, entry)| (entry.sensor_key.clone(), (640.0f32, 110.0f32 + idx as f32 * 56.0f32)))
        .collect::<HashMap<_, _>>();

    let (node_positions, set_node_positions) = signal(initial_positions);
    let (dragging_key, set_dragging_key) = signal(Option::<String>::None);
    let (drag_offset, set_drag_offset) = signal((0.0f32, 0.0f32));

    let on_mouse_move = move |ev: web_sys::MouseEvent| {
        if let Some(key) = dragging_key.get() {
            let (dx, dy) = drag_offset.get();
            let mut x = ev.offset_x() as f32 - dx;
            let mut y = ev.offset_y() as f32 - dy;
            x = x.clamp(500.0, 780.0);
            y = y.clamp(60.0, height - 20.0);

            set_node_positions.update(|map| {
                map.insert(key, (x, y));
            });
        }
    };

    let stop_drag = move |_ev: web_sys::MouseEvent| {
        set_dragging_key.set(None);
    };

    view! {
        <svg
            viewBox={format!("0 0 820 {:.0}", height)}
            class="virtual-iot-wiring-svg"
            xmlns="http://www.w3.org/2000/svg"
            on:mousemove=on_mouse_move
            on:mouseup=stop_drag
            on:mouseleave=stop_drag
        >
            <defs>
                <marker id="wire-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
                    <path d="M0,0 L8,4 L0,8 z" fill="#7dd3fc"/>
                </marker>
                <linearGradient id="wiring-board-glow" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0%" stop-color="rgba(255,255,255,0.12)"/>
                    <stop offset="35%" stop-color="rgba(255,255,255,0.02)"/>
                    <stop offset="100%" stop-color="rgba(0,0,0,0.20)"/>
                </linearGradient>
                <pattern id="wiring-pcb-grid" width="18" height="18" patternUnits="userSpaceOnUse">
                    <path d="M18 0H0V18" fill="none" stroke="rgba(255,255,255,0.03)" stroke-width="1"/>
                </pattern>
                <filter id="wire-shadow" x="-20%" y="-20%" width="140%" height="140%">
                    <feDropShadow dx="0" dy="1.5" stdDeviation="1.8" flood-color="rgba(0,0,0,0.4)"/>
                </filter>
            </defs>

            <rect x="40" y="36" width="280" height={format!("{:.0}", height - 72.0)} rx="18" fill="#153220" stroke="#102012" stroke-width="1.5"/>
            <rect x="40" y="36" width="280" height={format!("{:.0}", height - 72.0)} rx="18" fill="url(#wiring-pcb-grid)"/>
            <rect x="40" y="36" width="280" height={format!("{:.0}", height - 72.0)} rx="18" fill="url(#wiring-board-glow)"/>
            <rect x="78" y="92" width="168" height="76" rx="8" fill="#0f172a" stroke="#475569" stroke-width="1.2"/>
            <text x="180" y="64" text-anchor="middle" fill="#e3f1ff" font-size="15" font-family="sans-serif">{board_name}</text>
            <text x="180" y="84" text-anchor="middle" fill="#9cc2df" font-size="10.5" font-family="monospace">"Physical default jumper layout"</text>
            <text x="162" y="128" text-anchor="middle" fill="#d7e1eb" font-size="11" font-family="monospace">"MCU / SoC"</text>
            <text x="162" y="143" text-anchor="middle" fill="#7f93a8" font-size="8" font-family="monospace">"signal breakout area"</text>

            {(0..7).map(|i| {
                let y = 116 + i * 20;
                view! {
                    <path d={format!("M 52 {} C 82 {}, 82 {}, 98 {}", y, y, y, y)} stroke="rgba(210,180,88,0.22)" stroke-width="2" fill="none"/>
                }
            }).collect::<Vec<_>>()}

            {wiring_map.iter().enumerate().map(|(idx, entry)| {
                let default_sensor_y = 110.0 + idx as f32 * 56.0;
                let pin_anchor = virtual_iot_primary_pin(&entry.pin);
                let pin_y = virtual_iot_pin_anchor_y(&controller_type, &pin_anchor).min(height - 40.0);
                let stroke = wire_color_for_bus(&entry.pin, &entry.bus);
                let dash = if entry.pin == "INTERNAL" || entry.pin == "EXTERNAL" { "5 5" } else { "0" };
                let key_for_path = entry.sensor_key.clone();
                let key_for_transform = entry.sensor_key.clone();
                let key_for_drag = entry.sensor_key.clone();
                let icon = match entry.bus.as_str() {
                    bus if bus.contains("I2C") => "I2C",
                    bus if bus.contains("UART") => "UART",
                    bus if bus.contains("SPI") => "SPI",
                    bus if bus.contains("ADC") => "ADC",
                    bus if bus.contains("GPIO") => "GPIO",
                    bus if bus.contains("WiFi") => "RF",
                    bus if bus.contains("BLE") => "BLE",
                    _ => "SIG",
                };

                view! {
                    <>
                        <circle cx="320" cy={format!("{:.1}", pin_y)} r="7" fill="#111827" stroke="#9b7a25" stroke-width="1.3"/>
                        <circle cx="320" cy={format!("{:.1}", pin_y)} r="3.5" fill="#d4af37"/>
                        <text x="302" y={format!("{:.1}", pin_y - 11.0)} text-anchor="end" fill="#f6e7ae" font-size="9" font-family="monospace">{pin_anchor.clone()}</text>

                        <path
                            d={move || {
                                let (sensor_x, sensor_y) = node_positions
                                    .get()
                                    .get(&key_for_path)
                                    .copied()
                                    .unwrap_or((640.0, default_sensor_y));
                                format!(
                                    "M 327 {:.1} C 390 {:.1}, 430 {:.1}, 500 {:.1} L {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
                                    pin_y,
                                    pin_y,
                                    pin_y,
                                    pin_y,
                                    sensor_x - 70.0,
                                    pin_y,
                                    sensor_x - 30.0,
                                    pin_y,
                                    sensor_x - 20.0,
                                    sensor_y,
                                    sensor_x - 10.0,
                                    sensor_y,
                                )
                            }}
                            stroke={stroke}
                            stroke-width="3"
                            stroke-dasharray={dash}
                            fill="none"
                            marker-end="url(#wire-arrow)"
                            filter="url(#wire-shadow)"
                        />

                        <g
                            class="virtual-iot-draggable-node"
                            transform={move || {
                                let (sensor_x, sensor_y) = node_positions
                                    .get()
                                    .get(&key_for_transform)
                                    .copied()
                                    .unwrap_or((640.0, default_sensor_y));
                                format!("translate({:.1} {:.1})", sensor_x, sensor_y)
                            }}
                            on:mousedown={
                                let key = key_for_drag.clone();
                                move |ev: web_sys::MouseEvent| {
                                    let (sensor_x, sensor_y) = node_positions
                                        .get()
                                        .get(&key)
                                        .copied()
                                        .unwrap_or((640.0, default_sensor_y));
                                    set_dragging_key.set(Some(key.clone()));
                                    set_drag_offset.set((ev.offset_x() as f32 - sensor_x, ev.offset_y() as f32 - sensor_y));
                                }
                            }
                        >
                            <rect x="-148" y="-26" width="296" height="52" rx="12" fill="#111827" stroke="#334155" stroke-width="1.1"/>
                            <rect x="-140" y="-18" width="28" height="28" rx="7" fill={stroke} opacity="0.18" stroke={stroke} stroke-width="1"/>
                            <text x="-126" y="1.5" text-anchor="middle" fill="#e2e8f0" font-size="8.5" font-family="monospace">{icon}</text>
                            <text x="-102" y="-6" fill="#edf6ff" font-size="12" font-family="sans-serif">{entry.sensor_label.clone()}</text>
                            <text x="-102" y="11" fill="#97b3cf" font-size="9.6" font-family="monospace">{format!("{}  |  {}", entry.pin, entry.bus)}</text>
                            <circle cx="126" cy="0" r="6" fill="#1f2937" stroke="#475569" stroke-width="0.8"/>
                            <circle cx="126" cy="0" r="2.8" fill={stroke}/>
                        </g>
                    </>
                }
            }).collect::<Vec<_>>()}
        </svg>
    }
}
