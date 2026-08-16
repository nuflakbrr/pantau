use std::fs;
use std::path::PathBuf;

use crate::cleaner::clean::calculate_dir_size;
use crate::cleaner::uninstall::scanner::InstalledApp;

#[derive(Debug, Clone)]
pub struct AppRemnant {
    pub location_type: &'static str,
    pub path: PathBuf,
    pub size_bytes: u64,
}

pub fn find_app_leftovers(app: &InstalledApp) -> Vec<AppRemnant> {
    let mut remnants = Vec::new();
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    let app_name_lower = app.name.to_lowercase();
    let bundle_id_lower = app.bundle_id.as_deref().map(|s| s.to_lowercase());

    let search_locations: &[(&str, PathBuf)] = &[
        ("Application Support", home.join("Library/Application Support")),
        ("Caches", home.join("Library/Caches")),
        ("Preferences", home.join("Library/Preferences")),
        ("Logs", home.join("Library/Logs")),
        ("WebKit Storage", home.join("Library/WebKit")),
        ("Containers", home.join("Library/Containers")),
        ("Group Containers", home.join("Library/Group Containers")),
        ("Saved App State", home.join("Library/Saved Application State")),
        ("HTTPStorages", home.join("Library/HTTPStorages")),
        ("LaunchAgents", home.join("Library/LaunchAgents")),
        ("LaunchDaemons", PathBuf::from("/Library/LaunchDaemons")),
        ("Privileged Helper Tools", PathBuf::from("/Library/PrivilegedHelperTools")),
    ];

    for &(loc_name, ref loc_dir) in search_locations {
        if !loc_dir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(loc_dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let file_name = entry_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();

                let is_match = file_name.contains(&app_name_lower)
                    || bundle_id_lower
                        .as_ref()
                        .map_or(false, |b| file_name.contains(b));

                if is_match {
                    let sz = calculate_dir_size(&entry_path);
                    remnants.push(AppRemnant {
                        location_type: loc_name,
                        path: entry_path,
                        size_bytes: sz,
                    });
                }
            }
        }
    }

    remnants
}
