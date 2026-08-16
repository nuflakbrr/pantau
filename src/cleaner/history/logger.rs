use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::cleaner::config::CleanerConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OperationRecord {
    pub timestamp: String,
    pub command: String,
    pub target: String,
    pub size_bytes: u64,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeletionRecord {
    pub timestamp: String,
    pub path: String,
    pub size_bytes: u64,
    pub moved_to_trash: bool,
}

pub struct HistoryLogger {
    operations_log: PathBuf,
    deletions_log: PathBuf,
}

impl HistoryLogger {
    pub fn new() -> Self {
        let config = CleanerConfig::new();
        let operations_log = config.log_dir.join("operations.log");
        let deletions_log = config.log_dir.join("deletions.log");

        Self {
            operations_log,
            deletions_log,
        }
    }

    pub fn log_operation(&self, command: &str, target: &str, size_bytes: u64, status: &str) {
        let record = OperationRecord {
            timestamp: Utc::now().to_rfc3339(),
            command: command.to_string(),
            target: target.to_string(),
            size_bytes,
            status: status.to_string(),
        };

        if let Ok(json) = serde_json::to_string(&record) {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.operations_log)
            {
                let _ = writeln!(file, "{}", json);
            }
        }
    }

    pub fn log_deletion(&self, path: &Path, size_bytes: u64, moved_to_trash: bool) {
        let record = DeletionRecord {
            timestamp: Utc::now().to_rfc3339(),
            path: path.to_string_lossy().to_string(),
            size_bytes,
            moved_to_trash,
        };

        if let Ok(json) = serde_json::to_string(&record) {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.deletions_log)
            {
                let _ = writeln!(file, "{}", json);
            }
        }
    }

    pub fn read_recent_operations(&self, limit: usize) -> Vec<OperationRecord> {
        let mut records = Vec::new();
        if let Ok(file) = fs::File::open(&self.operations_log) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(rec) = serde_json::from_str::<OperationRecord>(&line) {
                    records.push(rec);
                }
            }
        }
        records.reverse();
        records.truncate(limit);
        records
    }

    pub fn read_recent_deletions(&self, limit: usize) -> Vec<DeletionRecord> {
        let mut records = Vec::new();
        if let Ok(file) = fs::File::open(&self.deletions_log) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(rec) = serde_json::from_str::<DeletionRecord>(&line) {
                    records.push(rec);
                }
            }
        }
        records.reverse();
        records.truncate(limit);
        records
    }
}
