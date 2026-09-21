use leptos::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::board_config::{bus_wire_color, default_wiring, find_controller, sensor_label};
use crate::components::board_layout::render_catalog_board_layout;

static TWIN_SEQ: AtomicUsize = AtomicUsize::new(0);

fn twin_ready() -> bool {
    js_sys::eval("typeof window.THREE !== 'undefined' && !!window.EfBoardTwin")
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn js_call(script: &str) {
    let _ = js_sys::eval(script);
}

#[derive(Clone, serde::Serialize)]
struct TwinSensorNode {
    key: String,
    label: String,
    bus: String,
    color: String,
}

/// 3D board twin (Three.js WebGL) with 2D SVG fallback.
///
/// - `controller`: controller id signal (e.g. "esp32-c6", "esp32-s3", "rpi-5")
/// - `enabled_sensors`: enabled sensor id list (drives the 3D node ring)
/// - `sensor_values`: live reading map (drives glow levels + motion tilt)
/// - `pulse`: optional sensor key to flash (event playback)
/// - `mini`: compact height for cards/panels
#[component]
pub fn BoardTwin3d(
    controller: ReadSignal<String>,
    enabled_sensors: ReadSignal<Vec<String>>,
    sensor_values: ReadSignal<HashMap<String, String>>,
    #[prop(optional)] pulse: Option<ReadSignal<Option<String>>>,
    #[prop(optional)] mini: bool,
) -> impl IntoView {
    let seq = TWIN_SEQ.fetch_add(1, Ordering::Relaxed);
    let container_id = format!("ef-board-twin-{seq}");

    let (view_mode, set_view_mode) = signal("3d".to_string());
    let (three_missing, set_three_missing) = signal(false);
    let (day_night, set_day_night) = signal(0.75f64);
    let (auto_rotate, set_auto_rotate) = signal(true);
    let (tilt_motion, set_tilt_motion) = signal(true);
    let (init_tick, set_init_tick) = signal(0u32);

    let (no_pulse, _) = signal(Option::<String>::None);
    let pulse_sig = pulse.unwrap_or(no_pulse);

    // CDN readiness: the Three.js script may arrive after WASM mounts.
    // Re-check at 1s / 3s / 6s; fall back to precise 2D if still missing.
    Effect::new(move || {
        for delay_ms in [1000u64, 3000, 6000] {
            set_timeout(
                move || {
                    if twin_ready() {
                        set_three_missing.set(false);
                        set_init_tick.update(|t| *t = t.wrapping_add(1));
                    } else if delay_ms == 6000 {
                        set_three_missing.set(true);
                        set_view_mode.set("2d".to_string());
                    }
                },
                Duration::from_millis(delay_ms),
            );
        }
    });

    // (Re-)init the scene when controller / sensor set / retry tick changes.
    {
        let container_id = container_id.clone();
        Effect::new(move || {
            let _ = init_tick.get();
            if view_mode.get() != "3d" || three_missing.get() || !twin_ready() {
                return;
            }
            // Defer one tick so the container div is in the DOM.
            let cid = container_id.clone();
            let ctl = controller.get();
            let sensors = enabled_sensors.get();
            let values = sensor_values.get_untracked();
            let day = day_night.get_untracked();
            let rot = auto_rotate.get_untracked();
            let tilt = tilt_motion.get_untracked();
            set_timeout(
                move || init_twin_scene(&cid, &ctl, &sensors, &values, day, rot, tilt),
                Duration::from_millis(60),
            );
        });
    }

    // Push live values (no re-init) on every sensor frame.
    {
        let container_id = container_id.clone();
        Effect::new(move || {
            let values = sensor_values.get();
            let pulse_key = pulse_sig.get();
            if view_mode.get_untracked() != "3d" || three_missing.get_untracked() {
                return;
            }
            push_twin_values(&container_id, &values, pulse_key.as_deref());
        });
    }

    // Push env (day/night, rotate, tilt) changes.
    {
        let container_id = container_id.clone();
        Effect::new(move || {
            let day = day_night.get();
            let rot = auto_rotate.get();
            let tilt = tilt_motion.get();
            if view_mode.get_untracked() != "3d" || three_missing.get_untracked() {
                return;
            }
            js_call(&format!(
                "window.EfBoardTwin && window.EfBoardTwin.setEnv({id:?}, {{dayNight:{day},autoRotate:{rot},tiltFromMotion:{tilt}}})",
                id = container_id,
                day = day,
                rot = rot,
                tilt = tilt
            ));
        });
    }

    // Dispose the WebGL context on unmount.
    {
        let container_id = container_id.clone();
        on_cleanup(move || {
            js_call(&format!(
                "window.EfBoardTwin && window.EfBoardTwin.dispose({id:?})",
                id = container_id
            ));
        });
    }

    let legend_nodes = move || twin_nodes(&controller.get(), &enabled_sensors.get());
    let fallback_profile = move || find_controller(&controller.get()).clone();
    let container_id_view = container_id.clone();

    view! {
        <div class="board-twin">
            <div class="board-twin-toolbar">
                <div class="board-twin-seg" role="tablist" aria-label="Board view mode">
                    <button
                        class:is-active=move || view_mode.get() == "3d"
                        on:click=move |_| {
                            if !three_missing.get_untracked() {
                                set_view_mode.set("3d".to_string());
                                set_init_tick.update(|t| *t = t.wrapping_add(1));
                            }
                        }
                        title="Realistic 3D twin (WebGL)"
                    >"3D"</button>
                    <button
                        class:is-active=move || view_mode.get() == "2d"
                        on:click=move |_| set_view_mode.set("2d".to_string())
                        title="Precise 2D schematic (SVG)"
                    >"2D"</button>
                </div>
                <Show when=move || view_mode.get() == "3d" && !three_missing.get()>
                    <label class="board-twin-env" title="Scene lighting: night ↔ day">
                        <span>"☾"</span>
                        <input
                            type="range" min="0" max="1" step="0.01"
                            prop:value=move || day_night.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    set_day_night.set(v.clamp(0.0, 1.0));
                                }
                            }
                        />
                        <span>"☀"</span>
                    </label>
                    <label class="board-twin-check" title="Slow turntable rotation">
                        <input
                            type="checkbox"
                            prop:checked=move || auto_rotate.get()
                            on:change=move |ev| set_auto_rotate.set(event_target_checked(&ev))
                        />
                        <span>"Spin"</span>
                    </label>
                    <label class="board-twin-check" title="Tilt the board from live accelerometer data">
                        <input
                            type="checkbox"
                            prop:checked=move || tilt_motion.get()
                            on:change=move |ev| set_tilt_motion.set(event_target_checked(&ev))
                        />
                        <span>"Motion"</span>
                    </label>
                </Show>
                <span class="board-twin-fallback-note">
                    {move || if three_missing.get() {
                        "WebGL unavailable — showing precise 2D".to_string()
                    } else if view_mode.get() == "2d" {
                        "Precise 2D schematic".to_string()
                    } else {
                        format!("{} · drag to orbit · scroll to zoom", controller.get())
                    }}
                </span>
            </div>

            <Show
                when=move || view_mode.get() == "3d" && !three_missing.get()
                fallback=move || {
                    let profile = fallback_profile();
                    let chip = profile.chip_label.clone();
                    view! {
                        <div class="board-preview-container">
                            {render_catalog_board_layout(&profile, &chip).unwrap_or_else(|| view! { <span class="empty-state">"No board visual"</span> }.into_any())}
                        </div>
                    }
                }
            >
                <div
                    class="board-twin-canvas-wrap"
                    class:is-mini=mini
                >
                    <div id={container_id_view.clone()} class="board-twin-canvas"></div>
                    <span class="board-twin-badge">"live twin"</span>
                    <span class="board-twin-hint">"drag · orbit — wheel · zoom"</span>
                </div>
            </Show>

            <div class="board-twin-legend">
                {move || legend_nodes().into_iter().map(|n| {
                    view! { <span class="chip" title={n.bus.clone()}>{n.label.clone()}</span> }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}

fn twin_nodes(controller: &str, sensors: &[String]) -> Vec<TwinSensorNode> {
    let wiring = default_wiring(controller, sensors);
    sensors
        .iter()
        .map(|key| {
            let entry = wiring.iter().find(|e| &e.sensor_key == key);
            let (bus, label) = match entry {
                Some(e) => (e.bus.clone(), e.sensor_label.clone()),
                None => ("Module".to_string(), sensor_label(key).to_string()),
            };
            let color = bus_wire_color("PIN_X", &bus)
                .or_else(|| bus_color_fallback(&bus))
                .unwrap_or("#94a3b8")
                .to_string();
            TwinSensorNode {
                key: key.clone(),
                label,
                bus,
                color,
            }
        })
        .collect()
}

fn bus_color_fallback(bus: &str) -> Option<&'static str> {
    let b = bus.to_ascii_uppercase();
    if b.contains("WIFI") {
        Some("#3b82f6")
    } else if b.contains("BLE") {
        Some("#8b5cf6")
    } else if b.contains("I2C") {
        Some("#38bdf8")
    } else if b.contains("SPI") {
        Some("#f59e0b")
    } else if b.contains("UART") || b.contains("SERIAL") {
        Some("#22c55e")
    } else if b.contains("ADC") {
        Some("#ef4444")
    } else if b.contains("GPIO") || b.contains("DIGITAL") {
        Some("#facc15")
    } else if b.contains("I2S") || b.contains("USB") {
        Some("#fb7185")
    } else {
        None
    }
}

fn init_twin_scene(
    container_id: &str,
    controller: &str,
    sensors: &[String],
    values: &HashMap<String, String>,
    day: f64,
    rot: bool,
    tilt: bool,
) {
    let profile = find_controller(controller);
    let nodes = twin_nodes(controller, sensors);
    let sensors_json = serde_json::to_string(&nodes).unwrap_or_else(|_| "[]".into());
    let values_json = serde_json::to_string(values).unwrap_or_else(|_| "{}".into());
    // serde JSON output is double-quoted with no raw `</script>` risk for our
    // sensor payloads; embed directly.
    js_call(&format!(
        "(function(){{ if(!window.EfBoardTwin) return; window.EfBoardTwin.init({id:?}, {{controller:{ctl:?},boardColor:{color:?},chipLabel:{chip:?},pinCount:{pins},sensors:{sensors},values:{values},dayNight:{day},autoRotate:{rot},tiltFromMotion:{tilt}}}); }})()",
        id = container_id,
        ctl = controller,
        color = profile.board_color.as_str(),
        chip = profile.chip_label.as_str(),
        pins = profile.pin_count,
        sensors = sensors_json,
        values = values_json,
        day = day,
        rot = rot,
        tilt = tilt
    ));
}

fn push_twin_values(
    container_id: &str,
    values: &HashMap<String, String>,
    pulse: Option<&str>,
) {
    let values_json = serde_json::to_string(values).unwrap_or_else(|_| "{}".into());
    let pulse_json = match pulse {
        Some(p) if !p.is_empty() => serde_json::to_string(p).unwrap_or_else(|_| "null".into()),
        _ => "null".into(),
    };
    js_call(&format!(
        "(function(){{ if(!window.EfBoardTwin) return; window.EfBoardTwin.update({id:?}, {{values:{values},pulse:{pulse}}}); }})()",
        id = container_id,
        values = values_json,
        pulse = pulse_json
    ));
}
