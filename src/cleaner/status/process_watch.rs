use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
}

pub fn get_top_processes(limit: usize) -> Vec<ProcessItem> {
    let mut list = Vec::new();
    let output = Command::new("ps")
        .args(["-eo", "pid,%cpu,rss,comm", "-r"])
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines().skip(1).take(limit) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let pid: u32 = parts[0].parse().unwrap_or(0);
                let cpu: f32 = parts[1].parse().unwrap_or(0.0);
                let rss_kb: u64 = parts[2].parse().unwrap_or(0);
                let comm = parts[3..].join(" ");
                let name = std::path::Path::new(&comm)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                list.push(ProcessItem {
                    pid,
                    name,
                    cpu_percent: cpu,
                    mem_bytes: rss_kb * 1024,
                });
            }
        }
    }

    list
}
