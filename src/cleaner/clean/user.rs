use std::path::PathBuf;

pub struct UserCleanTarget {
    pub name: &'static str,
    pub description: &'static str,
    pub paths: Vec<PathBuf>,
}

pub fn get_user_clean_targets() -> Vec<UserCleanTarget> {
    let mut targets = Vec::new();

    // 1. User Caches
    let cache_paths = vec![
        dirs_home_path("Library/Caches"),
        dirs_home_path(".cache"),
    ];
    targets.push(UserCleanTarget {
        name: "User App Caches",
        description: "Application cache files and temporary runtime data",
        paths: cache_paths.into_iter().filter(|p| p.exists()).collect(),
    });

    // 2. Trash
    let trash_paths = vec![dirs_home_path(".Trash")];
    targets.push(UserCleanTarget {
        name: "macOS Trash",
        description: "Items currently in the Trash bin",
        paths: trash_paths.into_iter().filter(|p| p.exists()).collect(),
    });

    // 3. Mail Downloads
    let mail_paths = vec![
        dirs_home_path("Library/Containers/com.apple.mail/Data/Library/Mail Downloads"),
        dirs_home_path("Library/Mail Downloads"),
    ];
    targets.push(UserCleanTarget {
        name: "Mail Downloads",
        description: "Cached attachments opened from Apple Mail",
        paths: mail_paths.into_iter().filter(|p| p.exists()).collect(),
    });

    // 4. QuickLook & Preview Caches
    let ql_paths = vec![
        dirs_home_path("Library/Caches/com.apple.QuickLook.thumbnailcache"),
        dirs_home_path("Library/Caches/Quick Look"),
    ];
    targets.push(UserCleanTarget {
        name: "QuickLook & Thumbnail Caches",
        description: "Generated QuickLook thumbnails and preview caches",
        paths: ql_paths.into_iter().filter(|p| p.exists()).collect(),
    });

    targets
}

fn dirs_home_path(sub: &str) -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(sub))
        .unwrap_or_else(|| PathBuf::from("/").join(sub))
}
