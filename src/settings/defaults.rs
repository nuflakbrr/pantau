use super::{ColorBand, IconStyle, IpProvider, PanelPosition, Settings, SizeUnit, SpeedFormat, SpeedUnit, TempUnit};
use crate::values::{PERCENT_BANDS, TEMPERATURE_BANDS};

fn percent_bands() -> Vec<ColorBand> {
    PERCENT_BANDS.iter().map(ColorBand::from).collect()
}

fn temperature_bands() -> Vec<ColorBand> {
    TEMPERATURE_BANDS.iter().map(ColorBand::from).collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hot_sensors: Vec::new(),
            update_time: 1,
            position_in_panel: PanelPosition::Right,
            use_higher_precision: false,
            alphabetize: false,
            hide_zeros: false,
            show_temperature: true,
            show_voltage: true,
            show_fan: true,
            show_memory: true,
            show_processor: true,
            show_system: true,
            show_storage: true,
            show_network: true,
            show_battery: true,
            show_gpu: true,
            unit: TempUnit::Celsius,
            include_public_ip: false,
            network_public_ip_interval: 300,
            network_public_ip_show_flag: true,
            network_public_ip_provider: IpProvider::Ipify,
            network_speed_format: SpeedFormat::Bytes,
            network_speed_unit: SpeedUnit::Auto,
            storage_path: "/".to_string(),
            memory_measurement: SizeUnit::Binary,
            storage_measurement: SizeUnit::Decimal,
            battery_slot: 0,
            fixed_widths: true,
            hide_icons: false,
            menu_centered: false,
            monitor_cmd: "open -a \"Activity Monitor\"".to_string(),
            include_static_info: true,
            include_static_gpu_info: true,
            icon_style: IconStyle::SfSymbols,
            temperature_colors: temperature_bands(),
            fan_colors: percent_bands(),
            memory_colors: percent_bands(),
            processor_colors: percent_bands(),
            system_colors: percent_bands(),
            battery_colors: percent_bands(),
            gpu_colors: percent_bands(),
        }
    }
}

fn config_path() -> Option<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "pantau-app")?;
    Some(dirs.config_dir().join("config.toml"))
}

impl Settings {
    /// Falls back to `Settings::default()` on any error — missing file,
    /// unreadable, or corrupt TOML never crashes the app at startup.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&contents).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path().ok_or("could not resolve config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let contents = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, contents).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_roundtrip_through_toml() {
        let settings = Settings::default();
        let serialized = toml::to_string_pretty(&settings).expect("serialize");
        let deserialized: Settings = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(settings.update_time, deserialized.update_time);
        assert_eq!(settings.temperature_colors.len(), deserialized.temperature_colors.len());
    }

    #[test]
    fn corrupt_toml_falls_back_to_default_shape() {
        let result: Result<Settings, _> = toml::from_str("not valid toml {{{");
        assert!(result.is_err());
        let fallback = result.unwrap_or_default();
        assert_eq!(fallback.update_time, Settings::default().update_time);
    }
}
