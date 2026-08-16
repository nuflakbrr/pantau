use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::cleaner::uninstall::scan_installed_apps;

pub struct AppCacheItem {
    pub app_name: String,
    pub path: PathBuf,
}

pub fn get_app_specific_caches() -> Vec<AppCacheItem> {
    let mut items = Vec::new();
    let mut seen_paths = HashSet::new();

    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    // 1. Curated known presets
    let cache_specs: &[(&str, &str)] = &[
        // Streaming / Media
        ("Spotify", "Library/Caches/com.spotify.client"),
        ("Spotify Storage", "Library/Application Support/Spotify/PersistentCache"),
        // Chat / Collaboration
        ("Slack", "Library/Caches/com.tinyspeck.slackmacgap"),
        ("Slack Logs", "Library/Application Support/Slack/logs"),
        ("Discord", "Library/Caches/com.hnc.Discord"),
        ("Discord GPUCache", "Library/Application Support/discord/GPUCache"),
        ("Telegram", "Library/Caches/ru.keepcoder.Telegram"),
        ("WhatsApp", "Library/Caches/net.whatsapp.WhatsApp"),
        ("Zoom", "Library/Caches/us.zoom.xos"),
        ("Microsoft Teams", "Library/Caches/com.microsoft.teams2"),
        // Development / Editors
        ("VSCode Cache", "Library/Caches/com.microsoft.VSCode"),
        ("VSCode GPUCache", "Library/Application Support/Code/GPUCache"),
        ("VSCode CachedData", "Library/Application Support/Code/CachedData"),
        ("Cursor Cache", "Library/Caches/com.todesktop.230313mzl4w4u92"),
        ("Cursor CachedData", "Library/Application Support/Cursor/CachedData"),
        ("JetBrains Caches", "Library/Caches/JetBrains"),
        ("Xcode DerivedData", "Library/Developer/Xcode/DerivedData"),
        ("Xcode Archives", "Library/Developer/Xcode/Archives"),
        ("Xcode iOS DeviceSupport", "Library/Developer/Xcode/iOS DeviceSupport"),
        ("Xcode CoreSimulator Caches", "Library/Developer/CoreSimulator/Caches"),
        ("Postman", "Library/Caches/com.postmanlabs.mac"),
        ("Docker Desktop", "Library/Caches/com.docker.docker"),
        // Design / Creativity
        ("Figma", "Library/Caches/com.figma.Desktop"),
        ("Canva", "Library/Caches/com.canva.CanvaDesktop"),
        ("Adobe Cache", "Library/Caches/Adobe"),
        ("Adobe Media Cache", "Library/Application Support/Adobe/Common/Media Cache Files"),
        ("DaVinci Resolve Cache", "Library/Caches/Blackmagic Design/DaVinci Resolve"),
    ];

    for &(app, rel_path) in cache_specs {
        let full_path = home.join(rel_path);
        if full_path.exists() && seen_paths.insert(full_path.clone()) {
            items.push(AppCacheItem {
                app_name: app.to_string(),
                path: full_path,
            });
        }
    }

    // 2. Dynamically scan all installed macOS applications
    let installed_apps = scan_installed_apps();
    for app in installed_apps {
        if let Some(ref bundle_id) = app.bundle_id {
            let bundle_cache = home.join("Library/Caches").join(bundle_id);
            if bundle_cache.exists() && seen_paths.insert(bundle_cache.clone()) {
                items.push(AppCacheItem {
                    app_name: format!("{} Cache", app.name),
                    path: bundle_cache,
                });
            }
        }
        let name_cache = home.join("Library/Caches").join(&app.name);
        if name_cache.exists() && seen_paths.insert(name_cache.clone()) {
            items.push(AppCacheItem {
                app_name: format!("{} Cache", app.name),
                path: name_cache,
            });
        }
    }

    // 3. Dynamically discover any remaining user application cache folders in ~/Library/Caches
    let user_caches_dir = home.join("Library/Caches");
    if let Ok(entries) = fs::read_dir(user_caches_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let fname = path.file_name().unwrap_or_default().to_string_lossy();
                // Exclude system critical caches handled separately
                if !fname.starts_with("com.apple.")
                    && !fname.starts_with("CloudKit")
                    && !fname.starts_with("ms-playwright")
                    && !fname.starts_with("JetBrains")
                    && !fname.starts_with('.')
                    && seen_paths.insert(path.clone())
                {
                    items.push(AppCacheItem {
                        app_name: format!("{} Cache", fname),
                        path,
                    });
                }
            }
        }
    }

    items
}
