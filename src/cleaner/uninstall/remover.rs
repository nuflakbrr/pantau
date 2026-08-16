use std::fs;

use crate::cleaner::history::HistoryLogger;
use crate::cleaner::safety::PathValidator;
use crate::cleaner::uninstall::leftover::AppRemnant;
use crate::cleaner::uninstall::scanner::InstalledApp;

pub fn uninstall_app(
    app: &InstalledApp,
    remnants: &[AppRemnant],
    dry_run: bool,
    validator: &PathValidator,
    logger: &HistoryLogger,
) -> (u64, Vec<String>) {
    let mut total_freed = 0u64;
    let mut logs = Vec::new();

    // 1. Uninstall App Bundle itself
    if validator.is_safe_to_delete(&app.path).is_ok() {
        total_freed += app.size_bytes;
        if dry_run {
            logs.push(format!("[DRY RUN] Would remove application: {}", app.path.display()));
        } else {
            let res = trash::delete(&app.path).or_else(|_| {
                if app.path.is_dir() {
                    fs::remove_dir_all(&app.path)
                } else {
                    fs::remove_file(&app.path)
                }
            });
            if res.is_ok() {
                logs.push(format!("✓ Removed application: {}", app.path.display()));
                logger.log_deletion(&app.path, app.size_bytes, true);
            } else {
                logs.push(format!("✗ Failed to remove application: {}", app.path.display()));
            }
        }
    }

    // 2. Remove remnants / leftovers
    for remnant in remnants {
        if validator.is_safe_to_delete(&remnant.path).is_ok() {
            total_freed += remnant.size_bytes;
            if dry_run {
                logs.push(format!(
                    "[DRY RUN] Would remove {} remnant: {}",
                    remnant.location_type,
                    remnant.path.display()
                ));
            } else {
                let res = trash::delete(&remnant.path).or_else(|_| {
                    if remnant.path.is_dir() {
                        fs::remove_dir_all(&remnant.path)
                    } else {
                        fs::remove_file(&remnant.path)
                    }
                });
                if res.is_ok() {
                    logs.push(format!(
                        "✓ Cleaned {} remnant: {}",
                        remnant.location_type,
                        remnant.path.display()
                    ));
                    logger.log_deletion(&remnant.path, remnant.size_bytes, true);
                }
            }
        }
    }

    logger.log_operation(
        "uninstall",
        &app.name,
        total_freed,
        if dry_run { "dry_run" } else { "success" },
    );

    (total_freed, logs)
}
