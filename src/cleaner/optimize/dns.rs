use std::process::Command;

pub fn flush_dns_cache(dry_run: bool) -> (bool, String) {
    if dry_run {
        return (true, "[DRY RUN] Would flush DNS cache and restart mDNSResponder".to_string());
    }

    let flush_res = Command::new("dscacheutil").arg("-flushcache").output();
    let kill_res = Command::new("killall").args(["-HUP", "mDNSResponder"]).output();

    if flush_res.is_ok() && kill_res.is_ok() {
        (true, "Flushed DNS cache & refreshed mDNSResponder".to_string())
    } else {
        (false, "Failed to flush DNS cache".to_string())
    }
}
