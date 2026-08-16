use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "pantau",
    author = "Pantau Contributors",
    version = "1.1.0",
    about = "🐾 Deep clean, optimize, and analyze your Mac with 100% native Rust."
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable verbose / debug logging
    #[arg(long, global = true)]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Deep clean system and application caches
    Clean(CleanArgs),

    /// Smart uninstaller for applications and hidden remnants
    Uninstall(UninstallArgs),

    /// Optimize and maintenance system services and caches
    #[command(alias = "optimise")]
    Optimize(OptimizeArgs),

    /// Visual disk space analyzer
    #[command(alias = "analyse")]
    Analyze(AnalyzeArgs),

    /// Live system health dashboard
    Status(StatusArgs),

    /// Purge project build artifacts (node_modules, target, build)
    Purge(PurgeArgs),

    /// Find and remove large raw installer files (.dmg, .pkg, .zip)
    Installer(InstallerArgs),

    /// Configure Touch ID authentication for sudo
    Touchid(TouchIdArgs),

    /// Review recent operation and deletion logs
    History(HistoryArgs),

    /// Generate shell tab completion script
    Completion(CompletionArgs),
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    /// Preview cleanup without deleting files
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Show protected paths whitelist
    #[arg(long)]
    pub whitelist: bool,
}

#[derive(Args, Debug)]
pub struct UninstallArgs {
    /// Preview uninstallation without removing files
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct OptimizeArgs {
    /// Preview optimizations without applying changes
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Show protected optimizations whitelist
    #[arg(long)]
    pub whitelist: bool,
}

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    /// Target directory path to analyze (defaults to Home)
    pub path: Option<PathBuf>,

    /// Output analysis as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Output live system metrics as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct PurgeArgs {
    /// Preview purge without deleting files
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Edit / show custom scan directory paths
    #[arg(long)]
    pub paths: bool,
}

#[derive(Args, Debug)]
pub struct InstallerArgs {
    /// Preview removal without deleting files
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct TouchIdArgs {
    #[command(subcommand)]
    pub action: Option<TouchIdAction>,

    /// Preview Touch ID configuration without applying
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Subcommand, Debug)]
pub enum TouchIdAction {
    /// Enable Touch ID for sudo
    Enable,
    /// Disable Touch ID for sudo
    Disable,
    /// Show current Touch ID status
    Status,
}

#[derive(Args, Debug)]
pub struct HistoryArgs {
    /// Output history logs as JSON
    #[arg(long)]
    pub json: bool,

    /// Limit the number of recent entries
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct CompletionArgs {
    /// Target shell (bash, zsh, fish)
    #[arg(value_enum)]
    pub shell: ShellType,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
}
