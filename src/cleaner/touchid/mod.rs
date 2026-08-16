use std::fs;
use std::path::Path;
use std::process::Command;

const PAM_SUDO_LOCAL: &str = "/etc/pam.d/sudo_local";
const PAM_SUDO: &str = "/etc/pam.d/sudo";
const PAM_TID_LINE: &str = "auth       sufficient     pam_tid.so";

pub fn is_touchid_configured() -> bool {
    if Path::new(PAM_SUDO_LOCAL).exists() {
        if let Ok(content) = fs::read_to_string(PAM_SUDO_LOCAL) {
            if content.contains("pam_tid.so") {
                return true;
            }
        }
    }

    if Path::new(PAM_SUDO).exists() {
        if let Ok(content) = fs::read_to_string(PAM_SUDO) {
            if content.contains("pam_tid.so") {
                return true;
            }
        }
    }

    false
}

pub fn is_touchid_supported() -> bool {
    // 1. Check bioutil
    if let Ok(out) = Command::new("bioutil").arg("-r").output() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.contains("Touch ID") {
            return true;
        }
    }

    // 2. Apple Silicon is always supported
    if std::env::consts::ARCH == "aarch64" {
        return true;
    }

    false
}

pub fn enable_touchid_in_terminal() {
    let script = "tell application \"Terminal\"\n\
        activate\n\
        do script \"echo '# sudo_local: local customizations for sudo' | sudo tee /etc/pam.d/sudo_local > /dev/null && echo 'auth       sufficient     pam_tid.so' | sudo tee -a /etc/pam.d/sudo_local > /dev/null && sudo chmod 444 /etc/pam.d/sudo_local && sudo chown root:wheel /etc/pam.d/sudo_local && echo '' && echo '✓ Touch ID for sudo enabled successfully!'\"\n\
    end tell";

    let _ = Command::new("osascript").args(["-e", script]).status();
}

pub fn disable_touchid_in_terminal() {
    let script = "tell application \"Terminal\"\n\
        activate\n\
        do script \"sudo rm -f /etc/pam.d/sudo_local && echo '' && echo '✓ Touch ID for sudo disabled.'\"\n\
    end tell";

    let _ = Command::new("osascript").args(["-e", script]).status();
}

pub fn enable_touchid(dry_run: bool) -> (bool, String) {
    if is_touchid_configured() {
        return (true, "Touch ID is already enabled for sudo".to_string());
    }

    if dry_run {
        return (
            true,
            "[DRY RUN] Would write 'auth sufficient pam_tid.so' to /etc/pam.d/sudo_local"
                .to_string(),
        );
    }

    enable_touchid_in_terminal();
    (true, "Opened Terminal to configure Touch ID. Please enter your password in Terminal to complete setup.".to_string())
}

pub fn disable_touchid(dry_run: bool) -> (bool, String) {
    if !is_touchid_configured() {
        return (true, "Touch ID is not currently configured".to_string());
    }

    if dry_run {
        return (
            true,
            "[DRY RUN] Would remove pam_tid.so configuration from /etc/pam.d/sudo_local"
                .to_string(),
        );
    }

    disable_touchid_in_terminal();
    (true, "Opened Terminal to disable Touch ID for sudo.".to_string())
}

pub fn is_sudo_authenticated() -> bool {
    Command::new("sudo")
        .args(["-n", "true"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn request_admin_elevation(prompt_msg: &str) -> bool {
    if is_sudo_authenticated() {
        return true;
    }

    if is_touchid_configured() {
        // When Touch ID is enabled, osascript administrator privileges prompts native Touch ID dialog
        let script = format!(
            "do shell script \"sudo -v\" with prompt \"{}\" with administrator privileges",
            prompt_msg.replace('"', "\\\"")
        );
        return Command::new("osascript")
            .args(["-e", &script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    // When Touch ID is not enabled, prompt password dialog (Mole GUI mode: sudo.sh lines 108-121)
    let script = format!(
        "display dialog \"{}\" default answer \"\" with title \"Pantau Deep Scan\" with icon caution with hidden answer",
        prompt_msg.replace('"', "\\\"")
    );
    let output = Command::new("osascript")
        .args(["-e", &script, "-e", "text returned of result"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let pwd = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !pwd.is_empty() {
                let child = Command::new("sudo")
                    .args(["-S", "-p", "", "-v"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();

                if let Ok(mut c) = child {
                    if let Some(mut stdin) = c.stdin.take() {
                        use std::io::Write;
                        let _ = stdin.write_all(format!("{}\n", pwd).as_bytes());
                    }
                    return c.wait().map(|s| s.success()).unwrap_or(false);
                }
            }
        }
    }

    false
}

pub fn check_directory_access() -> Vec<(&'static str, bool)> {
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("/"));

    let check_paths = [
        ("User Caches", home.join("Library/Caches")),
        ("Application Support", home.join("Library/Application Support")),
        ("Containers & Logs", home.join("Library/Containers")),
    ];

    check_paths
        .into_iter()
        .map(|(label, p)| {
            let accessible = p.exists() && fs::read_dir(&p).is_ok();
            (label, accessible)
        })
        .collect()
}
