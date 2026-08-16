use clap::Parser;
use std::path::PathBuf;

use crate::cleaner::analyze::analyze_path;
use crate::cleaner::clean::{format_bytes, run_clean};
use crate::cleaner::cli::args::{CliArgs, Commands, TouchIdAction};
use crate::cleaner::cli::completion::generate_completion;
use crate::cleaner::config::CleanerConfig;
use crate::cleaner::history::HistoryLogger;
use crate::cleaner::installer::{remove_selected_installers, scan_installer_files};
use crate::cleaner::optimize::run_optimize;
use crate::cleaner::purge::{purge_selected_artifacts, scan_project_artifacts};
use crate::cleaner::safety::PathValidator;
use crate::cleaner::status::collect_system_metrics;
use crate::cleaner::touchid::{disable_touchid, enable_touchid, is_touchid_configured, is_touchid_supported};
use crate::cleaner::tui::{
    run_interactive_analyzer, run_interactive_main_menu, run_interactive_selector,
    run_interactive_status_dashboard, run_interactive_whitelist_manager, MainMenuAction,
    SelectableItem,
};
use crate::cleaner::uninstall::{find_app_leftovers, scan_installed_apps, uninstall_app};

pub fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();
    let debug = args.debug;
    let config = CleanerConfig::new();
    let whitelist = config.load_whitelist();
    let validator = PathValidator::new(whitelist.clone());
    let logger = HistoryLogger::new();

    match args.command {
        Some(Commands::Clean(clean_args)) => {
            if clean_args.whitelist {
                run_interactive_whitelist_manager()?;
                return Ok(());
            }
            run_clean(clean_args.dry_run, debug);
        }
        Some(Commands::Uninstall(un_args)) => {
            let apps = scan_installed_apps();
            let items: Vec<SelectableItem> = apps
                .iter()
                .enumerate()
                .map(|(idx, app)| SelectableItem {
                    id: idx,
                    title: app.name.clone(),
                    detail: if app.is_protected { "Protected" } else { "Installed App" }.to_string(),
                    size_formatted: format_bytes(app.size_bytes),
                    is_selected: false,
                    is_disabled: app.is_protected,
                })
                .collect();

            if let Some(selected_ids) = run_interactive_selector("Select Apps to Uninstall", items)? {
                for id in selected_ids {
                    if let Some(app) = apps.get(id) {
                        let remnants = find_app_leftovers(app);
                        let (freed, logs) = uninstall_app(app, &remnants, un_args.dry_run, &validator, &logger);
                        for l in logs {
                            println!("{}", l);
                        }
                        println!("Freed: {}\n", format_bytes(freed));
                    }
                }
            }
        }
        Some(Commands::Optimize(opt_args)) => {
            if opt_args.whitelist {
                run_interactive_whitelist_manager()?;
                return Ok(());
            }
            run_optimize(opt_args.dry_run, debug);
        }
        Some(Commands::Analyze(an_args)) => {
            let target = an_args.path.unwrap_or_else(|| {
                directories::BaseDirs::new()
                    .map(|b| b.home_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("/"))
            });

            if an_args.json {
                let res = analyze_path(&target);
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                run_interactive_analyzer(&target)?;
            }
        }
        Some(Commands::Status(st_args)) => {
            if st_args.json {
                let snap = collect_system_metrics();
                println!("{}", serde_json::to_string_pretty(&snap)?);
            } else {
                run_interactive_status_dashboard()?;
            }
        }
        Some(Commands::Purge(pur_args)) => {
            let scan_roots = config.load_purge_paths();
            if pur_args.paths {
                println!("Configured Project Scan Paths:");
                for p in &scan_roots {
                    println!("  - {}", p.display());
                }
                return Ok(());
            }

            println!("Scanning project directories for build artifacts...");
            let artifacts = scan_project_artifacts(&scan_roots);
            let items: Vec<SelectableItem> = artifacts
                .iter()
                .enumerate()
                .map(|(idx, art)| SelectableItem {
                    id: idx,
                    title: format!("{} ({})", art.project_name, art.artifact_type),
                    detail: art.path.to_string_lossy().to_string(),
                    size_formatted: format_bytes(art.size_bytes),
                    is_selected: !art.is_recent,
                    is_disabled: false,
                })
                .collect();

            if let Some(selected_ids) = run_interactive_selector("Select Project Artifacts to Purge", items)? {
                let selected_artifacts: Vec<_> = selected_ids.into_iter().filter_map(|id| artifacts.get(id).cloned()).collect();
                let (freed, logs) = purge_selected_artifacts(&selected_artifacts, pur_args.dry_run, &validator, &logger);
                for l in logs {
                    println!("{}", l);
                }
                println!("Space freed: {}\n", format_bytes(freed));
            }
        }
        Some(Commands::Installer(inst_args)) => {
            println!("Scanning for raw installer files...");
            let installers = scan_installer_files();
            let items: Vec<SelectableItem> = installers
                .iter()
                .enumerate()
                .map(|(idx, inst)| SelectableItem {
                    id: idx,
                    title: inst.file_name.clone(),
                    detail: inst.source_category.to_string(),
                    size_formatted: format_bytes(inst.size_bytes),
                    is_selected: true,
                    is_disabled: false,
                })
                .collect();

            if let Some(selected_ids) = run_interactive_selector("Select Installers to Remove", items)? {
                let selected: Vec<_> = selected_ids.into_iter().filter_map(|id| installers.get(id).cloned()).collect();
                let (freed, logs) = remove_selected_installers(&selected, inst_args.dry_run, &validator, &logger);
                for l in logs {
                    println!("{}", l);
                }
                println!("Space freed: {}\n", format_bytes(freed));
            }
        }
        Some(Commands::Touchid(tid_args)) => match tid_args.action {
            Some(TouchIdAction::Enable) => {
                let (_, msg) = enable_touchid(tid_args.dry_run);
                println!("{}", msg);
            }
            Some(TouchIdAction::Disable) => {
                let (_, msg) = disable_touchid(tid_args.dry_run);
                println!("{}", msg);
            }
            Some(TouchIdAction::Status) | None => {
                let configured = is_touchid_configured();
                let supported = is_touchid_supported();
                println!("Touch ID supported on this Mac: {}", if supported { "Yes" } else { "No" });
                println!("Touch ID enabled for sudo:      {}", if configured { "Yes" } else { "No" });
            }
        },
        Some(Commands::History(hist_args)) => {
            let ops = logger.read_recent_operations(hist_args.limit);
            if hist_args.json {
                println!("{}", serde_json::to_string_pretty(&ops)?);
            } else {
                println!("📜 Recent Pantau Cleaner Operations:");
                for op in ops {
                    println!(
                        "  [{}] {:<12} {:<30} {:<10} ({})",
                        op.timestamp,
                        op.command,
                        op.target,
                        format_bytes(op.size_bytes),
                        op.status
                    );
                }
            }
        }
        Some(Commands::Completion(comp_args)) => {
            generate_completion(comp_args.shell);
        }
        None => {
            // Interactive Main Menu TUI
            loop {
                match run_interactive_main_menu()? {
                    MainMenuAction::Clean => {
                        run_clean(false, debug);
                        println!("Press Enter to continue...");
                        let mut line = String::new();
                        let _ = std::io::stdin().read_line(&mut line);
                    }
                    MainMenuAction::Uninstall => {
                        let apps = scan_installed_apps();
                        let items: Vec<SelectableItem> = apps
                            .iter()
                            .enumerate()
                            .map(|(idx, app)| SelectableItem {
                                id: idx,
                                title: app.name.clone(),
                                detail: if app.is_protected { "Protected" } else { "Installed App" }.to_string(),
                                size_formatted: format_bytes(app.size_bytes),
                                is_selected: false,
                                is_disabled: app.is_protected,
                            })
                            .collect();

                        if let Some(selected_ids) = run_interactive_selector("Select Apps to Uninstall", items)? {
                            for id in selected_ids {
                                if let Some(app) = apps.get(id) {
                                    let remnants = find_app_leftovers(app);
                                    let (freed, logs) = uninstall_app(app, &remnants, false, &validator, &logger);
                                    for l in logs {
                                        println!("{}", l);
                                    }
                                    println!("Freed: {}\n", format_bytes(freed));
                                }
                            }
                            println!("Press Enter to continue...");
                            let mut line = String::new();
                            let _ = std::io::stdin().read_line(&mut line);
                        }
                    }
                    MainMenuAction::Optimize => {
                        run_optimize(false, debug);
                        println!("Press Enter to continue...");
                        let mut line = String::new();
                        let _ = std::io::stdin().read_line(&mut line);
                    }
                    MainMenuAction::Analyze => {
                        let home = directories::BaseDirs::new()
                            .map(|b| b.home_dir().to_path_buf())
                            .unwrap_or_else(|| PathBuf::from("/"));
                        run_interactive_analyzer(&home)?;
                    }
                    MainMenuAction::Status => {
                        run_interactive_status_dashboard()?;
                    }
                    MainMenuAction::Purge => {
                        let scan_roots = config.load_purge_paths();
                        let artifacts = scan_project_artifacts(&scan_roots);
                        let items: Vec<SelectableItem> = artifacts
                            .iter()
                            .enumerate()
                            .map(|(idx, art)| SelectableItem {
                                id: idx,
                                title: format!("{} ({})", art.project_name, art.artifact_type),
                                detail: art.path.to_string_lossy().to_string(),
                                size_formatted: format_bytes(art.size_bytes),
                                is_selected: !art.is_recent,
                                is_disabled: false,
                            })
                            .collect();

                        if let Some(selected_ids) = run_interactive_selector("Select Project Artifacts to Purge", items)? {
                            let selected_artifacts: Vec<_> = selected_ids.into_iter().filter_map(|id| artifacts.get(id).cloned()).collect();
                            let (freed, logs) = purge_selected_artifacts(&selected_artifacts, false, &validator, &logger);
                            for l in logs {
                                println!("{}", l);
                            }
                            println!("Space freed: {}\n", format_bytes(freed));
                            println!("Press Enter to continue...");
                            let mut line = String::new();
                            let _ = std::io::stdin().read_line(&mut line);
                        }
                    }
                    MainMenuAction::Installer => {
                        let installers = scan_installer_files();
                        let items: Vec<SelectableItem> = installers
                            .iter()
                            .enumerate()
                            .map(|(idx, inst)| SelectableItem {
                                id: idx,
                                title: inst.file_name.clone(),
                                detail: inst.source_category.to_string(),
                                size_formatted: format_bytes(inst.size_bytes),
                                is_selected: true,
                                is_disabled: false,
                            })
                            .collect();

                        if let Some(selected_ids) = run_interactive_selector("Select Installers to Remove", items)? {
                            let selected: Vec<_> = selected_ids.into_iter().filter_map(|id| installers.get(id).cloned()).collect();
                            let (freed, logs) = remove_selected_installers(&selected, false, &validator, &logger);
                            for l in logs {
                                println!("{}", l);
                            }
                            println!("Space freed: {}\n", format_bytes(freed));
                            println!("Press Enter to continue...");
                            let mut line = String::new();
                            let _ = std::io::stdin().read_line(&mut line);
                        }
                    }
                    MainMenuAction::TouchID => {
                        let configured = is_touchid_configured();
                        if configured {
                            println!("Touch ID is currently ENABLED.");
                            print!("Disable Touch ID for sudo? (y/N): ");
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                            let mut input = String::new();
                            let _ = std::io::stdin().read_line(&mut input);
                            if input.trim().eq_ignore_ascii_case("y") {
                                let (_, msg) = disable_touchid(false);
                                println!("{}", msg);
                            }
                        } else {
                            println!("Touch ID is currently DISABLED.");
                            print!("Enable Touch ID for sudo? (Y/n): ");
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                            let mut input = String::new();
                            let _ = std::io::stdin().read_line(&mut input);
                            if !input.trim().eq_ignore_ascii_case("n") {
                                let (_, msg) = enable_touchid(false);
                                println!("{}", msg);
                            }
                        }
                        println!("Press Enter to continue...");
                        let mut line = String::new();
                        let _ = std::io::stdin().read_line(&mut line);
                    }
                    MainMenuAction::History => {
                        let ops = logger.read_recent_operations(30);
                        println!("📜 Recent Pantau Cleaner Operations:");
                        for op in ops {
                            println!(
                                "  [{}] {:<12} {:<30} {:<10} ({})",
                                op.timestamp,
                                op.command,
                                op.target,
                                format_bytes(op.size_bytes),
                                op.status
                            );
                        }
                        println!("Press Enter to continue...");
                        let mut line = String::new();
                        let _ = std::io::stdin().read_line(&mut line);
                    }
                    MainMenuAction::Quit => break,
                }
            }
        }
    }

    Ok(())
}
