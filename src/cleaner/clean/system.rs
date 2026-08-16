use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

pub struct CleanTarget {
    pub name: &'static str,
    pub description: &'static str,
    pub paths: Vec<PathBuf>,
}

pub fn get_system_clean_targets() -> Vec<CleanTarget> {
    let mut targets = Vec::new();

    // 1. Diagnostic Reports & Crash Dumps
    let diag_paths = vec![
        PathBuf::from("/Library/Logs/DiagnosticReports"),
        dirs_home_path("Library/Logs/DiagnosticReports"),
        dirs_home_path("Library/Logs/CrashReporter"),
    ];
    targets.push(CleanTarget {
        name: "Diagnostic & Crash Reports",
        description: "Application crash logs and diagnostic dumps",
        paths: diag_paths.into_iter().filter(|p| p.exists()).collect(),
    });

    // 2. System and User ASL / Unified Logs
    let log_paths = vec![
        dirs_home_path("Library/Logs"),
        PathBuf::from("/Library/Logs"),
    ];
    targets.push(CleanTarget {
        name: "System & User Logs",
        description: "Old log files and system activity records",
        paths: log_paths.into_iter().filter(|p| p.exists()).collect(),
    });

    targets
}

pub fn clean_local_time_machine_snapshots(dry_run: bool) -> (u64, Vec<String>) {
    let mut messages = Vec::new();
    let output = Command::new("tmutil")
        .arg("listlocalsnapshots")
        .arg("/")
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if let Some(date_part) = line.split('.').last() {
                let snapshot = date_part.trim();
                if !snapshot.is_empty() {
                    if dry_run {
                        messages.push(format!("[DRY RUN] Would delete TM snapshot: {}", snapshot));
                    } else {
                        let res = Command::new("tmutil")
                            .args(["deletelocalsnapshots", snapshot])
                            .output();
                        if res.is_ok() {
                            messages.push(format!("Deleted TM snapshot: {}", snapshot));
                        }
                    }
                }
            }
        }
    }

    (0, messages)
}

fn dirs_home_path(sub: &str) -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(sub))
        .unwrap_or_else(|| PathBuf::from("/").join(sub))
}

pub fn calculate_dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}
