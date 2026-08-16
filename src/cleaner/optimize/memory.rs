use std::process::Command;

pub fn purge_inactive_memory(dry_run: bool) -> (bool, String) {
    if dry_run {
        return (true, "[DRY RUN] Would purge inactive disk memory caches (purge)".to_string());
    }

    // Call native macOS /usr/sbin/purge
    let res = Command::new("purge").output();
    match res {
        Ok(out) if out.status.success() => {
            (true, "Purged inactive memory and disk caches".to_string())
        }
        _ => (false, "Could not run memory purge (requires sudo or admin rights)".to_string()),
    }
}
