use std::process::Command;

/// Launches a sub-command inside the user's terminal emulator executing `pnt`.
pub fn launch_in_terminal(subcommand: &str) {
    let sub = subcommand.trim();
    let cmd = if sub == "clean" || sub == "clean --dry-run" {
        format!("sudo pnt {}", sub)
    } else if sub.is_empty() {
        "pnt".to_string()
    } else {
        format!("pnt {}", sub)
    };

    // Ensure PATH includes ~/.local/bin where pnt symlink lives, and run pnt
    let shell_command = format!(
        "export PATH=\"$HOME/.local/bin:$PATH\"; {}",
        cmd
    );

    let script = format!(
        "tell application \"Terminal\"\n\
            activate\n\
            do script \"{}\"\n\
        end tell",
        shell_command.replace('\"', "\\\"")
    );

    let _ = Command::new("osascript").args(["-e", &script]).spawn();
}
