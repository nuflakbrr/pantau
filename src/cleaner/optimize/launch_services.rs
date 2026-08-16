use std::path::PathBuf;
use std::process::Command;

pub fn rebuild_launch_services(dry_run: bool) -> (bool, String) {
    if dry_run {
        return (
            true,
            "[DRY RUN] Would rebuild LaunchServices registration database".to_string(),
        );
    }

    let lsregister_path = PathBuf::from(
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
    );

    if !lsregister_path.exists() {
        return (false, "lsregister tool not found".to_string());
    }

    let res = Command::new(&lsregister_path)
        .args(["-kill", "-r", "-domain", "local", "-domain", "system", "-domain", "user"])
        .output();

    match res {
        Ok(out) if out.status.success() => (true, "Rebuilt LaunchServices database".to_string()),
        _ => (false, "Failed to rebuild LaunchServices database".to_string()),
    }
}
