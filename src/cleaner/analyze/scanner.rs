use rayon::prelude::*;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::cleaner::analyze::heap::LargeFileHeap;
use crate::cleaner::analyze::json::{DirEntrySummary, DiskAnalysisResult};
use crate::cleaner::clean::calculate_dir_size;

pub fn analyze_path(target_path: &Path) -> DiskAnalysisResult {
    let mut entries_raw = Vec::new();
    let mut large_heap = LargeFileHeap::new(20);
    let mut total_files = 0usize;

    if let Ok(entries) = fs::read_dir(target_path) {
        for entry in entries.flatten() {
            entries_raw.push(entry.path());
        }
    }

    // Parallel size computation for top-level entries
    let mut summaries: Vec<DirEntrySummary> = entries_raw
        .par_iter()
        .map(|path| {
            let is_dir = path.is_dir();
            let size = calculate_dir_size(path);
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            DirEntrySummary {
                name,
                path: path.to_string_lossy().to_string(),
                size_bytes: size,
                percent: 0.0,
                is_dir,
            }
        })
        .collect();

    // Scan for large files inside target
    for entry in WalkDir::new(target_path)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            total_files += 1;
            if let Ok(meta) = entry.metadata() {
                let len = meta.len();
                if len > 50 * 1024 * 1024 {
                    // > 50MB
                    large_heap.push(entry.path().to_path_buf(), len);
                }
            }
        }
    }

    let total_size: u64 = summaries.iter().map(|s| s.size_bytes).sum();
    if total_size > 0 {
        for s in &mut summaries {
            s.percent = (s.size_bytes as f64 / total_size as f64 * 100.0) as f32;
        }
    }

    summaries.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    DiskAnalysisResult {
        path: target_path.to_string_lossy().to_string(),
        total_size_bytes: total_size,
        total_files,
        entries: summaries,
        large_files: large_heap.into_sorted_vec(),
    }
}
