use serde::{Deserialize, Serialize};

use crate::cleaner::status::health::calculate_health_score;
use crate::cleaner::status::process_watch::{get_top_processes, ProcessItem};
use crate::sensors;

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemMetricsSnapshot {
    pub hostname: String,
    pub os_version: String,
    pub health_score: u8,
    pub cpu_total_pct: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub memory_used_pct: f64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_used_pct: f64,
    pub top_processes: Vec<ProcessItem>,
}

pub fn collect_system_metrics() -> SystemMetricsSnapshot {
    let cpu_info = sensors::cpu::read_static_info();
    let hostname = sensors::sys::sysctl_string("kern.hostname").unwrap_or_else(|| "Mac".into());
    let os_version = cpu_info.macos_version.unwrap_or_else(|| "macOS".into());

    let (load1, _, _) = sensors::system::read_load_average();
    let cpu_total_pct = load1.map(|l| (l * 10.0).clamp(0.0, 100.0)).unwrap_or(15.0);

    let mem_reading = sensors::memory::read_memory();
    let mem_used = mem_reading.allocated_bytes.unwrap_or(0);
    let mem_total = mem_reading.physical_bytes.unwrap_or(0);
    let mem_pct = mem_reading.usage_percent.unwrap_or(0.0);

    let disk_reading = sensors::disk::read_usage("/");
    let disk_used = disk_reading.used_bytes.unwrap_or(0);
    let disk_total = disk_reading.total_bytes.unwrap_or(0);
    let disk_pct = disk_reading.used_percent.unwrap_or(0.0);

    let top_processes = get_top_processes(10);
    let health_score = calculate_health_score(cpu_total_pct, mem_pct, disk_pct, None);

    SystemMetricsSnapshot {
        hostname,
        os_version,
        health_score,
        cpu_total_pct,
        memory_used_bytes: mem_used,
        memory_total_bytes: mem_total,
        memory_used_pct: mem_pct,
        disk_used_bytes: disk_used,
        disk_total_bytes: disk_total,
        disk_used_pct: disk_pct,
        top_processes,
    }
}
