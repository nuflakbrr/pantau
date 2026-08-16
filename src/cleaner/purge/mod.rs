pub mod scanner;

pub use scanner::{scan_project_artifacts, ProjectArtifact};

use std::fs;

use crate::cleaner::clean::format_bytes;
use crate::cleaner::history::HistoryLogger;
use crate::cleaner::safety::PathValidator;

pub fn purge_selected_artifacts(
    artifacts: &[ProjectArtifact],
    dry_run: bool,
    validator: &PathValidator,
    logger: &HistoryLogger,
) -> (u64, Vec<String>) {
    let mut total_freed = 0u64;
    let mut logs = Vec::new();

    for art in artifacts {
        if validator.is_safe_to_delete(&art.path).is_ok() {
            total_freed += art.size_bytes;
            if dry_run {
                logs.push(format!(
                    "[DRY RUN] Would purge {} ({}): {}",
                    art.project_name,
                    art.artifact_type,
                    art.path.display()
                ));
            } else {
                let res = trash::delete(&art.path).or_else(|_| fs::remove_dir_all(&art.path));
                if res.is_ok() {
                    logs.push(format!(
                        "✓ Purged {} ({}) -> {}",
                        art.project_name,
                        art.artifact_type,
                        format_bytes(art.size_bytes)
                    ));
                    logger.log_deletion(&art.path, art.size_bytes, true);
                }
            }
        }
    }

    logger.log_operation(
        "purge",
        "project_artifacts",
        total_freed,
        if dry_run { "dry_run" } else { "success" },
    );

    (total_freed, logs)
}
