use leptos::prelude::*;
use serde_json::Value;

use crate::api::{
    self, AttachVirtualIotBackendRequest, CreateVirtualIotRequest, UpdateVirtualIotConfigRequest,
    VirtualIotBackendListResponse, VirtualIotConfigResponse, VirtualIotEventDefinition,
    VirtualIotEventStep,
};
use crate::board_config::{controller_accessories, controllers, find_controller, sensor_categories};
use crate::components::board_layout::render_catalog_board_layout;

fn extract_host_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next().unwrap_or_default();
    let host = host_port.split(':').next().unwrap_or_default().trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn is_local_host_label(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "local" | "localhost" | "127.0.0.1" | "::1"
    )
}

fn format_agent_host_label(agent_id: Option<&str>, host_ip: &str) -> String {
    let host_missing = host_ip == "unknown-host" || host_ip == "—";
    let host_is_local = is_local_host_label(host_ip);

    match agent_id {
        None => {
            if host_missing || host_is_local {
                "local".to_string()
            } else {
                format!("local · {host_ip}")
            }
        }
        Some(id) => {
            let agent_label = if id.len() > 8 {
                format!("agent:{}", &id[..8])
            } else {
                format!("agent:{id}")
            };

            if host_missing {
                agent_label
            } else {
                format!("{agent_label} · {host_ip}")
            }
        }
    }
}

/// Shape of the value field(s) for a given sensor — drives the input widgets
/// in the event-step editor.
#[derive(Clone, PartialEq)]
enum ValueShape {
    /// Single number input (with optional min/max/step/unit label).
    Scalar,
    /// Three comma-joined numbers (x, y, z). Stored as "x,y,z".
    Vec3,
    /// Two comma-joined numbers (lat, lng). Stored as "lat,lng".
    LatLng,
    /// 0 / 1 binary — rendered as a small select.
    Toggle,
}

#[derive(Clone)]
struct ValueSpec {
    shape: ValueShape,
    /// Shown as a suffix chip on scalar inputs (e.g., "°C", "ppm", "dBm").
    unit: &'static str,
    /// Labels for Vec3 axes (x, y, z) or LatLng (lat, lng).
    axes: &'static [&'static str],
    /// Placeholder text when the field is empty.
    placeholder: &'static str,
    /// Min / max / step hints for `type=number` inputs.
    min: Option<f64>,
    max: Option<f64>,
    step: f64,
}

fn virtual_iot_value_spec(sensor: &str) -> ValueSpec {
    use ValueShape::*;
    match sensor {
        // --- motion (vectors) ---
        "accel_xyz" => ValueSpec {
            shape: Vec3, unit: "m/s²", axes: &["x", "y", "z"],
            placeholder: "0.0", min: Some(-40.0), max: Some(40.0), step: 0.1,
        },
        "gyro_xyz" => ValueSpec {
            shape: Vec3, unit: "°/s", axes: &["x", "y", "z"],
            placeholder: "0", min: Some(-2000.0), max: Some(2000.0), step: 1.0,
        },
        "magnetometer" => ValueSpec {
            shape: Vec3, unit: "µT", axes: &["x", "y", "z"],
            placeholder: "0.0", min: Some(-100.0), max: Some(100.0), step: 0.1,
        },

        // --- location ---
        "gps" => ValueSpec {
            shape: LatLng, unit: "°", axes: &["lat", "lng"],
            placeholder: "0.0", min: Some(-180.0), max: Some(180.0), step: 0.0001,
        },

        // --- environmental scalars ---
        "temperature"    => ValueSpec { shape: Scalar, unit: "°C",   axes: &[], placeholder: "22.0", min: Some(-40.0), max: Some(125.0),  step: 0.1 },
        "humidity"       => ValueSpec { shape: Scalar, unit: "%RH",  axes: &[], placeholder: "45",   min: Some(0.0),   max: Some(100.0),  step: 0.1 },
        "pressure"       => ValueSpec { shape: Scalar, unit: "hPa",  axes: &[], placeholder: "1013", min: Some(300.0), max: Some(1100.0), step: 0.1 },
        "air_quality_co2"=> ValueSpec { shape: Scalar, unit: "ppm",  axes: &[], placeholder: "450",  min: Some(300.0), max: Some(5000.0), step: 1.0 },
        "air_quality_tvoc"=>ValueSpec { shape: Scalar, unit: "ppb",  axes: &[], placeholder: "50",   min: Some(0.0),   max: Some(5000.0), step: 1.0 },
        "ambient_light"  => ValueSpec { shape: Scalar, unit: "lux",  axes: &[], placeholder: "400",  min: Some(0.0),   max: Some(100_000.0), step: 1.0 },
        "uv_index"       => ValueSpec { shape: Scalar, unit: "UVI",  axes: &[], placeholder: "3",    min: Some(0.0),   max: Some(12.0),   step: 0.1 },

        // --- proximity ---
        "proximity"      => ValueSpec { shape: Scalar, unit: "cm",   axes: &[], placeholder: "30",   min: Some(0.0),   max: Some(400.0),  step: 1.0 },

        // --- audio ---
        "sound_level"    => ValueSpec { shape: Scalar, unit: "dB",   axes: &[], placeholder: "55",   min: Some(20.0),  max: Some(120.0),  step: 1.0 },

        // --- power ---
        "battery_voltage"=> ValueSpec { shape: Scalar, unit: "V",    axes: &[], placeholder: "3.7",  min: Some(0.0),   max: Some(5.0),    step: 0.01 },
        "current"        => ValueSpec { shape: Scalar, unit: "mA",   axes: &[], placeholder: "100",  min: Some(-5000.0), max: Some(5000.0), step: 1.0 },

        // --- digital ---
        "gpio_states"    => ValueSpec { shape: Toggle, unit: "",     axes: &[], placeholder: "0 / 1", min: Some(0.0), max: Some(1.0),    step: 1.0 },
        "adc_raw"        => ValueSpec { shape: Scalar, unit: "raw",  axes: &[], placeholder: "2048", min: Some(0.0),   max: Some(4095.0), step: 1.0 },

        // --- network ---
        "wifi_rssi"      => ValueSpec { shape: Scalar, unit: "dBm",  axes: &[], placeholder: "-65",  min: Some(-100.0), max: Some(-20.0), step: 1.0 },
        "ble_scan"       => ValueSpec { shape: Scalar, unit: "dBm",  axes: &[], placeholder: "-75",  min: Some(-100.0), max: Some(-20.0), step: 1.0 },

        _ => ValueSpec { shape: Scalar, unit: "", axes: &[], placeholder: "value", min: None, max: None, step: 1.0 },
    }
}

/// Sensor-specific action list. Each entry is `(action_id, label)`.
fn virtual_iot_sensor_actions(sensor: &str) -> Vec<(&'static str, &'static str)> {
    match sensor {
        "accel_xyz" => vec![
            ("tilt",   "Tilt"),
            ("shake",  "Shake"),
            ("drop",   "Drop / free fall"),
            ("flip",   "Flip over"),
            ("hold",   "Hold steady"),
        ],
        "gyro_xyz" => vec![
            ("spin",   "Spin"),
            ("rotate", "Rotate slow"),
            ("shake",  "Shake"),
            ("hold",   "Hold"),
        ],
        "magnetometer" => vec![
            ("rotate",     "Rotate heading"),
            ("near_magnet","Near magnet"),
            ("far_magnet", "Away from magnet"),
            ("hold",       "Hold"),
        ],
        "gps" => vec![
            ("move_to",  "Move to"),
            ("drift",    "Drift"),
            ("teleport", "Teleport"),
            ("lose_fix", "Lose fix"),
            ("hold",     "Hold position"),
        ],
        "proximity" => vec![
            ("approach", "Approach"),
            ("retreat",  "Retreat"),
            ("dock",     "Dock"),
            ("pass_by",  "Pass by"),
            ("hold",     "Hold distance"),
        ],
        "gpio_states" => vec![
            ("set_high", "Set HIGH"),
            ("set_low",  "Set LOW"),
            ("toggle",   "Toggle"),
            ("pulse",    "Pulse"),
            ("hold",     "Hold"),
        ],
        "adc_raw" => vec![
            ("ramp_up",   "Ramp up"),
            ("ramp_down", "Ramp down"),
            ("noise",     "Noise spike"),
            ("hold",      "Hold"),
        ],
        "wifi_rssi" => vec![
            ("strong", "Strong signal"),
            ("weak",   "Weak signal"),
            ("drop",   "Signal drop"),
            ("gain",   "Signal gain"),
            ("flap",   "Flap (oscillate)"),
            ("hold",   "Hold"),
        ],
        "ble_scan" => vec![
            ("nearby", "Devices nearby"),
            ("far",    "Devices far"),
            ("drop",   "Signal drop"),
            ("hold",   "Hold"),
        ],
        "sound_level" => vec![
            ("silence",       "Silence"),
            ("conversation",  "Conversation"),
            ("music",         "Music"),
            ("alarm",         "Alarm / spike"),
            ("fade_in",       "Fade in"),
            ("fade_out",      "Fade out"),
            ("hold",          "Hold level"),
        ],
        "battery_voltage" => vec![
            ("charge",     "Charging"),
            ("discharge",  "Discharging"),
            ("drain_fast", "Drain fast"),
            ("hold",       "Hold"),
        ],
        "current" => vec![
            ("load_on",  "Load on"),
            ("load_off", "Load off"),
            ("spike",    "Inrush spike"),
            ("hold",     "Hold"),
        ],
        "temperature" => vec![
            ("heat_up",     "Heat up"),
            ("cool_down",   "Cool down"),
            ("fever_spike", "Fever spike"),
            ("cold_snap",   "Cold snap"),
            ("hold",        "Hold"),
        ],
        "humidity" => vec![
            ("humidify",     "Humidify"),
            ("dry_out",      "Dry out"),
            ("condensation", "Condensation spike"),
            ("hold",         "Hold"),
        ],
        "pressure" => vec![
            ("storm",  "Storm approach"),
            ("clear",  "Clear skies"),
            ("ascend", "Ascend altitude"),
            ("hold",   "Hold"),
        ],
        "air_quality_co2" => vec![
            ("ventilate", "Ventilate"),
            ("crowded",   "Crowded room"),
            ("spike",     "CO₂ spike"),
            ("hold",      "Hold"),
        ],
        "air_quality_tvoc" => vec![
            ("fresh_air", "Fresh air"),
            ("emission",  "Emission spike"),
            ("hold",      "Hold"),
        ],
        "ambient_light" => vec![
            ("sunrise", "Sunrise"),
            ("sunset",  "Sunset"),
            ("flash",   "Flash"),
            ("hold",    "Hold"),
        ],
        "uv_index" => vec![
            ("clear_noon", "Clear noon"),
            ("cloud_cover","Cloud cover"),
            ("hold",       "Hold"),
        ],
        _ => vec![
            ("increase", "Increase"),
            ("decrease", "Decrease"),
            ("hold",     "Hold"),
        ],
    }
}

/// For a given (sensor, action), return recommended (value, end_value, duration_ms).
/// The user can still fine-tune these; this just gives sensible defaults.
fn virtual_iot_action_preset(
    sensor: &str,
    action: &str,
) -> (String, Option<String>, Option<i32>) {
    match (sensor, action) {
        // --- accelerometer (m/s²) ---
        ("accel_xyz", "tilt")  => ("0,0,9.8".into(), Some("6.5,0,7.2".into()), Some(1500)),
        ("accel_xyz", "shake") => ("0,0,9.8".into(), Some("18,-14,11".into()), Some(800)),
        ("accel_xyz", "drop")  => ("0,0,9.8".into(), Some("0,0,0".into()),     Some(600)),
        ("accel_xyz", "flip")  => ("0,0,9.8".into(), Some("0,0,-9.8".into()),  Some(1200)),
        ("accel_xyz", "hold")  => ("0,0,9.8".into(), None,                     Some(1000)),

        // --- gyroscope (°/s) ---
        ("gyro_xyz", "spin")   => ("0,0,0".into(),   Some("0,0,720".into()),   Some(2000)),
        ("gyro_xyz", "rotate") => ("0,0,0".into(),   Some("0,0,90".into()),    Some(3000)),
        ("gyro_xyz", "shake")  => ("0,0,0".into(),   Some("60,-40,80".into()), Some(600)),
        ("gyro_xyz", "hold")   => ("0,0,0".into(),   None,                     Some(1000)),

        // --- magnetometer (µT) ---
        ("magnetometer", "rotate")      => ("25,0,-40".into(), Some("-25,0,-40".into()), Some(2500)),
        ("magnetometer", "near_magnet") => ("25,0,-40".into(), Some("80,60,-20".into()), Some(1500)),
        ("magnetometer", "far_magnet")  => ("80,60,-20".into(),Some("25,0,-40".into()),  Some(1500)),
        ("magnetometer", "hold")        => ("25,0,-40".into(), None,                     Some(1500)),

        // --- gps (lat,lng) ---
        ("gps", "move_to")  => ("37.7749,-122.4194".into(), Some("37.7849,-122.4094".into()), Some(4000)),
        ("gps", "drift")    => ("37.7749,-122.4194".into(), Some("37.7752,-122.4197".into()), Some(6000)),
        ("gps", "teleport") => ("37.7749,-122.4194".into(), Some("40.7128,-74.0060".into()),  Some(250)),
        ("gps", "lose_fix") => ("37.7749,-122.4194".into(), Some("0,0".into()),               Some(500)),
        ("gps", "hold")     => ("37.7749,-122.4194".into(), None,                             Some(2000)),

        // --- proximity (cm) ---
        ("proximity", "approach") => ("100".into(), Some("5".into()),   Some(2500)),
        ("proximity", "retreat")  => ("5".into(),   Some("100".into()), Some(2500)),
        ("proximity", "dock")     => ("30".into(),  Some("2".into()),   Some(1800)),
        ("proximity", "pass_by")  => ("80".into(),  Some("80".into()),  Some(1500)),
        ("proximity", "hold")     => ("30".into(),  None,               Some(1500)),

        // --- gpio ---
        ("gpio_states", "set_high") => ("0".into(), Some("1".into()), Some(100)),
        ("gpio_states", "set_low")  => ("1".into(), Some("0".into()), Some(100)),
        ("gpio_states", "toggle")   => ("0".into(), Some("1".into()), Some(100)),
        ("gpio_states", "pulse")    => ("0".into(), Some("1".into()), Some(40)),
        ("gpio_states", "hold")     => ("1".into(), None,             Some(1000)),

        // --- adc (raw 0..4095) ---
        ("adc_raw", "ramp_up")   => ("0".into(),    Some("4095".into()), Some(2000)),
        ("adc_raw", "ramp_down") => ("4095".into(), Some("0".into()),    Some(2000)),
        ("adc_raw", "noise")     => ("1800".into(), Some("2300".into()), Some(200)),
        ("adc_raw", "hold")      => ("2048".into(), None,                Some(1000)),

        // --- wifi (dBm) ---
        ("wifi_rssi", "strong") => ("-45".into(), None,              Some(1500)),
        ("wifi_rssi", "weak")   => ("-85".into(), None,              Some(1500)),
        ("wifi_rssi", "drop")   => ("-55".into(), Some("-95".into()), Some(1800)),
        ("wifi_rssi", "gain")   => ("-85".into(), Some("-45".into()), Some(2000)),
        ("wifi_rssi", "flap")   => ("-60".into(), Some("-80".into()), Some(400)),
        ("wifi_rssi", "hold")   => ("-65".into(), None,               Some(1500)),

        // --- ble (dBm) ---
        ("ble_scan", "nearby") => ("-55".into(), None,               Some(1500)),
        ("ble_scan", "far")    => ("-90".into(), None,               Some(1500)),
        ("ble_scan", "drop")   => ("-60".into(), Some("-99".into()), Some(1800)),
        ("ble_scan", "hold")   => ("-75".into(), None,               Some(1500)),

        // --- sound (dB SPL) ---
        ("sound_level", "silence")      => ("30".into(), None,             Some(1500)),
        ("sound_level", "conversation") => ("55".into(), None,             Some(1500)),
        ("sound_level", "music")        => ("75".into(), None,             Some(1500)),
        ("sound_level", "alarm")        => ("45".into(), Some("105".into()), Some(300)),
        ("sound_level", "fade_in")      => ("30".into(), Some("75".into()),  Some(2500)),
        ("sound_level", "fade_out")     => ("75".into(), Some("30".into()),  Some(2500)),
        ("sound_level", "hold")         => ("55".into(), None,              Some(1500)),

        // --- battery (V) ---
        ("battery_voltage", "charge")     => ("3.3".into(), Some("4.1".into()), Some(3000)),
        ("battery_voltage", "discharge")  => ("4.1".into(), Some("3.3".into()), Some(3000)),
        ("battery_voltage", "drain_fast") => ("4.0".into(), Some("3.0".into()), Some(1500)),
        ("battery_voltage", "hold")       => ("3.7".into(), None,               Some(2000)),

        // --- current (mA) ---
        ("current", "load_on")  => ("5".into(),   Some("450".into()), Some(400)),
        ("current", "load_off") => ("450".into(), Some("5".into()),   Some(400)),
        ("current", "spike")    => ("100".into(), Some("2400".into()),Some(150)),
        ("current", "hold")     => ("120".into(), None,               Some(1500)),

        // --- temperature (°C) ---
        ("temperature", "heat_up")     => ("22".into(), Some("38".into()), Some(4000)),
        ("temperature", "cool_down")   => ("28".into(), Some("10".into()), Some(4000)),
        ("temperature", "fever_spike") => ("22".into(), Some("55".into()), Some(500)),
        ("temperature", "cold_snap")   => ("22".into(), Some("-5".into()), Some(500)),
        ("temperature", "hold")        => ("22".into(), None,              Some(1500)),

        // --- humidity (%RH) ---
        ("humidity", "humidify")     => ("40".into(), Some("85".into()), Some(4000)),
        ("humidity", "dry_out")      => ("75".into(), Some("20".into()), Some(4000)),
        ("humidity", "condensation") => ("60".into(), Some("98".into()), Some(300)),
        ("humidity", "hold")         => ("45".into(), None,              Some(1500)),

        // --- pressure (hPa) ---
        ("pressure", "storm")  => ("1018".into(), Some("982".into()),  Some(4000)),
        ("pressure", "clear")  => ("995".into(),  Some("1022".into()), Some(4000)),
        ("pressure", "ascend") => ("1013".into(), Some("920".into()),  Some(3000)),
        ("pressure", "hold")   => ("1013".into(), None,                Some(1500)),

        // --- air quality ---
        ("air_quality_co2", "ventilate") => ("1600".into(), Some("450".into()),  Some(6000)),
        ("air_quality_co2", "crowded")   => ("500".into(),  Some("1800".into()), Some(5000)),
        ("air_quality_co2", "spike")     => ("500".into(),  Some("2500".into()), Some(400)),
        ("air_quality_co2", "hold")      => ("450".into(),  None,                Some(1500)),

        ("air_quality_tvoc", "fresh_air") => ("400".into(), Some("40".into()),  Some(5000)),
        ("air_quality_tvoc", "emission")  => ("50".into(),  Some("450".into()), Some(3000)),
        ("air_quality_tvoc", "hold")      => ("50".into(),  None,               Some(1500)),

        // --- light (lux) ---
        ("ambient_light", "sunrise") => ("5".into(),    Some("1200".into()), Some(4000)),
        ("ambient_light", "sunset")  => ("1500".into(), Some("5".into()),    Some(4000)),
        ("ambient_light", "flash")   => ("200".into(),  Some("10000".into()),Some(120)),
        ("ambient_light", "hold")    => ("400".into(),  None,                Some(1500)),

        // --- uv ---
        ("uv_index", "clear_noon")  => ("1".into(), Some("9".into()), Some(3000)),
        ("uv_index", "cloud_cover") => ("8".into(), Some("2".into()), Some(3000)),
        ("uv_index", "hold")        => ("3".into(), None,             Some(1500)),

        // Generic increase/decrease/hold fallbacks.
        (_, "increase") => ("0".into(), Some("1".into()), Some(2000)),
        (_, "decrease") => ("1".into(), Some("0".into()), Some(2000)),
        (_, "hold")     => ("0".into(), None,             Some(1500)),
        _               => (String::new(), None, Some(1000)),
    }
}

/// Parse a "x,y,z"-style string into N floats, padding/truncating to `n`.
fn parse_csv_floats(s: &str, n: usize) -> Vec<f64> {
    let mut out: Vec<f64> = s
        .split(',')
        .map(|p| p.trim().parse::<f64>().unwrap_or(0.0))
        .collect();
    out.resize(n, 0.0);
    out
}

/// Format a list of floats with minimal trailing-zero noise.
fn format_csv_floats(vs: &[f64]) -> String {
    vs.iter()
        .map(|v| {
            // Avoid "5.000001"-style noise from f64 round trips.
            let s = format!("{:.6}", v);
            let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
            if s.is_empty() { "0".to_string() } else { s }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// IoT Designer page — create or edit a virtual IoT device.
#[component]
pub fn VirtualIotDesignerPage() -> impl IntoView {
    // Check if editing — pull ?edit=<device_id> from URL
    let edit_device_id = {
        let params = leptos_router::hooks::use_query_map();
        let p = params.get_untracked();
        p.get("edit")
    };
    let is_edit = edit_device_id.is_some();
    let edit_id = edit_device_id.clone().unwrap_or_default();

    // Form signals
    let (device_name, set_device_name) = signal(String::new());
    let (region, set_region) = signal("local".to_string());
    let (controller_type, set_controller_type) = signal("esp32-devkit".to_string());
    let (sim_rate_ms, set_sim_rate_ms) = signal(500i32);
    let (enabled_sensors, set_enabled_sensors) = signal(Vec::<String>::new());
    let (event_definitions, set_event_definitions) = signal(Vec::<VirtualIotEventDefinition>::new());
    let (selected_backend_id, set_selected_backend_id) = signal(String::new());
    let (attached_backend_id, set_attached_backend_id) = signal(String::new());
    let (saving, set_saving) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (loaded, set_loaded) = signal(!is_edit); // if creating, no load needed

    let compatible_backends = LocalResource::new(move || {
        let controller = controller_type.get();
        let selected_region = region.get();
        async move {
            let path = format!(
                "/devices/virtual-iot/backends?controller_type={}&region={}&available_only=false",
                controller, selected_region
            );
            api::get::<VirtualIotBackendListResponse>(&path)
                .await
                .unwrap_or_default()
        }
    });
    let shell_href = StoredValue::new(format!("/virtual-iot/shell?device={edit_id}"));

    // In edit mode, fetch existing config
    if is_edit {
        let edit_id_c = edit_id.clone();
        leptos::task::spawn_local(async move {
            // Fetch device info
            match api::get::<Value>(&format!("/devices/{edit_id_c}")).await {
                Ok(val) => {
                    if let Some(dev) = val.get("device") {
                        if let Some(n) = dev.get("name").and_then(|v| v.as_str()) {
                            set_device_name.set(n.to_string());
                        }
                        if let Some(r) = dev.get("region").and_then(|v| v.as_str()) {
                            set_region.set(r.to_string());
                        }
                    }
                }
                Err(e) => set_error.set(Some(format!("Failed to load device: {e}"))),
            }
            // Fetch virtual iot config
            match api::get::<VirtualIotConfigResponse>(&format!("/devices/{edit_id_c}/virtual-iot/config")).await {
                Ok(resp) => {
                    set_controller_type.set(resp.config.controller_type);
                    set_sim_rate_ms.set(resp.config.sim_rate_ms);
                    set_event_definitions.set(resp.config.event_definitions);
                    if let Some(backend_id) = resp.config.backend_id {
                        set_selected_backend_id.set(backend_id.clone());
                        set_attached_backend_id.set(backend_id);
                    }
                    if let Ok(sensors) = serde_json::from_value::<Vec<String>>(resp.config.enabled_sensors) {
                        set_enabled_sensors.set(sensors);
                    }
                    set_loaded.set(true);
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to load config: {e}")));
                    set_loaded.set(true);
                }
            }
        });
    }

    // Toggle a sensor in the enabled list
    let toggle_sensor = move |sensor_id: String| {
        set_enabled_sensors.update(|list| {
            if let Some(pos) = list.iter().position(|s| s == &sensor_id) {
                list.remove(pos);
            } else {
                list.push(sensor_id);
            }
        });
    };

    let add_event = move |_| {
        set_event_definitions.update(|events| {
            events.push(VirtualIotEventDefinition {
                id: format!("event-{}", js_sys::Date::now() as i64),
                name: format!("Scenario {}", events.len() + 1),
                description: String::from("Describe what this scenario represents."),
                steps: Vec::new(),
            });
        });
    };

    // Save handler
    let edit_id_save = edit_id.clone();
    let on_save = move |_: web_sys::MouseEvent| {
        let name = device_name.get_untracked();
        if name.trim().is_empty() {
            set_error.set(Some("Device name is required".into()));
            return;
        }
        set_saving.set(true);
        set_error.set(None);

        let name = name.clone();
        let ct = controller_type.get_untracked();
        let reg = region.get_untracked();
        let rate = sim_rate_ms.get_untracked();
        let sensors = enabled_sensors.get_untracked();
        let events = event_definitions.get_untracked();
        let backend_id = selected_backend_id.get_untracked();
        let current_backend_id = attached_backend_id.get_untracked();
        let is_edit = is_edit;
        let edit_id = edit_id_save.clone();

        leptos::task::spawn_local(async move {
            let result = if is_edit {
                api::patch::<UpdateVirtualIotConfigRequest, Value>(
                    &format!("/devices/{edit_id}/virtual-iot/config"),
                    &UpdateVirtualIotConfigRequest {
                        controller_type: Some(ct),
                        enabled_sensors: Some(sensors),
                        sim_rate_ms: Some(rate),
                        event_definitions: Some(events),
                    },
                ).await
            } else {
                api::post::<CreateVirtualIotRequest, Value>(
                    "/devices/virtual-iot",
                    &CreateVirtualIotRequest {
                        name,
                        controller_type: ct,
                        region: reg,
                        sim_rate_ms: rate,
                        enabled_sensors: sensors,
                        event_definitions: events,
                    },
                ).await
            };

            match result {
                Ok(resp) => {
                    let device_id = if is_edit {
                        edit_id.clone()
                    } else {
                        resp.get("device")
                            .and_then(|d| d.get("id"))
                            .and_then(|id| id.as_str())
                            .unwrap_or_default()
                            .to_string()
                    };

                    if !device_id.is_empty() {
                        if backend_id.is_empty() && !current_backend_id.is_empty() {
                            let _ = api::post::<Value, Value>(
                                &format!("/devices/{device_id}/virtual-iot/backend/detach"),
                                &serde_json::json!({}),
                            ).await;
                        } else if !backend_id.is_empty() && backend_id != current_backend_id {
                            let _ = api::post::<AttachVirtualIotBackendRequest, Value>(
                                &format!("/devices/{device_id}/virtual-iot/backend/attach"),
                                &AttachVirtualIotBackendRequest { backend_id: backend_id.clone() },
                            ).await;
                        }
                    }

                    // Navigate to devices page
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href("/devices");
                    }
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_saving.set(false);
                }
            }
        });
    };

    view! {
        <div class="iot-designer-page">
            <div class="iot-designer-header">
                <a href="/devices" class="btn btn-ghost btn-sm">"← Back to Devices"</a>
                <h1>{if is_edit { "Edit Virtual IoT" } else { "IoT Designer" }}</h1>
                <div class="iot-designer-actions">
                    <a href="/devices" class="btn btn-ghost btn-sm">"Cancel"</a>
                    <button
                        class="btn btn-primary btn-sm"
                        on:click=on_save
                        disabled=move || saving.get()
                    >
                        {move || if saving.get() { "Saving…" } else if is_edit { "Update" } else { "Save Controller" }}
                    </button>
                </div>
            </div>

            <Show when=move || error.get().is_some()>
                <div class="alert error" style="margin-bottom: 1rem;">
                    {move || error.get().unwrap_or_default()}
                </div>
            </Show>

            <Show when=move || !loaded.get()>
                <p class="empty-state">"Loading configuration…"</p>
            </Show>

            <Show when=move || loaded.get()>
                <div class="iot-designer-layout">
                    // ── Left: Board Preview ──
                    <div class="iot-designer-preview">
                        <div class="card">
                            <div class="card-header"><h2>"Board Preview"</h2></div>
                            <div class="board-preview-container">
                                <BoardPreview controller_type=controller_type/>
                            </div>
                            <div class="controller-specs">
                                {move || {
                                    let ct = controller_type.get();
                                    let profile = find_controller(&ct);
                                    let accessory_summary = controller_accessories(&ct)
                                        .into_iter()
                                        .map(|accessory| format!("{} ({})", accessory.label, accessory.interface))
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    view! {
                                        <div class="spec-row"><span class="spec-label">"Flash"</span><span class="spec-value">{profile.flash.as_str()}</span></div>
                                        <div class="spec-row"><span class="spec-label">"RAM"</span><span class="spec-value">{profile.ram.as_str()}</span></div>
                                        <div class="spec-row"><span class="spec-label">"Connectivity"</span><span class="spec-value">{profile.connectivity.as_str()}</span></div>
                                        {if accessory_summary.is_empty() {
                                            view! { <></> }.into_any()
                                        } else {
                                            view! {
                                                <div class="spec-row"><span class="spec-label">"Accessories"</span><span class="spec-value">{accessory_summary}</span></div>
                                            }.into_any()
                                        }}
                                    }
                                }}
                            </div>
                        </div>
                    </div>

                    // ── Right: Configuration Panel ──
                    <div class="iot-designer-config">
                        <div class="card">
                            <div class="card-header"><h2>"Configuration"</h2></div>
                            <div style="padding: 1rem; display: flex; flex-direction: column; gap: 1rem;">
                                // Device name
                                <div class="form-group">
                                    <label>"Device Name"</label>
                                    <input
                                        type="text"
                                        class="form-control"
                                        placeholder="my-esp32-sim"
                                        prop:value=move || device_name.get()
                                        on:input=move |ev| set_device_name.set(event_target_value(&ev))
                                        disabled=is_edit
                                    />
                                </div>
                                // Region
                                <div class="form-group">
                                    <label>"Region"</label>
                                    <select
                                        class="form-control"
                                        on:change=move |ev| set_region.set(event_target_value(&ev))
                                        disabled=is_edit
                                    >
                                        <option value="local" selected=move || region.get() == "local">"local"</option>
                                        <option value="default" selected=move || region.get() == "default">"default"</option>
                                        <option value="edge" selected=move || region.get() == "edge">"edge"</option>
                                    </select>
                                </div>
                                // Controller type
                                <div class="form-group">
                                    <label>"Controller"</label>
                                    <select
                                        class="form-control"
                                        on:change=move |ev| set_controller_type.set(event_target_value(&ev))
                                    >
                                        {controllers().iter().map(|c| {
                                            let id = c.id.clone();
                                            let name = c.name.clone();
                                            let id_c = id.clone();
                                            view! {
                                                <option value={id} selected=move || controller_type.get() == id_c>{name}</option>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </select>
                                </div>
                                // Sim rate
                                <div class="form-group">
                                    <label>"Simulation Rate (ms)"</label>
                                    <input
                                        type="number"
                                        class="form-control"
                                        min="100"
                                        max="5000"
                                        step="100"
                                        prop:value=move || sim_rate_ms.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                                set_sim_rate_ms.set(v.clamp(100, 5000));
                                            }
                                        }
                                    />
                                </div>

                                <div class="form-group">
                                    <label>"Compatible Backend"</label>
                                    <select
                                        class="form-control"
                                        on:change=move |ev| set_selected_backend_id.set(event_target_value(&ev))
                                    >
                                        <option value="">"No backend attached"</option>
                                        {move || compatible_backends.get().map(|data| {
                                            let current_attached_backend = attached_backend_id.get();
                                            data.backends.iter().cloned().map(|backend| {
                                                let option_value = backend.id.clone();
                                                let host_ip = backend
                                                    .shell_host
                                                    .clone()
                                                    .or_else(|| {
                                                        backend
                                                            .agent_base_url
                                                            .as_deref()
                                                            .and_then(extract_host_from_url)
                                                    })
                                                    .unwrap_or_else(|| "unknown-host".to_string());
                                                let agent_host_label = format_agent_host_label(
                                                    backend.agent_id.as_deref(),
                                                    &host_ip,
                                                );
                                                let label = format!(
                                                    "{} · {} · {} · {}{}",
                                                    backend.name,
                                                    backend.provider,
                                                    backend.region,
                                                    agent_host_label,
                                                    if backend.attached_device_id.is_some() && backend.id != current_attached_backend {
                                                        " · attached"
                                                    } else {
                                                        ""
                                                    }
                                                );
                                                let selected_value = option_value.clone();
                                                view! {
                                                    <option value={option_value.clone()} selected=move || selected_backend_id.get() == selected_value>{label}</option>
                                                }
                                            }).collect::<Vec<_>>()
                                        }).unwrap_or_default()}
                                    </select>
                                    <div style="display:flex; gap:0.75rem; margin-top:0.5rem; align-items:center;">
                                        <a href="/virtual-iot/backends" class="btn btn-ghost btn-sm">"Manage Backends"</a>
                                        <Show when=move || is_edit && !selected_backend_id.get().is_empty()>
                                            <a href=shell_href.get_value() class="btn btn-primary btn-sm">"Open VeerOS Shell"</a>
                                        </Show>
                                    </div>
                                </div>

                                // ── Sensor Categories ──
                                <div class="sensor-categories">
                                    <h3 style="margin: 0 0 0.5rem 0; font-size: 0.95rem; color: #c9d1d9;">"Sensors"</h3>
                                    {sensor_categories().iter().map(|cat| {
                                        let cat_name = cat.name.clone();
                                        view! {
                                            <div class="sensor-category">
                                                <div class="sensor-category-header">{cat_name}</div>
                                                <div class="sensor-checkbox-group">
                                                    {cat.sensors.iter().map(|sensor| {
                                                        let sid = sensor.id.clone();
                                                        let label = sensor.label.clone();
                                                        let sid_toggle = sid.clone();
                                                        let sid_check = sid.clone();
                                                        view! {
                                                            <label class="sensor-checkbox">
                                                                <input
                                                                    type="checkbox"
                                                                    prop:checked=move || enabled_sensors.get().contains(&sid_check)
                                                                    on:change=move |_| toggle_sensor(sid_toggle.clone())
                                                                />
                                                                <span>{label}</span>
                                                            </label>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>

                                // Sensor count
                                <div style="text-align: right; color: #8b949e; font-size: 0.82rem;">
                                    {move || format!("{} sensor(s) enabled", enabled_sensors.get().len())}
                                </div>

                                <div class="virtual-iot-events-panel">
                                    <div class="virtual-iot-events-header">
                                        <div>
                                            <h3>"Event Scenarios"</h3>
                                            <p>"Save reusable fire, flooding, collision, or custom sensor sequences directly in this Virtual IoT design."</p>
                                        </div>
                                        <button class="btn btn-secondary btn-sm" on:click=add_event>"Add Event"</button>
                                    </div>

                                    <Show
                                        when=move || !event_definitions.get().is_empty()
                                        fallback=move || view! { <div class="virtual-iot-events-empty">"No event scenarios defined yet."</div> }
                                    >
                                        <div class="virtual-iot-events-list">
                                            {move || event_definitions.get().into_iter().enumerate().map(|(event_index, event)| {
                                                let event_name = event.name.clone();
                                                let event_description = event.description.clone();
                                                let event_steps = event.steps.clone();
                                                view! {
                                                    <div class="virtual-iot-event-card">
                                                        <div class="virtual-iot-event-card-header">
                                                            <div class="virtual-iot-event-heading">
                                                                <input
                                                                    type="text"
                                                                    class="form-control"
                                                                    placeholder="Motor vehicle accident"
                                                                    prop:value=event_name
                                                                    on:input=move |ev| {
                                                                        let next = event_target_value(&ev);
                                                                        set_event_definitions.update(|events| {
                                                                            if let Some(item) = events.get_mut(event_index) {
                                                                                item.name = next.clone();
                                                                            }
                                                                        });
                                                                    }
                                                                />
                                                                <textarea
                                                                    class="form-control virtual-iot-event-description"
                                                                    placeholder="Describe the event and expected behavior"
                                                                    prop:value=event_description
                                                                    on:input=move |ev| {
                                                                        let next = event_target_value(&ev);
                                                                        set_event_definitions.update(|events| {
                                                                            if let Some(item) = events.get_mut(event_index) {
                                                                                item.description = next.clone();
                                                                            }
                                                                        });
                                                                    }
                                                                ></textarea>
                                                            </div>
                                                            <button
                                                                class="btn btn-ghost btn-sm"
                                                                on:click=move |_| {
                                                                    set_event_definitions.update(|events| {
                                                                        if event_index < events.len() {
                                                                            events.remove(event_index);
                                                                        }
                                                                    });
                                                                }
                                                            >"Remove"</button>
                                                        </div>

                                                        <div class="virtual-iot-event-steps">
                                                            {event_steps.into_iter().enumerate().map(|(step_index, step)| {
                                                                let selected_sensor = step.sensor.clone();
                                                                let selected_action = step.action.clone().unwrap_or_default();
                                                                let start_value = step.value.clone();
                                                                let end_value = step.end_value.clone().unwrap_or_default();
                                                                let duration = step.duration_ms.unwrap_or(0).to_string();
                                                                let action_options = virtual_iot_sensor_actions(&selected_sensor);
                                                                let spec = virtual_iot_value_spec(&selected_sensor);
                                                                let shape = spec.shape.clone();
                                                                let unit: &'static str = spec.unit;
                                                                let placeholder: &'static str = spec.placeholder;
                                                                let n_min = spec.min.map(|v| v.to_string());
                                                                let n_max = spec.max.map(|v| v.to_string());
                                                                let n_step_attr = spec.step.to_string();
                                                                let axes = spec.axes;

                                                                // Build a widget for either the "start" (is_end=false) or "end" (is_end=true) slot.
                                                                let make_field = |is_end: bool, current: String| -> leptos::prelude::AnyView {
                                                                    match shape.clone() {
                                                                        ValueShape::Scalar => {
                                                                            let n_min = n_min.clone();
                                                                            let n_max = n_max.clone();
                                                                            let n_step_attr = n_step_attr.clone();
                                                                            view! {
                                                                                <div class="virtual-iot-value-group">
                                                                                    <input
                                                                                        type="number"
                                                                                        class="form-control"
                                                                                        inputmode="decimal"
                                                                                        placeholder={placeholder}
                                                                                        min={n_min}
                                                                                        max={n_max}
                                                                                        step={n_step_attr}
                                                                                        prop:value=current
                                                                                        on:input=move |ev| {
                                                                                            let next = event_target_value(&ev);
                                                                                            set_event_definitions.update(|events| {
                                                                                                if let Some(item) = events.get_mut(event_index) {
                                                                                                    if let Some(step) = item.steps.get_mut(step_index) {
                                                                                                        if is_end {
                                                                                                            step.end_value = if next.trim().is_empty() { None } else { Some(next.clone()) };
                                                                                                        } else {
                                                                                                            step.value = next.clone();
                                                                                                        }
                                                                                                    }
                                                                                                }
                                                                                            });
                                                                                        }
                                                                                    />
                                                                                    {(!unit.is_empty()).then(|| view! { <span class="virtual-iot-value-unit">{unit}</span> })}
                                                                                </div>
                                                                            }.into_any()
                                                                        }
                                                                        ValueShape::Toggle => {
                                                                            view! {
                                                                                <div class="virtual-iot-value-group">
                                                                                    <select
                                                                                        class="form-control"
                                                                                        on:change=move |ev| {
                                                                                            let next = event_target_value(&ev);
                                                                                            set_event_definitions.update(|events| {
                                                                                                if let Some(item) = events.get_mut(event_index) {
                                                                                                    if let Some(step) = item.steps.get_mut(step_index) {
                                                                                                        if is_end {
                                                                                                            step.end_value = if next.is_empty() { None } else { Some(next.clone()) };
                                                                                                        } else {
                                                                                                            step.value = next.clone();
                                                                                                        }
                                                                                                    }
                                                                                                }
                                                                                            });
                                                                                        }
                                                                                    >
                                                                                        <option value="" selected=current.is_empty()>{if is_end { "—" } else { "0" }}</option>
                                                                                        <option value="0" selected={current == "0"}>"0 · LOW"</option>
                                                                                        <option value="1" selected={current == "1"}>"1 · HIGH"</option>
                                                                                    </select>
                                                                                </div>
                                                                            }.into_any()
                                                                        }
                                                                        ValueShape::Vec3 | ValueShape::LatLng => {
                                                                            let n_axes = axes.len();
                                                                            let parsed = parse_csv_floats(&current, n_axes);
                                                                            let fields = (0..n_axes).map(|i| {
                                                                                let label = axes[i];
                                                                                let val = if current.trim().is_empty() { String::new() } else { parsed[i].to_string() };
                                                                                let n_min = n_min.clone();
                                                                                let n_max = n_max.clone();
                                                                                let n_step_attr = n_step_attr.clone();
                                                                                view! {
                                                                                    <label class="virtual-iot-value-axis">
                                                                                        <span class="virtual-iot-value-axis-label">{label}</span>
                                                                                        <input
                                                                                            type="number"
                                                                                            class="form-control"
                                                                                            inputmode="decimal"
                                                                                            placeholder={placeholder}
                                                                                            min={n_min}
                                                                                            max={n_max}
                                                                                            step={n_step_attr}
                                                                                            prop:value=val
                                                                                            on:input=move |ev| {
                                                                                                let next = event_target_value(&ev);
                                                                                                let parsed_now = next.trim().parse::<f64>().unwrap_or(0.0);
                                                                                                set_event_definitions.update(|events| {
                                                                                                    if let Some(item) = events.get_mut(event_index) {
                                                                                                        if let Some(step) = item.steps.get_mut(step_index) {
                                                                                                            let source = if is_end {
                                                                                                                step.end_value.clone().unwrap_or_default()
                                                                                                            } else {
                                                                                                                step.value.clone()
                                                                                                            };
                                                                                                            let mut vs = parse_csv_floats(&source, n_axes);
                                                                                                            vs[i] = parsed_now;
                                                                                                            let joined = format_csv_floats(&vs);
                                                                                                            if is_end {
                                                                                                                step.end_value = Some(joined);
                                                                                                            } else {
                                                                                                                step.value = joined;
                                                                                                            }
                                                                                                        }
                                                                                                    }
                                                                                                });
                                                                                            }
                                                                                        />
                                                                                    </label>
                                                                                }
                                                                            }).collect::<Vec<_>>();
                                                                            view! {
                                                                                <div class="virtual-iot-value-group is-vector">
                                                                                    {fields}
                                                                                    {(!unit.is_empty()).then(|| view! { <span class="virtual-iot-value-unit">{unit}</span> })}
                                                                                </div>
                                                                            }.into_any()
                                                                        }
                                                                    }
                                                                };

                                                                let start_field = make_field(false, start_value);
                                                                let end_field = make_field(true, end_value);
                                                                view! {
                                                                    <div class="virtual-iot-event-step-row">
                                                                        <select
                                                                            class="form-control"
                                                                            on:change=move |ev| {
                                                                                let next = event_target_value(&ev);
                                                                                set_event_definitions.update(|events| {
                                                                                    if let Some(item) = events.get_mut(event_index) {
                                                                                        if let Some(step) = item.steps.get_mut(step_index) {
                                                                                            step.sensor = next.clone();
                                                                                            // Clear action when sensor changes — its domain may differ.
                                                                                            step.action = None;
                                                                                        }
                                                                                    }
                                                                                });
                                                                            }
                                                                        >
                                                                            <option value="">"Sensor"</option>
                                                                            {move || enabled_sensors.get().into_iter().map(|sensor| {
                                                                                let is_selected = sensor == selected_sensor;
                                                                                let sensor_label = sensor.clone();
                                                                                view! {
                                                                                    <option value={sensor.clone()} selected=is_selected>{sensor_label}</option>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </select>
                                                                        <select
                                                                            class="form-control"
                                                                            title="Physical action to simulate"
                                                                            on:change=move |ev| {
                                                                                let next = event_target_value(&ev);
                                                                                set_event_definitions.update(|events| {
                                                                                    if let Some(item) = events.get_mut(event_index) {
                                                                                        if let Some(step) = item.steps.get_mut(step_index) {
                                                                                            if next.is_empty() {
                                                                                                step.action = None;
                                                                                            } else {
                                                                                                let (v, ev_end, dur) = virtual_iot_action_preset(&step.sensor, &next);
                                                                                                step.action = Some(next.clone());
                                                                                                step.value = v;
                                                                                                step.end_value = ev_end;
                                                                                                step.duration_ms = dur;
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                });
                                                                            }
                                                                        >
                                                                            <option value="">"Action"</option>
                                                                            {action_options.into_iter().map(|(id, label)| {
                                                                                let is_selected = id == selected_action;
                                                                                view! {
                                                                                    <option value={id} selected=is_selected>{label}</option>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </select>
                                                                        <div class="virtual-iot-value-cell">
                                                                            <span class="virtual-iot-value-tag">"Start"</span>
                                                                            {start_field}
                                                                        </div>
                                                                        <div class="virtual-iot-value-cell">
                                                                            <span class="virtual-iot-value-tag">"End"</span>
                                                                            {end_field}
                                                                        </div>
                                                                        <div class="virtual-iot-value-cell virtual-iot-duration-cell">
                                                                            <span class="virtual-iot-value-tag">"Duration"</span>
                                                                            <div class="virtual-iot-value-group">
                                                                                <input
                                                                                    type="number"
                                                                                    class="form-control"
                                                                                    min="0"
                                                                                    step="100"
                                                                                    placeholder="0"
                                                                                    prop:value=duration
                                                                                    on:input=move |ev| {
                                                                                        let next = event_target_value(&ev).parse::<i32>().unwrap_or(0);
                                                                                        set_event_definitions.update(|events| {
                                                                                            if let Some(item) = events.get_mut(event_index) {
                                                                                                if let Some(step) = item.steps.get_mut(step_index) {
                                                                                                    step.duration_ms = if next > 0 { Some(next) } else { None };
                                                                                                }
                                                                                            }
                                                                                        });
                                                                                    }
                                                                                />
                                                                                <span class="virtual-iot-value-unit">"ms"</span>
                                                                            </div>
                                                                        </div>
                                                                        <button
                                                                            class="btn btn-ghost btn-sm"
                                                                            on:click=move |_| {
                                                                                set_event_definitions.update(|events| {
                                                                                    if let Some(item) = events.get_mut(event_index) {
                                                                                        if step_index < item.steps.len() {
                                                                                            item.steps.remove(step_index);
                                                                                        }
                                                                                    }
                                                                                });
                                                                            }
                                                                        >"Delete"</button>
                                                                    </div>
                                                                }
                                                            }).collect::<Vec<_>>()}
                                                        </div>

                                                        <div class="virtual-iot-event-footer">
                                                            <button
                                                                class="btn btn-secondary btn-sm"
                                                                on:click=move |_| {
                                                                    let default_sensor = enabled_sensors.get_untracked().first().cloned().unwrap_or_default();
                                                                    set_event_definitions.update(|events| {
                                                                        if let Some(item) = events.get_mut(event_index) {
                                                                            item.steps.push(VirtualIotEventStep {
                                                                                sensor: default_sensor.clone(),
                                                                                action: None,
                                                                                value: String::from("1"),
                                                                                end_value: None,
                                                                                duration_ms: Some(3000),
                                                                            });
                                                                        }
                                                                    });
                                                                }
                                                            >"Add Sensor Step"</button>
                                                            <span class="virtual-iot-event-footnote">"Set an end value and duration to simulate a changing reading over time."</span>
                                                        </div>
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </Show>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Inline SVG board preview that swaps based on controller type.
#[component]
fn BoardPreview(controller_type: ReadSignal<String>) -> impl IntoView {
    move || {
        let ct = controller_type.get();
        let profile = find_controller(&ct);
        let chip_label = profile.chip_label.as_str();
        let has_wifi = profile.has_wifi;
        let pin_count = profile.pin_count;
        let board_color = profile.board_color.as_str();
        let board_title = profile.name.as_str();
        view! {
            <svg viewBox="0 0 260 360" class="board-shell-svg" xmlns="http://www.w3.org/2000/svg">
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
                </defs>

                <rect x="10" y="10" width="240" height="340" rx="18" fill={board_color} stroke="#111826" stroke-width="2"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-grid)"/>
                <rect x="10" y="10" width="240" height="340" rx="18" fill="url(#pcb-sheen)"/>

                <circle cx="28" cy="28" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>
                <circle cx="232" cy="28" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>
                <circle cx="28" cy="332" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>
                <circle cx="232" cy="332" r="6" fill="#0c111a" stroke="#818c99" stroke-width="1.2"/>

                <text x="130" y="30" text-anchor="middle" fill="rgba(255,255,255,0.85)" font-size="8" font-family="sans-serif">{board_title}</text>

                {if let Some(layout_view) = render_catalog_board_layout(profile, chip_label) {
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
                }}

                {if has_wifi {
                    view! {
                        <g transform="translate(176, 44)">
                            <rect x="0" y="0" width="54" height="18" rx="4" fill="#0b315a" stroke="#2b6cb0" stroke-width="0.8"/>
                            <text x="27" y="12.5" text-anchor="middle" fill="#a5d8ff" font-size="7" font-family="sans-serif">"Wireless"</text>
                        </g>
                    }.into_any()
                } else {
                    view! { <g/> }.into_any()
                }}
            </svg>
        }
    }
}
