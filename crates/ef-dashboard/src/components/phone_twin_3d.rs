use leptos::prelude::*;
use std::time::Duration;

/// Fixed container id for the live phone twin (a single DeviceViewer is open
/// at a time). The live screenshot `<img id="ef-live-screen">` is used by the
/// JS runtime as the screen texture source.
pub const PHONE_TWIN_CONTAINER_ID: &str = "ef-phone-twin-live";
pub const LIVE_SCREEN_IMG_ID: &str = "ef-live-screen";

fn twin_ready() -> bool {
    js_sys::eval("typeof window.THREE !== 'undefined' && !!window.EfPhoneTwin")
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn js_call(script: &str) {
    let _ = js_sys::eval(script);
}

/// Raycast a client point onto the 3D live screens → device pixels.
/// Returns `None` when the ray misses the screens.
pub fn phone_twin_pick(client_x: f64, client_y: f64) -> Option<(i32, i32)> {
    let v = js_sys::eval(&format!(
        "window.EfPhoneTwin ? window.EfPhoneTwin.pick({id:?}, {x}, {y}) : null",
        id = PHONE_TWIN_CONTAINER_ID,
        x = client_x,
        y = client_y
    ))
    .ok()?;
    if v.is_null() || v.is_undefined() {
        return None;
    }
    let s = v.as_string()?;
    let parsed: serde_json::Value = serde_json::from_str(&s).ok()?;
    Some((
        parsed.get("x")?.as_i64()? as i32,
        parsed.get("y")?.as_i64()? as i32,
    ))
}

/// 3D phone twin with the live simulator screen as its display texture.
///
/// - `model`: "iphone-18-pro" | "iphone-duo" | "iphone-generic"
/// - `frame_tick`: bumped on every streamed frame → refreshes the texture
/// - `resolution`: device pixels (drives tap/swipe mapping)
/// - `fold`: Duo open amount 0 (closed) .. 1 (open)
#[component]
pub fn PhoneTwin3d(
    model: ReadSignal<String>,
    frame_tick: ReadSignal<u64>,
    resolution: ReadSignal<[u32; 2]>,
    fold: ReadSignal<f64>,
    #[prop(optional)] mini: bool,
) -> impl IntoView {
    let (three_missing, set_three_missing) = signal(false);
    let (auto_rotate, set_auto_rotate) = signal(true);
    let (init_tick, set_init_tick) = signal(0u32);

    // CDN readiness: retry a few times, then report missing (parent falls back to 2D).
    Effect::new(move || {
        for delay_ms in [1000u64, 3000, 6000] {
            set_timeout(
                move || {
                    if twin_ready() {
                        set_three_missing.set(false);
                        set_init_tick.update(|t| *t = t.wrapping_add(1));
                    } else if delay_ms == 6000 {
                        set_three_missing.set(true);
                    }
                },
                Duration::from_millis(delay_ms),
            );
        }
    });

    // (Re-)init when the model changes.
    Effect::new(move || {
        let _ = init_tick.get();
        if three_missing.get() || !twin_ready() {
            return;
        }
        let m = model.get();
        let res = resolution.get_untracked();
        let f = fold.get_untracked();
        let rot = auto_rotate.get_untracked();
        set_timeout(
            move || {
                js_call(&format!(
                    "(function(){{ if(!window.EfPhoneTwin) return; window.EfPhoneTwin.init({id:?}, {{model:{m:?},resW:{w},resH:{h},fold:{f},autoRotate:{rot}}}); }})()",
                    id = PHONE_TWIN_CONTAINER_ID,
                    m = m,
                    w = res[0],
                    h = res[1],
                    f = f.clamp(0.0, 1.0),
                    rot = rot
                ));
            },
            Duration::from_millis(60),
        );
    });

    // Push resolution changes (tap mapping) without re-init.
    Effect::new(move || {
        let res = resolution.get();
        if three_missing.get_untracked() {
            return;
        }
        js_call(&format!(
            "window.EfPhoneTwin && window.EfPhoneTwin.setRes({id:?}, {w}, {h})",
            id = PHONE_TWIN_CONTAINER_ID,
            w = res[0],
            h = res[1]
        ));
    });

    // Duo fold angle.
    Effect::new(move || {
        let f = fold.get();
        if three_missing.get_untracked() {
            return;
        }
        js_call(&format!(
            "window.EfPhoneTwin && window.EfPhoneTwin.setFold({id:?}, {f})",
            id = PHONE_TWIN_CONTAINER_ID,
            f = f.clamp(0.0, 1.0)
        ));
    });

    // Spin toggle.
    Effect::new(move || {
        let rot = auto_rotate.get();
        if three_missing.get_untracked() {
            return;
        }
        js_call(&format!(
            "window.EfPhoneTwin && window.EfPhoneTwin.setEnv({id:?}, {{autoRotate:{rot}}})",
            id = PHONE_TWIN_CONTAINER_ID,
            rot = rot
        ));
    });

    // Refresh the screen texture on every streamed frame.
    Effect::new(move || {
        let _ = frame_tick.get();
        if three_missing.get_untracked() {
            return;
        }
        js_call(&format!(
            "window.EfPhoneTwin && window.EfPhoneTwin.refresh({id:?})",
            id = PHONE_TWIN_CONTAINER_ID
        ));
    });

    on_cleanup(|| {
        js_call(&format!(
            "window.EfPhoneTwin && window.EfPhoneTwin.dispose({id:?})",
            id = PHONE_TWIN_CONTAINER_ID
        ));
    });

    view! {
        <div class="board-twin phone-twin">
            <div class="board-twin-toolbar">
                <span class="board-twin-badge" style="position:static;">"3D · live"</span>
                <label class="board-twin-check" title="Slow turntable rotation">
                    <input
                        type="checkbox"
                        prop:checked=move || auto_rotate.get()
                        on:change=move |ev| set_auto_rotate.set(event_target_checked(&ev))
                    />
                    <span>"Spin"</span>
                </label>
                <span class="board-twin-fallback-note">
                    {move || if three_missing.get() {
                        "WebGL unavailable — use 2D view".to_string()
                    } else {
                        format!("{} · drag to orbit · scroll to zoom · tap the 3D screen", model.get())
                    }}
                </span>
            </div>
            <div class="board-twin-canvas-wrap phone-twin-canvas-wrap" class:is-mini=mini>
                <div id=PHONE_TWIN_CONTAINER_ID class="board-twin-canvas"></div>
                <span class="board-twin-hint">"drag · orbit — wheel · zoom — tap screen · input"</span>
            </div>
        </div>
    }
}
