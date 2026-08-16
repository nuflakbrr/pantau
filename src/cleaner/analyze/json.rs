use serde::{Deserialize, Serialize};

use crate::cleaner::analyze::heap::LargeFileEntry;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiskAnalysisResult {
    pub path: String,
    pub total_size_bytes: u64,
    pub total_files: usize,
    pub entries: Vec<DirEntrySummary>,
    pub large_files: Vec<LargeFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntrySummary {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub percent: f32,
    pub is_dir: bool,
}
