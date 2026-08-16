pub mod app_caches;
pub mod browsers;
pub mod dev;
pub mod system;
pub mod user;

pub use app_caches::get_app_specific_caches;
pub use browsers::get_browser_cache_targets;
pub use dev::get_dev_cache_targets;
pub use system::{calculate_dir_size, get_system_clean_targets};
pub use user::get_user_clean_targets;

use std::fs;
use std::path::Path;

use crate::cleaner::config::CleanerConfig;
use crate::cleaner::history::HistoryLogger;
use crate::cleaner::safety::PathValidator;

#[derive(Debug, Default)]
pub struct CleanReport {
    pub total_bytes_freed: u64,
    pub categories_cleaned: Vec<CategoryResult>,
}

#[derive(Debug)]
pub struct CategoryResult {
    pub name: String,
    pub bytes_freed: u64,
    pub items_count: usize,
}

pub fn run_clean(dry_run: bool, debug: bool) -> CleanReport {
    let config = CleanerConfig::new();
    let whitelist = config.load_whitelist();
    let validator = PathValidator::new(whitelist.clone());
    let logger = HistoryLogger::new();

    let mut report = CleanReport::default();

    println!("\x1b[1;35mClean Your Mac\x1b[0m\n");
    println!("\x1b[2m⚙ Use --dry-run to preview, --whitelist to manage protected paths\x1b[0m");

    let is_root = unsafe { libc::geteuid() == 0 };
    if is_root {
        println!("\x1b[32m[✓ Admin access granted]\x1b[0m\n");
    } else {
        println!("\x1b[33m! Running without root (run with sudo for full system cache purge)\x1b[0m\n");
    }

    let metrics = crate::cleaner::status::collect_system_metrics();
    let free_space = metrics.disk_total_bytes.saturating_sub(metrics.disk_used_bytes);
    println!("\x1b[34m⚙\x1b[0m Apple Silicon | Free space: {}", format_bytes(free_space));
    println!("\x1b[32m✓\x1b[0m Whitelist: {} core patterns active\n", whitelist.len());

    if dry_run {
        println!("\x1b[33m[DRY RUN MODE - No files will be deleted]\x1b[0m\n");
    }

    // 1. System Section
    println!("\x1b[1;35m> System\x1b[0m");
    let sys_targets = system::get_system_clean_targets();
    let mut sys_found = false;
    for target in sys_targets {
        let mut bytes = 0u64;
        let mut count = 0usize;
        for path in &target.paths {
            if validator.is_safe_to_delete(path).is_ok() {
                let sz = calculate_dir_size(path);
                if sz > 0 {
                    bytes += sz;
                    count += 1;
                    if !dry_run {
                        let _ = clean_path_contents(path);
                        logger.log_deletion(path, sz, true);
                    }
                    if debug {
                        println!("  - {}: {} ({})", target.name, path.display(), format_bytes(sz));
                    }
                }
            }
        }
        if bytes > 0 {
            sys_found = true;
            report.total_bytes_freed += bytes;
            report.categories_cleaned.push(CategoryResult {
                name: target.name.to_string(),
                bytes_freed: bytes,
                items_count: count,
            });
            println!("  \x1b[32m✓\x1b[0m {} · {} items, \x1b[32m{}\x1b[0m", target.name, count, format_bytes(bytes));
        }
    }
    if !sys_found {
        println!("  \x1b[2m✓ System caches clean (0 items)\x1b[0m");
    }
    println!();

    // 2. User essentials
    println!("\x1b[1;35m> User essentials\x1b[0m");
    let user_targets = user::get_user_clean_targets();
    let mut user_found = false;
    for target in user_targets {
        let mut bytes = 0u64;
        let mut count = 0usize;
        for path in &target.paths {
            if validator.is_safe_to_delete(path).is_ok() {
                let sz = calculate_dir_size(path);
                if sz > 0 {
                    bytes += sz;
                    count += 1;
                    if !dry_run {
                        let _ = clean_path_contents(path);
                        logger.log_deletion(path, sz, true);
                    }
                    if debug {
                        println!("  - {}: {} ({})", target.name, path.display(), format_bytes(sz));
                    }
                }
            }
        }
        if bytes > 0 {
            user_found = true;
            report.total_bytes_freed += bytes;
            report.categories_cleaned.push(CategoryResult {
                name: target.name.to_string(),
                bytes_freed: bytes,
                items_count: count,
            });
            println!("  \x1b[32m✓\x1b[0m {} · {} items, \x1b[32m{}\x1b[0m", target.name, count, format_bytes(bytes));
        }
    }
    if !user_found {
        println!("  \x1b[2m✓ User essentials clean (0 items)\x1b[0m");
    }
    println!();

    // 3. App caches
    println!("\x1b[1;35m> App caches\x1b[0m");
    let app_targets = app_caches::get_app_specific_caches();
    let mut app_found = false;
    for target in app_targets {
        if validator.is_safe_to_delete(&target.path).is_ok() {
            let sz = calculate_dir_size(&target.path);
            if sz > 0 {
                app_found = true;
                if !dry_run {
                    let _ = clean_path_contents(&target.path);
                    logger.log_deletion(&target.path, sz, true);
                }
                report.total_bytes_freed += sz;
                report.categories_cleaned.push(CategoryResult {
                    name: target.app_name.to_string(),
                    bytes_freed: sz,
                    items_count: 1,
                });
                println!("  \x1b[32m✓\x1b[0m {} · 1 items, \x1b[32m{}\x1b[0m", target.app_name, format_bytes(sz));
                if debug {
                    println!("  - App: {} ({})", target.path.display(), format_bytes(sz));
                }
            }
        }
    }

    let browser_targets = browsers::get_browser_cache_targets();
    for target in browser_targets {
        if validator.is_safe_to_delete(&target.path).is_ok() {
            let sz = calculate_dir_size(&target.path);
            if sz > 0 {
                app_found = true;
                if !dry_run {
                    let _ = clean_path_contents(&target.path);
                    logger.log_deletion(&target.path, sz, true);
                }
                report.total_bytes_freed += sz;
                report.categories_cleaned.push(CategoryResult {
                    name: target.browser_name.to_string(),
                    bytes_freed: sz,
                    items_count: 1,
                });
                println!("  \x1b[32m✓\x1b[0m {} · 1 items, \x1b[32m{}\x1b[0m", target.browser_name, format_bytes(sz));
            }
        }
    }
    if !app_found {
        println!("  \x1b[2m✓ App caches clean (0 items)\x1b[0m");
    }
    println!();

    // 4. Developer caches
    println!("\x1b[1;35m> Developer caches\x1b[0m");
    let dev_targets = dev::get_dev_cache_targets();
    let mut dev_found = false;
    for target in dev_targets {
        if validator.is_safe_to_delete(&target.path).is_ok() {
            let sz = calculate_dir_size(&target.path);
            if sz > 0 {
                dev_found = true;
                if !dry_run {
                    let _ = clean_path_contents(&target.path);
                    logger.log_deletion(&target.path, sz, true);
                }
                report.total_bytes_freed += sz;
                report.categories_cleaned.push(CategoryResult {
                    name: target.name.to_string(),
                    bytes_freed: sz,
                    items_count: 1,
                });
                println!("  \x1b[32m✓\x1b[0m {} · 1 items, \x1b[32m{}\x1b[0m", target.name, format_bytes(sz));
                if debug {
                    println!("  - Dev: {} ({})", target.path.display(), format_bytes(sz));
                }
            }
        }
    }
    if !dev_found {
        println!("  \x1b[2m✓ Developer caches clean (0 items)\x1b[0m");
    }
    println!();

    // Log operation
    logger.log_operation(
        "clean",
        "system_and_user_caches",
        report.total_bytes_freed,
        if dry_run { "dry_run" } else { "success" },
    );

    println!("\x1b[1;32m════════════════════════════════════════════════════════════════════\x1b[0m");
    println!(
        "\x1b[1mTotal space {}: \x1b[1;32m{}\x1b[0m",
        if dry_run { "would be freed" } else { "freed" },
        format_bytes(report.total_bytes_freed)
    );
    println!("\x1b[1;32m════════════════════════════════════════════════════════════════════\x1b[0m\n");

    report
}

/// Recursively removes files/subfolders inside `dir` while keeping the directory itself intact.
fn clean_path_contents(dir: &Path) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    if dir.is_file() {
        return fs::remove_file(dir);
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
    }
    Ok(())
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
