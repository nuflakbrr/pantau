use super::sys::{self, CpuTicks};
use super::{aggregate_group, GroupStats};

/// Static CPU info, read once — brand/core-count/cache never change at runtime.
#[derive(Debug, Clone, Default)]
pub struct CpuStaticInfo {
    pub brand: Option<String>,
    pub physical_cores: Option<u64>,
    pub logical_cores: Option<u64>,
    pub l1i_cache_bytes: Option<u64>,
    pub macos_version: Option<String>,
}

pub fn read_static_info() -> CpuStaticInfo {
    CpuStaticInfo {
        brand: sys::sysctl_string("machdep.cpu.brand_string"),
        physical_cores: sys::sysctl_u64("hw.physicalcpu"),
        logical_cores: sys::sysctl_u64("hw.logicalcpu"),
        l1i_cache_bytes: sys::sysctl_u64("hw.l1icachesize"),
        macos_version: sys::sysctl_string("kern.osproductversion"),
    }
}

/// `hw.cpufrequency` is Intel-only; Apple Silicon has no public API for it.
pub fn read_frequency_hz() -> Option<u64> {
    sys::sysctl_u64("hw.cpufrequency")
}

/// Tracks the previous tick snapshot so successive samples can compute
/// delta-based per-core/aggregate percentages instead of cumulative ratios.
#[derive(Debug, Default)]
pub struct CpuSampler {
    previous: Vec<CpuTicks>,
}

#[derive(Debug, Clone)]
pub struct CpuReading {
    pub per_core_percent: Vec<f64>,
    pub aggregate_percent: f64,
    pub group_stats: Option<GroupStats>,
    /// Fraction of wall-clock time since boot the system has been non-idle,
    /// analog of vitals-gnome's "Process Time" (uptime minus idle-equivalent).
    pub process_time_seconds: Option<f64>,
}

impl CpuSampler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sample(&mut self, uptime_seconds: Option<f64>) -> Option<CpuReading> {
        let current = sys::read_processor_cpu_load()?;

        let process_time_seconds = uptime_seconds.and_then(|uptime| {
            let total: u64 = current.iter().map(CpuTicks::total).sum();
            let idle: u64 = current.iter().map(|t| t.idle as u64).sum();
            if total == 0 {
                None
            } else {
                Some(uptime * (1.0 - idle as f64 / total as f64))
            }
        });

        let per_core_percent = if self.previous.len() == current.len() && !self.previous.is_empty() {
            current
                .iter()
                .zip(self.previous.iter())
                .map(|(now, prev)| {
                    let total_delta = now.total().saturating_sub(prev.total());
                    let busy_delta = now.busy().saturating_sub(prev.busy());
                    if total_delta == 0 {
                        0.0
                    } else {
                        busy_delta as f64 / total_delta as f64 * 100.0
                    }
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        self.previous = current;

        if per_core_percent.is_empty() {
            return Some(CpuReading {
                per_core_percent,
                aggregate_percent: 0.0,
                group_stats: None,
                process_time_seconds,
            });
        }

        let group_stats = aggregate_group(&per_core_percent);
        let aggregate_percent = group_stats.map(|s| s.avg).unwrap_or(0.0);

        Some(CpuReading {
            per_core_percent,
            aggregate_percent,
            group_stats,
            process_time_seconds,
        })
    }
}
