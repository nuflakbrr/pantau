use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

use crate::cleaner::cli::args::{CliArgs, ShellType};

pub fn generate_completion(shell_type: ShellType) {
    let mut cmd = CliArgs::command();
    let shell = match shell_type {
        ShellType::Bash => Shell::Bash,
        ShellType::Zsh => Shell::Zsh,
        ShellType::Fish => Shell::Fish,
    };

    generate(shell, &mut cmd, "pnt", &mut io::stdout());
}
