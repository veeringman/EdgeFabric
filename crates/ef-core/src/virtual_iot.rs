//! Types for Virtual IoT emulated controllers and sensor simulation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported virtual controller board profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControllerType {
    Esp32Devkit,
    ArduinoUno,
    RpiPico,
    Stm32Nucleo,
    GenericMcu,
}

impl ControllerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Esp32Devkit => "esp32-devkit",
            Self::ArduinoUno => "arduino-uno",
            Self::RpiPico => "rpi-pico",
            Self::Stm32Nucleo => "stm32-nucleo",
            Self::GenericMcu => "generic-mcu",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Esp32Devkit => "ESP32 DevKit V1",
            Self::ArduinoUno => "Arduino Uno R3",
            Self::RpiPico => "Raspberry Pi Pico",
            Self::Stm32Nucleo => "STM32 Nucleo-F4",
            Self::GenericMcu => "Generic MCU",
        }
    }

    pub fn flash_kb(&self) -> u32 {
        match self {
            Self::Esp32Devkit => 4096,
            Self::ArduinoUno => 32,
            Self::RpiPico => 2048,
            Self::Stm32Nucleo => 1024,
            Self::GenericMcu => 512,
        }
    }

    pub fn ram_kb(&self) -> u32 {
        match self {
            Self::Esp32Devkit => 520,
            Self::ArduinoUno => 2,
            Self::RpiPico => 264,
            Self::Stm32Nucleo => 192,
            Self::GenericMcu => 64,
        }
    }

    pub fn connectivity(&self) -> &'static str {
        match self {
            Self::Esp32Devkit => "WiFi + BLE",
            Self::ArduinoUno => "UART",
            Self::RpiPico => "UART + SPI",
            Self::Stm32Nucleo => "UART + SPI + I²C",
            Self::GenericMcu => "UART",
        }
    }
}

/// Available sensor kinds that can be enabled on a virtual IoT device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorKind {
    // Environmental
    Temperature,
    Humidity,
    Pressure,
    AirQualityCo2,
    AirQualityTvoc,
    // Motion
    AccelXyz,
    GyroXyz,
    Magnetometer,
    // Light & Proximity
    AmbientLight,
    UvIndex,
    Proximity,
    // Location
    Gps,
    // Power & Electrical
    BatteryVoltage,
    Current,
    GpioStates,
    AdcRaw,
    // Audio
    SoundLevel,
    // Connectivity
    WifiRssi,
    BleScan,
}

impl SensorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Temperature => "temperature",
            Self::Humidity => "humidity",
            Self::Pressure => "pressure",
            Self::AirQualityCo2 => "air_quality_co2",
            Self::AirQualityTvoc => "air_quality_tvoc",
            Self::AccelXyz => "accel_xyz",
            Self::GyroXyz => "gyro_xyz",
            Self::Magnetometer => "magnetometer",
            Self::AmbientLight => "ambient_light",
            Self::UvIndex => "uv_index",
            Self::Proximity => "proximity",
            Self::Gps => "gps",
            Self::BatteryVoltage => "battery_voltage",
            Self::Current => "current",
            Self::GpioStates => "gpio_states",
            Self::AdcRaw => "adc_raw",
            Self::SoundLevel => "sound_level",
            Self::WifiRssi => "wifi_rssi",
            Self::BleScan => "ble_scan",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Temperature => "Temperature",
            Self::Humidity => "Humidity",
            Self::Pressure => "Pressure",
            Self::AirQualityCo2 => "CO₂",
            Self::AirQualityTvoc => "TVOC",
            Self::AccelXyz => "Accelerometer XYZ",
            Self::GyroXyz => "Gyroscope XYZ",
            Self::Magnetometer => "Magnetometer",
            Self::AmbientLight => "Ambient Light",
            Self::UvIndex => "UV Index",
            Self::Proximity => "Proximity",
            Self::Gps => "GPS",
            Self::BatteryVoltage => "Battery Voltage",
            Self::Current => "Current",
            Self::GpioStates => "GPIO States",
            Self::AdcRaw => "ADC Raw",
            Self::SoundLevel => "Sound Level",
            Self::WifiRssi => "WiFi RSSI",
            Self::BleScan => "BLE Scan",
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            Self::Temperature | Self::Humidity | Self::Pressure
            | Self::AirQualityCo2 | Self::AirQualityTvoc => "Environmental",
            Self::AccelXyz | Self::GyroXyz | Self::Magnetometer => "Motion",
            Self::AmbientLight | Self::UvIndex | Self::Proximity => "Light & Proximity",
            Self::Gps => "Location",
            Self::BatteryVoltage | Self::Current | Self::GpioStates | Self::AdcRaw => "Power & Electrical",
            Self::SoundLevel => "Audio",
            Self::WifiRssi | Self::BleScan => "Connectivity",
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            Self::Temperature => "°C",
            Self::Humidity => "%",
            Self::Pressure => "hPa",
            Self::AirQualityCo2 => "ppm",
            Self::AirQualityTvoc => "ppb",
            Self::AccelXyz => "g",
            Self::GyroXyz => "°/s",
            Self::Magnetometer => "μT",
            Self::AmbientLight => "lux",
            Self::UvIndex => "",
            Self::Proximity => "mm",
            Self::Gps => "",
            Self::BatteryVoltage => "V",
            Self::Current => "mA",
            Self::GpioStates => "",
            Self::AdcRaw => "",
            Self::SoundLevel => "dB",
            Self::WifiRssi => "dBm",
            Self::BleScan => "",
        }
    }
}

/// Configuration for a virtual IoT device's emulator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualIotConfig {
    pub device_id: Uuid,
    pub controller_type: ControllerType,
    pub enabled_sensors: Vec<SensorKind>,
    pub sim_rate_ms: u32,
}

/// A single sensor reading value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SensorValue {
    Scalar { value: f64, unit: String },
    Vector3 { x: f64, y: f64, z: f64, unit: String },
    Gps { lat: f64, lng: f64, alt: f64, speed: f64 },
    GpioMap(std::collections::HashMap<String, u16>),
}

/// A complete frame of sensor readings streamed over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorFrame {
    #[serde(rename = "type")]
    pub msg_type: String, // always "sensor_frame"
    pub device_id: Uuid,
    pub ts: f64,
    pub readings: std::collections::HashMap<String, SensorValue>,
}

impl SensorFrame {
    pub fn new(device_id: Uuid) -> Self {
        Self {
            msg_type: "sensor_frame".to_string(),
            device_id,
            ts: 0.0,
            readings: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_type_serde_roundtrip() {
        let ct = ControllerType::Esp32Devkit;
        let json = serde_json::to_string(&ct).unwrap();
        assert_eq!(json, "\"esp32-devkit\"");
        let parsed: ControllerType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ct);
    }

    #[test]
    fn sensor_kind_serde_roundtrip() {
        let sk = SensorKind::Temperature;
        let json = serde_json::to_string(&sk).unwrap();
        assert_eq!(json, "\"temperature\"");
        let parsed: SensorKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, sk);
    }

    #[test]
    fn sensor_frame_serialization() {
        let mut frame = SensorFrame::new(Uuid::nil());
        frame.ts = 1712345678.123;
        frame.readings.insert("temperature".into(), SensorValue::Scalar {
            value: 24.7,
            unit: "°C".into(),
        });
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"sensor_frame\""));
        assert!(json.contains("\"temperature\""));
    }

    #[test]
    fn virtual_iot_config_serde() {
        let cfg = VirtualIotConfig {
            device_id: Uuid::nil(),
            controller_type: ControllerType::ArduinoUno,
            enabled_sensors: vec![SensorKind::Temperature, SensorKind::GpioStates],
            sim_rate_ms: 500,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: VirtualIotConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.controller_type, ControllerType::ArduinoUno);
        assert_eq!(parsed.enabled_sensors.len(), 2);
    }
}
