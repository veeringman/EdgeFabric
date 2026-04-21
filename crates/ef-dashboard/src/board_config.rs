use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct SensorDefinition {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SensorCategory {
    pub name: String,
    pub sensors: Vec<SensorDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SensorProfile {
    pub preview: String,
    pub module_profile: String,
    pub expected_range: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AccessoryProfile {
    pub label: String,
    pub interface: String,
    pub summary: String,
    pub accent: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WiringRule {
    pub pin: String,
    pub bus: String,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WiringFamily {
    pub id: String,
    pub rules: HashMap<String, WiringRule>,
    #[serde(default)]
    pub pin_anchors: HashMap<String, f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardLayoutBlock {
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub fill: String,
    pub stroke: String,
    #[serde(default)]
    pub text_fill: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HeaderLayout {
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub columns: usize,
    pub rows: usize,
    pub pitch_x: f32,
    pub pitch_y: f32,
    pub pin_width: f32,
    pub pin_height: f32,
    pub base_fill: String,
    pub base_stroke: String,
    pub pin_fill: String,
    #[serde(default)]
    pub text_fill: Option<String>,
    #[serde(default)]
    pub label_x: Option<f32>,
    #[serde(default)]
    pub label_y: Option<f32>,
    #[serde(default)]
    pub pin_corner_radius: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardIndicator {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub fill: String,
    pub stroke: String,
    pub label: String,
    #[serde(default)]
    pub text_fill: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AccessoryMount {
    pub accessory_id: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoardVisualLayout {
    pub module: BoardLayoutBlock,
    #[serde(default)]
    pub headers: Vec<HeaderLayout>,
    #[serde(default)]
    pub ports: Vec<BoardLayoutBlock>,
    #[serde(default)]
    pub power: Vec<BoardLayoutBlock>,
    #[serde(default)]
    pub indicators: Vec<BoardIndicator>,
    #[serde(default)]
    pub accessories: Vec<AccessoryMount>,
    pub footer_label: String,
    #[serde(default)]
    pub footer_text_fill: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerProfile {
    pub id: String,
    pub name: String,
    pub flash: String,
    pub ram: String,
    pub connectivity: String,
    pub visual_family: String,
    pub chip_label: String,
    pub has_wifi: bool,
    pub pin_count: usize,
    pub board_color: String,
    pub wiring_family: String,
    #[serde(default)]
    pub accessory_profiles: Vec<String>,
    #[serde(default)]
    pub visual_layout: Option<BoardVisualLayout>,
}

#[derive(Debug, Deserialize)]
struct BoardCatalog {
    sensor_categories: Vec<SensorCategory>,
    sensor_profiles: HashMap<String, SensorProfile>,
    #[serde(default)]
    accessory_profiles: HashMap<String, AccessoryProfile>,
    #[serde(default)]
    bus_wire_colors: HashMap<String, String>,
    wiring_families: Vec<WiringFamily>,
    controllers: Vec<ControllerProfile>,
}

#[derive(Clone, Debug)]
pub struct ResolvedWiringEntry {
    pub sensor_key: String,
    pub sensor_label: String,
    pub pin: String,
    pub bus: String,
    pub note: String,
}

static BOARD_CATALOG: OnceLock<BoardCatalog> = OnceLock::new();
static SENSOR_LABELS: OnceLock<HashMap<String, String>> = OnceLock::new();
static DEFAULT_SENSOR_PROFILE: OnceLock<SensorProfile> = OnceLock::new();

fn catalog() -> &'static BoardCatalog {
    BOARD_CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/board_catalog.json"))
            .expect("valid board catalog JSON")
    })
}

fn sensor_labels() -> &'static HashMap<String, String> {
    SENSOR_LABELS.get_or_init(|| {
        let mut labels = HashMap::new();
        for category in &catalog().sensor_categories {
            for sensor in &category.sensors {
                labels.insert(sensor.id.clone(), sensor.label.clone());
            }
        }
        labels
    })
}

fn default_sensor_profile() -> &'static SensorProfile {
    DEFAULT_SENSOR_PROFILE.get_or_init(|| SensorProfile {
        preview: String::from("--"),
        module_profile: String::from("Generic profile"),
        expected_range: String::from("Nominal range"),
    })
}

fn find_wiring_family(id: &str) -> Option<&'static WiringFamily> {
    catalog().wiring_families.iter().find(|family| family.id == id)
}

pub fn sensor_categories() -> &'static [SensorCategory] {
    &catalog().sensor_categories
}

pub fn controllers() -> &'static [ControllerProfile] {
    &catalog().controllers
}

pub fn find_controller(id: &str) -> &'static ControllerProfile {
    controllers()
        .iter()
        .find(|controller| controller.id == id)
        .unwrap_or_else(|| controllers().first().expect("board catalog contains controllers"))
}

pub fn sensor_label(id: &str) -> &'static str {
    sensor_labels().get(id).map(String::as_str).unwrap_or("Sensor")
}

pub fn controller_name(id: &str) -> &'static str {
    find_controller(id).name.as_str()
}

pub fn sensor_profile(id: &str) -> &'static SensorProfile {
    catalog()
        .sensor_profiles
        .get(id)
        .unwrap_or_else(default_sensor_profile)
}

pub fn accessory_profile(id: &str) -> Option<&'static AccessoryProfile> {
    catalog().accessory_profiles.get(id)
}

pub fn pin_anchor_for_family(family_id: &str, pin: &str) -> Option<f32> {
    find_wiring_family(family_id).and_then(|f| f.pin_anchors.get(pin).copied())
}

pub fn pin_anchors_for_family(family_id: &str) -> Vec<(String, f32)> {
    find_wiring_family(family_id)
        .map(|family| {
            let mut anchors = family
                .pin_anchors
                .iter()
                .map(|(pin, y)| (pin.clone(), *y))
                .collect::<Vec<_>>();
            anchors.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(std::cmp::Ordering::Equal));
            anchors
        })
        .unwrap_or_default()
}

pub fn bus_wire_color(pin: &str, bus: &str) -> Option<&'static str> {
    if pin == "INTERNAL" || pin == "N/A" {
        let c = catalog().bus_wire_colors.get("INTERNAL").map(String::as_str);
        return c;
    }
    for (key, color) in &catalog().bus_wire_colors {
        if bus.contains(key.as_str()) {
            return Some(color.as_str());
        }
    }
    None
}

pub fn controller_accessories(controller_type: &str) -> Vec<&'static AccessoryProfile> {
    find_controller(controller_type)
        .accessory_profiles
        .iter()
        .filter_map(|id| accessory_profile(id))
        .collect()
}

pub fn default_wiring(controller_type: &str, enabled_sensors: &[String]) -> Vec<ResolvedWiringEntry> {
    let controller = find_controller(controller_type);
    let controller_family = find_wiring_family(&controller.wiring_family);
    let generic_family = find_wiring_family("generic");

    enabled_sensors
        .iter()
        .map(|sensor_key| {
            let rule = controller_family
                .and_then(|family| family.rules.get(sensor_key))
                .or_else(|| generic_family.and_then(|family| family.rules.get(sensor_key)));

            match rule {
                Some(rule) => ResolvedWiringEntry {
                    sensor_key: sensor_key.clone(),
                    sensor_label: sensor_label(sensor_key).to_string(),
                    pin: rule.pin.clone(),
                    bus: rule.bus.clone(),
                    note: rule.note.clone(),
                },
                None => ResolvedWiringEntry {
                    sensor_key: sensor_key.clone(),
                    sensor_label: sensor_label(sensor_key).to_string(),
                    pin: String::from("PIN_1"),
                    bus: String::from("I2C"),
                    note: String::from("Default mapping"),
                },
            }
        })
        .collect()
}