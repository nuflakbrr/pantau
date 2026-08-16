use std::fs;
use std::path::{Path, PathBuf};

use crate::cleaner::clean::calculate_dir_size;
use crate::cleaner::safety::is_protected_app;

#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub name: String,
    pub path: PathBuf,
    pub bundle_id: Option<String>,
    pub size_bytes: u64,
    pub is_protected: bool,
}

pub fn scan_installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    let app_roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        home.join("Applications"),
    ];

    for root in app_roots {
        if !root.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.extension().map_or(false, |ext| ext == "app") {
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    let name = file_name.trim_end_matches(".app").to_string();
                    let is_protected = is_protected_app(&name);
                    let bundle_id = read_bundle_id(&path);
                    let size_bytes = calculate_dir_size(&path);

                    apps.push(InstalledApp {
                        name,
                        path,
                        bundle_id,
                        size_bytes,
                        is_protected,
                    });
                }
            }
        }
    }

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

pub fn read_bundle_id(app_path: &Path) -> Option<String> {
    let info_plist = app_path.join("Contents/Info.plist");
    if !info_plist.exists() {
        return None;
    }

    if let Ok(content) = fs::read_to_string(&info_plist) {
        if let Some(idx) = content.find("CFBundleIdentifier") {
            let rest = &content[idx..];
            if let Some(s_idx) = rest.find("<string>") {
                let start = s_idx + 8;
                if let Some(e_idx) = rest[start..].find("</string>") {
                    let id = rest[start..start + e_idx].trim().to_string();
                    if !id.is_empty() {
                        return Some(id);
                    }
                }
            }
        }
    }

    None
}
