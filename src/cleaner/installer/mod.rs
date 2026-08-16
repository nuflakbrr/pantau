pub mod scanner;

pub use scanner::{scan_installer_files, InstallerFile};

use std::fs;

use crate::cleaner::clean::format_bytes;
use crate::cleaner::history::HistoryLogger;
use crate::cleaner::safety::PathValidator;

pub fn remove_selected_installers(
    installers: &[InstallerFile],
    dry_run: bool,
    validator: &PathValidator,
    logger: &HistoryLogger,
) -> (u64, Vec<String>) {
    let mut total_freed = 0u64;
    let mut logs = Vec::new();

    for inst in installers {
        if validator.is_safe_to_delete(&inst.path).is_ok() {
            total_freed += inst.size_bytes;
            if dry_run {
                logs.push(format!(
                    "[DRY RUN] Would remove installer file: {}",
                    inst.path.display()
                ));
            } else {
                let res = trash::delete(&inst.path).or_else(|_| fs::remove_file(&inst.path));
                if res.is_ok() {
                    logs.push(format!(
                        "✓ Removed installer: {} ({})",
                        inst.file_name,
                        format_bytes(inst.size_bytes)
                    ));
                    logger.log_deletion(&inst.path, inst.size_bytes, true);
                }
            }
        }
    }

    logger.log_operation(
        "installer",
        "raw_installers",
        total_freed,
        if dry_run { "dry_run" } else { "success" },
    );

    (total_freed, logs)
}
