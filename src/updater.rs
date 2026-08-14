//! GitHub-Releases-based updater — not the Sparkle framework (integrating
//! that into a hand-rolled objc2 binary, not an Xcode project, means
//! vendoring the framework plus its XPC installer helper; a lot of
//! integration risk for what a plain HTTPS request already covers). Check
//! runs on demand (menu click), never automatically/periodically.
use serde::Deserialize;
use std::io::Read;
use std::sync::mpsc;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str = "https://api.github.com/repos/nuflakbrr/pantau/releases/latest";

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone)]
pub enum UpdateCheckResult {
    UpToDate,
    UpdateAvailable { version: String, download_url: String },
    Error(String),
}

/// Numeric dot-separated comparison (`"1.2.0" > "1.10.0"` would be wrong
/// under plain string comparison — this compares component-by-component
/// as integers instead). Non-numeric/missing components parse as 0.
fn parse_semver(v: &str) -> Vec<u32> {
    v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn is_newer(current: &str, latest: &str) -> bool {
    parse_semver(latest) > parse_semver(current)
}

fn fetch() -> UpdateCheckResult {
    let result = ureq::get(RELEASES_API)
        .set("User-Agent", "pantau-app-updater")
        .call()
        .map_err(|e| e.to_string())
        .and_then(|r| r.into_json::<ReleaseResponse>().map_err(|e| e.to_string()));
    let release = match result {
        Ok(r) => r,
        Err(e) => return UpdateCheckResult::Error(e),
    };
    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    if !is_newer(CURRENT_VERSION, &latest_version) {
        return UpdateCheckResult::UpToDate;
    }
    let Some(asset) = release.assets.iter().find(|a| a.name.ends_with(".zip")) else {
        return UpdateCheckResult::Error("Latest release has no .zip asset".to_string());
    };
    UpdateCheckResult::UpdateAvailable { version: latest_version, download_url: asset.browser_download_url.clone() }
}

/// Kicks off the version check on a background thread — a network round
/// trip on the main thread would freeze the menu bar for however long
/// GitHub takes to respond. Poll the returned receiver from a timer tick.
pub fn check_async() -> mpsc::Receiver<UpdateCheckResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(fetch());
    });
    rx
}

/// Downloads the release zip, swaps it into `/Applications/Pantau.app`,
/// and relaunches. Runs on a background thread (network + disk I/O) — the
/// caller is expected to call `NSApplication::terminate` on the main
/// thread once this returns `Ok`, handing off to the freshly-installed copy.
pub fn download_and_install(download_url: &str) -> Result<(), String> {
    let mut reader = ureq::get(download_url).call().map_err(|e| e.to_string())?.into_reader();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(|e| e.to_string())?;

    let tmp_dir = std::env::temp_dir().join(format!("pantau-update-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let zip_path = tmp_dir.join("update.zip");
    std::fs::write(&zip_path, &bytes).map_err(|e| e.to_string())?;

    // `ditto` (built into macOS) preserves the .app bundle's extended
    // attributes/resource forks correctly, unlike a plain `unzip`.
    let status = std::process::Command::new("ditto")
        .args(["-x", "-k"])
        .arg(&zip_path)
        .arg(&tmp_dir)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("ditto extraction failed".to_string());
    }

    let extracted_app = tmp_dir.join("Pantau.app");
    if !extracted_app.exists() {
        return Err("Pantau.app not found inside the downloaded archive".to_string());
    }

    let target = std::path::Path::new("/Applications/Pantau.app");
    if target.exists() {
        std::fs::remove_dir_all(target).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&extracted_app, target).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&tmp_dir);

    // `-n` forces a fresh instance — plain `open` on an already-running app
    // (this very process, mid-exit) just re-activates the old one instead
    // of launching the freshly-installed binary, so nothing survives once
    // the caller exits this process.
    std::process::Command::new("open").args(["-n"]).arg(target).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_version_detected() {
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.0", "1.0.0"));
        assert!(!is_newer("0.2.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
    }

    #[test]
    fn numeric_not_lexicographic_comparison() {
        // plain string comparison would say "1.10.0" < "1.2.0" — wrong
        assert!(is_newer("1.2.0", "1.10.0"));
    }

    #[test]
    fn v_prefix_stripped() {
        assert!(is_newer("0.1.0", "v0.2.0"));
    }
}
