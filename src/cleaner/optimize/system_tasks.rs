use std::process::Command;

pub fn reset_quicklook_cache(dry_run: bool) -> (bool, String) {
    if dry_run {
        return (true, "[DRY RUN] Would reset QuickLook thumbnail cache".to_string());
    }
    let res = Command::new("qlmanage").args(["-r", "cache"]).output();
    if res.is_ok() {
        (true, "Reset QuickLook thumbnail generator cache".to_string())
    } else {
        (false, "Failed to reset QuickLook cache".to_string())
    }
}

pub fn reset_font_caches(dry_run: bool) -> (bool, String) {
    if dry_run {
        return (true, "[DRY RUN] Would reset Apple system font databases".to_string());
    }
    let res = Command::new("atsutil").args(["databases", "-remove"]).output();
    if res.is_ok() {
        (true, "Reset Apple font databases".to_string())
    } else {
        (false, "Failed to reset font databases".to_string())
    }
}

pub fn detach_idle_disk_images(dry_run: bool) -> (usize, Vec<String>) {
    let mut count = 0;
    let mut logs = Vec::new();

    let output = Command::new("hdiutil").arg("info").output();
    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.contains("/Volumes/") {
                if let Some(mount) = line.split_whitespace().last() {
                    if mount.starts_with("/Volumes/") && !mount.contains("Macintosh") {
                        if dry_run {
                            logs.push(format!("[DRY RUN] Would inspect disk image mount: {}", mount));
                        } else {
                            logs.push(format!("Detected disk image mount: {}", mount));
                        }
                        count += 1;
                    }
                }
            }
        }
    }

    (count, logs)
}
