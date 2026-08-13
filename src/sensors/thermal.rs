//! Temp/Fan/Voltage over the SMC bridge (`smc.rs`). Key discovery happens
//! once at startup (`ThermalProbe::discover`) and is reused every poll tick
//! — walking the SMC's ~2000-entry flat key table every 5 seconds would be
//! wasteful and pointless since key *existence* doesn't change at runtime.
//!
//! Key sets are hand-verified on real Apple Silicon hardware during
//! development (see `smc.rs` test comments), not guessed from
//! documentation alone:
//! - CPU temp: Intel `TC0P`/`TC0D` tried first, else the Apple Silicon
//!   `Tp0*` P-core diode cluster (found by scanning for `T`-prefixed
//!   `flt`/`sp78` keys at startup).
//! - Fan: `FNum` gives the count; each fan `i` has `F{i}Ac`/`F{i}Mn`/`F{i}Mx`.
//! - Voltage: Intel `VC0C` tried first, else the Apple Silicon `VC1*-VC3*`
//!   core-voltage-rail cluster (same `V`-prefix scan).
use super::smc::SmcConnection;
use super::{aggregate_group, GroupStats};

const INTEL_CPU_TEMP_KEYS: [&str; 2] = ["TC0P", "TC0D"];
const INTEL_GPU_TEMP_KEY: &str = "TG0P";
const INTEL_VOLTAGE_KEY: &str = "VC0C";

#[derive(Debug, Clone, Default)]
pub struct FanKeys {
    pub index: u32,
    pub actual_key: String,
    pub min_key: String,
    pub max_key: String,
}

/// Discovered once at startup — the set of SMC keys this specific Mac
/// actually exposes. Reused every tick instead of re-scanning.
#[derive(Debug, Clone, Default)]
pub struct ThermalProbe {
    pub cpu_temp_keys: Vec<String>,
    pub gpu_temp_key: Option<String>,
    pub fans: Vec<FanKeys>,
    pub voltage_keys: Vec<String>,
}

impl ThermalProbe {
    pub fn discover(conn: &SmcConnection) -> Self {
        let mut cpu_temp_keys: Vec<String> = INTEL_CPU_TEMP_KEYS
            .iter()
            .filter(|k| conn.key_exists(k))
            .map(|k| k.to_string())
            .collect();

        let gpu_temp_key = conn.key_exists(INTEL_GPU_TEMP_KEY).then(|| INTEL_GPU_TEMP_KEY.to_string());

        let mut voltage_keys: Vec<String> =
            conn.key_exists(INTEL_VOLTAGE_KEY).then(|| INTEL_VOLTAGE_KEY.to_string()).into_iter().collect();

        // Fanless Macs report FNum = 0; graceful degradation, not an error.
        let fan_count = conn.read_key("FNum").map(|v| v as u32).unwrap_or(0);
        let fans: Vec<FanKeys> = (0..fan_count)
            .map(|i| FanKeys {
                index: i,
                actual_key: format!("F{i}Ac"),
                min_key: format!("F{i}Mn"),
                max_key: format!("F{i}Mx"),
            })
            .filter(|f| conn.key_exists(&f.actual_key))
            .collect();

        let need_cpu_scan = cpu_temp_keys.is_empty();
        let need_voltage_scan = voltage_keys.is_empty();
        if need_cpu_scan || need_voltage_scan {
            if let Some(count) = conn.read_key("#KEY") {
                for i in 0..(count as u32) {
                    let Some(name) = conn.key_name_at_index(i) else { continue };
                    let Some(ty) = conn.key_type_of(&name) else { continue };
                    if ty != "flt" && ty != "sp78" {
                        continue;
                    }
                    if need_cpu_scan && name.starts_with("Tp0") {
                        cpu_temp_keys.push(name.clone());
                    }
                    if need_voltage_scan && name.starts_with("VC") && name.len() == 4 {
                        voltage_keys.push(name);
                    }
                }
            }
        }

        Self { cpu_temp_keys, gpu_temp_key, fans, voltage_keys }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FanReading {
    pub index: u32,
    pub actual_rpm: Option<f64>,
    pub min_rpm: Option<f64>,
    pub max_rpm: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct ThermalReading {
    pub cpu_temp_stats: Option<GroupStats>,
    pub gpu_temp: Option<f64>,
    pub fans: Vec<FanReading>,
    pub voltage_avg: Option<f64>,
}

pub fn read(conn: &SmcConnection, probe: &ThermalProbe) -> ThermalReading {
    let cpu_temps: Vec<f64> = probe.cpu_temp_keys.iter().filter_map(|k| conn.read_key(k)).collect();
    let cpu_temp_stats = aggregate_group(&cpu_temps);

    let gpu_temp = probe.gpu_temp_key.as_ref().and_then(|k| conn.read_key(k));

    let fans: Vec<FanReading> = probe
        .fans
        .iter()
        .map(|f| FanReading {
            index: f.index,
            actual_rpm: conn.read_key(&f.actual_key),
            min_rpm: conn.read_key(&f.min_key),
            max_rpm: conn.read_key(&f.max_key),
        })
        .collect();

    let voltages: Vec<f64> = probe.voltage_keys.iter().filter_map(|k| conn.read_key(k)).collect();
    let voltage_avg = aggregate_group(&voltages).map(|s| s.avg);

    ThermalReading { cpu_temp_stats, gpu_temp, fans, voltage_avg }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn discovers_and_reads_real_thermal_data() {
        let conn = SmcConnection::open().expect("AppleSMC should be openable");
        let probe = ThermalProbe::discover(&conn);
        println!("probe: {probe:?}");
        assert!(!probe.cpu_temp_keys.is_empty(), "should discover at least one CPU temp key");

        let reading = read(&conn, &probe);
        println!("reading: {reading:?}");
        let stats = reading.cpu_temp_stats.expect("cpu_temp_stats should be populated");
        assert!(stats.avg > 0.0 && stats.avg < 130.0, "implausible avg CPU temp: {}", stats.avg);
        assert!(stats.min <= stats.avg && stats.avg <= stats.max);
    }
}
