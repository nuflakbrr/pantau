pub mod database;
pub mod dns;
pub mod launch_services;
pub mod memory;
pub mod system_tasks;

use crate::cleaner::history::HistoryLogger;

#[derive(Debug, Default)]
pub struct OptimizeSummary {
    pub applied: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub logs: Vec<String>,
}

pub fn run_optimize(dry_run: bool, debug: bool) -> OptimizeSummary {
    let logger = HistoryLogger::new();
    let mut summary = OptimizeSummary::default();

    println!(
        "\n⚡ {}Running system optimization & maintenance routines...",
        if dry_run { "[DRY RUN] " } else { "" }
    );

    // 1. Flush DNS
    let (dns_ok, dns_msg) = dns::flush_dns_cache(dry_run);
    if dns_ok {
        println!("  ✓ {}", dns_msg);
        summary.applied += 1;
        summary.logs.push(dns_msg);
    } else {
        println!("  - {}", dns_msg);
        summary.skipped += 1;
    }

    // 2. Memory Purge
    let (mem_ok, mem_msg) = memory::purge_inactive_memory(dry_run);
    if mem_ok {
        println!("  ✓ {}", mem_msg);
        summary.applied += 1;
        summary.logs.push(mem_msg);
    } else {
        if debug {
            println!("  - {}", mem_msg);
        }
        summary.unchanged += 1;
    }

    // 3. Launch Services Rebuild
    let (ls_ok, ls_msg) = launch_services::rebuild_launch_services(dry_run);
    if ls_ok {
        println!("  ✓ {}", ls_msg);
        summary.applied += 1;
        summary.logs.push(ls_msg);
    } else {
        if debug {
            println!("  - {}", ls_msg);
        }
        summary.skipped += 1;
    }

    // 4. QuickLook Cache Reset
    let (ql_ok, ql_msg) = system_tasks::reset_quicklook_cache(dry_run);
    if ql_ok {
        println!("  ✓ {}", ql_msg);
        summary.applied += 1;
        summary.logs.push(ql_msg);
    }

    // 5. Font Cache Reset
    let (font_ok, font_msg) = system_tasks::reset_font_caches(dry_run);
    if font_ok {
        println!("  ✓ {}", font_msg);
        summary.applied += 1;
        summary.logs.push(font_msg);
    }

    // 6. SQLite Databases Vacuum
    let (db_count, db_logs) = database::optimize_sqlite_databases(dry_run);
    if db_count > 0 {
        println!("  ✓ Optimized {} SQLite databases", db_count);
        summary.applied += db_count;
        summary.logs.extend(db_logs);
    }

    logger.log_operation(
        "optimize",
        "system_maintenance",
        0,
        if dry_run { "dry_run" } else { "success" },
    );

    println!("════════════════════════════════════════════════════════════════════");
    println!(
        "Optimization Complete: {} applied | {} unchanged | {} skipped",
        summary.applied, summary.unchanged, summary.skipped
    );
    println!("════════════════════════════════════════════════════════════════════\n");

    summary
}
