pub mod defaults;

use crate::values::ColorThreshold;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PanelPosition {
    FarLeft,
    Left,
    Center,
    Right,
    FarRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IpProvider {
    CoreCoding,
    MyIp,
    Ipify,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SpeedFormat {
    Bits,
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SpeedUnit {
    Auto,
    FixedK,
    FixedM,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SizeUnit {
    Binary,
    Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IconStyle {
    SfSymbols,
}

/// `ColorThreshold` lives in `values.rs` (pure formatting logic, no serde
/// dependency there by design) — this wrapper is the serializable mirror
/// used only for settings persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorBand {
    pub threshold: f64,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<&ColorThreshold> for ColorBand {
    fn from(c: &ColorThreshold) -> Self {
        Self { threshold: c.threshold, r: c.rgba.0, g: c.rgba.1, b: c.rgba.2, a: c.rgba.3 }
    }
}

impl From<&ColorBand> for ColorThreshold {
    fn from(b: &ColorBand) -> Self {
        ColorThreshold { threshold: b.threshold, rgba: (b.r, b.g, b.b, b.a) }
    }
}

/// 1:1 with the gschema key set from the original GNOME extension (see
/// plan section A) — compile-time exhaustive, not a dynamic key/value store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hot_sensors: Vec<String>,
    pub update_time: u32,
    pub position_in_panel: PanelPosition,
    pub use_higher_precision: bool,
    pub hide_zeros: bool,
    pub show_temperature: bool,
    pub show_voltage: bool,
    pub show_fan: bool,
    pub show_memory: bool,
    pub show_processor: bool,
    pub show_system: bool,
    pub show_storage: bool,
    pub show_network: bool,
    pub show_battery: bool,
    pub show_gpu: bool,
    pub unit: TempUnit,
    pub include_public_ip: bool,
    pub network_public_ip_interval: u32,
    pub network_public_ip_show_flag: bool,
    pub network_public_ip_provider: IpProvider,
    pub network_speed_format: SpeedFormat,
    pub network_speed_unit: SpeedUnit,
    pub storage_path: String,
    pub memory_measurement: SizeUnit,
    pub storage_measurement: SizeUnit,
    pub battery_slot: u32,
    pub fixed_widths: bool,
    /// No-op on macOS — AppKit auto-attaches `NSMenu` to the click point,
    /// there's no separate "centered" mode like GNOME Shell's panel menu.
    /// Kept for settings-struct compatibility, shown as "not applicable on
    /// macOS" in the prefs UI rather than hidden silently.
    pub menu_centered: bool,
    pub monitor_cmd: String,
    pub include_static_info: bool,
    pub include_static_gpu_info: bool,
    pub icon_style: IconStyle,
    pub temperature_colors: Vec<ColorBand>,
    pub fan_colors: Vec<ColorBand>,
    pub memory_colors: Vec<ColorBand>,
    pub processor_colors: Vec<ColorBand>,
    pub system_colors: Vec<ColorBand>,
    pub battery_colors: Vec<ColorBand>,
    pub gpu_colors: Vec<ColorBand>,
}
